//! Exact rational numbers with GnuCash's arithmetic and rounding.
//!
//! [`Numeric`] is GnuCash's `gnc_numeric`: a 64-bit numerator over a 64-bit denominator. Every amount, value, price
//! and rate in red-ale is a `Numeric`; floats are never used for money.
//!
//! The arithmetic follows the C API of `5.17:libgnucash/engine/gnc-numeric.cpp` (`gnc_numeric_add`, `_sub`, `_mul`,
//! `_div`, `_convert`) so results, denominators and rounding match GnuCash bit for bit. Intermediates are 128-bit.
//! Ported from GnuCash (GPL-2.0-or-later), Copyright (C) 2000 Bill Gribble, 2014 John Ralls.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// GnuCash's `GNC_DENOM_AUTO`: let the operation choose the denominator.
pub const DENOM_AUTO: i64 = 0;

/// Errors from Numeric arithmetic. Mirrors `GNCNumericErrorCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NumericError {
    /// An operand had a zero denominator (`GNC_ERROR_ARG`).
    #[error("invalid numeric argument")]
    Arg,
    /// The result does not fit in 64 bits (`GNC_ERROR_OVERFLOW`).
    #[error("numeric overflow")]
    Overflow,
    /// Rounding was needed but `Round::Never` was asked for (`GNC_ERROR_REMAINDER`).
    #[error("rounding required but not allowed")]
    Remainder,
    /// Division by zero.
    #[error("division by zero")]
    DivideByZero,
}

/// How to round when a result must change denominator (`GNC_HOW_RND_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Round {
    /// Toward negative infinity.
    Floor,
    /// Toward positive infinity.
    Ceiling,
    /// Toward zero.
    Truncate,
    /// Away from zero.
    Promote,
    /// To nearest; ties toward zero.
    HalfDown,
    /// To nearest; ties away from zero.
    HalfUp,
    /// To nearest; ties to even (`GNC_HOW_RND_ROUND`).
    Bankers,
    /// Never round: an inexact result is an error.
    #[default]
    Never,
}

/// How to choose the result's denominator when `DENOM_AUTO` is passed (`GNC_HOW_DENOM_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DenomMode {
    /// No flag: use the given denominator.
    #[default]
    None,
    /// Compute exactly with 128-bit rationals, then fit into 64 bits.
    Exact,
    /// Reduce the result to lowest terms.
    Reduce,
    /// Use the least common multiple of the operands' denominators.
    Lcd,
    /// Keep the denominator the operation produced.
    Fixed,
    /// A power-of-ten denominator keeping this many significant figures.
    SigFigs(u8),
}

/// GnuCash's `how` argument: a denominator mode and a rounding mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct How {
    /// Denominator mode.
    pub denom: DenomMode,
    /// Rounding mode.
    pub round: Round,
}

impl How {
    /// Round with `round`, using the denominator given to the operation.
    pub const fn round(round: Round) -> Self {
        Self { denom: DenomMode::None, round }
    }

    /// Combine a denominator mode and a rounding mode.
    pub const fn new(denom: DenomMode, round: Round) -> Self {
        Self { denom, round }
    }

    /// `GNC_HOW_DENOM_EXACT`: exact result.
    pub const EXACT: Self = Self { denom: DenomMode::Exact, round: Round::Never };
    /// `GNC_HOW_DENOM_FIXED | GNC_HOW_RND_NEVER`, as used by `gnc_numeric_add_fixed`.
    pub const FIXED_NEVER: Self = Self { denom: DenomMode::Fixed, round: Round::Never };
    /// `GNC_HOW_DENOM_REDUCE | GNC_HOW_RND_NEVER`.
    pub const REDUCE_NEVER: Self = Self { denom: DenomMode::Reduce, round: Round::Never };
    /// `GNC_HOW_DENOM_LCD | GNC_HOW_RND_NEVER`.
    pub const LCD_NEVER: Self = Self { denom: DenomMode::Lcd, round: Round::Never };
}

