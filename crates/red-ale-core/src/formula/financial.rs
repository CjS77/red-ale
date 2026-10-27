//! The financial functions of `5.17:bindings/guile/fin.scm` (Copyright 2002 Joshua Sled, 2012 Frank H. Ellenberger;
//! GPL-2.0-or-later; this translation is GPL-3.0-or-later), computed exactly.
//!
//! GnuCash calls them in Guile with each argument as a double and rounds the double result half up to 12 significant
//! figures (`func_op`, `5.17:libgnucash/app-utils/gnc-exp-parser.c:288-360`). red-ale never runs Guile and never
//! uses floats: with a whole number of periods `(1 + r)^n` is a rational, so each function is computed exactly with
//! big integers and then rounded as GnuCash rounds the double (`Exact::to_numeric`). The two agree unless the exact
//! value lies within the double's error (a few units in its 15th digit) of a 12-significant-figure rounding point
//! (`design/notes/engine-scheduled-transactions.md` §7.4). Where Guile's doubles overflow, red-ale fails too
//! (`Exact::pow`), but red-ale may compute a result whose intermediate products alone would overflow a double.
//!
//! | Function | fin.scm | Computed when |
//! |---|---|---|
//! | `foobar(v)` | `:27` | always |
//! | `pmt(rate : nper : pv : fv : type)` | `:49-51` | `nper` is whole |
//! | `ipmt(rate : per : nper : pv : fv : type)` | `:34-39` | `nper` and `min(per, nper)` are whole |
//! | `ppmt(rate : per : nper : pv : fv : type)` | `:42-46` | as `ipmt` |
//! | `futureValue(a : r : n : t)` | `:56-64` | `n × t` is whole |
//! | `computeInterestIncrement(pv : ann_rate : compounds : period)` | `:66-68` | `period` is whole |
//! | `periodic_rate(rate : yper : ycomp)` | `:110-112` | `ycomp / yper` is whole |
//!
//! The `cpd_*` functions (`:152-184`) and the `amort_*` functions (`:300-337`) are refused: they round to the cent with
//! Scheme's `round` on doubles, whose ties red-ale can't reproduce exactly (note §9.3, H9). Any other name is refused,
//! where GnuCash would call any Guile procedure named `gnc:…`. A division by zero (a zero rate, for one) fails, as GnuCash's NaN or infinity does (`gnc-numeric.cpp:89-95`).

use std::cmp::Ordering;

use num_bigint::{BigInt, Sign};

use super::{Argument, FunctionError, Functions};
use crate::{DENOM_AUTO, DenomMode, How, Numeric, Round};

/// The largest power (number of periods) computed exactly: a century of daily periods.
pub const MAX_POWER: i64 = 36_600;

/// GnuCash's functions from `fin.scm`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Financial;

type Function = fn(&[Exact]) -> Result<Exact, FunctionError>;

