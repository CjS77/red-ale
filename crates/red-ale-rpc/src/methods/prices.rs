//! Methods: the price database, the price editor and removing old prices. See `design/api/prices.md`.

use red_ale_core::ops::prices::{self as ops, AddResult, Keep, Nearest, PriceChanges, RemoveOld, SourceFlags};
use red_ale_core::{Book, Date, Numeric, Price, Time64};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{Amount, CommodityId, Day, Id, Timestamp};

/// The prices methods.
pub fn methods() -> Vec<Method> {
    vec![
        method("price.add", "Add a price; a same-day price of the pair is replaced or wins by source precedence, as GnuCash does.", Effect::Write, add),
        method("price.get", "Describe one price.", Effect::Read, get),
        method("price.list", "List prices, by commodity, currency and date range, in GnuCash's order.", Effect::Read, list),
        method("price.latest", "The latest price of a commodity in a currency, either direction.", Effect::Read, latest),
        method("price.nearest", "The price of a commodity in a currency nearest to a time, before it, or on its day.", Effect::Read, nearest),
        method("price.update", "Edit a price's value, time, source, type, commodity or currency, as the price editor does.", Effect::Write, update),
        method("price.delete", "Delete a price.", Effect::Destructive, delete),
        method("price.removeOld", "Remove old prices, keeping the last of each week, month, quarter or fiscal year.", Effect::Destructive, remove_old),
    ]
}

/// A price.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PriceInfo {
    /// Identity.
    pub guid: Id,
    /// The commodity priced.
    pub commodity: CommodityId,
    /// The currency (or commodity) the value is in.
    pub currency: CommodityId,
    /// When the price applies.
    pub time: Timestamp,
    /// The UTC day of `time`, which the same-day rules use.
    pub date: Option<Day>,
    /// Where it came from, such as `user:price-editor` or `Finance::Quote`; kept as read even when GnuCash does not
    /// know it.
    pub source: Option<String>,
    /// Its type, such as `last`, `bid`, `ask`, `nav`, `transaction` or `unknown`.
    #[serde(rename = "type")]
    pub price_type: Option<String>,
    /// Units of `currency` per unit of `commodity`, exactly as stored.
    pub value: Amount,
}

fn info(p: &Price) -> PriceInfo {
    PriceInfo {
        guid: p.guid.into(),
        commodity: p.commodity.clone().into(),
        currency: p.currency.clone().into(),
        time: p.time.into(),
        date: ops::price_day(p.time).map(Day),
        source: p.source.clone(),
        price_type: p.price_type.clone(),
        value: p.value.into(),
    }
}

fn find(book: &Book, guid: Id) -> Result<&Price, RpcError> {
    book.prices.get(&guid.0).map(AsRef::as_ref).ok_or_else(|| RpcError::not_found("price", guid.0))
}

/// Exactly one of a date (the price editor's: 10:59 UTC that day) or an exact time.
fn when(date: Option<Day>, time: Option<Timestamp>) -> Result<Option<Time64>, RpcError> {
    match (date, time) {
        (Some(_), Some(_)) => Err(RpcError::invalid_params("give date or time, not both")),
        (Some(d), None) => Ok(Some(d.0.neutral_time())),
        (None, t) => Ok(t.map(|t| t.0)),
    }
}

/// Params of `price.add`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AddParams {
    /// The commodity priced.
    pub commodity: CommodityId,
    /// The currency (or commodity) the value is in.
    pub currency: CommodityId,
    /// Units of `currency` per unit of `commodity`; stored exactly as given.
    pub value: Amount,
    /// The day it applies, stored at 10:59 UTC as the price editor does. Give this or `time`.
    #[serde(default)]
    pub date: Option<Day>,
    /// The exact time it applies. Give this or `date`.
    #[serde(default)]
    pub time: Option<Timestamp>,
    /// One of GnuCash's price sources; default `user:price-editor`.
    #[serde(default)]
    pub source: Option<String>,
    /// Its type, such as `last`, `bid`, `ask`, `nav` or `unknown`.
    #[serde(default, rename = "type")]
    pub price_type: Option<String>,
}

