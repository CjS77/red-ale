//! Formulas: GnuCash's expression parser, with variables and the financial functions, computed exactly.
//!
//! A translation of `5.17:libgnucash/app-utils/calculation/expression_parser.c` (Copyright Terry D. Boldt, 2000;
//! functions added 2002 by Joshua Sled) and `5.17:libgnucash/app-utils/gnc-exp-parser.c` (Copyright (C) 2000 Dave
//! Peticolas), both GPL-2.0-or-later; this translation is GPL-3.0-or-later. The grammar
//! (`expression_parser.c:871-1270`):
//!
//! ```text
//! assignment := add_sub [ ('=' | '+=' | '-=' | '*=' | '/=') assignment ]
//! add_sub    := mul_div ( ('+' | '-') mul_div )*
//! mul_div    := primary ( ('*' | '/') primary )*
//! primary    := '(' assignment ')' | ('+' | '-') primary | number | name
//!             | name '(' [ assignment ( ':' assignment )* ] ')' | '"' text '"'
//! ```
//!
//! Numbers are read in a [`NumberFormat`] (`trans_numeric`, `gnc-exp-parser.c:362-381`) and arithmetic is exact
//! (`numeric_ops`, `:383-421`: `GNC_DENOM_AUTO` with `GNC_HOW_DENOM_EXACT`); the result is reduced (`:546`). The
//! whole expression `(n)`, one number in parentheses, is `-n` (`expression_parser.c:609-620`). A name is a reference
//! to a variable, read when an operator uses it, so `(a = 42) + (b = 12) - a` is 12, and a sign in front of a variable
//! negates the variable itself for the rest of the formula (`:1153-1165`, `negate_numeric` at `gnc-exp-parser.c:424-435`):
//! with `a` = 1, `-a + a` is -2, as in GnuCash.
//!
//! Where red-ale differs from GnuCash (`design/milestone4.md`, "Formulas and variables"):
//!
//! - a variable read before it has a value is an error naming it ([`FormulaError::Unbound`]), where GnuCash creates it
//!   as 0 (`expression_parser.c:677-705`); bindings come only from the caller, never from the user's
//!   `expressions-2.0` table (`gnc-exp-parser.c:62-164`). A binding that is not a valid Numeric (GnuCash's
//!   `GNC_ERROR_ARG`, which marks a scheduled transaction's unset variable) counts as no binding;
//! - functions are [`Functions`], by default the [`Financial`] functions of `5.17:bindings/guile/fin.scm`, computed
//!   exactly where GnuCash calls Guile with doubles (`func_op`, `gnc-exp-parser.c:288-360`);
//! - after a function argument only `:` or `)` may follow, where GnuCash skips one token (`:1184-1197`); `(=`, `)=` and
//!   `:=` are refused, where GnuCash takes them for assignments with no operator (`:785-801`); a string must be closed;
//! - division by zero and overflow are errors, never GnuCash's numeric error or 0; nesting is limited to
//!   [`MAX_DEPTH`].

mod financial;
mod parser;

use std::collections::BTreeMap;

use indexmap::IndexSet;
use thiserror::Error;

use crate::{DENOM_AUTO, How, Numeric, NumericError};
pub use financial::{Financial, MAX_POWER};
use parser::{Mode, Semantics};

/// The deepest nesting of parentheses, signs, function calls and assignments a formula may have.
pub const MAX_DEPTH: usize = 200;

/// Why a formula can't be evaluated.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FormulaError {
    /// Text that is not a number, name or operator where it stands, with the rest of the formula from there.
    #[error("{0:?} is not a number or an arithmetic operator")]
    Unexpected(String),
    /// The formula stops where more was needed.
    #[error("the expression is incomplete")]
    Incomplete,
    /// Unbalanced parentheses.
    #[error("unbalanced parentheses")]
    Parens,
    /// The arithmetic failed: overflow or division by zero.
    #[error("the result can't be computed exactly: {0}")]
    Arithmetic(String),
    /// A variable is read but has no value.
    #[error("the variable {0} has no value")]
    Unbound(String),
    /// Something other than a variable is assigned to (GnuCash's `NOT_A_VARIABLE`, `expression_parser.c:949-954`).
    #[error("only a variable can be assigned to")]
    NotAVariable,
    /// A string is used as a number; strings may only be function arguments.
    #[error("the text {0:?} is not a number")]
    Text(String),
    /// A function call failed.
    #[error("function {name} {error}")]
    Function {
        /// The function's name, as written.
        name: String,
        /// Why it failed.
        error: FunctionError,
    },
    /// The formula nests deeper than [`MAX_DEPTH`].
    #[error("the expression nests more than {MAX_DEPTH} deep")]
    TooDeep,
}

