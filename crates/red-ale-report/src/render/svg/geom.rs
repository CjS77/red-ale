//! Exact chart geometry (`design/reports.md` §8, "Charts as SVG"): coordinates in hundredths of a viewBox unit,
//! rounded half-even from exact values, and axis ranges widened to nice tick steps of 1, 2 or 5 × 10ⁿ.

use num_traits::ToPrimitive;

use crate::number::{Rational, Rounding, SizeCap};

/// Hundredths of a viewBox unit: every coordinate is one of these.
pub(super) type Hu = i64;

/// One viewBox unit in [`Hu`].
pub(super) const UNIT: Hu = 100;

/// Written as an integer, or with two decimals: `123`, `123.45`, `-0.05`.
pub(super) fn fmt(v: Hu) -> String {
    let sign = if v < 0 { "-" } else { "" };
    let (whole, frac) = (v.unsigned_abs() / 100, v.unsigned_abs() % 100);
    if frac == 0 { format!("{sign}{whole}") } else { format!("{sign}{whole}.{frac:02}") }
}

/// `n / d` rounded half to even, for `d > 0`.
pub(super) fn div_round(n: i128, d: i128) -> i128 {
    let (q, r) = (n.div_euclid(d), n.rem_euclid(d));
    match (2 * r).cmp(&d) {
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Equal => q + (q & 1),
        std::cmp::Ordering::Less => q,
    }
}

/// `a × num / den`, rounded half to even: a point `num/den` of the way along a length `a`.
pub(super) fn part(a: Hu, num: i64, den: i64) -> Hu {
    if den <= 0 {
        return 0;
    }
    clamp_i64(div_round(i128::from(a) * i128::from(num), i128::from(den)))
}

fn clamp_i64(v: i128) -> i64 {
    i64::try_from(v).unwrap_or(if v < 0 { i64::MIN } else { i64::MAX })
}

/// Text width, estimated as three fifths of the font size per character, so layout needs no font metrics.
pub(super) fn text_width(text: &str, font: Hu) -> Hu {
    let chars = i64::try_from(text.chars().count()).unwrap_or(i64::MAX / 1000);
    chars.saturating_mul(font) * 3 / 5
}

/// `text` cut to fit `width` at `font`, ending in `…` when cut.
pub(super) fn fit(text: &str, width: Hu, font: Hu) -> String {
    if text_width(text, font) <= width {
        return text.to_owned();
    }
    let room = usize::try_from(width * 5 / (3 * font.max(1))).unwrap_or(0).saturating_sub(1);
    let mut cut: String = text.chars().take(room).collect();
    cut.push('…');
    cut
}

/// Division without a size cap: chart arithmetic only maps values already in a result.
fn div(a: &Rational, b: &Rational) -> Option<Rational> {
    a.checked_div(b, SizeCap::bits(u64::MAX)).ok()
}

/// A linear axis: its range, widened to whole tick steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Scale {
    pub lo: Rational,
    pub hi: Rational,
    pub step: Rational,
}

/// About this many tick intervals.
const TARGET_TICKS: i64 = 5;