/// An exact rational number with 64-bit numerator and denominator.
///
/// The denominator is kept as written: `1234/100` and `617/50` are equal values with different representations, and
/// both are preserved. Use [`Numeric::same_value`] or `==` (value equality) to compare values and
/// [`Numeric::same_repr`] to compare representations.
#[derive(Debug, Clone, Copy, Eq)]
pub struct Numeric {
    num: i64,
    denom: i64,
}

/// A 128-bit rational used for intermediates (GnuCash's `GncRational`).
#[derive(Debug, Clone, Copy)]
struct Rational {
    num: i128,
    den: i128,
}

/// Bounds of a value that fits a 64-bit leg; GnuCash's `GncInt128::isBig` tests `|x| > INT64_MAX`.
fn is_big(x: i128) -> bool {
    x > i128::from(i64::MAX) || x < -i128::from(i64::MAX)
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    i128::try_from(a).unwrap_or(i128::MAX)
}

fn lcm(a: i128, b: i128) -> Result<i128, NumericError> {
    if a == 0 || b == 0 {
        return Ok(0);
    }
    (a.abs() / gcd(a, b)).checked_mul(b.abs()).ok_or(NumericError::Overflow)
}

fn powten(exp: u32) -> i128 {
    10_i128.checked_pow(exp.min(38)).unwrap_or(i128::MAX)
}

/// `quotient_is_positive` from `5.17:libgnucash/engine/gnc-rational-rounding.hpp:31`.
fn quotient_is_positive(dividend: i128, divisor: i128) -> bool {
    (dividend > 0 && divisor > 0) || (dividend < 0 && divisor < 0)
}

/// Rounding with the generic templates, as `GncNumeric::convert` uses them (64-bit operands).
/// `5.17:libgnucash/engine/gnc-rational-rounding.hpp:66-210`.
#[allow(clippy::match_same_arms)] // One arm per GnuCash template, kept parallel to the source.
fn round_generic(num: i128, den: i128, rem: i128, round: Round) -> Result<i128, NumericError> {
    if rem == 0 {
        return Ok(num);
    }
    let away = |num: i128| {
        if num == 0 {
            if quotient_is_positive(rem, den) { 1 } else { -1 }
        } else {
            num + num.signum()
        }
    };
    let twice = (rem * 2).abs();
    Ok(match round {
        Round::Never => return Err(NumericError::Remainder),
        Round::Floor if num < 0 || (num == 0 && !quotient_is_positive(rem, den)) => num - 1,
        Round::Ceiling if num > 0 || (num == 0 && quotient_is_positive(rem, den)) => num + 1,
        Round::Floor | Round::Ceiling | Round::Truncate => num,
        Round::Promote => away(num),
        Round::HalfDown if twice > den.abs() => away(num),
        Round::HalfUp if twice >= den.abs() => away(num),
        Round::Bankers if twice > den.abs() || (twice == den.abs() && num % 2 != 0) => away(num),
        Round::HalfDown | Round::HalfUp | Round::Bankers => num,
    })
}

/// Rounding with the `GncInt128` specialisations, as `GncRational::convert` uses them.
///
/// These differ from the generic templates when the quotient is zero: a negative fraction is treated as positive
/// (for example, -0.6 rounds half-up to +1). GnuCash behaves this way, so red-ale does too.
/// `5.17:libgnucash/engine/gnc-rational-rounding.hpp:86-221`.
#[allow(clippy::match_same_arms)] // One arm per GnuCash specialisation.
fn round_int128(num: i128, den: i128, rem: i128, round: Round) -> Result<i128, NumericError> {
    if rem == 0 {
        return Ok(num);
    }
    let away = |num: i128| num + if num < 0 { -1 } else { 1 };
    let twice = rem.abs() * 2;
    Ok(match round {
        Round::Never => return Err(NumericError::Remainder),
        Round::Floor if num < 0 => num - 1,
        Round::Ceiling if num >= 0 => num + 1,
        Round::Floor | Round::Ceiling | Round::Truncate => num,
        Round::Promote => away(num),
        Round::HalfDown if twice > den.abs() => away(num),
        Round::HalfUp if twice >= den.abs() => away(num),
        Round::Bankers if twice > den.abs() || (twice == den.abs() && num % 2 != 0) => away(num),
        Round::HalfDown | Round::HalfUp | Round::Bankers => num,
    })
}

