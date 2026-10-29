//! `Rational` and `Monetary` as scripts see them (`design/reports.md` §3, "Rational and Monetary"): exact arithmetic
//! mixing Rationals and integers, GnuCash's rounding modes, and amounts of one commodity.

use std::rc::Rc;

use red_ale_core::ops::commodities::lookup;
use red_ale_locale::Conventions;
use rhai::{Dynamic, Engine, EvalAltResult, INT, Position};

use crate::number::{
    CommodityFormat, Monetary, NumberError, PrintInfo, Rational, Rounding, SizeCap, cagr, fixed, format_amount, growth_rate,
};
use crate::query::{BookHandle, Fallible, Snapshot, commodity_arg, rational_arg, script_error, small_count};

/// The error of a Rational past the size cap. Rhai's data-size errors cannot be caught by a script's `try`, so a
/// runaway computation always stops the run; the engine reads this name back as the `number-size` limit.
pub(crate) const NUMBER_SIZE: &str = "Rational";

#[allow(clippy::unnecessary_box_returns)]
/// An arithmetic error as Rhai raises it: past the cap, the `number-size` limit; anything else, a script error.
fn arithmetic(e: NumberError) -> Box<EvalAltResult> {
    match e {
        NumberError::TooLarge { .. } => EvalAltResult::ErrorDataTooLarge(NUMBER_SIZE.to_owned(), Position::NONE).into(),
        NumberError::DivisionByZero => EvalAltResult::ErrorArithmetic("division by zero".to_owned(), Position::NONE).into(),
        other => script_error(other.to_string()),
    }
}

/// Register `Rational` and `Monetary`. Arithmetic is checked against `cap`; `snap` gives commodities their SCU and
/// symbol, and `conv` is the Locale's number format for `m.to_string()`.
pub(crate) fn register(engine: &mut Engine, snap: &Rc<Snapshot>, cap: SizeCap, conv: Conventions) {
    rational(engine, cap);
    rational_operators(engine, cap);
    monetary(engine, snap, cap, conv);
}

fn rational(engine: &mut Engine, cap: SizeCap) {
    engine.register_type_with_name::<Rational>("Rational");
    engine.register_fn("rational", move |v: Dynamic| -> Fallible<Rational> {
        rational_arg(&v).and_then(|r| capped(r, cap))
    });
    engine.register_fn("rational", move |n: Dynamic, d: Dynamic| -> Fallible<Rational> {
        rational_arg(&n)?.checked_div(&rational_arg(&d)?, cap).map_err(arithmetic)
    });
    engine.register_fn("to_string", |r: &mut Rational| r.to_string());
    engine.register_fn("to_debug", |r: &mut Rational| format!("rational(\"{r}\")"));
    engine.register_fn("abs", |r: Rational| r.abs());
    engine.register_fn("sign", |r: Rational| r.sign());
    engine.register_fn("is_zero", |r: Rational| r.is_zero());
    engine.register_fn("is_integer", |r: Rational| r.is_integer());
    engine.register_fn("num", |r: Rational| Rational::from_integer(r.numer().clone()));
    engine.register_fn("denom", |r: Rational| Rational::from_integer(r.denom().clone()));
    engine.register_fn("min", |a: Rational, b: Rational| a.min(b));
    engine.register_fn("max", |a: Rational, b: Rational| a.max(b));
    engine.register_fn("round", |r: Rational| r.round());
    engine.register_fn("round_to", |r: Rational, denom: INT, mode: &str| -> Fallible<Rational> {
        let mode: Rounding = mode.parse().map_err(|e: NumberError| script_error(e.to_string()))?;
        r.round_to(denom, mode).map_err(arithmetic)
    });
    engine.register_fn("round_sigfigs", |r: Rational, figs: INT, mode: &str| -> Fallible<Rational> {
        let mode: Rounding = mode.parse().map_err(|e: NumberError| script_error(e.to_string()))?;
        r.round_sigfigs(small_count(figs, "figures")?, mode).map_err(arithmetic)
    });
    engine.register_fn("to_decimal", |r: Rational, places: INT| -> Fallible<String> {
        Ok(r.to_decimal(small_count(places, "places")?))
    });
    engine.register_fn("fixed", |r: Dynamic, width: Dynamic, places: INT| -> Fallible<String> {
        let width = if width.is_unit() {
            None
        } else {
            Some(usize::try_from(width.as_int()?).map_err(|_| script_error("width must not be negative"))?)
        };
        Ok(fixed(&rational_arg(&r)?, width, small_count(places, "places")?))
    });
    growth(engine, cap);
}

/// `growth_rate(ratio, years)` and `calculate_cagr(basis, end_value, years)`: a Rational, or `()` where GnuCash
/// has `#f`. `calculate_cagr` takes `()` for a missing basis or years, as GnuCash's takes `#f`.
fn growth(engine: &mut Engine, cap: SizeCap) {
    let to_dynamic = |r: Option<Rational>| r.map_or(Dynamic::UNIT, Dynamic::from);
    engine.register_fn("growth_rate", move |ratio: Dynamic, years: Dynamic| -> Fallible<Dynamic> {
        let rate = growth_rate(&rational_arg(&ratio)?, &rational_arg(&years)?, cap).map_err(arithmetic)?;
        Ok(to_dynamic(rate))
    });
    engine.register_fn(
        "calculate_cagr",
        move |basis: Dynamic, end: Dynamic, years: Dynamic| -> Fallible<Dynamic> {
            let optional = |v: &Dynamic| if v.is_unit() { Ok(None) } else { rational_arg(v).map(Some) };
            let (basis, years) = (optional(&basis)?, optional(&years)?);
            let rate = cagr(basis.as_ref(), &rational_arg(&end)?, years.as_ref(), cap).map_err(arithmetic)?;
            Ok(to_dynamic(rate))
        },
    );
}