/// What `price.add` did.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddOutcome {
    /// `added`, `replaced` (added, removing a same-day price), `rejected` (a same-day price with a better source is
    /// kept) or `duplicate` (an identical same-day price exists; nothing added).
    pub outcome: &'static str,
    /// The price added.
    pub price: Option<PriceInfo>,
    /// The same-day price removed.
    pub replaced: Option<Id>,
    /// The price that made this one rejected or a duplicate.
    pub existing: Option<Id>,
}

fn outcome(book: &Book, r: &AddResult) -> AddOutcome {
    let name = match (r.added, r.replaced, r.rejected_by) {
        (_, _, Some(_)) => "rejected",
        (None, _, None) => "duplicate",
        (Some(_), Some(_), None) => "replaced",
        (Some(_), None, None) => "added",
    };
    AddOutcome {
        outcome: name,
        price: r.added.and_then(|g| book.prices.get(&g)).map(|p| info(p)),
        replaced: r.replaced.map(Id),
        existing: r.rejected_by.or(r.duplicate_of).map(Id),
    }
}

fn add(ctx: &mut Context, p: AddParams) -> Result<AddOutcome, RpcError> {
    let time = when(p.date, p.time)?.ok_or_else(|| RpcError::invalid_params("give date or time"))?;
    let result = ctx.mutate(|book, env| {
        let price = Price {
            guid: env.new_guid(),
            commodity: p.commodity.0,
            currency: p.currency.0,
            time,
            source: Some(p.source.unwrap_or_else(|| ops::EDITOR_SOURCE.to_owned())),
            price_type: p.price_type.filter(|t| !t.is_empty()),
            value: p.value.0,
        };
        ops::add_price(book, price)
    })?;
    Ok(outcome(ctx.book()?, &result))
}

/// Params naming one price.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PriceParams {
    /// The price's GUID.
    pub price: Id,
}

fn get(ctx: &mut Context, p: PriceParams) -> Result<PriceInfo, RpcError> {
    find(ctx.book()?, p.price).map(info)
}

/// Params of `price.list`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListParams {
    /// Only prices of this commodity.
    #[serde(default)]
    pub commodity: Option<CommodityId>,
    /// Only prices in this currency (needs `commodity`).
    #[serde(default)]
    pub currency: Option<CommodityId>,
    /// Also prices of `currency` in `commodity` (needs both).
    #[serde(default)]
    pub both_directions: bool,
    /// Only prices on or after this UTC day.
    #[serde(default)]
    pub from: Option<Day>,
    /// Only prices on or before this UTC day.
    #[serde(default)]
    pub to: Option<Day>,
}

fn list(ctx: &mut Context, p: ListParams) -> Result<Vec<PriceInfo>, RpcError> {
    let book = ctx.book()?;
    let selected = match (&p.commodity, &p.currency) {
        (Some(c), cur) => ops::prices(book, &c.0, cur.as_ref().map(|c| &c.0), p.both_directions),
        (None, None) if !p.both_directions => ops::all_prices(book),
        _ => return Err(RpcError::invalid_params("currency and bothDirections need commodity")),
    };
    let from = p.from.map(|d| d.0.start_utc());
    let to = p.to.map(|d| d.0.end_utc());
    Ok(selected.into_iter().filter(|x| from.is_none_or(|f| x.time >= f) && to.is_none_or(|t| x.time <= t)).map(info).collect())
}

/// Params of `price.latest`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PairParams {
    /// The commodity priced.
    pub commodity: CommodityId,
    /// The currency wanted.
    pub currency: CommodityId,
}

/// A price found by a lookup, which looks in both directions.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoundPrice {
    /// The price as stored.
    pub price: PriceInfo,
    /// The price is of `currency` in `commodity`, the other way round from the request.
    pub reversed: bool,
    /// Units of the requested currency per unit of the requested commodity: the value, or its exact inverse when
    /// reversed (null if that value is zero).
    pub rate: Option<Amount>,
}

