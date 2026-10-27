//! Converting between commodities through the price database: GnuCash's `get_nearest_price` and the
//! `gnc_pricedb_convert_balance_*` functions (`5.17:libgnucash/engine/gnc-pricedb.cpp:2326-2586`), and the
//! "any currency" lookups their indirect path stands on (`:1770-2001`).
//!
//! A rate comes from a price of the pair, in either direction (a *direct* price), or else from two prices that share a
//! third commodity (an *indirect* price). See `design/notes/engine-commodities-prices-lots.md` §4.6.

use std::collections::BTreeMap;

use super::{Nearest, by_date, latest, nearest};
use crate::ops::commit::commodity_fraction;
use crate::ops::commodities::canonical;
use crate::{
    Book, CommodityRef, DENOM_AUTO, DenomMode, DomainError, DomainResult, How, Numeric, NumericError, Price, Round, Time64, TimeZone,
};

/// Which price a conversion takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceAt {
    /// The price nearest to the time; a tie goes to the older (`gnc_pricedb_get_nearest_price`,
    /// `gnc_pricedb_convert_balance_nearest_price_t64`).
    Nearest(Time64),
    /// The newest price at or before the time (`gnc_pricedb_get_nearest_before_price`,
    /// `gnc_pricedb_convert_balance_nearest_before_price_t64`).
    Before(Time64),
    /// The newest price (`gnc_pricedb_get_latest_price`, `gnc_pricedb_convert_balance_latest_price`). GnuCash splits
    /// this in two: a direct price is the newest even when it lies after `now`, an indirect one is the newest at or
    /// before `now` (`gnc_pricedb_lookup_latest_any_currency`, `:1954-1960`, which reads the clock).
    Latest {
        /// The current time.
        now: Time64,
    },
}

/// A price with its two commodities resolved as GnuCash's commodity table resolves them.
#[derive(Debug, Clone)]
struct Quote<'a> {
    price: &'a Price,
    commodity: CommodityRef,
    currency: CommodityRef,
}

impl<'a> Quote<'a> {
    fn new(price: &'a Price) -> Self {
        Self {
            price,
            commodity: canonical(&price.commodity),
            currency: canonical(&price.currency),
        }
    }

    /// The commodity on the far side from `target`.
    fn other(&self, target: &CommodityRef) -> &CommodityRef {
        if self.commodity == *target {
            &self.currency
        } else {
            &self.commodity
        }
    }

    fn time(&self) -> Time64 {
        self.price.time
    }
}

/// The price database as GnuCash holds it: one list per (commodity, currency), newest first, ties by GUID.
fn pair_lists(book: &Book) -> BTreeMap<(CommodityRef, CommodityRef), Vec<Quote<'_>>> {
    let mut lists: BTreeMap<(CommodityRef, CommodityRef), Vec<Quote<'_>>> = BTreeMap::new();
    book.prices
        .values()
        .map(|p| Quote::new(p))
        .for_each(|q| lists.entry((q.commodity.clone(), q.currency.clone())).or_default().push(q));
    lists.values_mut().for_each(|l| l.sort_by(|a, b| by_date(a.price, b.price)));
    lists
}

/// `price_list_scan_any_currency` (`:1770-1820`) on one list: the oldest price at or after `t` and the newest
/// before it, or the oldest price when all are at or after `t`.
fn scan_pair<'a, 'q>(list: &'q [Quote<'a>], t: Time64) -> &'q [Quote<'a>] {
    match list.iter().position(|q| q.time() < t) {
        Some(0) => &list[..1],
        Some(i) => &list[i - 1..=i],
        None => list.len().checked_sub(1).map_or(&list[..0], |last| &list[last..]),
    }
}

/// The prices near `t` of every pair `target` is in, newest first (the scan of
/// `gnc_pricedb_lookup_nearest_in_time_any_currency_t64` and its siblings, `:1962-2001`).
fn scan_any_currency<'a>(book: &'a Book, target: &CommodityRef, t: Time64) -> Vec<Quote<'a>> {
    let mut found: Vec<Quote<'a>> = pair_lists(book)
        .into_iter()
        .filter(|((commodity, currency), _)| commodity == target || currency == target)
        .flat_map(|(_, list)| scan_pair(&list, t).to_vec())
        .collect();
    found.sort_by(|a, b| by_date(a.price, b.price));
    found
}