impl Functions for Financial {
    fn call(&self, name: &str, args: &[Argument<'_>]) -> Result<Numeric, FunctionError> {
        let (arity, function): (usize, Function) = match name {
            "foobar" => (1, |a| Ok(a[0].clone())),
            "pmt" => (5, pmt),
            "ipmt" => (6, ipmt),
            "ppmt" => (6, ppmt),
            "futureValue" => (4, future_value),
            "computeInterestIncrement" => (4, interest_increment),
            "periodic_rate" => (3, |a| periodic_rate(&a[0], &a[1], &a[2])),
            "cpd_pmt" | "cpd_ipmt" | "cpd_ppmt" | "amort_balance" | "amort_pmt" | "amort_ppmt" | "amort_ipmt" => {
                return Err(FunctionError::Inexact("it rounds to the cent with Scheme's round on doubles"));
            }
            _ => return Err(FunctionError::Unknown),
        };
        if args.len() != arity {
            return Err(FunctionError::Arity(arity));
        }
        let args = args
            .iter()
            .map(|a| match a {
                Argument::Number(n) if n.is_valid() => Ok(Exact::from(*n)),
                Argument::Number(_) => Err(FunctionError::Range),
                Argument::Text(_) => Err(FunctionError::Text),
            })
            .collect::<Result<Vec<_>, _>>()?;
        function(&args)?.to_numeric()
    }
}

/// `calc-pmt` (`fin.scm:75-81`) with `calc-pvif` and `calc-fvifa` (`:83-89`): `(-pv·(1+r)^n - fv) / (fvifa·(1 + r·type))`.
fn calc_pmt(rate: &Exact, nper: &Exact, pv: &Exact, fv: &Exact, kind: &Exact) -> Result<Exact, FunctionError> {
    let pvif = Exact::int(1).add(rate).pow(nper.whole("nper")?)?;
    let fvifa = pvif.sub(&Exact::int(1)).div(rate)?;
    pv.neg().mul(&pvif).sub(fv).div(&fvifa.mul(&Exact::int(1).add(&rate.mul(kind))))
}

/// `calc-principal` (`fin.scm:91-96`): the balance after `per` payments of `pmt`.
fn calc_principal(pv: &Exact, pmt: &Exact, rate: &Exact, per: i64) -> Result<Exact, FunctionError> {
    let x = Exact::int(1).add(rate).pow(per)?;
    Ok(pv.mul(&x).add(&pmt.mul(&x.sub(&Exact::int(1)).div(rate)?)))
}

/// `gnc:pmt` (`fin.scm:49-51`).
fn pmt(a: &[Exact]) -> Result<Exact, FunctionError> {
    Ok(calc_pmt(&a[0], &a[1], &a[2], &a[3], &a[4])?.neg())
}

/// `gnc:ipmt` (`fin.scm:34-39`): `rate · principal(pv, calc-pmt, rate, min(per, nper) - 1)`.
fn ipmt(a: &[Exact]) -> Result<Exact, FunctionError> {
    let [rate, per, nper, pv, fv, kind] = a else { return Err(FunctionError::Arity(6)) };
    interest(rate, per, nper, pv, &calc_pmt(rate, nper, pv, fv, kind)?)
}

/// The interest of `gnc:ipmt` with `calc-pmt` already computed.
fn interest(rate: &Exact, per: &Exact, nper: &Exact, pv: &Exact, payment: &Exact) -> Result<Exact, FunctionError> {
    let per = if per.cmp(nper) == Ordering::Greater { nper } else { per };
    let k = per.sub(&Exact::int(1)).whole("per")?;
    Ok(rate.mul(&calc_principal(pv, payment, rate, k)?))
}

/// `gnc:ppmt` (`fin.scm:42-46`): `-(calc-pmt + ipmt)`.
fn ppmt(a: &[Exact]) -> Result<Exact, FunctionError> {
    let [rate, per, nper, pv, fv, kind] = a else { return Err(FunctionError::Arity(6)) };
    let payment = calc_pmt(rate, nper, pv, fv, kind)?;
    Ok(payment.add(&interest(rate, per, nper, pv, &payment)?).neg())
}

/// `gnc:futureValue` (`fin.scm:56-64`): `a·(1 + r/n)^(n·t)`.
fn future_value(a: &[Exact]) -> Result<Exact, FunctionError> {
    let [amount, rate, n, t] = a else { return Err(FunctionError::Arity(4)) };
    let power = n.mul(t).whole("n × t")?;
    Ok(amount.mul(&Exact::int(1).add(&rate.div(n)?).pow(power)?))
}

/// `gnc:computeInterestIncrement` (`fin.scm:66-68`): `(r/c)·pv·(1 + r/c)^(period - 1)`.
fn interest_increment(a: &[Exact]) -> Result<Exact, FunctionError> {
    let [pv, ann_rate, compounds, period] = a else { return Err(FunctionError::Arity(4)) };
    let rate = ann_rate.div(compounds)?;
    let power = period.sub(&Exact::int(1)).whole("period")?;
    Ok(rate.mul(&pv.mul(&Exact::int(1).add(&rate).pow(power)?)))
}

/// `gnc:periodic_rate` (`fin.scm:110-112`): `(1 + rate/ycomp)^(ycomp/yper) - 1`.
fn periodic_rate(rate: &Exact, yper: &Exact, ycomp: &Exact) -> Result<Exact, FunctionError> {
    let power = ycomp.div(yper)?.whole("ycomp / yper")?;
    Ok(Exact::int(1).add(&rate.div(ycomp)?).pow(power)?.sub(&Exact::int(1)))
}

/// An exact rational over big integers, unreduced: `num / den`, `den` > 0.
#[derive(Debug, Clone)]
struct Exact {
    num: BigInt,
    den: BigInt,
}

impl From<Numeric> for Exact {
    /// A negative denominator multiplies, as in `GncRational(gnc_numeric)`.
    fn from(n: Numeric) -> Self {
        let (num, den) = (BigInt::from(n.num()), BigInt::from(n.denom()));
        match den.sign() {
            Sign::Minus => Self { num: num * -den, den: BigInt::from(1) },
            _ => Self { num, den },
        }
    }
}

impl Exact {
    fn int(i: i64) -> Self {
        Self { num: BigInt::from(i), den: BigInt::from(1) }
    }

