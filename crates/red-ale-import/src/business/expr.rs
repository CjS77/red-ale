//! Numbers in business import files, read as GnuCash's expression parser reads them.
//!
//! GnuCash reads quantities, prices and discounts with `gnc_exp_parser_parse`
//! (`5.17:libgnucash/app-utils/gnc-exp-parser.c:459-597`, Copyright (C) 2000 Dave Peticolas;
//! `5.17:libgnucash/app-utils/calculation/expression_parser.c`, GPL-2.0-or-later): arithmetic with `+`, `-`, `*`,
//! `/`, unary signs and parentheses on exact numerics (`numeric_ops`, `gnc-exp-parser.c:383-421`: `GNC_DENOM_AUTO`
//! with `GNC_HOW_DENOM_EXACT`), numbers in the locale's monetary decimal point and group separator (`trans_numeric`,
//! `:362-381`, through `xaccParseAmount`), and the result reduced (`:546`). This translation is GPL-3.0-or-later.
//!
//! GnuCash's importer ignores whether the parse worked, so a number it can't read is 0
//! (`5.17:gnucash/import-export/bi-import/dialog-bi-import.c:793-795`). Here it is an error, and so are the things
//! the parser would accept that change a number silently (`design/api/import.md`, "Deviations from GnuCash"):
//!
//! - variables, assignments and function calls (GnuCash evaluates functions in Guile) are refused;
//! - a group separator after the decimal point is refused, where `xaccParseAmount` skips it (`1.234,5` would be
//!   1.2345 in a `.` locale);
//! - fraction digits are kept exactly, where `xaccParseAmount` cuts them at twelve; a number that doesn't fit 64 bits
//!   is refused;
//! - division by zero and results that don't fit are refused, where GnuCash's numeric error gives 0.

use red_ale_core::{DENOM_AUTO, How, Numeric};
use thiserror::Error;

/// Why an expression can't be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExprError {
    /// A character no expression may hold, at a byte offset.
    #[error("{0:?} is not a number or an arithmetic operator")]
    Unexpected(String),
    /// The expression stops where more was needed.
    #[error("the expression is incomplete")]
    Incomplete,
    /// Unbalanced parentheses.
    #[error("unbalanced parentheses")]
    Parens,
    /// The arithmetic failed: overflow or division by zero.
    #[error("the result can't be computed exactly: {0}")]
    Arithmetic(String),
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

/// One token.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Num(Numeric),
    Op(char),
    End,
}

/// The tokenizer and recursive-descent parser of `expression_parser.c`, without variables and functions.
struct Parser<'a> {
    rest: &'a str,
    format: NumberFormat,
    token: Token,
}

const EXACT: How = How::EXACT;

fn arithmetic(e: impl std::fmt::Display) -> ExprError {
    ExprError::Arithmetic(e.to_string())
}

/// The number at the start of `s`: digits with group separators, a decimal point, then digits.
fn scan_number(s: &str, f: NumberFormat) -> Result<(Numeric, &str), ExprError> {
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || c == f.decimal_point || c == f.group_separator))
        .unwrap_or(s.len());
    let (body, rest) = s.split_at(end);
    let (integer, fraction) = body.split_once(f.decimal_point).unwrap_or((body, ""));
    if fraction.contains([f.decimal_point, f.group_separator]) {
        return Err(ExprError::Unexpected(body.to_owned()));
    }
    let integer: String = integer.chars().filter(char::is_ascii_digit).collect();
    if integer.is_empty() && fraction.is_empty() {
        return Err(ExprError::Unexpected(s.to_owned()));
    }
    let digits = format!("{integer}{fraction}");
    let numer: i64 = if digits.is_empty() {
        0
    } else {
        digits
            .parse()
            .map_err(|_| ExprError::Arithmetic(format!("{digits} does not fit")))?
    };
    let exp = u32::try_from(fraction.len()).map_err(|_| ExprError::Arithmetic("too many digits".into()))?;
    let denom = 10_i64
        .checked_pow(exp)
        .ok_or_else(|| ExprError::Arithmetic(format!("{} fraction digits", fraction.len())))?;
    Ok((Numeric::new(numer, denom), rest))
}

impl<'a> Parser<'a> {
    fn new(text: &'a str, format: NumberFormat) -> Result<Self, ExprError> {
        let mut p = Self {
            rest: text,
            format,
            token: Token::End,
        };
        p.next()?;
        Ok(p)
    }

