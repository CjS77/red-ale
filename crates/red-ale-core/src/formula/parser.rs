//! The tokenizer and recursive-descent parser of `5.17:libgnucash/app-utils/calculation/expression_parser.c`
//! (Copyright Terry D. Boldt, 2000; GPL-2.0-or-later; this translation is GPL-3.0-or-later).
//!
//! As in GnuCash, parsing and evaluating are one pass: the parser hands each operation to a [`Semantics`] as it reads
//! it, so errors come in the order GnuCash meets them. [`Mode::Arithmetic`] is v0.3's business-import parser, which
//! reads only numbers, `+-*/` and parentheses, and does not negate `(n)`.

use super::{FormulaError, MAX_DEPTH, NumberFormat};
use crate::Numeric;

/// What the parser does with each operation it reads.
pub(super) trait Semantics<'a> {
    /// A value on the parser's stack.
    type Value;
    fn number(&mut self, n: Numeric) -> Self::Value;
    fn string(&mut self, s: &'a str) -> Self::Value;
    fn variable(&mut self, name: &'a str) -> Self::Value;
    fn is_variable(v: &Self::Value) -> bool;
    fn binary(&mut self, op: char, l: Self::Value, r: Self::Value) -> Result<Self::Value, FormulaError>;
    fn negate(&mut self, v: Self::Value) -> Result<Self::Value, FormulaError>;
    fn assign(&mut self, op: Option<char>, target: Self::Value, v: Self::Value) -> Result<Self::Value, FormulaError>;
    fn call(&mut self, name: &'a str, args: Vec<Self::Value>) -> Result<Self::Value, FormulaError>;
}

/// Which grammar the parser reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// Numbers and `+-*/()` only.
    Arithmetic,
    /// Everything GnuCash's parser reads.
    Formula,
}

/// One token.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Token<'a> {
    Num(Numeric),
    Op(char),
    /// `=`, or `+=`, `-=`, `*=`, `/=` with the operator.
    Assign(Option<char>),
    Name(&'a str),
    /// A name immediately followed by `(`, which is consumed with it.
    Func(&'a str),
    Str(&'a str),
    End,
}

impl Token<'_> {
    /// GnuCash's token code, as `add_token` records it (`expression_parser.c:445-449`, `:755-764`).
    fn code(self) -> char {
        match self {
            Token::Num(_) => 'I',
            Token::Op(c) => c,
            Token::Assign(_) => '=',
            Token::Name(_) => 'V',
            Token::Func(_) => 'F',
            Token::Str(_) => '"',
            Token::End => '\0',
        }
    }

    /// An operand, which may not directly follow another (`check_expression_grammar_error`, `:1083-1096`); red-ale
    /// adds `(`, which GnuCash refuses later or, inside a call's arguments, skips.
    fn starts_operand(self) -> bool {
        matches!(self, Token::Num(_) | Token::Name(_) | Token::Func(_) | Token::Str(_) | Token::Op('('))
    }
}

/// The number at the start of `s`: digits with group separators, a decimal point, then digits.
fn scan_number(s: &str, f: NumberFormat) -> Result<(Numeric, &str), FormulaError> {
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || c == f.decimal_point || c == f.group_separator))
        .unwrap_or(s.len());
    let (body, rest) = s.split_at(end);
    let (integer, fraction) = body.split_once(f.decimal_point).unwrap_or((body, ""));
    if fraction.contains([f.decimal_point, f.group_separator]) {
        return Err(FormulaError::Unexpected(body.to_owned()));
    }
    let integer: String = integer.chars().filter(char::is_ascii_digit).collect();
    if integer.is_empty() && fraction.is_empty() {
        return Err(FormulaError::Unexpected(s.to_owned()));
    }
    let digits = format!("{integer}{fraction}");
    let numer: i64 = if digits.is_empty() {
        0
    } else {
        digits
            .parse()
            .map_err(|_| FormulaError::Arithmetic(format!("{digits} does not fit")))?
    };
    let exp = u32::try_from(fraction.len()).map_err(|_| FormulaError::Arithmetic("too many digits".into()))?;
    let denom = 10_i64
        .checked_pow(exp)
        .ok_or_else(|| FormulaError::Arithmetic(format!("{} fraction digits", fraction.len())))?;
    Ok((Numeric::new(numer, denom), rest))
}

/// A name, `[A-Za-z_][A-Za-z0-9_]*` (`expression_parser.c:818-852`), and whether `(` follows it at once.
fn scan_name(s: &str) -> (Token<'_>, &str) {
    let end = s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(s.len());
    let (name, rest) = s.split_at(end);
    match rest.strip_prefix('(') {
        Some(rest) => (Token::Func(name), rest),
        None => (Token::Name(name), rest),
    }
}

/// The parser, handing what it reads to `S`.
struct Parser<'a, 's, S> {
    rest: &'a str,
    format: NumberFormat,
    mode: Mode,
    token: Token<'a>,
    /// The first token codes, enough to see `(I)`.
    shape: String,
    depth: usize,
    sem: &'s mut S,
}

/// Parse and evaluate `text`: `parse_string` (`expression_parser.c:583-639`).
pub(super) fn run<'a, S: Semantics<'a>>(text: &'a str, format: NumberFormat, mode: Mode, sem: &mut S) -> Result<S::Value, FormulaError> {
    let mut p = Parser {
        rest: text,
        format,
        mode,
        token: Token::End,
        shape: String::new(),
        depth: 0,
        sem,
    };
    p.next()?;
    let value = p.assignment()?;
    match p.token {
        Token::End if mode == Mode::Formula && p.shape == "(I)" => p.sem.negate(value),
        Token::End => Ok(value),
        Token::Op(')') => Err(FormulaError::Parens),
        _ => Err(FormulaError::Unexpected(p.rest.to_owned())),
    }
}

