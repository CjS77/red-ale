//! [`Rational`]: the exact, unbounded number reports compute with.
//!
//! GnuCash hands amounts to its Scheme reports as exact rationals and adds and multiplies them with Scheme's unbounded
//! arithmetic (`5.17:bindings/guile/gnc-engine-guile.cpp:1729-1734`, `5.17:gnucash/report/report-utilities.scm:289-296`).
//! A 64-bit [`Numeric`] would overflow where GnuCash prints a figure, so reports use this type instead. Its size is
//! bounded only by a [`SizeCap`], so a runaway computation stops with an error rather than exhausting memory.

use std::borrow::Cow;
use std::fmt;
use std::ops::{Add, Mul, Neg, Sub};
use std::str::FromStr;

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use red_ale_core::Numeric;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};

use super::NumberError;

/// The default [`SizeCap`]: 4,096 bits per part (`design/reports.md` §2, "Limits and their errors").
pub const DEFAULT_MAX_BITS: u64 = 4096;

/// The largest numerator or denominator a report may compute, in bits. Passing it is the `number-size` limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SizeCap(u64);

impl SizeCap {
    /// The default cap of [`DEFAULT_MAX_BITS`].
    pub const DEFAULT: Self = Self(DEFAULT_MAX_BITS);

    /// A cap of `max_bits` per part.
    pub const fn bits(max_bits: u64) -> Self {
        Self(max_bits)
    }

    /// The cap in bits.
    pub const fn max_bits(self) -> u64 {
        self.0
    }

    /// `r` itself when both its parts fit, else [`NumberError::TooLarge`].
    pub fn check(self, r: Rational) -> Result<Rational, NumberError> {
        let bits = r.bits();
        if bits > self.0 { Err(NumberError::TooLarge { bits, max: self.0 }) } else { Ok(r) }
    }
}

impl Default for SizeCap {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// How to round to a denominator: GnuCash's rounding modes (`GNC_HOW_RND_*`, `5.17:libgnucash/engine/gnc-numeric.h:140-175`).
///
/// The text forms are those scripts pass to `round_to`: `round`, `half-up`, `half-down`, `floor`, `ceiling`,
/// `truncate` and `promote`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rounding {
    /// Toward negative infinity (`GNC_HOW_RND_FLOOR`).
    Floor,
    /// Toward positive infinity (`GNC_HOW_RND_CEIL`).
    Ceiling,
    /// Toward zero (`GNC_HOW_RND_TRUNC`).
    Truncate,
    /// Away from zero (`GNC_HOW_RND_PROMOTE`).
    Promote,
    /// To nearest, ties toward zero (`GNC_HOW_RND_ROUND_HALF_DOWN`).
    HalfDown,
    /// To nearest, ties away from zero (`GNC_HOW_RND_ROUND_HALF_UP`).
    HalfUp,
    /// To nearest, ties to even: banker's rounding (`GNC_HOW_RND_ROUND`), what GnuCash displays amounts with.
    Bankers,
}

const ROUNDING_NAMES: [(Rounding, &str); 7] = [
    (Rounding::Bankers, "round"),
    (Rounding::HalfUp, "half-up"),
    (Rounding::HalfDown, "half-down"),
    (Rounding::Floor, "floor"),
    (Rounding::Ceiling, "ceiling"),
    (Rounding::Truncate, "truncate"),
    (Rounding::Promote, "promote"),
];

impl Rounding {
    /// The name scripts use.
    pub fn as_str(self) -> &'static str {
        ROUNDING_NAMES.iter().find(|(r, _)| *r == self).map_or("round", |(_, n)| n)
    }

    /// Round `num / den` (with `den > 0`) to an integer.
    fn apply(self, num: &BigInt, den: &BigInt) -> BigInt {
        let (q, r) = num.div_rem(den);
        if r.is_zero() {
            return q;
        }
        let away = || if r.is_negative() { &q - 1 } else { &q + 1 };
        let twice = r.abs() * 2;
        let up = match self {
            Self::Truncate => false,
            Self::Promote => true,
            Self::Floor => r.is_negative(),
            Self::Ceiling => r.is_positive(),
            Self::HalfUp => twice >= *den,
            Self::HalfDown => twice > *den,
            Self::Bankers => twice > *den || (twice == *den && q.is_odd()),
        };
        if up { away() } else { q }
    }
}