    /// `next_token` (`expression_parser.c:766-869`): operators first, so a sign is never part of a number.
    fn next(&mut self) -> Result<(), ExprError> {
        self.rest = self.rest.trim_start_matches(|c: char| c.is_ascii_whitespace());
        let Some(c) = self.rest.chars().next() else {
            self.token = Token::End;
            return Ok(());
        };
        if "+-*/()".contains(c) {
            self.token = Token::Op(c);
            self.rest = &self.rest[c.len_utf8()..];
            return Ok(());
        }
        let (n, rest) = scan_number(self.rest, self.format)?;
        self.token = Token::Num(n);
        self.rest = rest;
        Ok(())
    }

    /// `add_sub_op`.
    fn sum(&mut self) -> Result<Numeric, ExprError> {
        let mut value = self.product()?;
        while let Token::Op(op @ ('+' | '-')) = self.token {
            self.next()?;
            let right = self.product()?;
            value = if op == '+' {
                value.add(right, DENOM_AUTO, EXACT)
            } else {
                value.sub(right, DENOM_AUTO, EXACT)
            }
            .map_err(arithmetic)?;
        }
        Ok(value)
    }

    /// `multiply_divide_op`.
    fn product(&mut self) -> Result<Numeric, ExprError> {
        let mut value = self.primary()?;
        while let Token::Op(op @ ('*' | '/')) = self.token {
            self.next()?;
            let right = self.primary()?;
            if op == '/' && right.is_zero() {
                return Err(ExprError::Arithmetic("division by zero".into()));
            }
            value = if op == '*' {
                value.mul(right, DENOM_AUTO, EXACT)
            } else {
                value.div(right, DENOM_AUTO, EXACT)
            }
            .map_err(arithmetic)?;
        }
        Ok(value)
    }

    /// `primary_exp` (`expression_parser.c:1106-1270`): a number, a signed primary, or a parenthesised sum.
    fn primary(&mut self) -> Result<Numeric, ExprError> {
        match self.token {
            Token::Num(n) => {
                self.next()?;
                if matches!(self.token, Token::Num(_) | Token::Op('(')) {
                    return Err(ExprError::Unexpected(self.rest.to_owned()));
                }
                Ok(n)
            }
            Token::Op(sign @ ('+' | '-')) => {
                self.next()?;
                let v = self.primary()?;
                Ok(if sign == '-' { v.neg() } else { v })
            }
            Token::Op('(') => {
                self.next()?;
                let v = self.sum()?;
                if self.token != Token::Op(')') {
                    return Err(ExprError::Parens);
                }
                self.next()?;
                Ok(v)
            }
            Token::Op(')') => Err(ExprError::Parens),
            Token::Op(c) => Err(ExprError::Unexpected(c.to_string())),
            Token::End => Err(ExprError::Incomplete),
        }
    }
}

/// `gnc_exp_parser_parse` on `text`: the value, reduced.
pub fn parse_expression(text: &str, format: NumberFormat) -> Result<Numeric, ExprError> {
    let mut p = Parser::new(text, format)?;
    let value = p.sum()?;
    match p.token {
        Token::End => Ok(value.reduce()),
        Token::Op(')') => Err(ExprError::Parens),
        _ => Err(ExprError::Unexpected(p.rest.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn eval(text: &str) -> Result<(i64, i64), ExprError> {
        parse_expression(text, NumberFormat::default()).map(|n| (n.num(), n.denom()))
    }

    #[test]
    fn numbers_and_arithmetic_are_exact_and_reduced() {
        assert_eq!(eval("10.50"), Ok((21, 2)));
        assert_eq!(eval("1,234.5"), Ok((2469, 2)));
        assert_eq!(eval(" 2 * 3.5 "), Ok((7, 1)));
        assert_eq!(eval("1/3"), Ok((1, 3)));
        assert_eq!(eval("-(2+3)*-2"), Ok((10, 1)));
        assert_eq!(eval("10 - 2 - 3"), Ok((5, 1)));
        assert_eq!(eval("8/2/2"), Ok((2, 1)));
        assert_eq!(eval(".5"), Ok((1, 2)));
        assert_eq!(eval("5."), Ok((5, 1)));
        assert_eq!(eval("0.0000000000001"), Ok((1, 10_000_000_000_000)), "kept past twelve digits");
    }

    #[test]
    fn what_gnucash_reads_as_zero_is_an_error() {
        [
            "abc",
            "x+1",
            "sqrt(4)",
            "$10",
            "1.2.3",
            "1.234,5",
            "1/0",
            "(1+2",
            "1+2)",
            "2 3",
            "",
            "1+",
            "*2",
            "99999999999999999999",
        ]
        .iter()
        .for_each(|t| assert!(eval(t).is_err(), "{t:?} gave {:?}", eval(t)));
    }

    #[test]
    fn the_locale_separators() {
        let f = NumberFormat {
            decimal_point: ',',
            group_separator: '.',
        };
        assert_eq!(parse_expression("1.234,5", f).map(|n| (n.num(), n.denom())), Ok((2469, 2)));
    }
}
