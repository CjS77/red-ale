//! Growth rates: `ratio^(1/years) − 1`, Investment Lots' compound annual growth rate (`design/reports.md` §3,
//! "Growth rates").
//!
//! GnuCash's `calculate-cagr` raises a ratio of doubles to the power `1/years` with `expt`
//! (`5.17:gnucash/report/reports/standard/investment-lots.scm:447-461`). A fractional power of a rational is
//! irrational unless the ratio is a perfect power, so red-ale cannot keep it exact. It computes the power in integer
//! fixed point instead and returns it correctly rounded to 10⁻¹², ties to even: the same figure on every machine,
//! and the exact figure whenever that has twelve decimals or fewer.

use std::iter::successors;

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::{NumberError, Rational, SizeCap};

/// The result's unit, 10⁻¹², as its reciprocal.
const PLACES: i64 = 1_000_000_000_000;
/// Bits of `x` the first attempt is good to: `x` is known within 2^-128.
const FIRST_BITS: u64 = 128;
/// Bits of `x` the second attempt is good to, for a value the first left within 2^-128 of a rounding boundary.
const SECOND_BITS: u64 = 1024;
/// The most fraction bits one computation may work with. Each ln and exp costs about this many bits squared times a
/// third of this many terms, so a cap keeps an absurd ratio or holding period from stalling a report.
pub const MAX_WORKING_BITS: u64 = 16_384;

/// GnuCash's `calculate-cagr` (`5.17:gnucash/report/reports/standard/investment-lots.scm:447-461`): the growth rate
/// from `basis` to `end_value` over `years`, or `None` (GnuCash's `#f`) when `basis` is missing, zero or negative,
/// `end_value` is negative, or `years` is missing or zero. Those guards need the basis and end value apart, since
/// a negative basis and a negative end value give a positive ratio, so they live here rather than in each script.
pub fn cagr(basis: Option<&Rational>, end_value: &Rational, years: Option<&Rational>, cap: SizeCap) -> Result<Option<Rational>, NumberError> {
    match (basis, years) {
        (Some(basis), Some(years)) if basis.is_positive() && !end_value.is_negative() => {
            growth_rate(&end_value.checked_div(basis, cap)?, years, cap)
        }
        _ => Ok(None),
    }
}

/// `ratio^(1/years) − 1`, rounded to 10⁻¹² with ties to even, or `None` where GnuCash has no number: a negative
/// ratio, zero years, or a zero ratio over negative years (GnuCash's `+inf.0`). A negative `years` is computed, as
/// in GnuCash.
///
/// The result is correctly rounded. The power `x` is computed as `exp(ln(ratio) / years)` in fixed point with enough
/// fraction bits that `|x̃ − x| < 2^-128` (see [`Power::approximate`]). That decides the rounding unless `x − 1` lies
/// within `10¹² × 2^-128` units of a half-unit boundary; then, if `x − 1` is exactly that boundary (checked with
/// exact integers, so a perfect power at a tie is exact), it rounds to even; if not, `x` is computed again to within
/// 2^-1024. Only a value that is still undecided, within 2^-1024 of a boundary without being on it, could round
/// the wrong way, and it then rounds to even as a tie would.
///
/// A result past `cap` is [`NumberError::TooLarge`] (the `number-size` limit); inputs that would need more than
/// [`MAX_WORKING_BITS`] of precision are [`NumberError::Precision`].
pub fn growth_rate(ratio: &Rational, years: &Rational, cap: SizeCap) -> Result<Option<Rational>, NumberError> {
    if ratio.is_negative() || years.is_zero() || (ratio.is_zero() && years.is_negative()) {
        return Ok(None);
    }
    if ratio.is_zero() {
        return Ok(Some(Rational::from_int(-1)));
    }
    // ratio^(1/years) = (1/ratio)^(1/−years).
    let (n, d) = if years.is_negative() { (ratio.denom(), ratio.numer()) } else { (ratio.numer(), ratio.denom()) };
    let power = Power { n, d, a: years.numer().abs(), b: years.denom() };
    let units = power.rounded(cap)?;
    cap.check(Rational::new(units, PLACES)?).map(Some)
}

