//! Domain operations: the price database, the price editor and removing old prices.
//!
//! GnuCash keeps prices in lists per (commodity, currency), newest first with ties by GUID
//! (`compare_prices_by_date`, `5.17:libgnucash/engine/gnc-pricedb.cpp:674-692`). red-ale keeps them in book order and
//! sorts when it looks them up. See `design/notes/engine-commodities-prices-lots.md` §4.
//!
//! "The same day" is decided in the local time zone, as GnuCash decides it (`time64CanonicalDayTime`,
//! `5.17:libgnucash/engine/gnc-date.cpp:403-410`).

use std::cmp::Ordering;
use std::sync::Arc;

use chrono::{Datelike, NaiveDate};

use crate::ops::commodities::{canonical, require};
use crate::{Book, CommodityRef, Date, DomainError, DomainResult, Guid, Numeric, Price, Time64, TimeZone};

pub mod convert;

/// GnuCash's price sources, in precedence order: a lower index wins (`PriceSource`,
/// `5.17:libgnucash/engine/gnc-pricedb.h:169-182`; strings at `5.17:libgnucash/engine/gnc-pricedb.cpp:105-120`).
pub const SOURCES: [&str; 11] = [
    "user:price-editor",
    "Finance::Quote",
    "user:price",
    "user:xfer-dialog",
    "user:split-register",
    "user:split-import",
    "user:stock-split",
    "user:stock-transaction",
    "user:invoice-post",
    "temporary",
    "invalid",
];
/// The source the price editor gives a new price (`5.17:gnucash/gnome/dialog-price-editor.c:175`).
pub const EDITOR_SOURCE: &str = "user:price-editor";
/// `PRICE_SOURCE_FQ`.
pub const FQ_SOURCE: &str = "Finance::Quote";
/// `PRICE_SOURCE_USER_PRICE`.
pub const USER_PRICE_SOURCE: &str = "user:price";
/// `PRICE_SOURCE_XFER_DLG_VAL`: a rate the transfer dialog computed from the amount received.
pub const XFER_DIALOG_SOURCE: &str = "user:xfer-dialog";
/// `PRICE_SOURCE_SPLIT_REG`: a price recorded from a register transaction (`xaccTransRecordPrice`).
pub const SPLIT_REGISTER_SOURCE: &str = "user:split-register";
const INVALID_RANK: usize = 10;

/// A price source's precedence (lower wins). `gnc_price_set_source_string` matches only the known strings before
/// `invalid`; anything else, or no source, leaves a price at `PRICE_SOURCE_INVALID`
/// (`5.17:libgnucash/engine/gnc-pricedb.cpp:530-542`, `:90-96`).
pub fn source_rank(source: Option<&str>) -> usize {
    source.and_then(|s| SOURCES[..INVALID_RANK].iter().position(|k| *k == s)).unwrap_or(INVALID_RANK)
}

/// The local calendar day of a price's time, the day GnuCash compares prices by.
pub fn price_day(t: Time64, zone: &TimeZone) -> Option<Date> {
    zone.local_date(t)
}

/// `compare_prices_by_date`: newest first, then GUID ascending.
fn by_date(a: &Price, b: &Price) -> Ordering {
    b.time.cmp(&a.time).then_with(|| a.guid.cmp(&b.guid))
}

/// `gnc_commodity_compare`: by unique name `NAMESPACE::MNEMONIC`, byte order (`5.17:libgnucash/engine/gnc-commodity.cpp:1502`).
fn by_unique_name(a: &CommodityRef, b: &CommodityRef) -> Ordering {
    a.to_string().cmp(&b.to_string())
}

/// The prices of `commodity` in `currency` (any currency when `None`), newest first: `gnc_pricedb_get_prices`, which
/// looks in the forward direction only (`5.17:libgnucash/engine/gnc-pricedb.cpp:2047`). With `both_directions`, prices
/// of `currency` in `commodity` are merged in too, as GnuCash's lookups do (`pricedb_get_prices_internal`, `:1686`).
pub fn prices<'a>(book: &'a Book, commodity: &CommodityRef, currency: Option<&CommodityRef>, both_directions: bool) -> Vec<&'a Price> {
    let commodity = canonical(commodity);
    let currency = currency.map(canonical);
    let forward = |p: &Price| p.commodity == commodity && currency.as_ref().is_none_or(|c| p.currency == *c);
    let reverse = |p: &Price| both_directions && currency.as_ref().is_some_and(|c| p.commodity == *c && p.currency == commodity);
    let mut out: Vec<&Price> = book.prices.values().map(AsRef::as_ref).filter(|p| forward(p) || reverse(p)).collect();
    out.sort_by(|a, b| by_date(a, b));
    out
}