/// The parameters `prepare_conversion` computes: quotient, reduced conversion denominator and remainder.
struct Conversion {
    num: i128,
    den: i128,
    rem: i128,
}

impl Rational {
    fn new(num: i128, den: i128) -> Self {
        Self { num, den }
    }

    /// `GncRational(gnc_numeric)`: a negative denominator multiplies. `5.17:libgnucash/engine/gnc-rational.cpp:39`.
    fn from_numeric(n: Numeric) -> Self {
        let (num, den) = (i128::from(n.num), i128::from(n.denom));
        if den < 0 { Self::new(num * -den, 1) } else { Self::new(num, den) }
    }

    fn is_big(self) -> bool {
        is_big(self.num) || is_big(self.den)
    }

    fn reduce(self) -> Self {
        let g = gcd(self.den, self.num);
        if g == 0 { self } else { Self::new(self.num / g, self.den / g) }
    }

    fn neg(self) -> Self {
        Self::new(-self.num, self.den)
    }

    fn add(self, b: Self) -> Result<Self, NumericError> {
        let l = lcm(self.den, b.den)?;
        if l == 0 {
            return Err(NumericError::Arg);
        }
        let left = self.num.checked_mul(l / self.den).ok_or(NumericError::Overflow)?;
        let right = b.num.checked_mul(l / b.den).ok_or(NumericError::Overflow)?;
        Ok(Self::new(left.checked_add(right).ok_or(NumericError::Overflow)?, l))
    }

    fn mul(self, b: Self) -> Result<Self, NumericError> {
        let num = self.num.checked_mul(b.num).ok_or(NumericError::Overflow)?;
        let den = self.den.checked_mul(b.den).ok_or(NumericError::Overflow)?;
        Ok(Self::new(num, den))
    }

    /// `operator/(GncRational, GncRational)`, `5.17:libgnucash/engine/gnc-rational.cpp:290`.
    fn div(self, b: Self) -> Result<Self, NumericError> {
        let (mut a_num, mut b_num, mut a_den, mut b_den) = (self.num, b.num, self.den, b.den);
        if b_num == 0 {
            return Err(NumericError::DivideByZero);
        }
        if b_num < 0 {
            a_num = -a_num;
            b_num = -b_num;
        }
        if a_den == b_den {
            return Ok(Self::new(a_num, b_num));
        }
        if is_big(a_num) || is_big(a_den) || is_big(b_num) || is_big(b_den) {
            let g = gcd(b_den, a_den);
            b_den /= g;
            a_den /= g;
            let g = gcd(a_num, b_num);
            a_num /= g;
            b_num /= g;
        }
        let num = a_num.checked_mul(b_den).ok_or(NumericError::Overflow)?;
        let den = b_num.checked_mul(a_den).ok_or(NumericError::Overflow)?;
        Ok(Self::new(num, den))
    }

    /// `prepare_conversion`, `5.17:libgnucash/engine/gnc-rational.cpp:145`.
    fn prepare_conversion(self, new_denom: i128) -> Result<Conversion, NumericError> {
        if new_denom == self.den || new_denom == 0 {
            return Ok(Conversion { num: self.num, den: self.den, rem: 0 });
        }
        let conv = Self::new(new_denom, self.den).reduce();
        let scaled = self.num.checked_mul(conv.num).ok_or(NumericError::Overflow)?;
        Ok(Conversion { num: scaled / conv.den, den: conv.den, rem: scaled % conv.den })
    }

    /// `sigfigs_denom`, `5.17:libgnucash/engine/gnc-rational.cpp:161`.
    fn sigfigs_denom(self, figs: u32) -> i128 {
        if self.num == 0 {
            return 1;
        }
        let num_abs = self.num.abs();
        let not_frac = num_abs > self.den;
        let mut val = if not_frac { num_abs / self.den } else { self.den / num_abs };
        let mut digits = 0_u32;
        while val >= 10 {
            digits += 1;
            val /= 10;
        }
        if not_frac {
            powten(if digits < figs { figs - digits - 1 } else { 0 })
        } else {
            powten(figs + digits)
        }
    }