/// `x = (n/d)^(b/a)`, all four positive.
struct Power<'a> {
    n: &'a BigInt,
    d: &'a BigInt,
    a: BigInt,
    b: &'a BigInt,
}

/// What one approximation tells of `(x − 1) × 10¹²`: its rounding, or the half-integer `m − 1/2` it may lie on.
enum Decision {
    Rounded(BigInt),
    Near(BigInt),
}

impl Power<'_> {
    /// `(x − 1) × 10¹²` rounded to an integer, ties to even.
    fn rounded(&self, cap: SizeCap) -> Result<BigInt, NumberError> {
        // |b/a| < 2^s, so dividing ln(ratio) by `years` scales its error by less than 2^s.
        let s = (self.b.bits() + 1).saturating_sub(self.a.bits());
        // ln(ratio) sums about bits(n) + bits(d) multiples of ln 2, each adding its error.
        let g = bit_length(self.n.bits() + self.d.bits()) + 40;
        let c = self.integer_bits(s + g)?;
        if c > cap.max_bits() {
            return Err(NumberError::TooLarge { bits: c, max: cap.max_bits() });
        }
        let fixed = c + s + g;
        let second = SECOND_BITS.min(MAX_WORKING_BITS.saturating_sub(fixed));
        match self.decide(FIRST_BITS, fixed)? {
            Decision::Rounded(units) => Ok(units),
            Decision::Near(m) if second <= FIRST_BITS || self.is_exactly(&m) => Ok(to_even(m)),
            Decision::Near(_) => match self.decide(second, fixed)? {
                Decision::Rounded(units) => Ok(units),
                Decision::Near(m) => Ok(to_even(m)),
            },
        }
    }

    /// A `c ≥ 0` with `2^(c−1) > x`, from `y = ln x` known within 2^-40: `log2 x = y / ln 2 < 1.5 y`, plus margin.
    fn integer_bits(&self, extra: u64) -> Result<u64, NumberError> {
        let w = working_bits(64 + extra)?;
        let bits = (self.log(w, &ln2(w)) * 3_u32) >> (w + 1);
        Ok(match bits.to_i64() {
            Some(bits) => u64::try_from(bits.saturating_add(3)).unwrap_or(0),
            None if bits.is_negative() => 0,
            None => u64::MAX,
        })
    }

    /// Round `(x − 1) × 10¹²` from `x` known within 2^-p, or name the boundary it may lie on.
    fn decide(&self, p: u64, fixed: u64) -> Result<Decision, NumberError> {
        let w = working_bits(p + fixed)?;
        let one = BigInt::one() << w;
        let units = (self.approximate(w) - &one) * PLACES;
        let err = BigInt::from(PLACES) << (w - p);
        // floor(v + 1/2) for v = num / 2^w, and whether v + 1/2 is a whole number.
        let half_up = |num: &BigInt| {
            let (whole, rest) = (num * 2_u32 + &one).div_mod_floor(&(&one << 1));
            (whole, rest.is_zero())
        };
        let ((low, low_on_tie), (high, _)) = (half_up(&(&units - &err)), half_up(&(&units + err)));
        Ok(if low == high && !low_on_tie { Decision::Rounded(low) } else { Decision::Near(high) })
    }

    /// `x × 2^w`, within `2^(w−p−6)` when `w = p + c + s + g` (see [`Power::rounded`]). Every truncation below is one
    /// ulp (2^-w). ln of the mantissa and ln 2 are each within `4w + 30` ulp (`atanh`); ln(ratio) sums `bits(n) +
    /// bits(d)` multiples of ln 2 and two of mantissas, so within `2^(g−22)` ulp; dividing by `years` gives `y`
    /// within `2^(s+g−21)`. Reducing `y = k ln 2 + r` adds `|k| × (4w + 30) ≤ 2^32` ulp when `k ≥ 0` (scaled away by
    /// `2^k` when `k < 0`), `e^r < 1.5` keeps the error of `r` and adds `2w`, and `2^k < 2^c` scales it all: in total
    /// under `2^(c+s+g−6)` ulp, so `|x̃ − x| < 2^-(p+6)`.
    fn approximate(&self, w: u64) -> BigInt {
        let ln2 = ln2(w);
        exp(&self.log(w, &ln2), w, &ln2)
    }

    /// `ln(x) × 2^w = ln(n/d) × b/a × 2^w`, truncated.
    fn log(&self, w: u64, ln2: &BigInt) -> BigInt {
        (ln_int(self.n, w, ln2) - ln_int(self.d, w, ln2)) * self.b / &self.a
    }

    /// True when `x − 1` is exactly `(m − 1/2) × 10⁻¹²`: `x^a = (n/d)^b` for `x = (2U + 2m − 1) / 2U`, `U = 10¹²`.
    /// Skipped, as false, when the powers would pass [`MAX_WORKING_BITS`] four times over.
    fn is_exactly(&self, m: &BigInt) -> bool {
        let x = Rational::new(BigInt::from(2 * PLACES) + m * 2_u32 - 1_u32, 2 * PLACES);
        let (Ok(x), Some(a), Some(b)) = (x, self.a.to_u32(), self.b.to_u32()) else { return false };
        let size = |r: &BigInt, s: &BigInt, e: u32| r.bits().max(s.bits()).saturating_mul(u64::from(e));
        if !x.is_positive() || size(x.numer(), x.denom(), a).saturating_add(size(self.n, self.d, b)) > 4 * MAX_WORKING_BITS {
            return false;
        }
        x.numer().pow(a) * self.d.pow(b) == self.n.pow(b) * x.denom().pow(a)
    }
}

