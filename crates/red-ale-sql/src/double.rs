//! Double slots as text: the eight bytes of a `double_val` into the text GnuCash's XML writer gives the same double,
//! C's `%.18g` (`design/notes/xml-format-core.md` §4), which is what [`red_ale_core::SlotValue::Double`] holds.
//!
//! Eighteen significant digits identify every finite double, so the text parses back, correctly rounded, to the same
//! eight bytes (`design/sql-fidelity.md` §3, "Doubles"). This is formatting, not arithmetic: the double's exact
//! decimal expansion is computed with integers from its bits, then rounded half to even as glibc's `printf` rounds.

use std::fmt::Write as _;

/// Significant digits, as `%.18g` asks.
const PRECISION: usize = 18;

/// A non-negative integer in base 10⁹, least significant limb first.
struct Big(Vec<u32>);

impl Big {
    fn from_u64(n: u64) -> Self {
        const BASE: u64 = 1_000_000_000;
        let limbs = std::iter::successors(Some(n), |m| (*m >= BASE).then(|| m / BASE));
        Self(limbs.map(|m| u32::try_from(m % BASE).unwrap_or(0)).collect())
    }

    fn mul_small(&mut self, factor: u32) {
        let carry = self.0.iter_mut().fold(0_u64, |carry, limb| {
            let product = u64::from(*limb) * u64::from(factor) + carry;
            *limb = u32::try_from(product % 1_000_000_000).unwrap_or(0);
            product / 1_000_000_000
        });
        if carry > 0 {
            self.0.push(u32::try_from(carry).unwrap_or(0));
        }
    }

    fn to_decimal(&self) -> String {
        let mut limbs = self.0.iter().rev().skip_while(|l| **l == 0);
        let Some(first) = limbs.next() else { return "0".into() };
        limbs.fold(first.to_string(), |mut s, limb| {
            // Writing to a String cannot fail.
            let _ = write!(s, "{limb:09}");
            s
        })
    }
}

/// The exact decimal digits of `mantissa × 2^exp2` and the power of ten of their first digit.
fn exact_digits(mantissa: u64, exp2: i32) -> (Vec<u8>, i64) {
    let mut big = Big::from_u64(mantissa);
    let (factor, steps) = if exp2 >= 0 {
        (2, exp2.unsigned_abs())
    } else {
        (5, exp2.unsigned_abs())
    };
    (0..steps).for_each(|_| big.mul_small(factor));
    let digits: Vec<u8> = big.to_decimal().bytes().map(|b| b - b'0').collect();
    // mantissa × 5^k / 10^k when the exponent is negative: the point moves k places left.
    let shift = if exp2 >= 0 { 0 } else { i64::from(exp2.unsigned_abs()) };
    let len = i64::try_from(digits.len()).unwrap_or(0);
    (digits, len - 1 - shift)
}

/// Round `digits` to [`PRECISION`] significant digits, half to even, returning the digits and their new exponent.
fn round(mut digits: Vec<u8>, exponent: i64) -> (Vec<u8>, i64) {
    if digits.len() <= PRECISION {
        return (digits, exponent);
    }
    let rest = digits.split_off(PRECISION);
    let tie = rest[0] == 5 && rest[1..].iter().all(|d| *d == 0);
    let up = rest[0] > 5 || (rest[0] == 5 && !tie) || (tie && digits[PRECISION - 1] % 2 == 1);
    if !up {
        return (digits, exponent);
    }
    if let Some(i) = digits.iter().rposition(|d| *d != 9) {
        digits[i] += 1;
        digits[i + 1..].iter_mut().for_each(|d| *d = 0);
        (digits, exponent)
    } else {
        let mut carried = vec![1];
        carried.resize(PRECISION, 0);
        (carried, exponent + 1)
    }
}

fn text_of(digits: &[u8]) -> String {
    digits.iter().map(|d| char::from(b'0' + d)).collect()
}

/// `%.18g` of the double with these bits: trailing zeros dropped, exponent form outside `1e-4 ≤ |x| < 1e18`.
pub fn format_double(bits: u64) -> String {
    let negative = bits >> 63 == 1;
    let biased = i32::try_from((bits >> 52) & 0x7ff).unwrap_or(0);
    let fraction = bits & ((1 << 52) - 1);
    let sign = if negative { "-" } else { "" };
    let body = match (biased, fraction) {
        (0x7ff, 0) => "inf".to_owned(),
        (0x7ff, _) => "nan".to_owned(),
        (0, 0) => "0".to_owned(),
        (0, f) => finite(f, -1074),
        (b, f) => finite(f | 1 << 52, b - 1075),
    };
    format!("{sign}{body}")
}

fn finite(mantissa: u64, exp2: i32) -> String {
    let (digits, exponent) = exact_digits(mantissa, exp2);
    let (mut digits, exponent) = round(digits, exponent);
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    let precision = i64::try_from(PRECISION).unwrap_or(18);
    if exponent < -4 || exponent >= precision {
        let mantissa = match digits.split_first() {
            Some((first, [])) => text_of(&[*first]),
            Some((first, rest)) => format!("{}.{}", text_of(&[*first]), text_of(rest)),
            None => "0".into(),
        };
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exponent.unsigned_abs())
    } else if exponent < 0 {
        let zeros = "0".repeat(usize::try_from(-exponent - 1).unwrap_or(0));
        format!("0.{zeros}{}", text_of(&digits))
    } else {
        let point = usize::try_from(exponent + 1).unwrap_or(0);
        if digits.len() <= point {
            let mut whole = text_of(&digits);
            whole.push_str(&"0".repeat(point - digits.len()));
            whole
        } else {
            format!("{}.{}", text_of(&digits[..point]), text_of(&digits[point..]))
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// `'%.18g' % x` from Python (C's `printf`), for values chosen for each branch: zero, subnormals, the exponent
    /// thresholds, ties at the eighteenth digit, carries and the extremes.
    #[allow(clippy::unreadable_literal)] // Generated; the bits are printed as Python prints them.
    const PRINTF: &[(u64, &str)] = include!("double_vectors.in");

    #[test]
    fn matches_printf() {
        PRINTF
            .iter()
            .for_each(|(bits, text)| assert_eq!(format_double(*bits), *text, "{bits:#018x}"));
    }

    proptest! {
        #[test]
        fn every_finite_double_survives_its_text(bits in any::<u64>()) {
            let text = format_double(bits);
            if let Ok(back) = text.parse::<f64>() {
                if !back.is_nan() {
                    prop_assert_eq!(back.to_bits(), bits, "{}", text);
                }
            } else {
                prop_assert!(text.ends_with("nan"), "{}", text);
            }
        }

        #[test]
        fn digits_agree_with_rust_s_exact_formatting(x in any::<f64>().prop_filter("finite", |x| x.is_finite())) {
            // Rust's `{:e}` with a precision rounds the exact value too; only exact ties could differ, and the
            // vectors above cover those against printf.
            let ours = format_double(x.to_bits());
            let back: f64 = ours.parse().unwrap_or(f64::NAN);
            prop_assert_eq!(back.to_bits(), x.to_bits());
            let rust: f64 = format!("{x:.17e}").parse().unwrap_or(f64::NAN);
            prop_assert_eq!(rust.to_bits(), back.to_bits());
        }
    }
}