    fn add(&self, o: &Self) -> Self {
        Self { num: &self.num * &o.den + &o.num * &self.den, den: &self.den * &o.den }
    }

    fn sub(&self, o: &Self) -> Self {
        self.add(&o.neg())
    }

    fn mul(&self, o: &Self) -> Self {
        Self { num: &self.num * &o.num, den: &self.den * &o.den }
    }

    fn neg(&self) -> Self {
        Self { num: -&self.num, den: self.den.clone() }
    }

    fn div(&self, o: &Self) -> Result<Self, FunctionError> {
        match o.num.sign() {
            Sign::NoSign => Err(FunctionError::DivisionByZero),
            Sign::Plus => Ok(Self { num: &self.num * &o.den, den: &self.den * &o.num }),
            Sign::Minus => Ok(Self { num: -(&self.num * &o.den), den: -(&self.den * &o.num) }),
        }
    }

    fn cmp(&self, o: &Self) -> Ordering {
        (&self.num * &o.den).cmp(&(&o.num * &self.den))
    }

    /// `self^e` for a whole `e`, refusing a result past a double's range, where GnuCash's infinity fails.
    fn pow(&self, e: i64) -> Result<Self, FunctionError> {
        let n = u32::try_from(e.unsigned_abs()).map_err(|_| FunctionError::TooLarge("the power"))?;
        let x = Self { num: self.num.pow(n), den: self.den.pow(n) };
        let x = if e < 0 { Self::int(1).div(&x)? } else { x };
        if x.num.bits() > x.den.bits() + 1024 {
            return Err(FunctionError::Range);
        }
        Ok(x)
    }

    /// The value as a whole power, at most [`MAX_POWER`] in size.
    fn whole(&self, what: &'static str) -> Result<i64, FunctionError> {
        let (q, r) = (&self.num / &self.den, &self.num % &self.den);
        if r.sign() != Sign::NoSign {
            return Err(FunctionError::NotWhole(what));
        }
        i64::try_from(q).ok().filter(|q| q.abs() <= MAX_POWER).ok_or(FunctionError::TooLarge(what))
    }

    /// `floor(self)`.
    fn floor(&self) -> BigInt {
        let q = &self.num / &self.den;
        if self.num.sign() == Sign::Minus && &q * &self.den != self.num { q - 1 } else { q }
    }

    /// The Numeric GnuCash makes of the result: `GncNumeric(double)` takes `floor(d·10^k) / 10^k` with the largest
    /// `k` ≤ 18 that fits 64 bits, refusing |d| > 10^18 (`5.17:libgnucash/engine/gnc-numeric.cpp:87-119`), then
    /// `double_to_gnc_numeric` rounds it half up to 12 significant figures (`gnc-exp-parser.c:346-349`).
    fn to_numeric(&self) -> Result<Numeric, FunctionError> {
        let ten = BigInt::from(10);
        if self.num.magnitude() > (ten.pow(18) * &self.den).magnitude() {
            return Err(FunctionError::Range);
        }
        let limit = BigInt::from(i64::MAX) * &self.den;
        let k = (1..=18_u32).rev().find(|k| (&self.num * ten.pow(*k)).magnitude() <= limit.magnitude()).unwrap_or(1);
        let scaled = Self { num: &self.num * ten.pow(k), den: self.den.clone() };
        let num = i64::try_from(scaled.floor()).map_err(|_| FunctionError::Range)?;
        let den = 10_i64.pow(k);
        Numeric::new(num, den)
            .reduce()
            .convert(DENOM_AUTO, How::new(DenomMode::SigFigs(12), Round::HalfUp))
            .map_err(|_| FunctionError::Range)
    }
}