/// Every price, in the order GnuCash writes them: by commodity, then currency, then newest first
/// (`stable_price_traversal`, `5.17:libgnucash/engine/gnc-pricedb.cpp:2688-2729`).
pub fn all_prices(book: &Book) -> Vec<&Price> {
    let mut out: Vec<&Price> = book.prices.values().map(AsRef::as_ref).collect();
    out.sort_by(|a, b| a.commodity.cmp(&b.commodity).then_with(|| a.currency.cmp(&b.currency)).then_with(|| by_date(a, b)));
    out
}

/// How a lookup picks among prices around a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nearest {
    /// The closer of the newest price at or before the time and the oldest after it; a tie goes to the older
    /// (`gnc_pricedb_lookup_nearest_in_time64`).
    Nearest,
    /// The newest price at or before the time (`gnc_pricedb_lookup_nearest_before_t64`).
    Before,
    /// The nearest price on the same day (`gnc_pricedb_lookup_day_t64`).
    SameDay,
}

/// The latest price of `commodity` in `currency`, in either direction (`gnc_pricedb_lookup_latest`,
/// `5.17:libgnucash/engine/gnc-pricedb.cpp:1731`). A price found the other way round is returned as stored.
pub fn latest<'a>(book: &'a Book, commodity: &CommodityRef, currency: &CommodityRef) -> Option<&'a Price> {
    prices(book, commodity, Some(currency), true).into_iter().next()
}

/// The price of `commodity` in `currency` nearest to `t`, in either direction (`lookup_nearest_in_time` and
/// `gnc_pricedb_lookup_nearest_before_t64`, `5.17:libgnucash/engine/gnc-pricedb.cpp:2180-2322`).
pub fn nearest<'a>(
    book: &'a Book,
    commodity: &CommodityRef,
    currency: &CommodityRef,
    t: Time64,
    how: Nearest,
    zone: &TimeZone,
) -> Option<&'a Price> {
    let list = prices(book, commodity, Some(currency), true);
    match how {
        Nearest::Before => list.into_iter().find(|p| p.time <= t),
        Nearest::Nearest => nearest_in_time(&list, t, None),
        Nearest::SameDay => nearest_in_time(&list, t, Some(zone)),
    }
}

/// `lookup_nearest_in_time` over a newest-first list: `current` is the oldest price after `t` (the newest price when
/// none is after), `next` the newest at or before `t`. With `same_day`, only prices on `t`'s local day count.
fn nearest_in_time<'a>(list: &[&'a Price], t: Time64, same_day: Option<&TimeZone>) -> Option<&'a Price> {
    let split = list.iter().position(|p| p.time <= t);
    let current = match split {
        Some(0) => *list.first()?,
        Some(i) => list[i - 1],
        None => *list.last()?,
    };
    let on_day = |p: &Price| same_day.is_none_or(|zone| zone.same_day(p.time, t));
    let Some(next) = split.map(|i| list[i]) else {
        return Some(current).filter(|p| on_day(p));
    };
    let closer = if (current.time.0 - t.0).abs() < (next.time.0 - t.0).abs() { current } else { next };
    match (on_day(current), on_day(next)) {
        (true, true) => Some(closer),
        (true, false) => Some(current),
        (false, true) => Some(next),
        (false, false) => None,
    }
}

/// What adding a price did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AddResult {
    /// The price added, unless it was rejected or a duplicate.
    pub added: Option<Guid>,
    /// A same-day price it replaced.
    pub replaced: Option<Guid>,
    /// A same-day price with better precedence that made GnuCash reject it.
    pub rejected_by: Option<Guid>,
    /// An identical same-day price that made GnuCash drop it.
    pub duplicate_of: Option<Guid>,
}

/// Check a price's commodity and currency exist, and give them their canonical names.
fn resolve_sides(book: &Book, price: &mut Price) -> DomainResult<()> {
    price.commodity = require(book, &price.commodity)?;
    price.currency = require(book, &price.currency)?;
    if price.commodity == price.currency {
        return Err(DomainError::invalid(format!("a price of {} in itself means nothing", price.commodity)));
    }
    Ok(())
}