impl FormulaError {
    /// The API's `data.reason` for it (`design/api/scheduled-transactions.md`, "Evaluation"): `syntax`,
    /// `unknown-function`, `inexact`, `arguments`, `non-integer-power`, `string-argument`, `division-by-zero`,
    /// `overflow` or `unbound-variable`.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Unexpected(_) | Self::Incomplete | Self::Parens | Self::NotAVariable | Self::TooDeep => "syntax",
            Self::Arithmetic(m) if *m == NumericError::DivideByZero.to_string() => "division-by-zero",
            Self::Arithmetic(_) => "overflow",
            Self::Unbound(_) => "unbound-variable",
            Self::Text(_) => "string-argument",
            Self::Function { error, .. } => error.reason(),
        }
    }
}

/// Why a function call failed. Shown after the function's name ([`FormulaError::Function`]).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FunctionError {
    /// No such function.
    #[error("is not one of GnuCash's financial functions")]
    Unknown,
    /// The wrong number of arguments.
    #[error("takes {0} arguments")]
    Arity(usize),
    /// A string where a number is needed.
    #[error("takes numbers, not text")]
    Text,
    /// A power that is not a whole number, whose result is irrational.
    #[error("needs {0} to be a whole number to be computed exactly")]
    NotWhole(&'static str),
    /// A power too large to compute exactly.
    #[error("needs {0} to be at most {MAX_POWER} in size")]
    TooLarge(&'static str),
    /// A division by zero, where GnuCash's doubles give NaN or infinity and the call fails (`gnc-numeric.cpp:89-95`).
    #[error("divides by zero (with a zero rate, for example)")]
    DivisionByZero,
    /// A result, or a power within it, outside what GnuCash's doubles and Numerics hold.
    #[error("gives a result outside the range GnuCash computes")]
    Range,
    /// A function red-ale does not compute.
    #[error("can't be computed exactly: {0}")]
    Inexact(&'static str),
}

impl FunctionError {
    /// The API's `data.reason` for a call that failed this way.
    fn reason(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown-function",
            Self::Inexact(_) => "inexact",
            Self::Arity(_) => "arguments",
            Self::Text => "string-argument",
            Self::NotWhole(_) => "non-integer-power",
            Self::DivisionByZero => "division-by-zero",
            Self::TooLarge(_) | Self::Range => "overflow",
        }
    }
}

/// The locale's monetary separators, as `gnc_localeconv` gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumberFormat {
    /// `mon_decimal_point`.
    pub decimal_point: char,
    /// `mon_thousands_sep`.
    pub group_separator: char,
}

impl Default for NumberFormat {
    fn default() -> Self {
        Self {
            decimal_point: '.',
            group_separator: ',',
        }
    }
}

/// Variable bindings, by name.
pub type Variables = BTreeMap<String, Numeric>;

/// One argument of a function call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Argument<'a> {
    /// A number.
    Number(Numeric),
    /// A string, `"…"` in the formula.
    Text(&'a str),
}

