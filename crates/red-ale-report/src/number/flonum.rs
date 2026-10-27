//! How Guile prints an exact number made inexact: `(number->string (exact->inexact x))`, which GnuCash's CSV export
//! writes for every number (`gnc:lists->csv`, `5.17:gnucash/report/trep-engine.scm:463-512`). The figures are
//! red-ale's exact Rationals; only their text follows Guile, computed here with integers alone.
//!
//! `exact->inexact` rounds to the nearest double, ties to even. Guile 3.0 then prints the shortest digits that read
//! back as that double, the nearest to it when several are as short, as `23500.0`, `3.5714285714285716`, `0.001`,
//! `1.0e-4`, `1.0e7`: positional unless the number is below `1.0e-3`, or has more than seven digits before the point
//! of which more than three are trailing zeros. That rule matched Guile 3.0.9 on 40,000 random rationals; the cases
//! below are among them.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use super::Rational;

/// Bits of a double's significand, the hidden one included.
const MANTISSA_BITS: u64 = 53;
/// The same, signed, for exponent arithmetic.
const MANTISSA_BITS_I: i64 = 53;
/// The exponent of the smallest subnormal double's unit, 2^-1074.
const MIN_EXPONENT: i64 = -1074;
/// The largest exponent of a double's unit: the largest finite double is (2^53 - 1) × 2^971.
const MAX_EXPONENT: i64 = 971;
/// The most significant digits a double ever needs.
const MAX_DIGITS: u32 = 17;

/// `x` as Guile prints `(exact->inexact x)`: `"23500.0"`, `"-12.34"`, `"3.5714285714285716"`, `"1.0e-4"`.
pub fn guile_inexact(x: &Rational) -> String {
    let r = x.as_big();
    if r.is_zero() {
        return "0.0".to_owned();
    }
    let sign = if r.is_negative() { "-" } else { "" };
    match nearest_double(&r.abs()) {
        None => format!("{}inf.0", if r.is_negative() { "-" } else { "+" }),
        Some((m, e)) => {
            let (digits, exponent) = shortest(&m, e);
            format!("{sign}{}", layout(&digits, exponent))
        }
    }
}

/// The double nearest `value` (positive), ties to even, as `(m, e)` with value `m × 2^e`; `None` past the largest
/// double.
fn nearest_double(value: &BigRational) -> Option<(BigInt, i64)> {
    let bits = |i: &BigInt| i64::try_from(i.bits()).unwrap_or(i64::MAX);
    // 2^(lead - 1) <= value < 2^(lead + 1), so the significand at this exponent has 53 or 54 bits, or one more
    // after rounding up; each step up halves it.
    let lead = bits(value.numer()) - bits(value.denom());
    let first = (lead - MANTISSA_BITS_I).max(MIN_EXPONENT);
    let (significand, exponent) = (first..first + 3)
        .map(|exponent| (scaled_round(value, exponent), exponent))
        .find(|(significand, _)| significand.bits() <= MANTISSA_BITS)?;
    (exponent <= MAX_EXPONENT).then_some((significand, exponent))
}

/// `value / 2^exponent` rounded to an integer, ties to even.
fn scaled_round(value: &BigRational, exponent: i64) -> BigInt {
    let shift = |x: &BigInt, by: i64| x << usize::try_from(by).unwrap_or(0);
    let (numer, denom) = (value.numer(), value.denom());
    let (num, den) = if exponent >= 0 {
        (numer.clone(), shift(denom, exponent))
    } else {
        (shift(numer, -exponent), denom.clone())
    };
    round_half_even(&num, &den)
}

fn round_half_even(num: &BigInt, den: &BigInt) -> BigInt {
    let (q, r) = num.div_rem(den);
    let twice: BigInt = r * 2;
    if twice > *den || (twice == *den && q.is_odd()) { q + 1 } else { q }
}

/// `m × 2^e` as a rational.
fn exact(m: &BigInt, e: i64) -> BigRational {
    let two = BigInt::from(2);
    let power = two.pow(u32::try_from(e.unsigned_abs()).unwrap_or(u32::MAX));
    if e >= 0 {
        BigRational::from_integer(m * power)
    } else {
        BigRational::new(m.clone(), power)
    }
}

/// The shortest digits that read back as the double `m × 2^e`, the nearest to it when several are as short, and the
/// decimal exponent of the first digit.
fn shortest(m: &BigInt, e: i64) -> (String, i64) {
    let v = exact(m, e);
    let half_unit = exact(&BigInt::one(), e - 1);
    // Below a power of two the doubles are twice as dense, unless the double is the smallest normal one.
    let half_below = if m.bits() == MANTISSA_BITS && *m == BigInt::one() << (MANTISSA_BITS - 1) && e > MIN_EXPONENT {
        exact(&BigInt::one(), e - 2)
    } else {
        half_unit.clone()
    };
    let (low, high) = (&v - half_below, &v + half_unit);
    let inclusive = m.is_even();
    let inside = |c: &BigRational| {
        if inclusive {
            *c >= low && *c <= high
        } else {
            *c > low && *c < high
        }
    };
    let lead = decimal_exponent(&v);
    (1..=MAX_DIGITS)
        .find_map(|k| {
            let scale = power_of_ten(lead - i64::from(k) + 1);
            let t = &v / &scale;
            let candidates = [t.floor().to_integer(), t.ceil().to_integer()];
            let best = candidates
                .into_iter()
                .filter(|c| inside(&(BigRational::from_integer(c.clone()) * &scale)))
                .min_by(|a, b| {
                    let da = (BigRational::from_integer(a.clone()) * &scale - &v).abs();
                    let db = (BigRational::from_integer(b.clone()) * &scale - &v).abs();
                    da.cmp(&db).then_with(|| a.is_odd().cmp(&b.is_odd()))
                })?;
            Some(digits_of(&best, lead - i64::from(k) + 1))
        })
        .unwrap_or_else(|| (m.to_string(), 0))
}

