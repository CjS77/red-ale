//! Numbers in business import files, read as GnuCash's expression parser reads them.
//!
//! GnuCash reads quantities, prices and discounts with `gnc_exp_parser_parse`
//! (`5.17:libgnucash/app-utils/gnc-exp-parser.c:459-597`, Copyright (C) 2000 Dave Peticolas;
//! `5.17:libgnucash/app-utils/calculation/expression_parser.c`, GPL-2.0-or-later): arithmetic with `+`, `-`, `*`,
//! `/`, unary signs and parentheses on exact numerics (`numeric_ops`, `gnc-exp-parser.c:383-421`: `GNC_DENOM_AUTO`
//! with `GNC_HOW_DENOM_EXACT`), numbers in the locale's monetary decimal point and group separator (`trans_numeric`,
//! `:362-381`, through `xaccParseAmount`), and the result reduced (`:546`). The translation is
//! [`red_ale_core::formula`]; this importer uses its arithmetic-only mode.
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

use red_ale_core::Numeric;
use red_ale_core::formula;
pub use red_ale_core::formula::{FormulaError as ExprError, NumberFormat};

/// `gnc_exp_parser_parse` on `text`: the value, reduced.
pub fn parse_expression(text: &str, format: NumberFormat) -> Result<Numeric, ExprError> {
    formula::evaluate_arithmetic(text, format)
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

    /// A lone parenthesised number is negative, accounting style, as GnuCash's parser reads it
    /// (`5.17:libgnucash/app-utils/calculation/expression_parser.c:609-620`); anything more in the parentheses is not.
    #[test]
    fn a_parenthesised_number_is_negative_as_in_gnucash() {
        assert_eq!(eval("(5)"), Ok((-5, 1)));
        assert_eq!(eval("(12.50)"), Ok((-25, 2)));
        assert_eq!(eval("(2+3)"), Ok((5, 1)));
        assert_eq!(eval("(5)*2"), Ok((10, 1)));
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