impl FromStr for Rounding {
    type Err = NumberError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ROUNDING_NAMES
            .iter()
            .find(|(_, n)| *n == s)
            .map(|(r, _)| *r)
            .ok_or_else(|| NumberError::Parse(format!("not a rounding mode: {s:?}")))
    }
}

impl fmt::Display for Rounding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An exact rational with arbitrary-precision numerator and denominator, always reduced, denominator positive.
///
/// Equality and order are by value. On the wire it is `{ "num": "…", "denom": "…" }` with decimal strings of any
/// length; it is read from that form, from text (`"12.34"`, `"1234/100"`, `"-5"`) or from a JSON integer, never from a
/// JSON float.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Rational(BigRational);

impl Rational {
    /// The value as a `BigRational`.
    pub(crate) fn as_big(&self) -> &BigRational {
        &self.0
    }

    /// Zero.
    pub fn zero() -> Self {
        Self(BigRational::zero())
    }

    /// One.
    pub fn one() -> Self {
        Self(BigRational::one())
    }

    /// A whole number.
    pub fn from_int(n: i64) -> Self {
        Self(BigRational::from_integer(BigInt::from(n)))
    }

    /// A whole number of any size.
    pub fn from_integer(n: BigInt) -> Self {
        Self(BigRational::from_integer(n))
    }

    /// `num / den`, reduced. A zero denominator is [`NumberError::DivisionByZero`].
    pub fn new(num: impl Into<BigInt>, den: impl Into<BigInt>) -> Result<Self, NumberError> {
        let den = den.into();
        if den.is_zero() {
            return Err(NumberError::DivisionByZero);
        }
        Ok(Self(BigRational::new(num.into(), den)))
    }

    /// The exact value of a core [`Numeric`]. A negative denominator multiplies, as in GnuCash
    /// (`5.17:libgnucash/engine/gnc-rational.cpp:39`); a zero denominator, GnuCash's error value, is refused.
    pub fn from_numeric(n: Numeric) -> Result<Self, NumberError> {
        match n.denom() {
            0 => Err(NumberError::InvalidNumeric(n.to_string())),
            d if d < 0 => Ok(Self::from_integer(BigInt::from(n.num()) * -BigInt::from(d))),
            d => Self::new(n.num(), d),
        }
    }

    /// The value as a reduced 64-bit [`Numeric`], when both parts fit.
    pub fn to_numeric(&self) -> Option<Numeric> {
        Some(Numeric::new(self.numer().to_i64()?, self.denom().to_i64()?))
    }

    /// The numerator, carrying the sign.
    pub fn numer(&self) -> &BigInt {
        self.0.numer()
    }

    /// The denominator, always positive.
    pub fn denom(&self) -> &BigInt {
        self.0.denom()
    }

    /// True for zero.
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// True below zero.
    pub fn is_negative(&self) -> bool {
        self.0.is_negative()
    }

    /// True above zero.
    pub fn is_positive(&self) -> bool {
        self.0.is_positive()
    }

    /// True for a whole number.
    pub fn is_integer(&self) -> bool {
        self.0.is_integer()
    }

    /// -1, 0 or 1.
    pub fn sign(&self) -> i64 {
        if self.is_negative() { -1 } else { i64::from(self.is_positive()) }
    }

    /// The absolute value.
    #[must_use]
    pub fn abs(&self) -> Self {
        Self(self.0.abs())
    }

    /// The bits of the larger part: what [`SizeCap`] limits.
    pub fn bits(&self) -> u64 {
        self.numer().bits().max(self.denom().bits())
    }

    /// `self + rhs`, within `cap`.
    pub fn checked_add(&self, rhs: &Self, cap: SizeCap) -> Result<Self, NumberError> {
        cap.check(self + rhs)
    }

    /// `self - rhs`, within `cap`.
    pub fn checked_sub(&self, rhs: &Self, cap: SizeCap) -> Result<Self, NumberError> {
        cap.check(self - rhs)
    }

    /// `self × rhs`, within `cap`.
    pub fn checked_mul(&self, rhs: &Self, cap: SizeCap) -> Result<Self, NumberError> {
        cap.check(self * rhs)
    }