/// `latest_before` (`:1822-1847`): per other commodity, the newest price at or before `t`.
fn latest_before<'a>(quotes: Vec<Quote<'a>>, target: &CommodityRef, t: Time64) -> Vec<Quote<'a>> {
    let mut seen: Vec<CommodityRef> = Vec::new();
    quotes
        .into_iter()
        .filter(|q| {
            let repeat = (q.commodity == *target && seen.contains(&q.currency)) || (q.currency == *target && seen.contains(&q.commodity));
            if t < q.time() || repeat {
                return false;
            }
            seen.push(q.other(target).clone());
            true
        })
        .collect()
}

/// `nearest_to` and `add_nearest_price` (`:1849-1950`): per other commodity, the nearer of the newest price at or
/// before `t` and the oldest after it, a tie to the older; or the oldest after it when there is none before.
fn nearest_to<'a>(quotes: &[Quote<'a>], target: &CommodityRef, t: Time64) -> Vec<Quote<'a>> {
    let mut newer: Vec<Quote<'a>> = Vec::new();
    let mut out: Vec<Quote<'a>> = Vec::new();
    quotes.iter().for_each(|q| {
        let Some(i) = newer.iter().position(|n| n.other(target) == q.other(target)) else {
            newer.push(q.clone());
            if q.time() <= t {
                out.push(q.clone());
            }
            return;
        };
        let held = newer[i].time();
        if held <= t {
            return;
        }
        if q.time() <= t {
            let closer = if i128::from(held.0) - i128::from(t.0) < i128::from(t.0) - i128::from(q.time().0) {
                &newer[i]
            } else {
                q
            };
            out.push(closer.clone());
        }
        newer[i] = q.clone();
    });
    out.extend(newer.into_iter().filter(|n| n.time() >= t));
    out.sort_by(|a, b| by_date(a.price, b.price));
    out
}

/// The prices of `commodity` in any other commodity nearest to `t`, one per other commodity, newest first
/// (`gnc_pricedb_lookup_nearest_in_time_any_currency_t64`, `5.17:libgnucash/engine/gnc-pricedb.cpp:1962-1980`).
/// A price at exactly `t` can be listed twice, as GnuCash lists it.
pub fn nearest_any_currency<'a>(book: &'a Book, commodity: &CommodityRef, t: Time64) -> Vec<&'a Price> {
    let target = canonical(commodity);
    nearest_to(&scan_any_currency(book, &target, t), &target, t)
        .into_iter()
        .map(|q| q.price)
        .collect()
}

/// The newest price of `commodity` in each other commodity at or before `t`, newest first
/// (`gnc_pricedb_lookup_nearest_before_any_currency_t64`, `:1982-2001`).
pub fn latest_before_any_currency<'a>(book: &'a Book, commodity: &CommodityRef, t: Time64) -> Vec<&'a Price> {
    let target = canonical(commodity);
    latest_before(scan_any_currency(book, &target, t), &target, t)
        .into_iter()
        .map(|q| q.price)
        .collect()
}

fn any_currency<'a>(book: &'a Book, commodity: &CommodityRef, at: PriceAt) -> Vec<Quote<'a>> {
    let target = canonical(commodity);
    match at {
        PriceAt::Nearest(t) => nearest_to(&scan_any_currency(book, &target, t), &target, t),
        PriceAt::Before(t) | PriceAt::Latest { now: t } => latest_before(scan_any_currency(book, &target, t), &target, t),
    }
}