/// `bits` as a shift, or [`NumberError::Precision`] past [`MAX_WORKING_BITS`].
fn working_bits(bits: u64) -> Result<u64, NumberError> {
    if bits > MAX_WORKING_BITS {
        Err(NumberError::Precision { bits, max: MAX_WORKING_BITS })
    } else {
        Ok(bits)
    }
}

/// Bits needed to write `v`.
fn bit_length(v: u64) -> u64 {
    u64::from(u64::BITS - v.leading_zeros())
}

/// The nearer even of `m − 1` and `m`, either side of the tie `m − 1/2`.
fn to_even(m: BigInt) -> BigInt {
    if m.is_even() { m } else { m - 1_u32 }
}

/// `ln 2 × 2^w = 2 atanh(1/3) × 2^w`, within `4w + 30` ulp.
fn ln2(w: u64) -> BigInt {
    atanh(&((BigInt::one() << w) / 3), w) << 1
}

/// `ln(n) × 2^w` for `n > 0`: `n = 2^k m` with `1 ≤ m < 2`, `ln n = k ln 2 + 2 atanh((m − 1)/(m + 1))`.
fn ln_int(n: &BigInt, w: u64, ln2: &BigInt) -> BigInt {
    let twos = n.bits().saturating_sub(1);
    let mantissa = if twos <= w { n << (w - twos) } else { n >> (twos - w) };
    let one = BigInt::one() << w;
    let t = ((&mantissa - &one) << w) / (&mantissa + &one);
    ln2 * twos + (atanh(&t, w) << 1)
}

/// `atanh(t) × 2^w = Σ t^(2j+1) / (2j+1)` for `0 ≤ t < 1/3`, `t` within 2 ulp. Each power is within 3 ulp, as
/// `t² < 1/9` shrinks the error it carries; each term within 4; at most `w/3 + 2` terms are not zero, and the tail
/// they leave is under 4 ulp: within `4w/3 + 12` ulp in all.
fn atanh(t: &BigInt, w: u64) -> BigInt {
    let t2 = (t * t) >> w;
    successors(Some(t.clone()), |p| Some((p * &t2) >> w).filter(|next| !next.is_zero()))
        .zip((1_u32..).step_by(2))
        .map(|(p, k)| p / k)
        .sum()
}

/// `e^y × 2^w` for `y = Y / 2^w`: `y = k ln 2 + r` with `|r| ≤ ln 2 / 2`, `e^y = 2^k e^r`. A `k` below `−w` leaves
/// nothing above 2^-w; a `k` past `c` cannot happen, as [`Power::integer_bits`] checked the size of `x`.
fn exp(y: &BigInt, w: u64, ln2: &BigInt) -> BigInt {
    let k = (y * 2_u32 + ln2).div_floor(&(ln2 * 2_u32));
    let small = exp_small(&(y - &k * ln2), w);
    match k.to_i64() {
        Some(k) if k >= 0 => small << k.unsigned_abs(),
        Some(k) if k.unsigned_abs() <= w + 2 => small >> k.unsigned_abs(),
        _ => BigInt::zero(),
    }
}