/// The error for a monetary amount in a commodity the book does not have.
#[allow(clippy::unnecessary_box_returns)]
fn unknown(m: &Monetary) -> Box<EvalAltResult> {
    script_error(format!("unknown commodity {}", m.commodity))
}

fn capped(r: Rational, cap: SizeCap) -> Fallible<Rational> {
    cap.check(r).map_err(arithmetic)
}

type Test = fn(std::cmp::Ordering) -> bool;

type Op = fn(&Rational, &Rational, SizeCap) -> Result<Rational, NumberError>;

/// `+ - * /` and the comparisons, for every mix of Rational and integer.
fn rational_operators(engine: &mut Engine, cap: SizeCap) {
    let ops: [(&str, Op); 4] = [
        ("+", Rational::checked_add),
        ("-", Rational::checked_sub),
        ("*", Rational::checked_mul),
        ("/", Rational::checked_div),
    ];
    ops.into_iter().for_each(|(name, op)| {
        engine.register_fn(name, move |a: Rational, b: Rational| op(&a, &b, cap).map_err(arithmetic));
        engine.register_fn(name, move |a: Rational, b: INT| {
            op(&a, &Rational::from_int(b), cap).map_err(arithmetic)
        });
        engine.register_fn(name, move |a: INT, b: Rational| {
            op(&Rational::from_int(a), &b, cap).map_err(arithmetic)
        });
    });
    engine.register_fn("-", |a: Rational| -a);
    engine.register_fn("+", |a: Rational| a);
    let compare: [(&str, Test); 6] = [
        ("==", std::cmp::Ordering::is_eq),
        ("!=", std::cmp::Ordering::is_ne),
        ("<", std::cmp::Ordering::is_lt),
        ("<=", std::cmp::Ordering::is_le),
        (">", std::cmp::Ordering::is_gt),
        (">=", std::cmp::Ordering::is_ge),
    ];
    compare.into_iter().for_each(|(name, test)| {
        engine.register_fn(name, move |a: Rational, b: Rational| test(a.cmp(&b)));
        engine.register_fn(name, move |a: Rational, b: INT| test(a.cmp(&Rational::from_int(b))));
        engine.register_fn(name, move |a: INT, b: Rational| test(Rational::from_int(a).cmp(&b)));
    });
}

fn monetary(engine: &mut Engine, snap: &Rc<Snapshot>, cap: SizeCap, conv: Conventions) {
    engine.register_type_with_name::<Monetary>("Monetary");
    let book = BookHandle(Rc::clone(snap));
    let s = Rc::clone(snap);
    engine.register_fn("monetary", move |c: Dynamic, amount: Dynamic| -> Fallible<Monetary> {
        Ok(Monetary::new(commodity_arg(s.book(), &c)?, capped(rational_arg(&amount)?, cap)?))
    });
    engine.register_get("commodity", move |m: &mut Monetary| book.commodity(&m.commodity));
    engine.register_get("amount", |m: &mut Monetary| m.amount.clone());
    engine.register_fn("+", move |a: Monetary, b: Monetary| a.checked_add(&b, cap).map_err(arithmetic));
    engine.register_fn("-", move |a: Monetary, b: Monetary| a.checked_sub(&b, cap).map_err(arithmetic));
    engine.register_fn("-", |a: Monetary| -a);
    engine.register_fn("==", |a: Monetary, b: Monetary| a == b);
    engine.register_fn("!=", |a: Monetary, b: Monetary| a != b);
    let s = Rc::clone(snap);
    engine.register_fn("round_scu", move |m: Monetary| -> Fallible<Monetary> {
        let fraction = lookup(s.book(), &m.commodity).map(|c| c.fraction).ok_or_else(|| unknown(&m))?;
        m.round_scu(fraction).map_err(arithmetic)
    });
    let s = Rc::clone(snap);
    engine.register_fn("to_string", move |m: &mut Monetary| -> Fallible<String> {
        let format = CommodityFormat::from_book(s.book(), &m.commodity).ok_or_else(|| unknown(m))?;
        Ok(m.to_display_string(&format, &conv))
    });
    // `xaccPrintAmount` with `gnc-commodity-print-info commodity use-symbol`
    // (`5.17:gnucash/report/reports/standard/investment-lots.scm:808-812`): the exact amount, the symbol only when asked.
    let s = Rc::clone(snap);
    engine.register_fn("to_string", move |m: &mut Monetary, use_symbol: bool| -> Fallible<String> {
        let format = CommodityFormat::from_book(s.book(), &m.commodity).ok_or_else(|| unknown(m))?;
        Ok(format_amount(&m.amount, &PrintInfo::commodity(&format, use_symbol), &conv))
    });
    engine.register_fn("to_debug", |m: &mut Monetary| {
        format!("monetary(\"{}\", \"{}\")", m.commodity, m.amount)
    });
}