impl<'a, S: Semantics<'a>> Parser<'a, '_, S> {
    /// `next_token` (`expression_parser.c:766-869`): operators first, so a sign is never part of a number.
    fn next(&mut self) -> Result<(), FormulaError> {
        self.rest = self.rest.trim_start_matches(|c: char| c.is_ascii_whitespace());
        let (token, rest) = self.scan()?;
        self.token = token;
        self.rest = rest;
        if self.shape.len() < 4 && token != Token::End {
            self.shape.push(token.code());
        }
        Ok(())
    }

    fn scan(&self) -> Result<(Token<'a>, &'a str), FormulaError> {
        let s = self.rest;
        let Some(c) = s.chars().next() else {
            return Ok((Token::End, s));
        };
        let after = &s[c.len_utf8()..];
        if "+-*/()".contains(c) {
            return Ok(match after.strip_prefix('=') {
                Some(rest) if self.mode == Mode::Formula && "+-*/".contains(c) => (Token::Assign(Some(c)), rest),
                _ => (Token::Op(c), after),
            });
        }
        if self.mode == Mode::Arithmetic {
            return scan_number(s, self.format).map(|(n, rest)| (Token::Num(n), rest));
        }
        match c {
            '=' if after.starts_with('=') => Err(FormulaError::Unexpected(s.to_owned())),
            '=' => Ok((Token::Assign(None), after)),
            ':' => Ok((Token::Op(':'), after)),
            '"' => after
                .split_once('"')
                .map(|(text, rest)| (Token::Str(text), rest))
                .ok_or(FormulaError::Incomplete),
            c if c.is_ascii_alphabetic() || c == '_' => Ok(scan_name(s)),
            _ => scan_number(s, self.format).map(|(n, rest)| (Token::Num(n), rest)),
        }
    }

    /// Run `f` one level deeper, refusing past [`MAX_DEPTH`].
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, FormulaError>) -> Result<T, FormulaError> {
        if self.depth >= MAX_DEPTH {
            return Err(FormulaError::TooDeep);
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }

    /// `assignment_op` (`expression_parser.c:879-976`): right-associative, onto a variable only.
    fn assignment(&mut self) -> Result<S::Value, FormulaError> {
        let target = self.sum()?;
        let Token::Assign(op) = self.token else {
            return Ok(target);
        };
        if !S::is_variable(&target) {
            return Err(FormulaError::NotAVariable);
        }
        self.next()?;
        let value = self.nested(Self::assignment)?;
        self.sem.assign(op, target, value)
    }

    /// `add_sub_op`.
    fn sum(&mut self) -> Result<S::Value, FormulaError> {
        let mut value = self.product()?;
        while let Token::Op(op @ ('+' | '-')) = self.token {
            self.next()?;
            let right = self.product()?;
            value = self.sem.binary(op, value, right)?;
        }
        Ok(value)
    }

    /// `multiply_divide_op`.
    fn product(&mut self) -> Result<S::Value, FormulaError> {
        let mut value = self.nested(Self::primary)?;
        while let Token::Op(op @ ('*' | '/')) = self.token {
            self.next()?;
            let right = self.nested(Self::primary)?;
            value = self.sem.binary(op, value, right)?;
        }
        Ok(value)
    }

    /// The token after an operand, which may not be another operand.
    fn after_operand(&mut self) -> Result<(), FormulaError> {
        self.next()?;
        if self.token.starts_operand() {
            return Err(FormulaError::Unexpected(self.rest.to_owned()));
        }
        Ok(())
    }

    /// `primary_exp` (`expression_parser.c:1106-1270`).
    fn primary(&mut self) -> Result<S::Value, FormulaError> {
        match self.token {
            Token::Num(n) => {
                self.after_operand()?;
                Ok(self.sem.number(n))
            }
            Token::Name(name) => {
                self.after_operand()?;
                Ok(self.sem.variable(name))
            }
            Token::Func(name) => self.call(name),
            Token::Str(text) => {
                self.next()?;
                if !matches!(self.token, Token::Op(')' | ':')) {
                    return Err(FormulaError::Unexpected(self.rest.to_owned()));
                }
                Ok(self.sem.string(text))
            }
            Token::Op(sign @ ('+' | '-')) => {
                self.next()?;
                let v = self.nested(Self::primary)?;
                if sign == '-' { self.sem.negate(v) } else { Ok(v) }
            }
            Token::Op('(') => {
                self.next()?;
                let v = self.assignment()?;
                if self.token != Token::Op(')') {
                    return Err(FormulaError::Parens);
                }
                self.next()?;
                Ok(v)
            }
            Token::Op(')') => Err(FormulaError::Parens),
            Token::Op(c) => Err(FormulaError::Unexpected(c.to_string())),
            Token::Assign(_) => Err(FormulaError::Unexpected("=".into())),
            Token::End => Err(FormulaError::Incomplete),
        }
    }

    /// A function call, its `(` already read: arguments separated by `:` up to `)` (`expression_parser.c:1180-1244`).
    fn call(&mut self, name: &'a str) -> Result<S::Value, FormulaError> {
        self.next()?;
        let mut args = Vec::new();
        if self.token != Token::Op(')') {
            loop {
                args.push(self.assignment()?);
                match self.token {
                    Token::Op(':') => self.next()?,
                    Token::Op(')') => break,
                    Token::End => return Err(FormulaError::Parens),
                    _ => return Err(FormulaError::Unexpected(self.rest.to_owned())),
                }
            }
        }
        let value = self.sem.call(name, args)?;
        self.after_operand()?;
        Ok(value)
    }
}