/// A source must be one GnuCash can store, so that what red-ale writes reads back the same in GnuCash.
pub fn check_source(source: Option<&str>) -> DomainResult<()> {
    match source {
        Some(s) if !SOURCES.contains(&s) => Err(DomainError::invalid(format!("unknown price source {s:?}; GnuCash knows {SOURCES:?}"))),
        _ => Ok(()),
    }
}

/// Add a price as `gnc_pricedb_add_price` does (`add_price`, `5.17:libgnucash/engine/gnc-pricedb.cpp:1013-1110`):
/// a same-day price of the pair, in either direction, is replaced if the new price's source has equal or better
/// precedence, and otherwise the new price is rejected; then a same-day price of equal value in the same direction
/// makes the new price a silent duplicate. "Same day" is a local day in `zone`.
pub fn add_price(book: &mut Book, mut price: Price, zone: &TimeZone) -> DomainResult<AddResult> {
    resolve_sides(book, &mut price)?;
    check_source(price.source.as_deref())?;
    if book.prices.contains_key(&price.guid) {
        return Err(DomainError::conflict(format!("price {} already exists", price.guid)));
    }
    let mut result = AddResult::default();
    let old = nearest(book, &price.commodity, &price.currency, price.time, Nearest::SameDay, zone).map(|p| (p.guid, source_rank(p.source.as_deref())));
    if let Some((old, old_rank)) = old {
        if source_rank(price.source.as_deref()) > old_rank {
            result.rejected_by = Some(old);
            return Ok(result);
        }
        book.prices.shift_remove(&old);
        result.replaced = Some(old);
    }
    result.duplicate_of = duplicate_of(book, &price, zone);
    if result.duplicate_of.is_none() {
        result.added = Some(price.guid);
        book.prices.insert(price.guid, Arc::new(price));
    }
    Ok(result)
}

/// `price_is_duplicate`: same day, equal value, same pair (`5.17:libgnucash/engine/gnc-pricedb.cpp:696-703`).
fn duplicate_of(book: &Book, price: &Price, zone: &TimeZone) -> Option<Guid> {
    prices(book, &price.commodity, Some(&price.currency), false)
        .into_iter()
        .find(|p| zone.same_day(p.time, price.time) && p.value.cmp_value(price.value) == Ordering::Equal)
        .map(|p| p.guid)
}

/// Changes to a price, as the price editor makes them. `None` keeps a field; `price_type: Some(None)` clears it.
#[derive(Debug, Clone, PartialEq, Default)]
#[allow(clippy::option_option)] // Absent, cleared and set are three different requests.
pub struct PriceChanges {
    /// The commodity priced.
    pub commodity: Option<CommodityRef>,
    /// The currency it is priced in.
    pub currency: Option<CommodityRef>,
    /// When it applies.
    pub time: Option<Time64>,
    /// Its source.
    pub source: Option<String>,
    /// Its type, such as `last`.
    pub price_type: Option<Option<String>>,
    /// Its value, kept exactly as given.
    pub value: Option<Numeric>,
}

/// Edit a price as the price editor does (`gui_to_price`, `5.17:gnucash/gnome/dialog-price-editor.c:253-327`).
/// Moving it to another pair or time re-runs the adding rules (`gnc_price_set_commodity` and friends remove and
/// re-add it, `5.17:libgnucash/engine/gnc-pricedb.cpp:457-518`), so a same-day price it beats is removed and returned.
/// Where GnuCash would then drop the edited price itself (a same-day price with better precedence, or an identical
/// one), red-ale refuses with `Conflict` instead of losing it.
pub fn update_price(book: &mut Book, guid: Guid, changes: &PriceChanges, zone: &TimeZone) -> DomainResult<Option<Guid>> {
    let mut price = book.prices.get(&guid).map(|p| p.as_ref().clone()).ok_or_else(|| DomainError::not_found("price", &guid))?;
    let moved = changes.commodity.as_ref().is_some_and(|c| canonical(c) != price.commodity)
        || changes.currency.as_ref().is_some_and(|c| canonical(c) != price.currency)
        || changes.time.is_some_and(|t| t != price.time);
    apply_changes(&mut price, changes);
    resolve_sides(book, &mut price)?;
    if changes.source.is_some() {
        check_source(price.source.as_deref())?;
    }
    if !moved {
        book.prices.insert(guid, Arc::new(price));
        return Ok(None);
    }
    readd_moved(book, price, zone)
}