/// `extract_common_prices` (`:2333-2373`): the first pair, in from-major order, of a price of `from` and a price of
/// `to` that share a third commodity.
fn common_prices<'a>(
    from_prices: &[Quote<'a>],
    to_prices: &[Quote<'a>],
    from: &CommodityRef,
    to: &CommodityRef,
) -> Option<(Quote<'a>, Quote<'a>)> {
    let third = |c: &CommodityRef| c != from && c != to;
    let shares = |f: &Quote<'_>, t: &Quote<'_>| {
        ((t.commodity == f.commodity || t.commodity == f.currency) && third(&t.commodity))
            || ((t.currency == f.commodity || t.currency == f.currency) && third(&t.currency))
    };
    from_prices
        .iter()
        .find_map(|f| to_prices.iter().find(|t| shares(f, t)).map(|t| (f.clone(), t.clone())))
}

/// `convert_price` (`:2375-2406`): `to` per `from` from a price of `from` and a price of `to` in a third commodity.
fn rate_through(from: &CommodityRef, to: &CommodityRef, p1: &Quote<'_>, p2: &Quote<'_>) -> Result<Numeric, NumericError> {
    let how = How::new(DenomMode::Reduce, Round::Bankers);
    let (v1, v2) = (p1.price.value, p2.price.value);
    if (p1.commodity == *from && p2.commodity == *to) || (p1.currency == *from && p2.currency == *to) {
        let price = v2.div(v1, DENOM_AUTO, how)?;
        return if p1.currency == *from { Ok(price) } else { price.invert() };
    }
    let price = v1.mul(v2, DENOM_AUTO, how)?;
    if p1.currency == *from { price.invert() } else { Ok(price) }
}

/// `indirect_price_conversion` (`:2408-2449`); zero when no pair of prices shares a third commodity.
fn indirect_rate(book: &Book, from: &CommodityRef, to: &CommodityRef, at: PriceAt) -> Result<Numeric, NumericError> {
    let from_prices = any_currency(book, from, at);
    if from_prices.is_empty() {
        return Ok(Numeric::ZERO);
    }
    let to_prices = any_currency(book, to, at);
    common_prices(&from_prices, &to_prices, from, to).map_or(Ok(Numeric::ZERO), |(p1, p2)| rate_through(from, to, &p1, &p2))
}

/// `direct_price_conversion` (`:2452-2477`); zero when the pair has no price.
fn direct_rate(book: &Book, from: &CommodityRef, to: &CommodityRef, at: PriceAt) -> Result<Numeric, NumericError> {
    // Only the same-day lookup reads the zone; these never do.
    let utc = TimeZone::utc();
    let price = match at {
        PriceAt::Latest { .. } => latest(book, from, to),
        PriceAt::Before(t) => nearest(book, from, to, t, Nearest::Before, &utc),
        PriceAt::Nearest(t) => nearest(book, from, to, t, Nearest::Nearest, &utc),
    };
    match price {
        None => Ok(Numeric::ZERO),
        Some(p) if canonical(&p.commodity) == *from => Ok(p.value),
        Some(p) => p.value.invert(),
    }
}

/// How many units of `to` one unit of `from` is worth: GnuCash's `get_nearest_price` (`:2479-2501`), behind
/// `gnc_pricedb_get_nearest_price`, `_nearest_before_price` and `_latest_price`. One for the same commodity; else a
/// direct price, inverted when it is quoted the other way; else an indirect price; zero when there is none. Reduced.
/// An error is GnuCash's error value (an overflow, or a zero price to divide by).
pub fn rate(book: &Book, from: &CommodityRef, to: &CommodityRef, at: PriceAt) -> Result<Numeric, NumericError> {
    let (from, to) = (canonical(from), canonical(to));
    if from == to {
        return Ok(Numeric::ONE);
    }
    let direct = direct_rate(book, &from, &to, at)?;
    let rate = if direct.is_zero() {
        indirect_rate(book, &from, &to, at)?
    } else {
        direct
    };
    Ok(rate.reduce())
}

/// `amount` of `from` in `to`: GnuCash's `gnc_pricedb_convert_balance_*` (`convert_amount_at_date`, `:2529-2586`).
/// The amount times [`rate`], rounded to `to`'s fraction with banker's rounding; zero when there is no price, or
/// when the rate is GnuCash's error value. A zero amount comes back unchanged.
pub fn convert_balance(book: &Book, amount: Numeric, from: &CommodityRef, to: &CommodityRef, at: PriceAt) -> DomainResult<Numeric> {
    if amount.is_zero() {
        return Ok(amount);
    }
    let fraction = commodity_fraction(book, &canonical(to)).ok_or_else(|| DomainError::not_found("commodity", to))?;
    let rate = rate(book, from, to, at).unwrap_or(Numeric::ZERO);
    Ok(amount.mul(rate, fraction, How::new(DenomMode::Exact, Round::Bankers))?)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use proptest::prelude::*;

    use super::*;
    use crate::{Date, FixedEnv, Guid};

    fn cur(code: &str) -> CommodityRef {
        CommodityRef::currency(code)
    }

    fn day(d: &str) -> Time64 {
        Date::parse(d).unwrap().neutral_time()
    }

    fn book(prices: &[(u8, &str, &str, &str, i64, i64)]) -> Book {
        let mut b = Book::new_empty(&mut FixedEnv::new(Time64(0), 1));
        prices.iter().for_each(|(n, c, currency, d, num, den)| {
            let p = Price {
                guid: Guid::from_bytes([*n; 16]),
                commodity: cur(c),
                currency: cur(currency),
                time: day(d),
                source: None,
                price_type: None,
                value: Numeric::new(*num, *den),
            };
            b.prices.insert(p.guid, Arc::new(p));
        });
        b
    }

    fn r(n: i64, d: i64) -> Numeric {
        Numeric::new(n, d)
    }

    #[test]
    fn a_direct_price_wins_and_is_inverted_when_quoted_the_other_way() {
        let b = book(&[
            (1, "EUR", "USD", "2024-01-10", 110, 100),
            (2, "GBP", "USD", "2024-01-10", 125, 100),
            (3, "EUR", "GBP", "2024-01-10", 9, 10),
        ]);
        let at = PriceAt::Before(day("2024-02-01"));
        assert_eq!(rate(&b, &cur("EUR"), &cur("EUR"), at), Ok(Numeric::ONE));
        assert!(rate(&b, &cur("EUR"), &cur("GBP"), at).unwrap().same_repr(r(9, 10)));
        assert!(rate(&b, &cur("GBP"), &cur("EUR"), at).unwrap().same_repr(r(10, 9)));
        assert!(rate(&b, &cur("USD"), &cur("EUR"), at).unwrap().same_repr(r(10, 11)), "reduced");
    }

    #[test]
    fn an_indirect_price_goes_through_a_shared_commodity_either_way_round() {
        let b = book(&[(1, "EUR", "USD", "2024-01-10", 110, 100), (2, "GBP", "USD", "2024-01-10", 125, 100)]);
        let at = PriceAt::Nearest(day("2024-01-10"));
        assert!(
            rate(&b, &cur("EUR"), &cur("GBP"), at).unwrap().same_repr(r(22, 25)),
            "both priced in USD: 1.10 / 1.25"
        );
        assert!(rate(&b, &cur("GBP"), &cur("EUR"), at).unwrap().same_repr(r(25, 22)));
        let mixed = book(&[(1, "USD", "EUR", "2024-01-10", 90, 100), (2, "GBP", "USD", "2024-01-10", 125, 100)]);
        assert!(
            rate(&mixed, &cur("EUR"), &cur("GBP"), at).unwrap().same_repr(r(8, 9)),
            "1 / (0.90 × 1.25)"
        );
        assert!(rate(&mixed, &cur("GBP"), &cur("EUR"), at).unwrap().same_repr(r(9, 8)));
        assert!(
            rate(&mixed, &cur("GBP"), &cur("JPY"), at).unwrap().is_zero(),
            "no price of JPY at all"
        );
    }

    #[test]
    fn modes_pick_their_prices() {
        let b = book(&[
            (1, "EUR", "USD", "2024-01-10", 110, 100),
            (2, "EUR", "USD", "2024-01-20", 120, 100),
            (3, "EUR", "USD", "2099-01-01", 2, 1),
        ]);
        let eur_usd = |at| rate(&b, &cur("EUR"), &cur("USD"), at).unwrap();
        assert_eq!(eur_usd(PriceAt::Before(day("2024-01-19"))), r(11, 10));
        assert_eq!(eur_usd(PriceAt::Nearest(day("2024-01-19"))), r(6, 5));
        assert_eq!(eur_usd(PriceAt::Nearest(day("2024-01-15"))), r(11, 10), "a tie goes to the older");
        assert!(eur_usd(PriceAt::Before(day("2024-01-01"))).is_zero());
        assert_eq!(
            eur_usd(PriceAt::Nearest(day("2024-01-01"))),
            r(11, 10),
            "nearest looks forward when nothing is before"
        );
        assert_eq!(
            eur_usd(PriceAt::Latest { now: day("2024-06-01") }),
            r(2, 1),
            "a direct latest price may lie in the future"
        );
    }

    #[test]
    fn latest_goes_indirect_only_up_to_now() {
        let b = book(&[
            (1, "EUR", "USD", "2024-01-10", 110, 100),
            (2, "GBP", "USD", "2024-01-10", 125, 100),
            (3, "GBP", "USD", "2099-01-01", 2, 1),
        ]);
        let latest = |now| rate(&b, &cur("EUR"), &cur("GBP"), PriceAt::Latest { now: day(now) }).unwrap();
        assert_eq!(latest("2024-06-01"), r(22, 25), "the future GBP price is not seen");
        assert_eq!(latest("2100-01-01"), r(11, 20));
    }

    #[test]
    fn any_currency_lookups_keep_one_price_per_other_commodity() {
        let b = book(&[
            (1, "EUR", "USD", "2024-01-10", 110, 100),
            (2, "EUR", "USD", "2024-01-20", 120, 100),
            (3, "USD", "EUR", "2024-01-12", 90, 100),
            (4, "GBP", "EUR", "2024-01-30", 115, 100),
            (5, "EUR", "JPY", "2024-01-18", 160, 1),
        ]);
        let ids = |ps: Vec<&Price>| ps.iter().map(|p| p.guid.to_string()[..2].to_owned()).collect::<Vec<_>>();
        assert_eq!(
            ids(latest_before_any_currency(&b, &cur("EUR"), day("2024-01-19"))),
            ["05", "03"],
            "USD: the newer of both directions"
        );
        assert_eq!(
            ids(nearest_any_currency(&b, &cur("EUR"), day("2024-01-19"))),
            ["04", "02", "05"],
            "GBP only after; USD nearer after"
        );
        let at_a_price = nearest_any_currency(&b, &cur("EUR"), day("2024-01-18"));
        assert_eq!(ids(at_a_price), ["04", "02", "05", "05"], "a price at the time is listed twice");
    }

    #[test]
    fn conversion_rounds_to_the_target_with_bankers_rounding() {
        let b = book(&[(1, "EUR", "GBP", "2024-01-10", 1, 8)]);
        let at = PriceAt::Before(day("2024-02-01"));
        let convert = |amount| convert_balance(&b, amount, &cur("EUR"), &cur("GBP"), at).unwrap();
        assert!(convert(r(100, 100)).same_repr(r(12, 100)), "0.125 rounds to even");
        assert!(convert(r(300, 100)).same_repr(r(38, 100)), "0.375 rounds to even");
        assert!(convert(r(0, 1000)).same_repr(r(0, 1000)), "zero comes back unchanged");
        assert!(
            convert_balance(&b, r(1, 1), &cur("EUR"), &cur("JPY"), at).unwrap().is_zero(),
            "no price"
        );
    }

    proptest! {
        #[test]
        fn rates_invert_exactly(a in 1i64..100_000, b in 1i64..100_000, direct in any::<bool>()) {
            let bk = if direct {
                book(&[(1, "EUR", "GBP", "2024-01-10", a, 1000)])
            } else {
                book(&[(1, "EUR", "USD", "2024-01-10", a, 1000), (2, "GBP", "USD", "2024-01-10", b, 1000)])
            };
            let at = PriceAt::Nearest(day("2024-03-01"));
            let there = rate(&bk, &cur("EUR"), &cur("GBP"), at).unwrap();
            let back = rate(&bk, &cur("GBP"), &cur("EUR"), at).unwrap();
            prop_assert_eq!(there.mul(back, DENOM_AUTO, How::REDUCE_NEVER).unwrap(), Numeric::ONE);
        }
    }
}
