//! The `Book` handle (`design/reports.md` §5, "Book").

use red_ale_core::book_options::{company_name, uses_split_action_for_num, uses_trading_accounts};
use red_ale_core::currencies::{convert_from_euro, convert_to_euro, is_euro_currency};
use red_ale_core::ops::accounts::descendants_sorted;
use red_ale_core::ops::commodities::{canonical, lookup};
use red_ale_core::ops::prices::convert::{PriceAt, convert_balance, latest_before_any_currency, nearest_any_currency, rate};
use red_ale_core::ops::prices::prices;
use red_ale_core::ops::scheduled::cash_flow;
use red_ale_core::ops::transactions::find::cross_commodity_splits;
use red_ale_core::{AccountType, Guid, Numeric, Time64};
use rhai::{Array, Dynamic, Engine, INT, Map};

use super::args::{accounts_arg, commodity_arg, optional_commodity_arg, optional_time_arg};
use super::{BookHandle, Fallible, script_error};
use crate::number::{Monetary, Rational};
use crate::wire::parse_commodity;

pub(super) fn register(engine: &mut Engine) {
    engine.register_type_with_name::<BookHandle>("Book");
    engine.register_fn("to_string", |_: &mut BookHandle| "Book".to_owned());
    engine.register_fn("root", |b: &mut BookHandle| b.account(b.0.book().root));
    engine.register_fn("account", account);
    engine.register_fn("accounts", |b: &mut BookHandle| -> Array {
        b.0.book()
            .descendants(b.0.book().root)
            .iter()
            .map(|a| Dynamic::from(b.account(a.guid)))
            .collect()
    });
    engine.register_fn("accounts_of_types", accounts_of_types);
    engine.register_fn("commodity", |b: &mut BookHandle, id: &str| -> Dynamic {
        let found = parse_commodity(id)
            .ok()
            .and_then(|id| lookup(b.0.book(), &canonical(&id)).map(|c| c.id.clone()));
        found.map_or(Dynamic::UNIT, |id| Dynamic::from(b.commodity(&id)))
    });
    engine.register_fn("uses_trading_accounts", |b: &mut BookHandle| uses_trading_accounts(b.0.book()));
    // `qof-book-use-split-action-for-num-field`: the book's num field source.
    engine.register_fn("uses_split_action_for_num", |b: &mut BookHandle| uses_split_action_for_num(b.0.book()));
    engine.register_fn("company_name", |b: &mut BookHandle| company_name(b.0.book()).to_owned());
    engine.register_fn("prices", book_prices);
    engine.register_fn("prices_any_currency", prices_any_currency);
    engine.register_fn("price_rate", price_rate);
    engine.register_fn("price_convert", price_convert);
    engine.register_fn("commodity_splits", commodity_splits);
    engine.register_fn("split_query", super::split_query::split_query);
    engine.register_fn("is_euro_currency", |c: Dynamic| -> Fallible<bool> {
        euro_side(&c).map(|c| is_euro_currency(&c))
    });
    engine.register_fn("euro_convert", euro_convert);
    engine.register_fn("sx_cashflow", sx_cashflow);
}

/// `book.account(ref)`: by GUID or full name, `()` when there is none.
fn account(b: &mut BookHandle, reference: &str) -> Dynamic {
    let book = b.0.book();
    let found = Guid::parse(reference.trim())
        .ok()
        .and_then(|g| book.account(g))
        .or_else(|| book.account_by_full_name(reference));
    found.map_or(Dynamic::UNIT, |a| Dynamic::from(b.account(a.guid)))
}

/// `gnc:filter-accountlist-type` over the root's descendants in display order (`report/report-utilities.scm:178-181`).
fn accounts_of_types(b: &mut BookHandle, types: Array) -> Fallible<Array> {
    let types: Vec<AccountType> = types
        .iter()
        .map(|t| {
            let name = t
                .read_lock::<rhai::ImmutableString>()
                .ok_or_else(|| script_error("account types are names such as \"ASSET\""))?;
            name.parse::<AccountType>().map_err(|e| script_error(e.to_string()))
        })
        .collect::<Fallible<_>>()?;
    let book = b.0.book();
    let found = descendants_sorted(book, book.root)
        .into_iter()
        .filter(|a| types.contains(&a.account_type));
    Ok(found.map(|a| Dynamic::from(b.account(a.guid))).collect())
}

/// `gnc-pricedb-get-prices`: forward only, newest first, ties by GUID.
fn book_prices(b: &mut BookHandle, commodity: Dynamic, currency: Dynamic) -> Fallible<Array> {
    let book = b.0.book();
    let (commodity, currency) = (commodity_arg(book, &commodity)?, commodity_arg(book, &currency)?);
    let found = prices(book, &commodity, Some(&currency), false);
    Ok(found.into_iter().map(|p| Dynamic::from(b.price(p.guid))).collect())
}