/// The digits of `c × 10^p` without trailing zeros, and the decimal exponent of the first.
fn digits_of(c: &BigInt, p: i64) -> (String, i64) {
    let text = c.to_string();
    let exponent = p + i64::try_from(text.len()).unwrap_or(0) - 1;
    let trimmed = text.trim_end_matches('0');
    (if trimmed.is_empty() { "0".to_owned() } else { trimmed.to_owned() }, exponent)
}

/// `n` with `10^n <= v < 10^(n + 1)`, for `v` positive.
fn decimal_exponent(v: &BigRational) -> i64 {
    let digits = |i: &BigInt| i64::try_from(i.to_string().len()).unwrap_or(0);
    let guess = digits(v.numer()) - digits(v.denom());
    [guess - 1, guess, guess + 1]
        .into_iter()
        .rev()
        .find(|n| power_of_ten(*n) <= *v)
        .unwrap_or(guess - 1)
}

fn power_of_ten(n: i64) -> BigRational {
    let p = BigInt::from(10).pow(u32::try_from(n.unsigned_abs()).unwrap_or(u32::MAX));
    if n >= 0 {
        BigRational::from_integer(p)
    } else {
        BigRational::new(BigInt::one(), p)
    }
}

/// Guile's layout of `digits` with the first at decimal exponent `exponent`.
fn layout(digits: &str, exponent: i64) -> String {
    let count = i64::try_from(digits.len()).unwrap_or(0);
    let before_point = exponent + 1;
    if exponent < -3 || (before_point > 7 && before_point - count > 3) {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        return format!("{first}.{rest}e{exponent}");
    }
    if exponent < 0 {
        let zeros = "0".repeat(usize::try_from(-exponent - 1).unwrap_or(0));
        return format!("0.{zeros}{digits}");
    }
    let whole = usize::try_from(before_point).unwrap_or(0);
    match digits.get(whole..) {
        Some(frac) if !frac.is_empty() => format!("{}.{frac}", &digits[..whole]),
        _ => format!("{digits:0<whole$}.0"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(text: &str) -> String {
        guile_inexact(&Rational::parse(text).unwrap())
    }

    /// Each printed by Guile 3.0.9's `(number->string (exact->inexact x))`.
    #[test]
    fn prints_as_guile() {
        let cases = [
            ("23500", "23500.0"),
            ("25/7", "3.5714285714285716"),
            ("1/10", "0.1"),
            ("-617/50", "-12.34"),
            ("0", "0.0"),
            ("1/3", "0.3333333333333333"),
            ("2/3", "0.6666666666666666"),
            ("-1/3", "-0.3333333333333333"),
            ("1234567/100", "12345.67"),
            ("1000000", "1000000.0"),
            ("10000000", "1.0e7"),
            ("1200000", "1200000.0"),
            ("12300000", "1.23e7"),
            ("12345000", "12345000.0"),
            ("123450000", "1.2345e8"),
            ("1234567", "1234567.0"),
            ("123456780000", "1.2345678e11"),
            ("12345678000", "12345678000.0"),
            ("123456789012345678", "123456789012345680.0"),
            ("12345678901234567890", "12345678901234567000.0"),
            ("123456789012345678901", "1.2345678901234568e20"),
            ("100000000000000000000", "1.0e20"),
            ("0.001", "0.001"),
            ("0.0015", "0.0015"),
            ("0.0001", "1.0e-4"),
            ("0.00012345", "1.2345e-4"),
            ("1/1024", "9.765625e-4"),
            ("99999999999999999", "1.0e17"),
            ("999999999999999999999", "1.0e21"),
            ("1180591620717411303424", "1.1805916207174113e21"),
        ];
        cases.iter().for_each(|(x, want)| assert_eq!(g(x), *want, "{x}"));
    }

    #[test]
    fn extremes() {
        let tiny = Rational::new(1, BigInt::from(2).pow(1074)).unwrap();
        assert_eq!(guile_inexact(&tiny), "5.0e-324");
        let huge = Rational::from_integer(BigInt::from(10).pow(400));
        assert_eq!(guile_inexact(&huge), "+inf.0");
        let max = Rational::from_integer((BigInt::from(2).pow(53) - 1) << 971_usize);
        assert_eq!(guile_inexact(&max), "1.7976931348623157e308");
    }
}
