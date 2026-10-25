//! Transfers between accounts in different commodities, and the prices they record.
//!
//! [`transfer`] does what GnuCash's Transfer dialog does on OK (`5.17:gnucash/gnome-utils/dialog-transfer.cpp`):
//! it creates a two-split transaction from an amount and either a rate or the amount received, then records the
//! rate in the price database. [`record_prices`] is `xaccTransRecordPrice`, which the register calls after
//! committing a transaction with splits in priced accounts. See `design/notes/engine-accounts-transactions.md` §10.
//!
//! The price database helpers here (same-day and nearest lookups, source ranks) follow `gnc-pricedb.cpp`; the
//! prices package owns the price methods.

use std::sync::Arc;

use super::post;
use crate::currencies::{euro_rate, is_euro_currency};
use crate::ops::commit::{NewSplit, NewTransaction, account_scu, commodity_fraction};
use crate::{AccountType, Book, CommodityRef, Date, DomainError, DomainResult, Env, Guid, How, Numeric, Price, Round, Time64, Transaction};

/// `PRICE_TYPE_TRN`.
pub const PRICE_TYPE_TRANSACTION: &str = "transaction";
/// Price source for a rate typed in the transfer dialog (`PRICE_SOURCE_USER_PRICE`).
pub const SOURCE_USER_PRICE: &str = "user:price";
/// Price source for a rate computed from the amount received (`PRICE_SOURCE_XFER_DLG_VAL`).
pub const SOURCE_XFER_DIALOG: &str = "user:xfer-dialog";
/// Price source for prices recorded from a register transaction (`PRICE_SOURCE_SPLIT_REG`).
pub const SOURCE_SPLIT_REGISTER: &str = "user:split-register";

/// Price sources in preference order, best first (`PriceSource`, `5.17:libgnucash/engine/gnc-pricedb.h:165-182`).
const SOURCES: [&str; 11] = [
    "user:price-editor",
    "Finance::Quote",
    SOURCE_USER_PRICE,
    SOURCE_XFER_DIALOG,
    SOURCE_SPLIT_REGISTER,
    "user:split-import",
    "user:stock-split",
    "user:stock-transaction",
    "user:invoice-post",
    "temporary",
    "invalid",
];

/// A source's rank: lower is preferred. An unknown or missing source ranks as `invalid`, as
/// `gnc_price_set_source_string` leaves it (`5.17:libgnucash/engine/gnc-pricedb.cpp:90-96`, `:530-542`).
pub fn source_rank(source: Option<&str>) -> usize {
    source.and_then(|s| SOURCES[..SOURCES.len() - 2].iter().position(|x| *x == s)).unwrap_or(SOURCES.len() - 1)
}

/// `gnc_numeric_invert`.
fn invert(n: Numeric) -> Numeric {
    if n.num() < 0 { Numeric::new(-n.denom(), -n.num()) } else { Numeric::new(n.denom(), n.num()) }
}

/// A price between `a` and `b` in either direction, with `reversed` true when it is stored as `b` priced in `a`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    /// The price.
    pub guid: Guid,
    /// The price is stored the other way round.
    pub reversed: bool,
}

fn between<'a>(book: &'a Book, a: &'a CommodityRef, b: &'a CommodityRef) -> impl Iterator<Item = (&'a Price, bool)> {
    book.prices.values().map(AsRef::as_ref).filter_map(move |p| match (&p.commodity, &p.currency) {
        (c, cur) if c == a && cur == b => Some((p, false)),
        (c, cur) if c == b && cur == a => Some((p, true)),
        _ => None,
    })
}

/// The price nearest to `t`, either side; on a tie the older (`lookup_nearest_in_time`,
/// `5.17:libgnucash/engine/gnc-pricedb.cpp:2179-2280`). With `same_day`, only prices on `t`'s calendar day (UTC here,
/// local time in GnuCash) count.
pub fn lookup_nearest(book: &Book, a: &CommodityRef, b: &CommodityRef, t: Time64, same_day: bool) -> Option<Found> {
    between(book, a, b)
        .filter(|(p, _)| !same_day || p.time.utc_date() == t.utc_date())
        .min_by_key(|(p, _)| ((p.time.0 - t.0).abs(), p.time, p.guid))
        .map(|(p, reversed)| Found { guid: p.guid, reversed })
}