    /// `self ÷ rhs`, within `cap`. Dividing by zero is an error, as `(/ x 0)` is in Scheme.
    pub fn checked_div(&self, rhs: &Self, cap: SizeCap) -> Result<Self, NumberError> {
        if rhs.is_zero() {
            return Err(NumberError::DivisionByZero);
        }
        cap.check(Self(&self.0 / &rhs.0))
    }

    /// The nearest integer, ties to even: Scheme's `round` on an exact number
    /// (`5.17:gnucash/report/reports/standard/category-barchart.scm:309`).
    #[must_use]
    pub fn round(&self) -> Self {
        Self::from_integer(Rounding::Bankers.apply(self.numer(), self.denom()))
    }

    /// The value in units of `1/denom`, rounded by `mode`: GnuCash's `gnc-numeric-convert` to a fixed denominator.
    /// The result is reduced, so the denominator may end up smaller than `denom`. `denom` must be positive.
    pub fn round_to(&self, denom: i64, mode: Rounding) -> Result<Self, NumberError> {
        if denom <= 0 {
            return Err(NumberError::InvalidDenominator(denom));
        }
        let denom = BigInt::from(denom);
        let units = mode.apply(&(self.numer() * &denom), self.denom());
        Self::new(units, denom)
    }

    /// Rounded half away from zero to `places` decimals, as an integer count of `10^-places`.
    pub(crate) fn scaled_half_up(&self, places: u32) -> BigInt {
        let scale = BigInt::from(10).pow(places);
        Rounding::HalfUp.apply(&(self.numer() * scale), self.denom())
    }

    /// The decimals the value needs, when it is a terminating decimal: its reduced denominator divides a power of ten.
    pub fn decimal_places(&self) -> Option<u64> {
        let denom = self.denom();
        let twos = denom.trailing_zeros().unwrap_or(0);
        let five = BigInt::from(5);
        let (rest, fives) = std::iter::successors(Some((denom >> twos, 0_u64)), |(rest, n)| {
            let (q, r) = rest.div_rem(&five);
            r.is_zero().then_some((q, n + 1))
        })
        .last()?;
        rest.is_one().then_some(twos.max(fives))
    }

    /// The exact value as decimal text (`-12.375`), when it is a terminating decimal.
    pub fn to_exact_decimal(&self) -> Option<String> {
        let places = u32::try_from(self.decimal_places()?).ok()?;
        Some(self.to_decimal(places))
    }

    /// Text with exactly `places` decimals, rounded half away from zero: for labels.
    pub fn to_decimal(&self, places: u32) -> String {
        let scaled = self.scaled_half_up(places);
        let sign = if scaled.is_negative() { "-" } else { "" };
        let (whole, frac) = split_decimal(&scaled.abs(), places);
        if places == 0 { format!("{sign}{whole}") } else { format!("{sign}{whole}.{frac}") }
    }

    /// Parse an integer (`-12`), a decimal (`12.34`, `.5`, `3.`) or a ratio (`1234/100`), with an optional sign and
    /// surrounding spaces. No exponent and no grouping separators.
    pub fn parse(s: &str) -> Result<Self, NumberError> {
        let err = || NumberError::Parse(format!("not a number: {s:?}"));
        let t = s.trim();
        if let Some((num, den)) = t.split_once('/') {
            let num = parse_integer(num.trim()).ok_or_else(err)?;
            let den = parse_integer(den.trim()).filter(|d| !d.is_negative()).ok_or_else(err)?;
            return Self::new(num, den);
        }
        let (negative, digits) = match t.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, t.strip_prefix('+').unwrap_or(t)),
        };
        let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
        let all_digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
        if (whole.is_empty() && frac.is_empty()) || !all_digits(whole) || !all_digits(frac) {
            return Err(err());
        }
        let magnitude: BigInt = format!("0{whole}{frac}").parse().map_err(|_| err())?;
        let scale = BigInt::from(10).pow(u32::try_from(frac.len()).map_err(|_| err())?);
        Self::new(if negative { -magnitude } else { magnitude }, scale)
    }
}