impl Scale {
    /// The nice scale covering `values`, with zero in it when `zero` is set. A range of one value is widened to
    /// include zero, or to 0..1 when that value is zero.
    pub fn covering<'a>(values: impl Iterator<Item = &'a Rational>, zero: bool) -> Self {
        let (min, max) = values.fold((None::<&Rational>, None::<&Rational>), |(lo, hi), v| {
            (Some(lo.map_or(v, |l| l.min(v))), Some(hi.map_or(v, |h| h.max(v))))
        });
        let (mut min, mut max) = (min.cloned().unwrap_or_default(), max.cloned().unwrap_or_default());
        if zero {
            min = min.min(Rational::zero());
            max = max.max(Rational::zero());
        }
        if min == max {
            if min.is_zero() {
                max = Rational::one();
            } else if min.is_positive() {
                min = Rational::zero();
            } else {
                max = Rational::zero();
            }
        }
        Self::nice(&min, &max)
    }

    /// `min` to `max` widened to multiples of the smallest of 1, 2, 5 × 10ⁿ that gives at most five intervals.
    fn nice(min: &Rational, max: &Rational) -> Self {
        let raw = div(&(max - min), &Rational::from_int(TARGET_TICKS)).unwrap_or_else(Rational::one);
        let decade = pow10(decade_of(&raw));
        let step = [1, 2, 5, 10].iter().map(|m| &decade * &Rational::from_int(*m)).find(|s| *s >= raw).unwrap_or(decade);
        let whole = |v: &Rational, mode| div(v, &step).map(|q| &q.round_to(1, mode).unwrap_or(q) * &step);
        let lo = whole(min, Rounding::Floor).unwrap_or_else(|| min.clone());
        let hi = whole(max, Rounding::Ceiling).unwrap_or_else(|| max.clone());
        Self { lo, hi, step }
    }

    /// The tick values, `lo` to `hi` by `step`.
    pub fn ticks(&self) -> Vec<Rational> {
        std::iter::successors(Some(self.lo.clone()), |t| Some(t + &self.step)).take_while(|t| *t <= self.hi).take(64).collect()
    }

    /// Where `v` falls between `from` (at `lo`) and `to` (at `hi`), rounded half to even and kept within them.
    pub fn map(&self, v: &Rational, from: Hu, to: Hu) -> Hu {
        let along = div(&(&(v - &self.lo) * &Rational::from_int(to - from)), &(&self.hi - &self.lo)).unwrap_or_default();
        let at = (&Rational::from_int(from) + &along).round();
        let at = at.numer().to_i64().unwrap_or(if at.is_negative() { i64::MIN } else { i64::MAX });
        at.clamp(from.min(to), from.max(to))
    }
}

/// The n with 10ⁿ ≤ `v` < 10ⁿ⁺¹, for `v > 0`.
fn decade_of(v: &Rational) -> i64 {
    if !v.is_positive() {
        return 0;
    }
    let digits = |n: &num_bigint::BigInt| i64::try_from(n.magnitude().to_string().len()).unwrap_or(0);
    let mut n = digits(v.numer()) - digits(v.denom());
    while pow10(n) > *v {
        n -= 1;
    }
    while pow10(n + 1) <= *v {
        n += 1;
    }
    n
}

fn pow10(n: i64) -> Rational {
    let p = Rational::from_integer(num_bigint::BigInt::from(10).pow(u32::try_from(n.unsigned_abs()).unwrap_or(u32::MAX)));
    if n >= 0 { p } else { div(&Rational::one(), &p).unwrap_or(p) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> Rational {
        Rational::parse(s).unwrap()
    }

    fn scale(values: &[&str], zero: bool) -> (String, String, String) {
        let values: Vec<Rational> = values.iter().map(|v| r(v)).collect();
        let s = Scale::covering(values.iter(), zero);
        (s.lo.to_string(), s.hi.to_string(), s.step.to_string())
    }

    #[test]
    fn coordinates_print_as_integers_or_hundredths() {
        assert_eq!([fmt(0), fmt(12_300), fmt(12_345), fmt(-5), fmt(-100)], ["0", "123", "123.45", "-0.05", "-1"]);
        assert_eq!([div_round(5, 2), div_round(7, 2), div_round(-5, 2), div_round(-7, 2), div_round(10, 3)], [2, 4, -2, -4, 3]);
        assert_eq!(part(1000, 1, 3), 333);
        assert_eq!(fit("Assets:Current Assets:Checking", 1200, 100), "Assets:Current Asse…");
        assert_eq!(fit("Bank", 1200, 100), "Bank");
    }

    #[test]
    fn scales_widen_to_nice_steps() {
        assert_eq!(scale(&["2500", "-12.5"], true), ("-1000".into(), "3000".into(), "1000".into()));
        assert_eq!(scale(&["0.3", "0.7"], false), ("3/10".into(), "7/10".into(), "1/10".into()));
        assert_eq!(scale(&["7"], false), ("0".into(), "8".into(), "2".into()));
        assert_eq!(scale(&[], true), ("0".into(), "1".into(), "1/5".into()));
        assert_eq!(scale(&["-3"], true), ("-3".into(), "0".into(), "1".into()));
        assert_eq!(scale(&["116006.33"], true), ("0".into(), "150000".into(), "50000".into()));
        let s = Scale::covering([r("0"), r("100")].iter(), true);
        assert_eq!(s.ticks().len(), 6);
        assert_eq!((s.map(&r("0"), 50_000, 10_000), s.map(&r("100"), 50_000, 10_000), s.map(&r("25"), 50_000, 10_000)), (50_000, 10_000, 40_000));
        assert_eq!(s.map(&r("1000"), 50_000, 10_000), 10_000, "kept within the range");
    }
}