fn found(p: &Price, requested: &red_ale_core::CommodityRef) -> FoundPrice {
    let reversed = p.commodity != red_ale_core::ops::commodities::canonical(requested);
    let rate = if reversed { invert(p.value) } else { Some(p.value) };
    FoundPrice { price: info(p), reversed, rate: rate.map(Amount) }
}

/// `gnc_numeric_invert`: swap numerator and denominator, keeping the denominator positive.
fn invert(n: Numeric) -> Option<Numeric> {
    match n.num() {
        0 => None,
        num if num < 0 => Some(Numeric::new(-n.denom(), -num)),
        num => Some(Numeric::new(n.denom(), num)),
    }
}

fn latest(ctx: &mut Context, p: PairParams) -> Result<Option<FoundPrice>, RpcError> {
    let book = ctx.book()?;
    Ok(ops::latest(book, &p.commodity.0, &p.currency.0).map(|x| found(x, &p.commodity.0)))
}

/// How `price.nearest` chooses.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NearestMode {
    /// The closer of the prices either side of the time; a tie goes to the older.
    #[default]
    Nearest,
    /// The newest price at or before the time.
    Before,
    /// The nearest price on the same UTC day, if any.
    SameDay,
}

/// Params of `price.nearest`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NearestParams {
    /// The commodity priced.
    pub commodity: CommodityId,
    /// The currency wanted.
    pub currency: CommodityId,
    /// A day, meaning 10:59 UTC on it. Give this or `time`.
    #[serde(default)]
    pub date: Option<Day>,
    /// An exact time. Give this or `date`.
    #[serde(default)]
    pub time: Option<Timestamp>,
    /// `nearest` (default), `before` or `sameDay`.
    #[serde(default)]
    pub mode: NearestMode,
}

fn nearest(ctx: &mut Context, p: NearestParams) -> Result<Option<FoundPrice>, RpcError> {
    let t = when(p.date, p.time)?.ok_or_else(|| RpcError::invalid_params("give date or time"))?;
    let how = match p.mode {
        NearestMode::Nearest => Nearest::Nearest,
        NearestMode::Before => Nearest::Before,
        NearestMode::SameDay => Nearest::SameDay,
    };
    let book = ctx.book()?;
    Ok(ops::nearest(book, &p.commodity.0, &p.currency.0, t, how).map(|x| found(x, &p.commodity.0)))
}

/// Read a present field as `Some(value)`, so that `null` (clear) and absent (keep) differ.
#[allow(clippy::option_option, clippy::unnecessary_wraps)]
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// Params of `price.update`. Absent fields are kept.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(clippy::option_option)]
pub struct UpdateParams {
    /// The price to edit.
    pub price: Id,
    /// New commodity.
    #[serde(default)]
    pub commodity: Option<CommodityId>,
    /// New currency.
    #[serde(default)]
    pub currency: Option<CommodityId>,
    /// New day (10:59 UTC on it).
    #[serde(default)]
    pub date: Option<Day>,
    /// New exact time.
    #[serde(default)]
    pub time: Option<Timestamp>,
    /// New source: one of GnuCash's price sources.
    #[serde(default)]
    pub source: Option<String>,
    /// New type; null clears it.
    #[serde(default, rename = "type", deserialize_with = "present")]
    pub price_type: Option<Option<String>>,
    /// New value, stored exactly.
    #[serde(default)]
    pub value: Option<Amount>,
}

/// What `price.update` did.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOutcome {
    /// The price as it now is.
    pub price: PriceInfo,
    /// A same-day price it replaced after moving.
    pub replaced: Option<Id>,
}