/// The functions a formula may call: GnuCash's `func_op` (`gnc-exp-parser.c:288-360`).
pub trait Functions {
    /// The value of `name(args…)`, rounded as GnuCash rounds a function's result.
    fn call(&self, name: &str, args: &[Argument<'_>]) -> Result<Numeric, FunctionError>;
}

/// `gnc_exp_parser_parse` without variables, assignments or functions: what the business importer reads numbers with.
pub fn evaluate_arithmetic(text: &str, format: NumberFormat) -> Result<Numeric, FormulaError> {
    let bound = Variables::new();
    let mut eval = Eval::new(&bound, &Financial);
    let value = parser::run(text, format, Mode::Arithmetic, &mut eval)?;
    eval.finish(&value)
}

/// The value of a formula with `variables` bound and the [`Financial`] functions.
pub fn evaluate(text: &str, format: NumberFormat, variables: &Variables) -> Result<Numeric, FormulaError> {
    evaluate_with(text, format, variables, &Financial)
}

/// The value of a formula with `variables` bound and `functions` to call.
pub fn evaluate_with(text: &str, format: NumberFormat, variables: &Variables, functions: &dyn Functions) -> Result<Numeric, FormulaError> {
    let mut eval = Eval::new(variables, functions);
    let value = parser::run(text, format, Mode::Formula, &mut eval)?;
    eval.finish(&value)
}

/// Every variable a formula names, read or assigned, in order of first appearance: the names
/// `gnc_sx_parse_vars_from_formula` adds to a scheduled transaction's variables
/// (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:234-263`, through `parser_get_vars`). Function names are not
/// variables. GnuCash evaluates the formula to find them and stops at the first call that fails; red-ale reads the
/// whole formula without evaluating it, so a malformed formula is an error but a failing call is not.
pub fn free_variables(text: &str, format: NumberFormat) -> Result<Vec<String>, FormulaError> {
    let mut names = Names::default();
    parser::run(text, format, Mode::Formula, &mut names)?;
    Ok(names.0.into_iter().map(str::to_owned).collect())
}

/// A value on GnuCash's parser stack: a temporary, a reference to a named variable, or a string.
#[derive(Debug, Clone, Copy)]
enum Value<'a> {
    Num(Numeric),
    Var(&'a str),
    Str(&'a str),
}

fn arithmetic(e: NumericError) -> FormulaError {
    FormulaError::Arithmetic(e.to_string())
}

/// Evaluation, with the caller's bindings and the variables the formula assigns.
struct Eval<'a, 'b> {
    bound: &'b Variables,
    assigned: BTreeMap<&'a str, Numeric>,
    functions: &'b dyn Functions,
}

impl<'a, 'b> Eval<'a, 'b> {
    fn new(bound: &'b Variables, functions: &'b dyn Functions) -> Self {
        Self {
            bound,
            assigned: BTreeMap::new(),
            functions,
        }
    }

    fn read(&self, v: &Value<'a>) -> Result<Numeric, FormulaError> {
        match *v {
            Value::Num(n) => Ok(n),
            Value::Var(name) => self
                .assigned
                .get(name)
                .or_else(|| self.bound.get(name))
                .copied()
                .filter(|n| n.is_valid())
                .ok_or_else(|| FormulaError::Unbound(name.to_owned())),
            Value::Str(s) => Err(FormulaError::Text(s.to_owned())),
        }
    }

    fn finish(&self, v: &Value<'a>) -> Result<Numeric, FormulaError> {
        self.read(v).map(Numeric::reduce)
    }

    fn argument(&self, v: &Value<'a>) -> Result<Argument<'a>, FormulaError> {
        match *v {
            Value::Str(s) => Ok(Argument::Text(s)),
            _ => self.read(v).map(Argument::Number),
        }
    }
}

impl<'a> Semantics<'a> for Eval<'a, '_> {
    type Value = Value<'a>;

    fn number(&mut self, n: Numeric) -> Value<'a> {
        Value::Num(n)
    }

    fn string(&mut self, s: &'a str) -> Value<'a> {
        Value::Str(s)
    }

    fn variable(&mut self, name: &'a str) -> Value<'a> {
        Value::Var(name)
    }

    fn is_variable(v: &Value<'a>) -> bool {
        matches!(v, Value::Var(_))
    }

    fn binary(&mut self, op: char, l: Value<'a>, r: Value<'a>) -> Result<Value<'a>, FormulaError> {
        let (l, r) = (self.read(&l)?, self.read(&r)?);
        let exact = How::EXACT;
        match op {
            '+' => l.add(r, DENOM_AUTO, exact),
            '-' => l.sub(r, DENOM_AUTO, exact),
            '*' => l.mul(r, DENOM_AUTO, exact),
            _ if r.is_zero() => return Err(arithmetic(NumericError::DivideByZero)),
            _ => l.div(r, DENOM_AUTO, exact),
        }
        .map(Value::Num)
        .map_err(arithmetic)
    }

    fn negate(&mut self, v: Value<'a>) -> Result<Value<'a>, FormulaError> {
        let n = self.read(&v)?;
        let negated = n.num().checked_neg().map(|num| Numeric::new(num, n.denom())).ok_or(NumericError::Overflow).map_err(arithmetic)?;
        Ok(match v {
            Value::Var(name) => {
                self.assigned.insert(name, negated);
                v
            }
            _ => Value::Num(negated),
        })
    }

    fn assign(&mut self, op: Option<char>, target: Value<'a>, v: Value<'a>) -> Result<Value<'a>, FormulaError> {
        let value = match op {
            Some(op) => self.binary(op, target, v)?,
            None => v,
        };
        let n = self.read(&value)?;
        if let Value::Var(name) = target {
            self.assigned.insert(name, n);
        }
        Ok(target)
    }

    fn call(&mut self, name: &'a str, args: Vec<Value<'a>>) -> Result<Value<'a>, FormulaError> {
        let args = args.iter().map(|a| self.argument(a)).collect::<Result<Vec<_>, _>>()?;
        self.functions.call(name, &args).map(Value::Num).map_err(|error| FormulaError::Function {
            name: name.to_owned(),
            error,
        })
    }
}

/// The names a formula uses, without evaluating it.
#[derive(Default)]
struct Names<'a>(IndexSet<&'a str>);

impl<'a> Semantics<'a> for Names<'a> {
    /// The variable's name, if the value is a variable.
    type Value = Option<&'a str>;

    fn number(&mut self, _: Numeric) -> Self::Value {
        None
    }

    fn string(&mut self, _: &'a str) -> Self::Value {
        None
    }

    fn variable(&mut self, name: &'a str) -> Self::Value {
        self.0.insert(name);
        Some(name)
    }

    fn is_variable(v: &Self::Value) -> bool {
        v.is_some()
    }

    fn binary(&mut self, _: char, _: Self::Value, _: Self::Value) -> Result<Self::Value, FormulaError> {
        Ok(None)
    }

    fn negate(&mut self, v: Self::Value) -> Result<Self::Value, FormulaError> {
        Ok(v)
    }

    fn assign(&mut self, _: Option<char>, target: Self::Value, _: Self::Value) -> Result<Self::Value, FormulaError> {
        Ok(target)
    }

    fn call(&mut self, _: &'a str, _: Vec<Self::Value>) -> Result<Self::Value, FormulaError> {
        Ok(None)
    }
}
