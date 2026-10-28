//! Numbers as red-ale writes them into Formulas: in the Locale's decimal form without grouping, never rounded
//! (`design/api/scheduled-transactions.md`, "Schedule from transaction"). GnuCash writes them with the locale's
//! grouping (`xaccPrintAmount`, `5.17:libgnucash/app-utils/gnc-ui-util.cpp:833-859`), which another locale's parser
//! reads differently, since it skips a group separator wherever one stands before the decimal point.

use crate::Numeric;
use crate::formula::NumberFormat;

/// The most decimal places a Numeric's exact decimal form can need: an `i64` denominator has at most 62 factors of
/// two, but a denominator of 10^18 is the largest power of ten it holds.
const MAX_PLACES: u32 = 18;

/// `value` with at least `min_places` decimals and as many more as it needs, in `format`'s decimal point and with no
/// group separator: `1234.50` in the default format. A value with no finite decimal form is written `num / denom`,
/// which the parser reads exactly.
pub fn formula_number(value: Numeric, min_places: u32, format: NumberFormat) -> String {
    let r = value.reduce();
    let (num, denom) = (i128::from(r.num()), i128::from(r.denom()));
    let places = (0..=MAX_PLACES).find(|k| denom > 0 && 10_i128.pow(*k) % denom == 0);
    let Some(places) = places.map(|k| k.max(min_places.min(MAX_PLACES))) else {
        return format!("{num} / {denom}");
    };
    let scaled = num * (10_i128.pow(places) / denom);
    let digits = format!(
        "{:0>width$}",
        scaled.unsigned_abs(),
        width = usize::try_from(places).unwrap_or(0) + 1
    );
    let (whole, fraction) = digits.split_at(digits.len() - usize::try_from(places).unwrap_or(0));
    let sign = if scaled < 0 { "-" } else { "" };
    if fraction.is_empty() {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}{}{fraction}", format.decimal_point)
    }
}

/// The decimal places of a currency's fraction: 2 for 1/100, 0 for 1; at least 2 when the fraction is not a power of
/// ten.
pub fn places_of(fraction: i64) -> u32 {
    (0..=MAX_PLACES).find(|k| 10_i64.checked_pow(*k) == Some(fraction)).unwrap_or(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(num: i64, denom: i64) -> Numeric {
        Numeric::new(num, denom)
    }

    #[test]
    fn numbers_are_exact_and_ungrouped_in_the_locales_decimal_point() {
        let c = NumberFormat::default();
        let de = NumberFormat {
            decimal_point: ',',
            group_separator: '.',
        };
        assert_eq!(formula_number(n(123_450, 100), 2, c), "1234.50");
        assert_eq!(formula_number(n(123_450, 100), 2, de), "1234,50");
        assert_eq!(formula_number(n(100_000, 1), 2, c), "100000.00");
        assert_eq!(formula_number(n(6, 100), 5, c), "0.06000");
        assert_eq!(formula_number(n(1, 8), 2, c), "0.125", "never rounded");
        assert_eq!(formula_number(n(-5, 1), 0, c), "-5");
        assert_eq!(formula_number(n(-1, 200), 2, c), "-0.005");
        assert_eq!(formula_number(n(10, 3), 2, c), "10 / 3");
        assert_eq!(formula_number(n(52, 3), 2, c), "52 / 3");
    }

    #[test]
    fn currency_places() {
        assert_eq!([places_of(100), places_of(1), places_of(1000), places_of(3)], [2, 0, 3, 2]);
    }
}