/// `e^r × 2^w = Σ r^i / i!` for `|r| < 1/2`. Each term is within 2 ulp, since `|r| < 1/2` halves the error it
/// carries, and the terms reach zero within `w` steps.
fn exp_small(r: &BigInt, w: u64) -> BigInt {
    successors(Some((BigInt::one() << w, 1_u32)), |(term, i)| {
        Some(((term * r) >> w) / *i).filter(|next| !next.is_zero()).map(|next| (next, i + 1))
    })
    .map(|(term, _)| term)
    .sum()
}

#[cfg(test)]
mod tests {
    use num_rational::BigRational;
    use proptest::prelude::*;

    use super::*;
    use crate::number::Rounding;

    fn r(s: &str) -> Rational {
        Rational::parse(s).unwrap()
    }

    fn gr(ratio: &str, years: &str) -> Option<Rational> {
        growth_rate(&r(ratio), &r(years), SizeCap::DEFAULT).unwrap()
    }

    fn pow(x: &Rational, n: u32) -> Rational {
        Rational::new(x.numer().pow(n), x.denom().pow(n)).unwrap()
    }

    fn round12(x: &Rational) -> Rational {
        x.round_to(PLACES, Rounding::Bankers).unwrap()
    }

    fn cagr_of(basis: Option<i64>, end: i64, years: Option<i64>) -> Option<Rational> {
        let (basis, years) = (basis.map(Rational::from_int), years.map(Rational::from_int));
        cagr(basis.as_ref(), &Rational::from_int(end), years.as_ref(), SizeCap::DEFAULT).unwrap()
    }

    /// `5.17:gnucash/report/reports/standard/test/test-investment-lots.scm:514-556`: GnuCash asserts ranges; these
    /// are red-ale's exact figures, each inside GnuCash's range.
    #[test]
    fn calculate_cagr_cases_from_gnucash() {
        let within = |got: Option<Rational>, want: &str, low: &str, high: &str| {
            let got = got.unwrap();
            assert_eq!(got, r(want));
            assert!(got >= r(low) && got <= r(high), "{got} outside [{low}, {high}]");
        };
        within(cagr_of(Some(100), 200, Some(1)), "1", "0.99", "1.01");
        within(cagr_of(Some(100), 100, Some(1)), "0", "-0.01", "0.01");
        assert_eq!(cagr_of(Some(0), 100, Some(1)), None);
        assert_eq!(cagr_of(None, 100, Some(1)), None);
        assert_eq!(cagr_of(Some(-100), 100, Some(1)), None);
        assert_eq!(cagr_of(Some(100), -50, Some(1)), None);
        within(cagr_of(Some(1000), 1100, Some(1)), "0.1", "0.09", "0.11");
        // sqrt(1.1) - 1 = 0.048808848170|15...
        within(cagr_of(Some(100), 110, Some(2)), "0.048808848170", "0.04", "0.06");
    }

    #[test]
    fn the_other_guards_of_calculate_cagr() {
        assert_eq!(cagr_of(Some(100), 110, None), None);
        assert_eq!(cagr_of(Some(100), 110, Some(0)), None);
        assert_eq!(cagr_of(Some(-100), -50, Some(1)), None);
        assert_eq!(cagr_of(Some(100), 0, Some(2)), Some(r("-1")));
    }

    #[test]
    fn exact_where_the_power_is() {
        assert_eq!(gr("4", "2"), Some(r("1")));
        assert_eq!(gr("1", "7/3"), Some(r("0")));
        assert_eq!(gr("27/8", "3"), Some(r("0.5")));
        assert_eq!(gr("0.75", "1/2"), Some(r("-0.4375")));
        assert_eq!(gr("1.5", "1/4"), Some(r("4.0625")));
        assert_eq!(gr("32", "5/2"), Some(r("3")));
        assert_eq!(gr("4", "-2"), Some(r("-0.5")));
        assert_eq!(gr("0", "3"), Some(r("-1")));
    }