/// `gnc-pricedb-lookup-nearest-in-time-any-currency-t64`, `-nearest-before-any-currency-t64` and
/// `-latest-any-currency` (`5.17:libgnucash/engine/gnc-pricedb.cpp:1955-2001`): the price of `commodity` in each other
/// commodity nearest to `when`, the newest at or before it, or the newest at or before the run's now; newest first.
/// Each price is as stored, quoted either way round.
fn prices_any_currency(b: &mut BookHandle, commodity: Dynamic, when: INT, mode: &str) -> Fallible<Array> {
    let book = b.0.book();
    let commodity = commodity_arg(book, &commodity)?;
    let found = match price_at(b, when, mode)? {
        PriceAt::Nearest(t) => nearest_any_currency(book, &commodity, t),
        PriceAt::Before(t) | PriceAt::Latest { now: t } => latest_before_any_currency(book, &commodity, t),
    };
    Ok(found.into_iter().map(|p| Dynamic::from(b.price(p.guid))).collect())
}

/// Which price a lookup takes: `"nearest"`, `"before"` or `"latest"`; `"latest"` reads the run's now, never the clock.
fn price_at(b: &BookHandle, when: INT, mode: &str) -> Fallible<PriceAt> {
    match mode {
        "nearest" => Ok(PriceAt::Nearest(Time64(when))),
        "before" => Ok(PriceAt::Before(Time64(when))),
        "latest" => Ok(PriceAt::Latest { now: b.0.now() }),
        other => Err(script_error(format!(
            "price mode must be \"nearest\", \"before\" or \"latest\", got {other:?}"
        ))),
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
    let amount = m
        .amount
        .to_numeric()
        .ok_or_else(|| script_error(format!("{} does not fit a 64-bit amount", m.amount)))?;
    let converted = convert_balance(book, amount, &m.commodity, &to, price_at(b, when, mode)?).map_err(|e| script_error(e.to_string()))?;
    Ok(Monetary::new(to, numeric(converted)?))
}

/// `gnc-get-match-commodity-splits`.
fn commodity_splits(b: &mut BookHandle, accounts: Array, until: Dynamic, commodity: Dynamic, sorted: bool) -> Fallible<Array> {
    let book = b.0.book();
    let accounts = accounts_arg(book, &accounts)?;
    let commodity = optional_commodity_arg(book, &commodity)?;
    Ok(b.splits(cross_commodity_splits(
        book,
        &accounts,
        optional_time_arg(&until)?,
        commodity.as_ref(),
        sorted,
    )))
}

/// A commodity for the euro functions, which take any currency, in the Book or not.
fn euro_side(v: &Dynamic) -> Fallible<red_ale_core::CommodityRef> {
    if let Some(c) = v.read_lock::<super::CommodityHandle>() {
        return Ok(canonical(&c.id));
    }
    let text = v
        .read_lock::<rhai::ImmutableString>()
        .ok_or_else(|| script_error(format!("expected a commodity, got {}", v.type_name())))?;
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
    let amount = m
        .amount
        .to_numeric()
        .ok_or_else(|| script_error(format!("{} does not fit a 64-bit amount", m.amount)))?;
    let fail = |e: red_ale_core::NumericError| script_error(e.to_string());
    let euros = convert_to_euro(&from, amount).map_err(fail)?;
    let converted = convert_from_euro(&to, euros).map_err(fail)?;
    Ok(Dynamic::from(Monetary::new(to, numeric(converted)?)))
}

/// `gnc-sx-all-instantiate-cashflow-all` (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1968-1976`): what every
/// enabled scheduled transaction adds to each account from `from` to `to`, by account GUID, in the account's
/// commodity, debits positive, computed from the model without creating anything (`ops::scheduled::cash_flow`).
/// GnuCash's Guile wrapper takes each time's local date, so both days count whole (`gnc_time64_to_GDate`,
/// `5.17:bindings/guile/expressions.i:27-32`), and hands each sum to Scheme as an exact rational (`:46-70`). Formulas
/// are read with every variable 0; what fails counts as 0, as there.
fn sx_cashflow(b: &mut BookHandle, from: INT, to: INT) -> Fallible<Map> {
    let zone = b.0.zone();
    let date = |t: INT| {
        zone.local_date(Time64(t))
            .ok_or_else(|| script_error(format!("{t} is not a time a date can be taken of")))
    };
    let flow = cash_flow(b.0.book(), date(from)?, date(to)?, b.0.formula_format()).map_err(|e| script_error(e.to_string()))?;
    flow.amounts
        .into_iter()
        .map(|(guid, amount)| Ok((guid.to_string().into(), Dynamic::from(numeric(amount)?))))
        .collect()
}

/// A core Numeric as a Rational.
pub(super) fn numeric(n: Numeric) -> Fallible<Rational> {
    Rational::from_numeric(n).map_err(|e| script_error(e.to_string()))
}