    /// `GncRational::convert<RT>` with the `GncInt128` rounding specialisations.
    fn convert(self, new_denom: i128, round: Round) -> Result<Self, NumericError> {
        let params = self.prepare_conversion(new_denom)?;
        let target = if new_denom == 0 { self.den } else { new_denom };
        if params.rem == 0 {
            return Ok(Self::new(params.num, target));
        }
        Ok(Self::new(round_int128(params.num, params.den, params.rem, round)?, target))
    }

    fn convert_sigfigs(self, figs: u32, round: Round) -> Result<Self, NumericError> {
        let new_denom = self.sigfigs_denom(figs);
        let params = self.prepare_conversion(new_denom)?;
        let target = if new_denom == 0 { 1 } else { new_denom };
        if params.rem == 0 {
            return Ok(Self::new(params.num, target));
        }
        Ok(Self::new(round_int128(params.num, params.den, params.rem, round)?, target))
    }

    /// `round_to_numeric`, `5.17:libgnucash/engine/gnc-rational.cpp:190`: approximate a big rational so it fits.
    fn round_to_numeric(self) -> Result<Self, NumericError> {
        if self.num == 0 {
            return Ok(Self::new(0, 1));
        }
        if !self.is_big() {
            return Ok(self);
        }
        let mut ll_bits = 64_u32;
        if self.num.abs() > self.den {
            if is_big(self.num / self.den) {
                return Err(NumericError::Overflow);
            }
            while ll_bits > 0 {
                let shifted = self.num.abs() >> ll_bits;
                if shifted != 0
                    && let Ok(candidate) = self.convert(self.den / shifted, Round::HalfDown)
                    && candidate.num != 0
                    && !candidate.is_big()
                {
                    return Ok(candidate);
                }
                ll_bits -= 1;
            }
            return Err(NumericError::Overflow);
        }
        if is_big(self.den / self.num) {
            return Ok(Self::new(0, 1));
        }
        while ll_bits > 0 {
            let divisor = self.den >> ll_bits;
            if divisor != 0 {
                if is_big(self.num) {
                    let (num, rem) = (self.num / divisor, self.num % divisor);
                    let den = self.den / divisor;
                    let num = num + i128::from(rem * 2 >= den);
                    if !is_big(num) && !is_big(den) {
                        return Ok(Self::new(num, den));
                    }
                } else if let Ok(candidate) = self.convert(self.den / divisor, Round::HalfDown)
                    && candidate.num != 0
                    && !candidate.is_big()
                {
                    return Ok(candidate);
                }
            }
            ll_bits -= 1;
        }
        Err(NumericError::Overflow)
    }

    fn to_numeric(self) -> Result<Numeric, NumericError> {
        let num = i64::try_from(self.num).map_err(|_| NumericError::Overflow)?;
        let denom = i64::try_from(self.den).map_err(|_| NumericError::Overflow)?;
        Ok(Numeric { num, denom })
    }

    /// `GncNumeric(GncRational)`, `5.17:libgnucash/engine/gnc-numeric.cpp:71`: fit into 64 bits, rounding if big.
    fn fit(self) -> Result<Numeric, NumericError> {
        if self.is_big() { self.reduce().round_to_numeric()?.to_numeric() } else { self.to_numeric() }
    }
}

/// `GncNumeric::convert<RT>` (64-bit rounding templates), `5.17:libgnucash/engine/gnc-numeric.hpp:192`.
fn convert_numeric(n: Numeric, new_denom: i64, round: Round) -> Result<Numeric, NumericError> {
    let r = Rational::from_numeric(n);
    let params = r.prepare_conversion(i128::from(new_denom))?;
    let target = if new_denom == DENOM_AUTO { n.denom } else { new_denom };
    if is_big(params.num) {
        let truncated = Rational::new(params.num, i128::from(target)).convert(i128::from(target), Round::Truncate)?;
        return Rational::new(truncated.num, i128::from(target)).to_numeric();
    }
    if params.rem == 0 {
        return Rational::new(params.num, i128::from(target)).to_numeric();
    }
    Rational::new(round_generic(params.num, params.den, params.rem, round)?, i128::from(target)).to_numeric()
}