/// `n` as whole and fractional digits of a count of `10^-places`; `n` is not negative. No fractional digits for 0
/// places.
pub(crate) fn split_decimal(n: &BigInt, places: u32) -> (String, String) {
    let (whole, frac) = n.div_rem(&BigInt::from(10).pow(places));
    let width = usize::try_from(places).unwrap_or(0);
    let frac = if width == 0 { String::new() } else { format!("{frac:0>width$}") };
    (whole.to_string(), frac)
}

fn parse_integer(s: &str) -> Option<BigInt> {
    let digits = s.strip_prefix(['-', '+']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

impl From<i64> for Rational {
    fn from(n: i64) -> Self {
        Self::from_int(n)
    }
}

impl From<BigInt> for Rational {
    fn from(n: BigInt) -> Self {
        Self::from_integer(n)
    }
}

impl FromStr for Rational {
    type Err = NumberError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// `num/denom`, or just `num` for a whole number.
impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_integer() { write!(f, "{}", self.numer()) } else { write!(f, "{}/{}", self.numer(), self.denom()) }
    }
}

macro_rules! binary_op {
    ($trait:ident, $method:ident) => {
        impl $trait<&Rational> for &Rational {
            type Output = Rational;

            fn $method(self, rhs: &Rational) -> Rational {
                Rational($trait::$method(&self.0, &rhs.0))
            }
        }

        impl $trait for Rational {
            type Output = Rational;

            fn $method(self, rhs: Rational) -> Rational {
                Rational($trait::$method(self.0, rhs.0))
            }
        }
    };
}

binary_op!(Add, add);
binary_op!(Sub, sub);
binary_op!(Mul, mul);

impl Neg for Rational {
    type Output = Rational;

    fn neg(self) -> Rational {
        Rational(-self.0)
    }
}

impl Neg for &Rational {
    type Output = Rational;

    fn neg(self) -> Rational {
        Rational(-&self.0)
    }
}

impl Serialize for Rational {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire {
            num: String,
            denom: String,
        }
        Wire { num: self.numer().to_string(), denom: self.denom().to_string() }.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Rational {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(RationalVisitor)
    }
}

struct RationalVisitor;

impl<'de> Visitor<'de> for RationalVisitor {
    type Value = Rational;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(r#"an exact number: {"num": "1234", "denom": "100"}, "12.34", "1234/100" or an integer"#)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Rational, E> {
        Rational::parse(v).map_err(E::custom)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Rational, E> {
        Ok(Rational::from_int(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Rational, E> {
        Ok(Rational::from_integer(BigInt::from(v)))
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Rational, E> {
        Err(E::custom("exact numbers are never JSON floats; use a string such as \"12.34\""))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Rational, A::Error> {
        let (mut num, mut denom) = (None, None);
        while let Some(key) = map.next_key::<Cow<'de, str>>()? {
            let slot = match key.as_ref() {
                "num" => &mut num,
                "denom" => &mut denom,
                other => return Err(de::Error::unknown_field(other, &["num", "denom"])),
            };
            let text: Cow<'de, str> = map.next_value()?;
            *slot = Some(parse_integer(text.trim()).ok_or_else(|| de::Error::custom(format!("{key} is not an integer: {text:?}")))?);
        }
        let num = num.ok_or_else(|| de::Error::missing_field("num"))?;
        let denom = denom.ok_or_else(|| de::Error::missing_field("denom"))?;
        if !denom.is_positive() {
            return Err(de::Error::custom(format!("denom must be positive, got {denom}")));
        }
        Rational::new(num, denom).map_err(de::Error::custom)
    }
}

impl JsonSchema for Rational {
    fn schema_name() -> Cow<'static, str> {
        "Rational".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "An exact rational of any size, reduced, with a positive denominator. Never a JSON number.",
            "type": "object",
            "properties": {
                "num": { "type": "string", "pattern": "^-?\\d+$" },
                "denom": { "type": "string", "pattern": "^[1-9]\\d*$" }
            },
            "required": ["num", "denom"],
            "additionalProperties": false
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    fn r(s: &str) -> Rational {
        Rational::parse(s).unwrap()
    }

    use Rounding::{Bankers, Ceiling, Floor, HalfDown, HalfUp, Promote, Truncate};

    const MODES: [Rounding; 7] = [Floor, Ceiling, Truncate, Promote, HalfUp, HalfDown, Bankers];

    #[test]
    fn parses_every_text_form() {
        assert_eq!(r("12.34"), Rational::new(617, 50).unwrap());
        assert_eq!(r("1234/100"), r("12.34"));
        assert_eq!(r("-5"), Rational::from_int(-5));
        assert_eq!(r(" .5 "), Rational::new(1, 2).unwrap());
        assert_eq!(r("3."), Rational::from_int(3));
        assert_eq!(r("-6/4"), Rational::new(-3, 2).unwrap());
        assert_eq!(r("123456789012345678901234567890").to_string(), "123456789012345678901234567890");
        ["", ".", "1e5", "1,000", "1/0", "1/-2", "--1", "a/2", "1.2.3"].iter().for_each(|s| assert!(Rational::parse(s).is_err(), "{s}"));
    }

    #[test]
    fn numerics_convert_exactly() {
        assert_eq!(Rational::from_numeric(Numeric::new(1234, 100)).unwrap(), r("12.34"));
        assert_eq!(Rational::from_numeric(Numeric::new(3, -10)).unwrap(), Rational::from_int(30), "a negative denominator multiplies");
        assert!(Rational::from_numeric(Numeric::new(1, 0)).is_err());
        assert_eq!(r("12.34").to_numeric(), Some(Numeric::new(617, 50)));
        assert_eq!(r("123456789012345678901234567890").to_numeric(), None);
    }

    #[test]
    fn arithmetic_is_exact_and_capped() {
        let cap = SizeCap::DEFAULT;
        assert_eq!(r("1/3").checked_add(&r("1/6"), cap).unwrap(), r("1/2"));
        assert_eq!(r("1/3").checked_mul(&r("3"), cap).unwrap(), Rational::one());
        assert_eq!(r("1").checked_div(&r("0"), cap), Err(NumberError::DivisionByZero));
        let big = Rational::from_integer(BigInt::one() << 4000u32);
        assert_eq!(big.checked_mul(&big, cap), Err(NumberError::TooLarge { bits: 8001, max: 4096 }));
        assert!(big.checked_add(&big, cap).is_ok());
        assert_eq!(SizeCap::bits(8).check(r("256")), Err(NumberError::TooLarge { bits: 9, max: 8 }));
    }

    #[test]
    fn sign_abs_and_order() {
        assert_eq!(r("-1/2").sign(), -1);
        assert_eq!(Rational::zero().sign(), 0);
        assert_eq!(r("-1/2").abs(), r("1/2"));
        assert!(r("1/3") < r("1/2"));
        assert_eq!(r("1/3").clone().max(r("1/2")), r("1/2"));
        assert_eq!(-r("2"), r("-2"));
    }

    #[test]
    fn rounding_modes_match_gnucash() {
        let to = |s: &str, mode| r(s).round_to(1, mode).unwrap().to_string();
        let cases = [
            ("2.5", ["2", "3", "2", "3", "3", "2", "2"]),
            ("-2.5", ["-3", "-2", "-2", "-3", "-3", "-2", "-2"]),
            ("3.5", ["3", "4", "3", "4", "4", "3", "4"]),
            ("2.4", ["2", "3", "2", "3", "2", "2", "2"]),
            ("-2.6", ["-3", "-2", "-2", "-3", "-3", "-3", "-3"]),
        ];
        let modes = MODES;
        cases.iter().for_each(|(v, want)| modes.iter().zip(want).for_each(|(m, w)| assert_eq!(to(v, *m), *w, "{v} {m}")));
        assert_eq!(r("0.125").round_to(100, Rounding::Bankers).unwrap(), r("0.12"));
        assert_eq!(r("0.135").round_to(100, Rounding::Bankers).unwrap(), r("0.14"));
        assert_eq!(r("1/3").round_to(100, Rounding::Bankers).unwrap(), r("0.33"));
        assert_eq!(r("0.5").round(), Rational::zero());
        assert_eq!(r("1.5").round(), r("2"));
        assert!(r("1").round_to(0, Rounding::Bankers).is_err());
        assert_eq!("half-up".parse::<Rounding>().unwrap(), Rounding::HalfUp);
        assert!("never".parse::<Rounding>().is_err());
    }

    #[test]
    fn decimals_round_half_up() {
        assert_eq!(r("1/3").to_decimal(2), "0.33");
        assert_eq!(r("0.125").to_decimal(2), "0.13");
        assert_eq!(r("-0.125").to_decimal(2), "-0.13");
        assert_eq!(r("-0.001").to_decimal(2), "0.00");
        assert_eq!(r("2.5").to_decimal(0), "3");
        assert_eq!(r("1234.5").to_decimal(3), "1234.500");
        assert_eq!(r("-12.375").to_exact_decimal().as_deref(), Some("-12.375"));
        assert_eq!(r("7").to_exact_decimal().as_deref(), Some("7"));
        assert_eq!(r("1/3").to_exact_decimal(), None);
        assert_eq!(r("1/80").decimal_places(), Some(4));
    }

    #[test]
    fn json_is_exact_strings() {
        assert_eq!(serde_json::to_value(r("-12.5")).unwrap(), json!({ "num": "-25", "denom": "2" }));
        let read = |v| serde_json::from_value::<Rational>(v);
        assert_eq!(read(json!({ "num": "1234", "denom": "100" })).unwrap(), r("12.34"));
        assert_eq!(read(json!("12.34")).unwrap(), r("12.34"));
        assert_eq!(read(json!(7)).unwrap(), r("7"));
        assert!(read(json!(1.5)).is_err());
        assert!(read(json!({ "num": "1", "denom": "0" })).is_err());
        assert!(read(json!({ "num": "1", "denom": "-2" })).is_err());
        assert!(read(json!({ "num": "1", "denom": "2", "x": "3" })).is_err());
    }

    fn rational() -> impl Strategy<Value = Rational> {
        (any::<i64>(), 1..=i64::MAX).prop_map(|(n, d)| Rational::new(n, d).unwrap())
    }

    proptest! {
        #[test]
        fn sums_and_products_are_exact(a in rational(), b in rational(), c in rational()) {
            let cap = SizeCap::DEFAULT;
            let ab = a.checked_add(&b, cap).unwrap();
            prop_assert_eq!(ab.checked_sub(&b, cap).unwrap(), a.clone());
            prop_assert_eq!((&a + &b) + c.clone(), a.clone() + (&b + &c));
            prop_assert_eq!(&a * &(&b + &c), &(&a * &b) + &(&a * &c));
            if !b.is_zero() {
                prop_assert_eq!(a.checked_div(&b, cap).unwrap().checked_mul(&b, cap).unwrap(), a);
            }
        }

        #[test]
        fn wire_form_round_trips(a in rational()) {
            let json = serde_json::to_value(&a).unwrap();
            prop_assert_eq!(serde_json::from_value::<Rational>(json).unwrap(), a.clone());
            prop_assert_eq!(Rational::parse(&a.to_string()).unwrap(), a);
        }

        #[test]
        fn bankers_rounding_ties_to_even(n in -1_000_000_i64..1_000_000, places in 0_u32..4) {
            // n + 1/2 units of 10^-places: an exact tie.
            let unit = Rational::new(1, 10_i64.pow(places)).unwrap();
            let tie = &(&Rational::from_int(n) + &Rational::new(1, 2).unwrap()) * &unit;
            let rounded = tie.round_to(10_i64.pow(places), Rounding::Bankers).unwrap();
            let units = &rounded * &Rational::from_int(10_i64.pow(places));
            prop_assert!(units.is_integer());
            prop_assert!(units.numer().is_even(), "{} rounded to {}", tie, rounded);
            prop_assert!((&units - &Rational::from_int(n)).abs() <= Rational::one());
        }

        #[test]
        fn rounding_stays_within_one_unit(a in rational(), denom in 1_i64..1_000_000) {
            let unit = Rational::new(1, denom).unwrap();
            MODES
                .iter()
                .try_for_each(|m| {
                    let rounded = a.round_to(denom, *m).unwrap();
                    prop_assert!((&rounded - &a).abs() < unit);
                    prop_assert!((&rounded * &Rational::from_int(denom)).is_integer());
                    Ok(())
                })?;
        }
    }
}