/// `round_price` in the transfer dialog: to 1/10000 between two currencies, else to the currency side's fraction
/// × 10000 (`5.17:gnucash/gnome-utils/dialog-transfer.cpp:201-217`).
fn round_price(book: &Book, a: &CommodityRef, b: &CommodityRef, v: Numeric) -> DomainResult<Numeric> {
    let denom = match (a.is_currency(), b.is_currency()) {
        (true, true) => 10_000,
        (false, true) => commodity_fraction(book, b).unwrap_or(100) * 10_000,
        (true, false) => commodity_fraction(book, a).unwrap_or(100) * 10_000,
        (false, false) => return Ok(v),
    };
    Ok(v.to_denom(denom, Round::HalfUp)?)
}

/// What happened to the price database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceChange {
    /// A new price was added.
    Created(Guid),
    /// An existing price on the same day was updated.
    Updated(Guid),
    /// An existing price on the same day was kept: it is preferred or already equal.
    Kept(Guid),
}

/// How the amount received is decided.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rate {
    /// One unit of the from commodity buys this much of the to commodity.
    Rate(Numeric),
    /// The amount received, in the to commodity.
    ToAmount(Numeric),
    /// The rate the dialog fills in: the euro's fixed rates between two euro currencies, else the price database's
    /// price on the day, else the nearest one.
    FromPriceDb,
}

/// A transfer, as the Transfer dialog takes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Transfer {
    /// The account the amount leaves.
    pub from: Guid,
    /// The account it reaches.
    pub to: Guid,
    /// The amount, in the from account's currency.
    pub amount: Numeric,
    /// The posted date.
    pub date: Date,
    /// Description.
    pub description: Option<String>,
    /// Num.
    pub num: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// Memo, put on both splits.
    pub memo: Option<String>,
    /// How the amount received is decided when the commodities differ.
    pub rate: Rate,
    /// Record the rate in the price database, as the dialog always does.
    pub record_price: bool,
    /// The user's default currency, which decides the direction a new price between two currencies is stored in.
    pub default_currency: Option<CommodityRef>,
}

/// The result of [`transfer`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransferOutcome {
    /// The transaction created.
    pub transaction: Guid,
    /// The amount received, in the to commodity.
    pub to_amount: Numeric,
    /// The rate used: to commodity per from commodity.
    pub rate: Option<Numeric>,
    /// What the price database did, when the commodities differ and a price was recorded.
    pub price: Option<PriceChange>,
}

struct Leg {
    from: CommodityRef,
    to: CommodityRef,
    scu: i64,
}

fn check_accounts(book: &Book, x: &Transfer) -> DomainResult<Leg> {
    if x.from == x.to {
        return Err(DomainError::invalid("you can't transfer from and to the same account"));
    }
    super::check_postable(book, x.from)?;
    super::check_postable(book, x.to)?;
    let commodity = |g: Guid| book.account(g).and_then(|a| a.commodity.clone()).ok_or_else(|| DomainError::invalid(format!("account {g} has no commodity")));
    let (from, to) = (commodity(x.from)?, commodity(x.to)?);
    if !from.is_currency() {
        return Err(DomainError::invalid("you can't transfer from a non-currency account; reverse the accounts and negate the amount"));
    }
    if x.amount.is_zero() {
        return Err(DomainError::invalid("you must enter an amount to transfer"));
    }
    Ok(Leg { from, to, scu: account_scu(book, x.to)? })
}

/// The rate and its price source, and the amount received (`gnc_xfer_update_to_amount`, `:975-1011`, and
/// `gnc_xfer_to_amount_update_cb`, `:1040-1056`).
fn decide(book: &Book, x: &Transfer, leg: &Leg) -> DomainResult<(Numeric, &'static str, Numeric)> {
    let by_rate = |rate: Numeric| -> DomainResult<(Numeric, &'static str, Numeric)> {
        if !rate.is_positive() {
            return Err(DomainError::invalid("the exchange rate must be positive"));
        }
        Ok((rate, SOURCE_USER_PRICE, x.amount.mul(rate, leg.scu, How::round(Round::HalfUp))?))
    };
    let (rate, source, to_amount) = match x.rate {
        Rate::Rate(r) => by_rate(r)?,
        Rate::ToAmount(to) => (to.div(x.amount, crate::DENOM_AUTO, How::REDUCE_NEVER)?, SOURCE_XFER_DIALOG, to),
        Rate::FromPriceDb => by_rate(prefilled_rate(book, x, leg)?)?,
    };
    if to_amount.is_zero() {
        return Err(DomainError::invalid("the amount received is zero"));
    }
    Ok((rate, source, to_amount))
}