fn apply_changes(price: &mut Price, changes: &PriceChanges) {
    if let Some(c) = &changes.commodity {
        price.commodity = c.clone();
    }
    if let Some(c) = &changes.currency {
        price.currency = c.clone();
    }
    if let Some(t) = changes.time {
        price.time = t;
    }
    if let Some(s) = &changes.source {
        price.source = Some(s.clone());
    }
    if let Some(t) = &changes.price_type {
        price.price_type = t.clone().filter(|t| !t.is_empty());
    }
    if let Some(v) = changes.value {
        price.value = v;
    }
}

/// Re-add a moved price in its old place in book order, applying the same-day rules against the other prices.
fn readd_moved(book: &mut Book, price: Price, zone: &TimeZone) -> DomainResult<Option<Guid>> {
    let guid = price.guid;
    let mut others = book.clone();
    others.prices.shift_remove(&guid);
    let other = nearest(&others, &price.commodity, &price.currency, price.time, Nearest::SameDay, zone).map(|p| (p.guid, source_rank(p.source.as_deref())));
    if let Some((other, rank)) = other {
        if source_rank(price.source.as_deref()) > rank {
            return Err(DomainError::conflict(format!("price {other} on that day has a source with better precedence")));
        }
        others.prices.shift_remove(&other);
    }
    if let Some(dup) = duplicate_of(&others, &price, zone) {
        return Err(DomainError::conflict(format!("price {dup} on that day has the same value")));
    }
    let replaced = other.map(|(g, _)| g);
    if let Some(r) = replaced {
        book.prices.shift_remove(&r);
    }
    book.prices.insert(guid, Arc::new(price));
    Ok(replaced)
}

/// Delete a price (`gnc_pricedb_remove_price`, `5.17:libgnucash/engine/gnc-pricedb.cpp:1219`).
pub fn delete_price(book: &mut Book, guid: Guid) -> DomainResult<Price> {
    book.prices.shift_remove(&guid).map(|p| p.as_ref().clone()).ok_or_else(|| DomainError::not_found("price", &guid))
}

/// Which prices "remove old prices" may delete, by source (`PriceRemoveSourceFlags`,
/// `5.17:libgnucash/engine/gnc-pricedb.h:381-388`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)] // Three independent flags, as GnuCash has them.
pub struct SourceFlags {
    /// `Finance::Quote` prices.
    pub fetched: bool,
    /// `user:price` prices.
    pub user: bool,
    /// Every other source, including the price editor's and unknown ones.
    pub app: bool,
}

impl SourceFlags {
    /// `check_one_price_date`'s source test (`5.17:libgnucash/engine/gnc-pricedb.cpp:1261-1283`).
    fn matches(self, source: Option<&str>) -> bool {
        let rank = source_rank(source);
        let fq = rank == source_rank(Some(FQ_SOURCE));
        let user = rank == source_rank(Some(USER_PRICE_SOURCE));
        (fq && self.fetched) || (user && self.user) || (!fq && !user && self.app)
    }
}

/// Which prices "remove old prices" keeps (`PriceRemoveKeepOptions`, `5.17:libgnucash/engine/gnc-pricedb.h:390-397`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// Delete every candidate.
    None,
    /// The last price of each ISO week number.
    LastOfWeek,
    /// The last price of each month number.
    LastOfMonth,
    /// The last price of each fiscal quarter number.
    LastOfQuarter,
    /// The last price of each fiscal year.
    LastOfPeriod,
    /// The dialog's "scaled": all of the last six months before the cutoff, weekly for the six before that, then
    /// monthly (`5.17:gnucash/gnome/dialog-price-edit-db.cpp:572-596`).
    Scaled,
}

/// The options of "remove old prices".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveOld {
    /// The commodities whose prices (in any currency, forward direction only) are considered.
    pub commodities: Vec<CommodityRef>,
    /// Only prices strictly before this time are candidates.
    pub cutoff: Time64,
    /// Which sources are candidates.
    pub sources: SourceFlags,
    /// What to keep.
    pub keep: Keep,
    /// The last day of a fiscal year; GnuCash's default is 31 December of the current year.
    pub fiscal_year_end: Date,
}