fn convert_numeric_sigfigs(n: Numeric, figs: u8, round: Round) -> Result<Numeric, NumericError> {
    let r = Rational::from_numeric(n);
    let new_denom = r.sigfigs_denom(u32::from(figs));
    let params = r.prepare_conversion(new_denom)?;
    let target = if new_denom == 0 { 1 } else { new_denom };
    if params.rem == 0 {
        return Rational::new(params.num, target).to_numeric();
    }
    Rational::new(round_generic(params.num, params.den, params.rem, round)?, target).to_numeric()
}

/// The free `convert(T, denom, how)` of `5.17:libgnucash/engine/gnc-numeric.cpp:666`, for `GncNumeric`.
fn convert_how_numeric(n: Numeric, denom: i64, how: How) -> Result<Numeric, NumericError> {
    let n = if how.denom == DenomMode::Reduce { n.reduce() } else { n };
    match how.denom {
        DenomMode::SigFigs(figs) => convert_numeric_sigfigs(n, figs, how.round),
        _ => convert_numeric(n, denom, how.round),
    }
}

/// The same for `GncRational`.
fn convert_how_rational(r: Rational, denom: i64, how: How) -> Result<Rational, NumericError> {
    let r = if how.denom == DenomMode::Reduce { r.reduce() } else { r };
    match how.denom {
        DenomMode::SigFigs(figs) => r.convert_sigfigs(u32::from(figs), how.round),
        _ => r.convert(i128::from(denom), how.round),
    }
}

#[derive(Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    fn apply(self, a: Rational, b: Rational) -> Result<Rational, NumericError> {
        match self {
            Op::Add => a.add(b),
            Op::Sub => a.add(b.neg()),
            Op::Mul => a.mul(b),
            Op::Div => a.div(b),
        }
    }

    /// The `GncNumeric` operators short-cut zero operands. `5.17:libgnucash/engine/gnc-numeric.cpp:619-663`.
    #[allow(clippy::match_same_arms)]
    fn apply_numeric(self, a: Numeric, b: Numeric) -> Result<Numeric, NumericError> {
        match self {
            Op::Add if a.num == 0 => return Ok(b),
            Op::Add if b.num == 0 => return Ok(a),
            Op::Sub if a.num == 0 => return Ok(b.neg()),
            Op::Sub if b.num == 0 => return Ok(a),
            Op::Mul if a.num == 0 || b.num == 0 => return Ok(Numeric::ZERO),
            Op::Div if b.num == 0 => return Err(NumericError::DivideByZero),
            Op::Div if a.num == 0 => return Ok(Numeric::ZERO),
            _ => {}
        }
        self.apply(Rational::from_numeric(a), Rational::from_numeric(b))?.fit()
    }
}

impl Numeric {
    /// Zero, as `0/1`.
    pub const ZERO: Self = Self { num: 0, denom: 1 };
    /// One, as `1/1`.
    pub const ONE: Self = Self { num: 1, denom: 1 };

    /// A Numeric with this exact representation. A zero denominator gives an invalid Numeric, as in GnuCash.
    pub const fn new(num: i64, denom: i64) -> Self {
        Self { num, denom }
    }

    /// A whole number.
    pub const fn from_int(n: i64) -> Self {
        Self { num: n, denom: 1 }
    }

    /// The numerator as stored.
    pub const fn num(self) -> i64 {
        self.num
    }

    /// The denominator as stored.
    pub const fn denom(self) -> i64 {
        self.denom
    }

    /// False for GnuCash's error values (zero denominator).
    pub const fn is_valid(self) -> bool {
        self.denom != 0
    }

    /// True when the value is zero.
    pub const fn is_zero(self) -> bool {
        self.num == 0
    }

    /// True when the value is below zero.
    pub const fn is_negative(self) -> bool {
        (self.num < 0 && self.denom > 0) || (self.num > 0 && self.denom < 0)
    }

    /// True when the value is above zero.
    pub const fn is_positive(self) -> bool {
        (self.num > 0 && self.denom > 0) || (self.num < 0 && self.denom < 0)
    }