/// The rate the dialog fills in (`gnc_xfer_dialog_set_price_auto`, `:367-404`; `gnc_xfer_dialog_update_price`,
/// `:306-334`).
fn prefilled_rate(book: &Book, x: &Transfer, leg: &Leg) -> DomainResult<Numeric> {
    if let (Some(f), Some(t)) = (euro_rate(&leg.from), euro_rate(&leg.to)) {
        return Ok(t.div(f, crate::DENOM_AUTO, How::REDUCE_NEVER)?);
    }
    let time = x.date.neutral_time();
    let found = lookup_nearest(book, &leg.from, &leg.to, time, true)
        .or_else(|| lookup_nearest(book, &leg.from, &leg.to, time, false))
        .ok_or_else(|| DomainError::invalid(format!("no price of {} in {}; give a rate or the amount received", leg.from, leg.to)))?;
    let value = book.prices.get(&found.guid).map_or(Numeric::ZERO, |p| p.value);
    Ok(if found.reversed { invert(value) } else { value })
}

/// Transfer between two accounts as the dialog does (`create_transaction`, `:1473-1530`): the transaction is in the
/// from currency, the from split has value and amount −amount, the to split value +amount and amount the amount
/// received, the memo goes on both splits. Then the price is recorded (`create_price`, `:1625-1640`) unless both
/// commodities are euro currencies or `record_price` is off.
pub fn transfer(book: &mut Book, env: &mut dyn Env, x: &Transfer) -> DomainResult<TransferOutcome> {
    let leg = check_accounts(book, x)?;
    let same = leg.from == leg.to;
    let decided = if same {
        if !matches!(x.rate, Rate::FromPriceDb) {
            return Err(DomainError::invalid("both accounts hold the same commodity; give no rate or amount received"));
        }
        None
    } else {
        Some(decide(book, x, &leg)?)
    };
    let to_amount = decided.map_or(x.amount, |(_, _, to)| to);
    let with_memo = |s: NewSplit| match &x.memo {
        Some(m) => s.with_memo(m.clone()),
        None => s,
    };
    let new = NewTransaction {
        description: x.description.clone(),
        num: x.num.clone(),
        notes: x.notes.clone(),
        splits: vec![
            with_memo(NewSplit::new(x.from, x.amount.neg()).with_amount(x.amount.neg())),
            with_memo(NewSplit::new(x.to, x.amount).with_amount(to_amount)),
        ],
        ..NewTransaction::new(leg.from.clone(), x.date)
    };
    let transaction = post(book, env, new)?;
    let price = match decided {
        Some((rate, source, _)) if x.record_price && !(is_euro_currency(&leg.from) && is_euro_currency(&leg.to)) => {
            Some(create_price(book, env, &leg, x, rate, source)?)
        }
        _ => None,
    };
    Ok(TransferOutcome { transaction, to_amount, rate: decided.map(|(r, _, _)| r), price })
}

/// `create_price` (`:1625-1640`): update a same-day price (`update_price`, `:1550-1588`) or add one (`new_price`,
/// `:1591-1620`).
fn create_price(book: &mut Book, env: &mut dyn Env, leg: &Leg, x: &Transfer, rate: Numeric, source: &str) -> DomainResult<PriceChange> {
    let time = x.date.neutral_time();
    if let Some(found) = lookup_nearest(book, &leg.from, &leg.to, time, true) {
        return update_price(book, leg, found, rate, source, time);
    }
    let (mut commodity, mut currency, mut value) = (leg.from.clone(), leg.to.clone(), rate.abs());
    let default_first = x.default_currency.as_ref().is_some_and(|d| *d == leg.from && *d != leg.to);
    if (leg.from.is_currency() && !leg.to.is_currency()) || default_first {
        std::mem::swap(&mut commodity, &mut currency);
        value = invert(value);
    }
    let price = Price {
        guid: env.new_guid(),
        commodity,
        currency,
        time,
        source: Some(source.to_owned()),
        price_type: Some(PRICE_TYPE_TRANSACTION.to_owned()),
        value,
    };
    let guid = price.guid;
    book.prices.insert(guid, Arc::new(price));
    Ok(PriceChange::Created(guid))
}

