//! `gnc-numeric-from-string`, which `qif-file:parse-price-line` reads a price with.
//!
//! GnuCash's `gnc_numeric_from_string` (`5.17:libgnucash/engine/gnc-numeric.cpp:1405-1426`) is the
//! `GncNumeric(const std::string&)` constructor, translated in [`red_ale_core::numeric::text`].
//!
//! Only the value matters to the QIF reader: GnuCash hands the result to Scheme, which reduces it
//! (`gnc_numeric_to_scm`, `5.17:bindings/guile/gnc-engine-guile.cpp:1729-1734`), so the result here is reduced
//! too. Where GnuCash throws or returns an error value, this returns `None`.

use red_ale_core::Numeric;
use red_ale_core::numeric::text;

use super::parse::reduce_to_numeric;

/// `gnc_numeric_from_string`, reduced. `gnc-numeric.cpp:1405-1426`, `:247-389`.
pub fn gnc_numeric_from_string(s: &str) -> Option<Numeric> {
    let (num, den) = text::parts(s)?;
    reduce_to_numeric(num, den)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(s: &str) -> Option<(i64, i64)> {
        gnc_numeric_from_string(s).map(|n| (n.num(), n.denom()))
    }

    #[test]
    fn price_values() {
        // test-qif-imp.scm:65-84 reads "1.0" as 1 and "1 3/4" as 7/4, and an empty value as no price.
        assert_eq!(parsed("1.0"), Some((1, 1)));
        assert_eq!(parsed("1 3/4"), Some((7, 4)));
        assert_eq!(parsed(""), None);
    }

    #[test]
    fn constructor_forms() {
        // gnc-numeric.cpp:290-389, form by form.
        assert_eq!(parsed("42"), Some((42, 1)));
        assert_eq!(parsed("-85/2"), Some((-85, 2)));
        assert_eq!(parsed(" 42.50 "), Some((85, 2)));
        assert_eq!(parsed("-1 1/2"), Some((-3, 2)));
        assert_eq!(parsed("3 / 4"), Some((3, 4)));
        assert_eq!(parsed("1.5e2"), Some((150, 1)));
        assert_eq!(parsed("15e-1"), Some((3, 2)));
        assert_eq!(parsed("1 234"), Some((1234, 1)));
        assert_eq!(parsed("1.234.567"), Some((1_234_567, 1)));
        assert_eq!(parsed("0x1F"), Some((31, 1)));
        assert_eq!(parsed("$12"), Some((12, 1)));
        assert_eq!(parsed("1/0"), None);
        assert_eq!(parsed("abc"), None);
        assert_eq!(parsed("99999999999999999999"), None);
    }
}