    /// The negation, keeping the denominator (`gnc_numeric_neg`).
    #[must_use]
    pub const fn neg(self) -> Self {
        Self { num: -self.num, denom: self.denom }
    }

    /// The absolute value, keeping the denominator (`gnc_numeric_abs`).
    #[must_use]
    pub const fn abs(self) -> Self {
        Self { num: self.num.abs(), denom: self.denom }
    }

    /// Lowest terms (`gnc_numeric_reduce`).
    #[must_use]
    pub fn reduce(self) -> Self {
        let r = Rational::from_numeric(self).reduce();
        r.to_numeric().unwrap_or(self)
    }

    /// True when both have the same value, whatever their denominators (`gnc_numeric_equal`).
    pub fn same_value(self, other: Self) -> bool {
        self.cmp_value(other) == Ordering::Equal
    }

    /// True when both have the same numerator and denominator (`gnc_numeric_eq`).
    pub const fn same_repr(self, other: Self) -> bool {
        self.num == other.num && self.denom == other.denom
    }

    /// Compare by value (`gnc_numeric_compare`). Invalid values compare equal to everything, as in GnuCash.
    pub fn cmp_value(self, other: Self) -> Ordering {
        if !self.is_valid() || !other.is_valid() {
            return Ordering::Equal;
        }
        let a = Rational::from_numeric(self);
        let b = Rational::from_numeric(other);
        (a.num * b.den).cmp(&(b.num * a.den))
    }

    fn arith(self, b: Self, denom: i64, how: How, op: Op) -> Result<Self, NumericError> {
        if !self.is_valid() || !b.is_valid() {
            return Err(NumericError::Arg);
        }
        let denom = if denom == DENOM_AUTO && how.denom == DenomMode::Lcd {
            i64::try_from(lcm(i128::from(self.denom), i128::from(b.denom))?).map_err(|_| NumericError::Overflow)?
        } else {
            denom
        };
        if how.denom != DenomMode::Exact {
            let result = op.apply_numeric(self, b)?;
            return convert_how_numeric(result, denom, how);
        }
        let result = op.apply(Rational::from_numeric(self), Rational::from_numeric(b))?;
        if denom == DENOM_AUTO && how.round != Round::Never {
            return result.round_to_numeric()?.to_numeric();
        }
        convert_how_rational(result, denom, how)?.to_numeric()
    }

    /// `gnc_numeric_add(a, b, denom, how)`.
    pub fn add(self, b: Self, denom: i64, how: How) -> Result<Self, NumericError> {
        self.arith(b, denom, how, Op::Add)
    }

    /// `gnc_numeric_sub(a, b, denom, how)`.
    pub fn sub(self, b: Self, denom: i64, how: How) -> Result<Self, NumericError> {
        self.arith(b, denom, how, Op::Sub)
    }

    /// `gnc_numeric_mul(a, b, denom, how)`.
    pub fn mul(self, b: Self, denom: i64, how: How) -> Result<Self, NumericError> {
        self.arith(b, denom, how, Op::Mul)
    }

    /// `gnc_numeric_div(a, b, denom, how)`.
    pub fn div(self, b: Self, denom: i64, how: How) -> Result<Self, NumericError> {
        self.arith(b, denom, how, Op::Div)
    }

    /// `gnc_numeric_convert(in, denom, how)`.
    pub fn convert(self, denom: i64, how: How) -> Result<Self, NumericError> {
        if !self.is_valid() {
            return Err(NumericError::Arg);
        }
        convert_how_numeric(self, denom, how)
    }

    /// Convert to `denom`, rounding with `round`. The common case `gnc_numeric_convert(x, scu, ROUND_HALF_UP)`.
    pub fn to_denom(self, denom: i64, round: Round) -> Result<Self, NumericError> {
        self.convert(denom, How::round(round))
    }

    /// `gnc_numeric_add_fixed`: `add(a, b, DENOM_AUTO, FIXED | NEVER)`.
    pub fn add_fixed(self, b: Self) -> Result<Self, NumericError> {
        self.add(b, DENOM_AUTO, How::FIXED_NEVER)
    }