/// `update_price`: keep an existing price whose source is preferred or whose rounded value is the same; otherwise
/// set its time, type and unrounded value (inverted when it is stored the other way round), keeping its source.
fn update_price(book: &mut Book, leg: &Leg, found: Found, rate: Numeric, source: &str, time: Time64) -> DomainResult<PriceChange> {
    let Some(existing) = book.prices.get(&found.guid) else { return Err(DomainError::not_found("price", &found.guid)) };
    if source_rank(existing.source.as_deref()) < source_rank(Some(source)) {
        return Ok(PriceChange::Kept(found.guid));
    }
    let value = if found.reversed { invert(rate) } else { rate };
    let rounded_existing = round_price(book, &leg.from, &leg.to, existing.value)?;
    if round_price(book, &leg.from, &leg.to, value)?.same_value(rounded_existing) {
        return Ok(PriceChange::Kept(found.guid));
    }
    if let Some(p) = book.prices.get_mut(&found.guid) {
        let p = Arc::make_mut(p);
        p.time = time;
        p.price_type = Some(PRICE_TYPE_TRANSACTION.to_owned());
        p.value = value;
    }
    Ok(PriceChange::Updated(found.guid))
}

/// `xaccAccountIsPriced`: stock, mutual fund and currency accounts (`5.17:libgnucash/engine/Account.cpp:4539-4548`).
fn is_priced(t: AccountType) -> bool {
    matches!(t, AccountType::Stock | AccountType::Mutual | AccountType::Currency)
}

/// `xaccTransRecordPrice` (`5.17:libgnucash/engine/Transaction.cpp:2713-2807`): for each split in a priced account
/// with a non-zero amount, record value/amount as the price of the account's commodity in the transaction currency
/// on the posted date. A same-day price that is equal, or whose source is preferred (except `user:xfer-dialog`, which
/// `user:split-register` overrides), is kept; otherwise it is updated, source included. Values are rounded to the
/// currency's fraction × 10000.
pub fn record_prices(book: &mut Book, env: &mut dyn Env, guid: Guid, source: &str) -> DomainResult<Vec<PriceChange>> {
    let t: Transaction = super::get(book, guid)?.clone();
    t.splits
        .iter()
        .filter(|s| !s.amount.is_zero())
        .filter_map(|s| s.account.and_then(|a| book.account(a)).filter(|a| is_priced(a.account_type)).and_then(|a| a.commodity.clone()).map(|c| (s, c)))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(s, commodity)| record_price(book, env, &t, s.value.div(s.amount, crate::DENOM_AUTO, How::EXACT)?, &commodity, source))
        .collect()
}

