//! Reading the arguments scripts pass: commodities, accounts, times and numbers, each in the forms a script may hold
//! them.

use red_ale_core::ops::commodities::{canonical, lookup};
use red_ale_core::{Book, CommodityRef, Guid, Time64};
use rhai::{Array, Dynamic, INT};

use super::{AccountHandle, CommodityHandle, Fallible, script_error};
use crate::number::Rational;
use crate::wire::parse_commodity;

/// A commodity: a `Commodity` handle, or its id as text (`CURRENCY::USD`, or a bare currency code). It must be in the
/// Book or be a built-in currency.
pub(crate) fn commodity_arg(book: &Book, v: &Dynamic) -> Fallible<CommodityRef> {
    let id = if let Some(c) = v.read_lock::<CommodityHandle>() {
        c.id.clone()
    } else if let Some(text) = v.read_lock::<rhai::ImmutableString>() {
        parse_commodity(&text).map_err(script_error)?
    } else {
        return Err(script_error(format!("expected a commodity, got {}", v.type_name())));
    };
    lookup(book, &canonical(&id)).map(|c| c.id.clone()).ok_or_else(|| script_error(format!("unknown commodity {id}")))
}

/// A commodity, or `()` for none.
pub(crate) fn optional_commodity_arg(book: &Book, v: &Dynamic) -> Fallible<Option<CommodityRef>> {
    if v.is_unit() { Ok(None) } else { commodity_arg(book, v).map(Some) }
}

/// A Time64: seconds since the epoch, an integer.
pub(crate) fn time_arg(v: &Dynamic) -> Fallible<Time64> {
    v.as_int().map(Time64).map_err(|t| script_error(format!("expected a time (an integer), got {t}")))
}

/// A Time64, or `()` for none.
pub(crate) fn optional_time_arg(v: &Dynamic) -> Fallible<Option<Time64>> {
    if v.is_unit() { Ok(None) } else { time_arg(v).map(Some) }
}

/// An account: an `Account` handle, or a GUID or full name as text.
pub(crate) fn account_arg(book: &Book, v: &Dynamic) -> Fallible<Guid> {
    if let Some(a) = v.read_lock::<AccountHandle>() {
        return Ok(a.guid);
    }
    let text = v.read_lock::<rhai::ImmutableString>().ok_or_else(|| script_error(format!("expected an account, got {}", v.type_name())))?;
    let found = Guid::parse(text.trim()).ok().and_then(|g| book.account(g)).or_else(|| book.account_by_full_name(&text));
    found.map(|a| a.guid).ok_or_else(|| script_error(format!("unknown account {:?}", text.as_str())))
}

/// Accounts: an array of what [`account_arg`] reads.
pub(crate) fn accounts_arg(book: &Book, items: &Array) -> Fallible<Vec<Guid>> {
    items.iter().map(|v| account_arg(book, v)).collect()
}

/// An exact number: a `Rational`, an integer, or text such as `"12.34"` or `"1234/100"`.
pub(crate) fn rational_arg(v: &Dynamic) -> Fallible<Rational> {
    if let Some(r) = v.read_lock::<Rational>() {
        return Ok(r.clone());
    }
    if let Ok(n) = v.as_int() {
        return Ok(Rational::from_int(n));
    }
    match v.read_lock::<rhai::ImmutableString>() {
        Some(text) => Rational::parse(&text).map_err(|e| script_error(e.to_string())),
        None => Err(script_error(format!("expected a number, got {}", v.type_name()))),
    }
}

/// A non-negative integer that fits a `u32`, such as a count of decimal places.
pub(crate) fn small_count(n: INT, what: &str) -> Fallible<u32> {
    u32::try_from(n).map_err(|_| script_error(format!("{what} must be from 0 to {}, got {n}", u32::MAX)))
}
