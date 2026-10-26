//! The `Book` handle (`design/reports.md` §5, "Book").

use std::rc::Rc;

use red_ale_core::book_options::{company_name, uses_trading_accounts};
use red_ale_core::currencies::{convert_from_euro, convert_to_euro, is_euro_currency};
use red_ale_core::ops::accounts::descendants_sorted;
use red_ale_core::ops::commodities::{canonical, lookup};
use red_ale_core::ops::prices::convert::{PriceAt, convert_balance, rate};
use red_ale_core::ops::prices::prices;
use red_ale_core::ops::transactions::find::{DescriptionPattern, PeriodQuery, cross_commodity_splits, period_splits};
use red_ale_core::{AccountType, Guid, Numeric, Time64};
use rhai::{Array, Dynamic, Engine, INT, Map};

use super::args::{account_arg, accounts_arg, commodity_arg, optional_commodity_arg, optional_time_arg};
use super::{BookHandle, Fallible, PriceHandle, script_error};
use crate::number::{Monetary, Rational};
use crate::wire::parse_commodity;

pub(super) fn register(engine: &mut Engine) {
    engine.register_type_with_name::<BookHandle>("Book");
    engine.register_fn("to_string", |_: &mut BookHandle| "Book".to_owned());
    engine.register_fn("root", |b: &mut BookHandle| b.account(b.0.book().root));
    engine.register_fn("account", account);
    engine.register_fn("accounts", |b: &mut BookHandle| -> Array {
        b.0.book().descendants(b.0.book().root).iter().map(|a| Dynamic::from(b.account(a.guid))).collect()
    });
    engine.register_fn("accounts_of_types", accounts_of_types);
    engine.register_fn("commodity", |b: &mut BookHandle, id: &str| -> Dynamic {
        let found = parse_commodity(id).ok().and_then(|id| lookup(b.0.book(), &canonical(&id)).map(|c| c.id.clone()));
        found.map_or(Dynamic::UNIT, |id| Dynamic::from(b.commodity(id)))
    });
    engine.register_fn("uses_trading_accounts", |b: &mut BookHandle| uses_trading_accounts(b.0.book()));
    engine.register_fn("company_name", |b: &mut BookHandle| company_name(b.0.book()).to_owned());
    engine.register_fn("prices", book_prices);
    engine.register_fn("price_rate", price_rate);
    engine.register_fn("price_convert", price_convert);
    engine.register_fn("commodity_splits", commodity_splits);
    engine.register_fn("split_query", split_query);
    engine.register_fn("is_euro_currency", |c: Dynamic| -> Fallible<bool> { euro_side(&c).map(|c| is_euro_currency(&c)) });
    engine.register_fn("euro_convert", euro_convert);
}

/// `book.account(ref)`: by GUID or full name, `()` when there is none.
fn account(b: &mut BookHandle, reference: &str) -> Dynamic {
    let book = b.0.book();
    let found = Guid::parse(reference.trim()).ok().and_then(|g| book.account(g)).or_else(|| book.account_by_full_name(reference));
    found.map_or(Dynamic::UNIT, |a| Dynamic::from(b.account(a.guid)))
}

/// `gnc:filter-accountlist-type` over the root's descendants in display order (`report/report-utilities.scm:178-181`).
fn accounts_of_types(b: &mut BookHandle, types: Array) -> Fallible<Array> {
    let types: Vec<AccountType> = types
        .iter()
        .map(|t| {
            let name = t.read_lock::<rhai::ImmutableString>().ok_or_else(|| script_error("account types are names such as \"ASSET\""))?;
            name.parse::<AccountType>().map_err(|e| script_error(e.to_string()))
        })
        .collect::<Fallible<_>>()?;
    let book = b.0.book();
    Ok(descendants_sorted(book, book.root).into_iter().filter(|a| types.contains(&a.account_type)).map(|a| Dynamic::from(b.account(a.guid))).collect())
}

/// `gnc-pricedb-get-prices`: forward only, newest first, ties by GUID.
fn book_prices(b: &mut BookHandle, commodity: Dynamic, currency: Dynamic) -> Fallible<Array> {
    let book = b.0.book();
    let (commodity, currency) = (commodity_arg(book, &commodity)?, commodity_arg(book, &currency)?);
    let found = prices(book, &commodity, Some(&currency), false);
    Ok(found.into_iter().map(|p| Dynamic::from(PriceHandle { snap: Rc::clone(&b.0), guid: p.guid })).collect())
}

/// Which price a lookup takes: `"nearest"`, `"before"` or `"latest"`; `"latest"` reads the run's now, never the clock.
fn price_at(b: &BookHandle, when: INT, mode: &str) -> Fallible<PriceAt> {
    match mode {
        "nearest" => Ok(PriceAt::Nearest(Time64(when))),
        "before" => Ok(PriceAt::Before(Time64(when))),
        "latest" => Ok(PriceAt::Latest { now: b.0.now() }),
        other => Err(script_error(format!("price mode must be \"nearest\", \"before\" or \"latest\", got {other:?}"))),
    }
}