fn record_price(book: &mut Book, env: &mut dyn Env, t: &Transaction, value: Numeric, comm: &CommodityRef, source: &str) -> DomainResult<PriceChange> {
    let curr = &t.currency;
    let mut fraction = commodity_fraction(book, curr).unwrap_or(100);
    let Some(found) = lookup_nearest(book, comm, curr, t.date_posted, true) else {
        let price = Price {
            guid: env.new_guid(),
            commodity: comm.clone(),
            currency: curr.clone(),
            time: t.date_posted,
            source: Some(source.to_owned()),
            price_type: Some(PRICE_TYPE_TRANSACTION.to_owned()),
            value: value.to_denom(fraction * 10_000, Round::HalfUp)?,
        };
        let guid = price.guid;
        book.prices.insert(guid, Arc::new(price));
        return Ok(PriceChange::Created(guid));
    };
    let existing = book.prices.get(&found.guid).map(|p| (p.value, source_rank(p.source.as_deref())));
    let Some((existing_value, old_rank)) = existing else { return Err(DomainError::not_found("price", &found.guid)) };
    let compared = if found.reversed { invert(value) } else { value };
    let register_over_dialog = old_rank == source_rank(Some(SOURCE_XFER_DIALOG)) && source == SOURCE_SPLIT_REGISTER;
    if compared.same_value(existing_value) || (old_rank < source_rank(Some(source)) && !register_over_dialog) {
        return Ok(PriceChange::Kept(found.guid));
    }
    if found.reversed {
        fraction = commodity_fraction(book, comm).unwrap_or(100);
    }
    let new_value = compared.to_denom(fraction * 10_000, Round::HalfUp)?;
    if let Some(p) = book.prices.get_mut(&found.guid) {
        let p = Arc::make_mut(p);
        p.time = t.date_posted;
        p.source = Some(source.to_owned());
        p.price_type = Some(PRICE_TYPE_TRANSACTION.to_owned());
        p.value = new_value;
    }
    Ok(PriceChange::Updated(found.guid))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{fixture, usd};
    use super::*;

    fn xfer(f: &super::super::tests::Fixture, rate: Rate) -> Transfer {
        Transfer {
            from: f.bank,
            to: f.eur,
            amount: usd(11000),
            date: Date::from_ymd(2024, 6, 3).unwrap(),
            description: Some("To euro account".into()),
            num: None,
            notes: None,
            memo: Some("fx".into()),
            rate,
            record_price: true,
            default_currency: None,
        }
    }

    #[test]
    fn transfer_by_rate_records_a_user_price() {
        let mut f = fixture();
        let x = xfer(&f, Rate::Rate(Numeric::new(9091, 10_000)));
        let out = transfer(&mut f.book, &mut f.env, &x).unwrap();
        assert!(out.to_amount.same_repr(usd(10000)), "110 × 0.9091 = 100.001, rounded to cents");
        let t = f.book.transaction(out.transaction).unwrap();
        assert_eq!(t.splits[0].account, Some(f.eur), "the to split is the debit and sorts first");
        assert!(t.splits.iter().all(|s| s.memo.as_deref() == Some("fx")));
        let Some(PriceChange::Created(p)) = out.price else { panic!("{:?}", out.price) };
        let p = f.book.prices[&p].clone();
        assert_eq!((p.commodity.mnemonic.as_str(), p.currency.mnemonic.as_str()), ("USD", "EUR"));
        assert!(p.value.same_repr(Numeric::new(9091, 10_000)));
        assert_eq!(p.source.as_deref(), Some(SOURCE_USER_PRICE));
        // A second transfer on the same day by amount received keeps it: user:xfer-dialog ranks below user:price.
        let x = xfer(&f, Rate::ToAmount(usd(9900)));
        let again = transfer(&mut f.book, &mut f.env, &x).unwrap();
        assert_eq!(again.price, Some(PriceChange::Kept(p.guid)));
    }

    #[test]
    fn transfer_from_the_price_db_and_refusals() {
        let mut f = fixture();
        let x = xfer(&f, Rate::FromPriceDb);
        assert!(transfer(&mut f.book, &mut f.env, &x).is_err(), "no price to fill in");
        let x = xfer(&f, Rate::ToAmount(usd(10000)));
        transfer(&mut f.book, &mut f.env, &x).unwrap();
        let mut later = xfer(&f, Rate::FromPriceDb);
        later.date = Date::from_ymd(2024, 6, 9).unwrap();
        let out = transfer(&mut f.book, &mut f.env, &later).unwrap();
        assert!(out.to_amount.same_repr(usd(10000)));
        assert!(matches!(out.price, Some(PriceChange::Created(_))), "a nearest price fills the rate and a new day's price is added");
        let mut same = xfer(&f, Rate::Rate(Numeric::ONE));
        same.to = f.food;
        assert!(transfer(&mut f.book, &mut f.env, &same).is_err());
        let mut back = xfer(&f, Rate::Rate(Numeric::ONE));
        back.to = back.from;
        assert!(transfer(&mut f.book, &mut f.env, &back).is_err());
    }

    #[test]
    fn source_ranks_follow_the_enum() {
        assert!(source_rank(Some("user:price-editor")) < source_rank(Some(SOURCE_USER_PRICE)));
        assert!(source_rank(Some(SOURCE_XFER_DIALOG)) < source_rank(Some(SOURCE_SPLIT_REGISTER)));
        assert_eq!(source_rank(Some("whatever")), source_rank(None));
    }
}