    /// `gnc_numeric_sub_fixed`.
    pub fn sub_fixed(self, b: Self) -> Result<Self, NumericError> {
        self.sub(b, DENOM_AUTO, How::FIXED_NEVER)
    }

    /// Exact sum with 128-bit intermediates: `add(a, b, DENOM_AUTO, DENOM_EXACT)`.
    pub fn add_exact(self, b: Self) -> Result<Self, NumericError> {
        self.add(b, DENOM_AUTO, How::EXACT)
    }

    /// Exact difference.
    pub fn sub_exact(self, b: Self) -> Result<Self, NumericError> {
        self.sub(b, DENOM_AUTO, How::EXACT)
    }

    /// Sum of an iterator of Numerics with `add_fixed`, as GnuCash totals splits.
    pub fn sum_fixed<I: IntoIterator<Item = Self>>(items: I) -> Result<Self, NumericError> {
        items.into_iter().try_fold(Self::ZERO, Self::add_fixed)
    }

    /// Parse GnuCash's `num/denom` (or a bare integer) as written in files.
    pub fn parse(s: &str) -> Result<Self, ParseNumericError> {
        s.parse()
    }

    /// Parse a decimal such as `12.34`, `-0.5` or `100`, keeping the denominator the digits imply: `12.30` is
    /// `1230/100`. No exponent, no grouping separators.
    pub fn parse_decimal(s: &str) -> Result<Self, ParseNumericError> {
        let err = || ParseNumericError(s.to_owned());
        let t = s.trim();
        let (negative, digits) = match t.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, t.strip_prefix('+').unwrap_or(t)),
        };
        let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
        let valid = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
        if (whole.is_empty() && frac.is_empty()) || !valid(whole) || !valid(frac) {
            return Err(err());
        }
        let scale = u32::try_from(frac.len()).map_err(|_| err())?;
        let denom = 10_i64.checked_pow(scale).ok_or_else(err)?;
        let magnitude: i64 = format!("{whole}{frac}").parse().map_err(|_| err())?;
        Ok(Self { num: if negative { -magnitude } else { magnitude }, denom })
    }

    /// Parse either form: `num/denom` (or a bare integer) or a decimal.
    pub fn parse_any(s: &str) -> Result<Self, ParseNumericError> {
        if s.contains('/') { s.parse() } else { Self::parse_decimal(s) }
    }

    /// The value as a decimal string when the denominator is a power of ten, such as `12.34`; otherwise `num/denom`.
    pub fn to_decimal_string(self) -> String {
        let mut scale = 0_usize;
        let mut d = self.denom;
        while d > 1 && d % 10 == 0 {
            d /= 10;
            scale += 1;
        }
        if d != 1 || self.denom <= 0 {
            return self.to_string();
        }
        let sign = if self.num < 0 { "-" } else { "" };
        let digits = self.num.unsigned_abs().to_string();
        if scale == 0 {
            return format!("{sign}{digits}");
        }
        let padded = format!("{digits:0>width$}", width = scale + 1);
        let (whole, frac) = padded.split_at(padded.len() - scale);
        format!("{sign}{whole}.{frac}")
    }
}

impl PartialEq for Numeric {
    /// Value equality. Use [`Numeric::same_repr`] for representation equality.
    fn eq(&self, other: &Self) -> bool {
        self.same_value(*other)
    }
}

impl Default for Numeric {
    fn default() -> Self {
        Self::ZERO
    }
}

/// `gnc_numeric_to_string`: `num/denom`. `5.17:libgnucash/engine/gnc-numeric.cpp:1376`.
impl fmt::Display for Numeric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.num, self.denom)
    }
}

/// A string that is not a GnuCash numeric.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("not a numeric: {0:?}")]
pub struct ParseNumericError(pub String);

impl FromStr for Numeric {
    type Err = ParseNumericError;

    /// Accepts `num/denom` and bare integers, with surrounding whitespace, as `string_to_gnc_numeric` does.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseNumericError(s.to_owned());
        let trimmed = s.trim();
        let (num, denom) = match trimmed.split_once('/') {
            Some((n, d)) => (n.trim(), d.trim()),
            None => (trimmed, "1"),
        };
        let num = num.parse::<i64>().map_err(|_| err())?;
        let denom = denom.parse::<i64>().map_err(|_| err())?;
        Ok(Self { num, denom })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(num: i64, denom: i64) -> Numeric {
        Numeric::new(num, denom)
    }