/// `gnc-pricedb-get-{nearest,nearest-before,latest}-price`.
fn price_rate(b: &mut BookHandle, commodity: Dynamic, currency: Dynamic, when: INT, mode: &str) -> Fallible<Rational> {
    let book = b.0.book();
    let (from, to) = (commodity_arg(book, &commodity)?, commodity_arg(book, &currency)?);
    let found = rate(book, &from, &to, price_at(b, when, mode)?).map_err(|e| script_error(format!("price of {from} in {to}: {e}")))?;
    numeric(found)
}

/// `gnc-pricedb-convert-balance-*`: the amount times the rate, at the currency's SCU with banker's rounding.
fn price_convert(b: &mut BookHandle, m: Monetary, currency: Dynamic, when: INT, mode: &str) -> Fallible<Monetary> {
    let book = b.0.book();
    let to = commodity_arg(book, &currency)?;
    // GnuCash makes the amount a gnc_numeric here and fails past 64 bits (`design/reports.md` §3).
    let amount = m.amount.to_numeric().ok_or_else(|| script_error(format!("{} does not fit a 64-bit amount", m.amount)))?;
    let converted = convert_balance(book, amount, &m.commodity, &to, price_at(b, when, mode)?).map_err(|e| script_error(e.to_string()))?;
    Ok(Monetary::new(to, numeric(converted)?))
}

/// `gnc-get-match-commodity-splits`.
fn commodity_splits(b: &mut BookHandle, accounts: Array, until: Dynamic, commodity: Dynamic, sorted: bool) -> Fallible<Array> {
    let book = b.0.book();
    let accounts = accounts_arg(book, &accounts)?;
    let commodity = optional_commodity_arg(book, &commodity)?;
    Ok(b.splits(cross_commodity_splits(book, &accounts, optional_time_arg(&until)?, commodity.as_ref(), sorted)))
}

/// The split query of `gnc:account-get-trans-type-splits-interval`: `#{ accounts, from, to, description, closing }`.
fn split_query(b: &mut BookHandle, q: Map) -> Fallible<Array> {
    let book = b.0.book();
    let known = ["accounts", "from", "to", "description", "closing"];
    if let Some(key) = q.keys().find(|k| !known.contains(&k.as_str())) {
        return Err(script_error(format!("unknown split query field {key:?}")));
    }
    let field = |name: &str| q.get(name).cloned().unwrap_or(Dynamic::UNIT);
    let accounts = field("accounts");
    let accounts = match accounts.read_lock::<Array>() {
        Some(items) => accounts_arg(book, &items)?,
        None if accounts.is_unit() => Vec::new(),
        None => vec![account_arg(book, &accounts)?],
    };
    let closing = field("closing");
    let query = PeriodQuery {
        accounts,
        from: optional_time_arg(&field("from"))?,
        to: optional_time_arg(&field("to"))?,
        description: description(&field("description"))?,
        closing: if closing.is_unit() { false } else { closing.as_bool().map_err(|_| script_error("closing must be true or false"))? },
    };
    let found = period_splits(book, &query).map_err(|e| script_error(e.to_string()))?;
    Ok(b.splits(found))
}

/// `#{ text, cased, regexp }`, or `()` for none.
fn description(v: &Dynamic) -> Fallible<Option<DescriptionPattern>> {
    if v.is_unit() {
        return Ok(None);
    }
    let map = v.read_lock::<Map>().ok_or_else(|| script_error("description must be #{ text, cased, regexp }"))?;
    let flag = |name: &str| map.get(name).map_or(Ok(false), |f| f.as_bool().map_err(|_| script_error(format!("{name} must be true or false"))));
    let text = map.get("text").and_then(|t| t.read_lock::<rhai::ImmutableString>().map(|s| s.to_string()));
    Ok(Some(DescriptionPattern {
        text: text.ok_or_else(|| script_error("description needs its text"))?,
        cased: flag("cased")?,
        regexp: flag("regexp")?,
    }))
}

/// A commodity for the euro functions, which take any currency, in the Book or not.
fn euro_side(v: &Dynamic) -> Fallible<red_ale_core::CommodityRef> {
    if let Some(c) = v.read_lock::<super::CommodityHandle>() {
        return Ok(canonical(&c.id));
    }
    let text = v.read_lock::<rhai::ImmutableString>().ok_or_else(|| script_error(format!("expected a commodity, got {}", v.type_name())))?;
    parse_commodity(&text).map(|c| canonical(&c)).map_err(script_error)
}

/// `gnc:exchange-by-euro` (`report/commodity-utilities.scm:635-644`): through the euro's fixed rates when both
/// currencies have one, else `()`.
fn euro_convert(m: Monetary, currency: Dynamic) -> Fallible<Dynamic> {
    let to = euro_side(&currency)?;
    let from = canonical(&m.commodity);
    if !(is_euro_currency(&from) && is_euro_currency(&to)) {
        return Ok(Dynamic::UNIT);
    }
    let amount = m.amount.to_numeric().ok_or_else(|| script_error(format!("{} does not fit a 64-bit amount", m.amount)))?;
    let fail = |e: red_ale_core::NumericError| script_error(e.to_string());
    let euros = convert_to_euro(&from, amount).map_err(fail)?;
    let converted = convert_from_euro(&to, euros).map_err(fail)?;
    Ok(Dynamic::from(Monetary::new(to, numeric(converted)?)))
}

/// A core Numeric as a Rational.
pub(super) fn numeric(n: Numeric) -> Fallible<Rational> {
    Rational::from_numeric(n).map_err(|e| script_error(e.to_string()))
}