/// `gnc_pricedb_remove_old_prices` (`5.17:libgnucash/engine/gnc-pricedb.cpp:1549-1610`), with the dialog's
/// "scaled" option. Returns the deleted prices in the order they were deleted. Periods compare local dates in `zone`.
pub fn remove_old_prices(book: &mut Book, opts: &RemoveOld, zone: &TimeZone) -> DomainResult<Vec<Guid>> {
    let commodities = opts.commodities.iter().map(|c| require(book, c)).collect::<DomainResult<Vec<_>>>()?;
    let pass = |book: &mut Book, cutoff: Time64, keep: Keep| {
        let rule = Removal { sources: opts.sources, keep, fiscal_end: opts.fiscal_year_end, zone };
        remove_pass(book, &commodities, cutoff, &rule)
    };
    if opts.keep != Keep::Scaled {
        return Ok(pass(book, opts.cutoff, opts.keep));
    }
    let date = price_day(opts.cutoff, zone).ok_or_else(|| DomainError::invalid("cutoff out of range"))?;
    let six = date.add_months(-6);
    let mut removed = pass(book, zone.neutral_time(six), Keep::LastOfWeek);
    removed.extend(pass(book, zone.neutral_time(six.add_months(-6)), Keep::LastOfMonth));
    Ok(removed)
}

/// One run of `gnc_pricedb_remove_old_prices` and `gnc_pricedb_process_removal_list`
/// (`5.17:libgnucash/engine/gnc-pricedb.cpp:1439-1546`).
struct Removal<'a> {
    sources: SourceFlags,
    keep: Keep,
    fiscal_end: Date,
    zone: &'a TimeZone,
}

fn remove_pass(book: &mut Book, commodities: &[CommodityRef], cutoff: Time64, rule: &Removal) -> Vec<Guid> {
    let Removal { sources, keep, fiscal_end, zone } = *rule;
    let mut candidates: Vec<&Price> = book
        .prices
        .values()
        .map(AsRef::as_ref)
        .filter(|p| commodities.contains(&p.commodity) && sources.matches(p.source.as_deref()) && p.time < cutoff)
        .collect();
    candidates.sort_by(|a, b| {
        by_unique_name(&a.commodity, &b.commodity).then_with(|| by_unique_name(&a.currency, &b.currency)).then_with(|| by_date(a, b))
    });
    let mut kept: Option<&Price> = None;
    let doomed: Vec<Guid> = candidates
        .into_iter()
        .filter(|p| {
            if keep == Keep::None {
                return true;
            }
            let same_pair = kept.is_some_and(|k| k.commodity == p.commodity && k.currency == p.currency);
            let key = |t: Time64| period_key(price_day(t, zone), keep, fiscal_end);
            let remove = same_pair && kept.is_some_and(|k| key(k.time) == key(p.time));
            if !remove {
                kept = Some(p);
            }
            remove
        })
        .map(|p| p.guid)
        .collect();
    doomed.iter().for_each(|g| {
        book.prices.shift_remove(g);
    });
    doomed
}

/// The number two prices are compared by. Weeks, months and quarters are compared by number only, so the same
/// month of different years compares equal, as in GnuCash.
fn period_key(day: Option<Date>, keep: Keep, fiscal_end: Date) -> i64 {
    let Some(d) = day else { return 0 };
    match keep {
        Keep::LastOfWeek => NaiveDate::from_ymd_opt(d.year(), d.month(), d.day()).map_or(0, |n| i64::from(n.iso_week().week())),
        Keep::LastOfMonth => i64::from(d.month()),
        Keep::LastOfQuarter => fiscal_quarter(d.month(), fiscal_end.month() + 1),
        Keep::LastOfPeriod => i64::from(fiscal_year_end_year(d, fiscal_end)),
        Keep::None | Keep::Scaled => 0,
    }
}

/// `get_fiscal_quarter`, with the fiscal start month GnuCash computes as the end's month plus one (13 for a December
/// end) (`5.17:libgnucash/engine/gnc-pricedb.cpp:1415-1431`, `:1449-1454`).
fn fiscal_quarter(month: u32, fiscal_start: u32) -> i64 {
    let n = 22 - i64::from(fiscal_start) + i64::from(month);
    let rounded = if n % 3 == 0 { n } else { n + 3 - n % 3 };
    (rounded / 3) % 4 + 1
}