    #[test]
    fn no_number_where_gnucash_has_none() {
        assert_eq!(gr("-2", "1"), None);
        assert_eq!(gr("2", "0"), None);
        assert_eq!(gr("0", "-1"), None);
    }

    /// Checked against Python's `decimal` at 80 digits.
    #[test]
    fn irrational_powers_round_correctly() {
        assert_eq!(gr("2", "2"), Some(r("0.414213562373")));
        assert_eq!(gr("2", "3"), Some(r("0.259921049895")));
        assert_eq!(gr("1.1", "400/366"), Some(r("0.091124498571")));
        assert_eq!(gr("3", "-5/7"), Some(r("-0.785201995008")));
    }

    #[test]
    fn exact_ties_round_to_even() {
        let tie = |x: &str, n: u32, want: &str| {
            let x = r(x);
            let ratio = pow(&x, n);
            assert_eq!(growth_rate(&ratio, &Rational::from_int(n.into()), SizeCap::DEFAULT).unwrap(), Some(r(want)));
        };
        tie("1.0000000000005", 3, "0");
        tie("1.0000000000015", 7, "0.000000000002");
        tie("0.9999999999975", 2, "-0.000000000002");
    }

    #[test]
    fn tiny_powers_are_minus_one() {
        assert_eq!(gr("1/2", "1/10000"), Some(r("-1")));
        let years = Rational::new(1, BigInt::one() << 4000).unwrap();
        assert_eq!(growth_rate(&r("1/2"), &years, SizeCap::DEFAULT).unwrap(), Some(r("-1")));
    }

    #[test]
    fn absurd_inputs_are_refused() {
        let huge = Rational::new(1, BigInt::one() << 4000).unwrap();
        assert!(matches!(growth_rate(&r("2"), &huge, SizeCap::DEFAULT), Err(NumberError::TooLarge { .. })));
        let wide = SizeCap::bits(1 << 20);
        let years = Rational::new(1, BigInt::one() << 20_000).unwrap();
        let ratio = Rational::new((BigInt::one() << 20_000) + 1, BigInt::one() << 20_000).unwrap();
        assert!(matches!(growth_rate(&ratio, &years, wide), Err(NumberError::Precision { .. })));
    }

    fn ratio() -> impl Strategy<Value = Rational> {
        (1_i64..2_000_000, 1_i64..2_000_000).prop_map(|(n, d)| Rational::new(n, d).unwrap())
    }

    proptest! {
        #[test]
        fn whole_powers_come_back_exactly(x in ratio(), n in 1_i32..=10) {
            let power = pow(&x, n.unsigned_abs());
            let got = growth_rate(&power, &Rational::from_int(n.into()), SizeCap::DEFAULT).unwrap();
            prop_assert_eq!(got, Some(round12(&(&x - &Rational::one()))));
        }

        #[test]
        fn rises_with_the_ratio(a in ratio(), b in ratio(), p in 1_i64..2000, q in 1_i64..400) {
            let years = Rational::new(p, q).unwrap();
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            let rate = |x: &Rational| growth_rate(x, &years, SizeCap::DEFAULT).unwrap().unwrap();
            prop_assert!(rate(&low) <= rate(&high));
        }

        /// `g` is correctly rounded when `(g + 1 ∓ ½·10⁻¹²)^a` brackets `ratio^b`, for `years = a/b`.
        #[test]
        fn rounds_correctly(x in ratio(), a in 1_i32..=6, b in 1_i32..=6) {
            let years = Rational::new(a, b).unwrap();
            let g = growth_rate(&x, &years, SizeCap::DEFAULT).unwrap().unwrap();
            let (a, b) = (years.numer().to_i32().unwrap(), years.denom().to_i32().unwrap());
            let half = BigRational::new(1.into(), (2 * PLACES).into());
            let target = x.as_big().pow(b);
            let base = g.as_big() + BigRational::one();
            let below = &base - &half;
            prop_assert!(below.is_negative() || below.pow(a) <= target);
            prop_assert!((&base + &half).pow(a) >= target);
        }
    }
}