fn update(ctx: &mut Context, p: UpdateParams) -> Result<UpdateOutcome, RpcError> {
    let changes = PriceChanges {
        commodity: p.commodity.map(|c| c.0),
        currency: p.currency.map(|c| c.0),
        time: when(p.date, p.time)?,
        source: p.source,
        price_type: p.price_type,
        value: p.value.map(|v| v.0),
    };
    let replaced = ctx.mutate(|book, _| ops::update_price(book, p.price.0, &changes))?;
    Ok(UpdateOutcome { price: info(find(ctx.book()?, p.price)?), replaced: replaced.map(Id) })
}

fn delete(ctx: &mut Context, p: PriceParams) -> Result<PriceInfo, RpcError> {
    let deleted = ctx.mutate(|book, _| ops::delete_price(book, p.price.0))?;
    Ok(info(&deleted))
}

/// Which old prices are kept.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum KeepOption {
    /// None: every candidate is removed.
    None,
    /// The last of each ISO week number (GnuCash's default).
    #[default]
    LastOfWeek,
    /// The last of each month number.
    LastOfMonth,
    /// The last of each fiscal quarter number.
    LastOfQuarter,
    /// The last of each fiscal year.
    LastOfPeriod,
    /// Everything in the six months before the cutoff, then weekly for six months, then monthly.
    Scaled,
}

/// Which price sources `price.removeOld` may remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RemoveSource {
    /// Prices fetched online (`Finance::Quote`).
    Fetched,
    /// Prices entered as `user:price`.
    User,
    /// Every other source, including the price editor's.
    App,
}

fn default_sources() -> Vec<RemoveSource> {
    vec![RemoveSource::Fetched]
}

/// Params of `price.removeOld`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RemoveOldParams {
    /// The commodities whose prices (in any currency) are considered; at least one.
    pub commodities: Vec<CommodityId>,
    /// Only prices before 10:59 UTC on this day are considered.
    pub before: Day,
    /// Which sources may be removed; default `["fetched"]`.
    #[serde(default = "default_sources")]
    pub sources: Vec<RemoveSource>,
    /// What to keep; default `lastOfWeek`.
    #[serde(default)]
    pub keep: KeepOption,
    /// The last day of a fiscal year, for quarters and periods; default 31 December of the current year.
    #[serde(default)]
    pub fiscal_year_end: Option<Day>,
}

/// What `price.removeOld` removed.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoveOldResult {
    /// The prices removed.
    pub removed: Vec<Id>,
}

fn remove_old(ctx: &mut Context, p: RemoveOldParams) -> Result<RemoveOldResult, RpcError> {
    if p.commodities.is_empty() || p.sources.is_empty() {
        return Err(RpcError::invalid_params("give at least one commodity and one source"));
    }
    let keep = match p.keep {
        KeepOption::None => Keep::None,
        KeepOption::LastOfWeek => Keep::LastOfWeek,
        KeepOption::LastOfMonth => Keep::LastOfMonth,
        KeepOption::LastOfQuarter => Keep::LastOfQuarter,
        KeepOption::LastOfPeriod => Keep::LastOfPeriod,
        KeepOption::Scaled => Keep::Scaled,
    };
    let sources = SourceFlags {
        fetched: p.sources.contains(&RemoveSource::Fetched),
        user: p.sources.contains(&RemoveSource::User),
        app: p.sources.contains(&RemoveSource::App),
    };
    let removed = ctx.mutate(|book, env| {
        // gnc_pricedb_remove_old_prices defaults the fiscal year end to 31 December this year (gnc-pricedb.cpp:1595-1600).
        let this_year = env.now().utc_date().map_or(1970, Date::year);
        let fiscal_year_end = p.fiscal_year_end.map(|d| d.0).or_else(|| Date::from_ymd(this_year, 12, 31)).unwrap_or(p.before.0);
        let opts = RemoveOld { commodities: p.commodities.into_iter().map(|c| c.0).collect(), cutoff: p.before.0.neutral_time(), sources, keep, fiscal_year_end };
        ops::remove_old_prices(book, &opts)
    })?;
    Ok(RemoveOldResult { removed: removed.into_iter().map(Id).collect() })
}