    #[test]
    fn decimal_text() {
        assert!(Numeric::parse_decimal("12.30").unwrap().same_repr(n(1230, 100)));
        assert!(Numeric::parse_decimal("-0.5").unwrap().same_repr(n(-5, 10)));
        assert!(Numeric::parse_decimal("7").unwrap().same_repr(n(7, 1)));
        assert!(Numeric::parse_decimal(".25").unwrap().same_repr(n(25, 100)));
        assert!(Numeric::parse_decimal("1e3").is_err() && Numeric::parse_decimal("").is_err() && Numeric::parse_decimal("1,000").is_err());
        assert!(Numeric::parse_any("1234/100").unwrap().same_repr(n(1234, 100)));
        assert_eq!(n(-5, 100).to_decimal_string(), "-0.05");
        assert_eq!(n(1230, 100).to_decimal_string(), "12.30");
        assert_eq!(n(7, 1).to_decimal_string(), "7");
        assert_eq!(n(1, 3).to_decimal_string(), "1/3");
    }

    #[test]
    fn keeps_representation_and_compares_by_value() {
        assert_eq!(n(1234, 100), n(617, 50));
        assert!(!n(1234, 100).same_repr(n(617, 50)));
        assert_eq!(n(1234, 100).to_string(), "1234/100");
    }

    #[test]
    fn add_fixed_uses_lcm_denominator() {
        assert!(n(1, 2).add_fixed(n(1, 3)).unwrap().same_repr(n(5, 6)));
        assert!(n(150, 100).add_fixed(n(-150, 100)).unwrap().same_repr(n(0, 100)));
    }

    #[test]
    fn convert_rounds_like_gnucash() {
        let half_up = |x: Numeric| x.to_denom(100, Round::HalfUp).unwrap();
        assert!(half_up(n(12345, 1000)).same_repr(n(1235, 100)));
        assert!(half_up(n(-12345, 1000)).same_repr(n(-1235, 100)));
        assert!(half_up(n(-5, 1000)).same_repr(n(-1, 100)));
        let bankers = |x: Numeric| x.to_denom(100, Round::Bankers).unwrap();
        assert!(bankers(n(12345, 1000)).same_repr(n(1234, 100)));
        assert!(bankers(n(12355, 1000)).same_repr(n(1236, 100)));
        assert_eq!(n(1, 3).to_denom(100, Round::Never), Err(NumericError::Remainder));
    }

    #[test]
    fn mul_rounds_to_scu() {
        let price = n(3333, 1000);
        let qty = n(3, 1);
        let v = qty.mul(price, 100, How::round(Round::HalfUp)).unwrap();
        assert!(v.same_repr(n(1000, 100)));
    }

    #[test]
    fn div_sigfigs() {
        let r = n(100, 1).div(n(3, 1), DENOM_AUTO, How::new(DenomMode::SigFigs(6), Round::Bankers)).unwrap();
        assert!(r.same_repr(n(333_333, 10_000)));
    }

    #[test]
    fn exact_path_quirk_matches_gnucash() {
        // GncRational's int128 rounding rounds -0.6 half-up to +1 when the quotient is zero.
        let r = n(-6, 10).convert(1, How::new(DenomMode::Exact, Round::HalfUp)).unwrap();
        // convert() takes the GncNumeric path, which is correct:
        assert!(r.same_repr(n(-1, 1)));
        let quirky = n(-6, 10).add(n(0, 1), 1, How::new(DenomMode::Exact, Round::HalfUp)).unwrap();
        assert!(quirky.same_repr(n(1, 1)));
    }

    #[test]
    fn parses_file_format() {
        assert!(Numeric::parse("-617/50").unwrap().same_repr(n(-617, 50)));
        assert!(Numeric::parse("42").unwrap().same_repr(n(42, 1)));
        assert!(Numeric::parse("1/x").is_err());
    }
}