/// The year of the end of the fiscal year containing `d` (`gnc_gdate_set_fiscal_year_end`,
/// `5.17:libgnucash/engine/gnc-date.cpp:1674-1694`). A 29 February end falls on the 28th in other years.
fn fiscal_year_end_year(d: Date, fiscal_end: Date) -> i32 {
    let day = fiscal_end.day().min(crate::time::days_in_month(d.year(), fiscal_end.month()));
    let end = Date::from_ymd(d.year(), fiscal_end.month(), day).unwrap_or(d);
    if d > end { d.year() + 1 } else { d.year() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FixedEnv;

    fn usd() -> CommodityRef {
        CommodityRef::currency("USD")
    }

    fn eur() -> CommodityRef {
        CommodityRef::currency("EUR")
    }

    fn add_utc(b: &mut Book, p: Price) -> DomainResult<AddResult> {
        add_price(b, p, &TimeZone::utc())
    }

    fn book() -> Book {
        Book::new_empty(&mut FixedEnv::new(Time64(0), 1))
    }

    fn at(day: &str) -> Time64 {
        Date::parse(day).unwrap().neutral_time()
    }

    fn price(n: u8, commodity: CommodityRef, currency: CommodityRef, day: &str, source: &str, value: i64) -> Price {
        Price {
            guid: Guid::from_bytes([n; 16]),
            commodity,
            currency,
            time: at(day),
            source: Some(source.to_owned()),
            price_type: None,
            value: Numeric::new(value, 100),
        }
    }

    #[test]
    fn same_day_prices_follow_source_precedence_in_both_directions() {
        let mut b = book();
        let first = add_utc(&mut b, price(1, eur(), usd(), "2024-01-10", "user:xfer-dialog", 110)).unwrap();
        assert_eq!(first.added, Some(Guid::from_bytes([1; 16])));
        let reverse = add_utc(&mut b, price(2, usd(), eur(), "2024-01-10", EDITOR_SOURCE, 91)).unwrap();
        assert_eq!(reverse.replaced, Some(Guid::from_bytes([1; 16])));
        let fq = add_utc(&mut b, price(3, usd(), eur(), "2024-01-10", FQ_SOURCE, 92)).unwrap();
        assert_eq!(fq.rejected_by, Some(Guid::from_bytes([2; 16])));
        assert_eq!(b.prices.len(), 1);
        assert!(add_utc(&mut b, price(4, usd(), eur(), "2024-01-11", "my-source", 92)).is_err());
    }

    #[test]
    fn lookups_pick_like_gnucash() {
        let mut b = book();
        [("2024-01-01", 1), ("2024-01-03", 3), ("2024-01-05", 5)].iter().for_each(|(d, n)| {
            add_utc(&mut b, price(*n, eur(), usd(), d, EDITOR_SOURCE, i64::from(*n))).unwrap();
        });
        let guid = |p: Option<&Price>| p.map(|p| p.guid.as_bytes()[0]);
        assert_eq!(guid(latest(&b, &usd(), &eur())), Some(5));
        assert_eq!(guid(nearest(&b, &eur(), &usd(), at("2024-01-02"), Nearest::Nearest, &TimeZone::utc())), Some(1), "a tie goes to the older");
        assert_eq!(guid(nearest(&b, &eur(), &usd(), at("2024-01-04").plus_seconds(1), Nearest::Nearest, &TimeZone::utc())), Some(5));
        assert_eq!(guid(nearest(&b, &eur(), &usd(), at("2024-01-04"), Nearest::Before, &TimeZone::utc())), Some(3));
        assert_eq!(guid(nearest(&b, &eur(), &usd(), at("2023-12-01"), Nearest::Before, &TimeZone::utc())), None);
        assert_eq!(guid(nearest(&b, &eur(), &usd(), at("2023-12-01"), Nearest::Nearest, &TimeZone::utc())), Some(1));
        assert_eq!(guid(nearest(&b, &eur(), &usd(), at("2024-01-04"), Nearest::SameDay, &TimeZone::utc())), None);
        assert_eq!(guid(nearest(&b, &eur(), &usd(), at("2024-01-03").plus_seconds(3600), Nearest::SameDay, &TimeZone::utc())), Some(3));
    }

    #[test]
    fn moving_a_price_onto_a_better_one_is_refused() {
        let mut b = book();
        add_utc(&mut b, price(1, eur(), usd(), "2024-01-10", EDITOR_SOURCE, 110)).unwrap();
        add_utc(&mut b, price(2, eur(), usd(), "2024-01-11", FQ_SOURCE, 111)).unwrap();
        let onto = PriceChanges { time: Some(at("2024-01-10")), ..PriceChanges::default() };
        assert!(matches!(update_price(&mut b, Guid::from_bytes([2; 16]), &onto, &TimeZone::utc()), Err(DomainError::Conflict(_))));
        let replaced = update_price(&mut b, Guid::from_bytes([1; 16]), &PriceChanges { time: Some(at("2024-01-11")), ..PriceChanges::default() }, &TimeZone::utc());
        assert_eq!(replaced.unwrap(), Some(Guid::from_bytes([2; 16])));
    }

    #[test]
    fn remove_old_keeps_the_last_of_each_month_number() {
        let mut b = book();
        let days = ["2021-01-15", "2022-01-10", "2022-01-20", "2023-01-05", "2023-06-01", "2023-06-02"];
        (1u8..).zip(days).for_each(|(n, d)| {
            b.prices.insert(Guid::from_bytes([n; 16]), Arc::new(price(n, eur(), usd(), d, FQ_SOURCE, 1)));
        });
        let opts = RemoveOld {
            commodities: vec![eur()],
            cutoff: at("2024-01-01"),
            sources: SourceFlags { fetched: true, ..SourceFlags::default() },
            keep: Keep::LastOfMonth,
            fiscal_year_end: Date::from_ymd(2024, 12, 31).unwrap(),
        };
        let removed = remove_old_prices(&mut b, &opts, &TimeZone::utc()).unwrap();
        assert_eq!(removed.len(), 4);
        let mut left: Vec<String> = b.prices.values().map(|p| p.time.utc_date().unwrap().to_string()).collect();
        left.sort();
        assert_eq!(left, ["2023-01-05", "2023-06-02"]);
    }

    #[test]
    fn fiscal_quarters_and_years() {
        assert_eq!((1..=12).map(|m| fiscal_quarter(m, 13)).collect::<Vec<_>>(), [1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4]);
        assert_eq!(fiscal_quarter(4, 4), 1);
        let end = Date::from_ymd(2024, 3, 31).unwrap();
        assert_eq!(fiscal_year_end_year(Date::from_ymd(2023, 3, 31).unwrap(), end), 2023);
        assert_eq!(fiscal_year_end_year(Date::from_ymd(2023, 4, 1).unwrap(), end), 2024);
        assert!(SourceFlags { app: true, ..SourceFlags::default() }.matches(Some("weird")));
        assert!(!SourceFlags { app: true, ..SourceFlags::default() }.matches(Some(USER_PRICE_SOURCE)));
    }

    proptest::proptest! {
        /// Prices added one by one leave at most one price per pair (either direction) per day, each stored with the
        /// exact value given.
        #[test]
        fn adding_keeps_one_price_per_pair_and_day(adds in proptest::collection::vec((0u8..2, 0u32..4, 0usize..4, 1i64..10_000), 1..30)) {
            let mut b = book();
            let sources = [EDITOR_SOURCE, FQ_SOURCE, USER_PRICE_SOURCE, "user:xfer-dialog"];
            let added: Vec<(Guid, i64)> = (1u8..)
                .zip(adds)
                .filter_map(|(n, (dir, day, src, value))| {
                    let (c, cur) = if dir == 0 { (eur(), usd()) } else { (usd(), eur()) };
                    let mut p = price(n, c, cur, "2024-01-01", sources[src], value);
                    p.time = p.time.plus_seconds(i64::from(day) * 86_400 + i64::from(n));
                    add_utc(&mut b, p).unwrap().added.map(|g| (g, value))
                })
                .collect();
            let exact = added.iter().filter_map(|(g, v)| b.prices.get(g).map(|p| (p.value, *v))).all(|(p, v)| p.same_repr(Numeric::new(v, 100)));
            proptest::prop_assert!(exact);
            let mut days: Vec<Option<Date>> = b.prices.values().map(|p| price_day(p.time, &TimeZone::utc())).collect();
            let count = days.len();
            days.sort();
            days.dedup();
            proptest::prop_assert_eq!(days.len(), count);
        }
    }
}
