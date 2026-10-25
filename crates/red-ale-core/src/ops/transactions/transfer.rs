//! Transfers between accounts in different commodities, and the prices they record.
//!
//! [`transfer`] does what GnuCash's Transfer dialog does on OK (`5.17:gnucash/gnome-utils/dialog-transfer.cpp`):
//! it creates a two-split transaction from an amount and either a rate or the amount received, then records the
//! rate in the price database. [`record_prices`] is `xaccTransRecordPrice`, which the register calls after
//! committing a transaction with splits in priced accounts. See `design/notes/engine-accounts-transactions.md` §10.
//!
//! Price lookups, source precedence and adding prices are `ops::prices`; this module adds only the dialog's and
//! `xaccTransRecordPrice`'s own rules for when an existing same-day price is kept or updated.

use super::post;
use crate::book_options::uses_split_action_for_num;
use crate::currencies::{euro_rate, is_euro_currency};
use crate::ops::commit::{NewSplit, NewTransaction, account_scu, commodity_fraction};
use crate::ops::prices::{self, Nearest, PriceChanges, SPLIT_REGISTER_SOURCE, USER_PRICE_SOURCE, XFER_DIALOG_SOURCE, source_rank};
use crate::{AccountType, Book, CommodityRef, Date, DomainError, DomainResult, Env, Guid, How, Numeric, Price, Round, Time64, TimeZone, Transaction};

/// `PRICE_TYPE_TRN`.
pub const PRICE_TYPE_TRANSACTION: &str = "transaction";

/// `gnc_numeric_invert`.
fn invert(n: Numeric) -> Numeric {
    if n.num() < 0 { Numeric::new(-n.denom(), -n.num()) } else { Numeric::new(n.denom(), n.num()) }
}

/// A price between `a` and `b` in either direction: its GUID, value and source rank, and whether it is stored the
/// other way round (as `b` priced in `a`, which GnuCash tests as `gnc_commodity_equiv(price currency, a)`).
struct Found {
    guid: Guid,
    value: Numeric,
    rank: usize,
    reversed: bool,
}

fn lookup(book: &Book, a: &CommodityRef, b: &CommodityRef, t: Time64, how: Nearest, zone: &TimeZone) -> Option<Found> {
    prices::nearest(book, a, b, t, how, zone).map(|p| Found {
        guid: p.guid,
        value: p.value,
        rank: source_rank(p.source.as_deref()),
        reversed: p.currency == crate::ops::commodities::canonical(a),
    })
}

/// A new price of the transaction type, added by `gnc_pricedb_add_price`'s rules.
fn add(book: &mut Book, env: &mut dyn Env, commodity: CommodityRef, currency: CommodityRef, time: Time64, source: &str, value: Numeric) -> DomainResult<PriceChange> {
    let price = Price {
        guid: env.new_guid(),
        commodity,
        currency,
        time,
        source: Some(source.to_owned()),
        price_type: Some(PRICE_TYPE_TRANSACTION.to_owned()),
        value,
    };
    let zone = env.zone().clone();
    let added = prices::add_price(book, price, &zone)?;
    Ok(match (added.added, added.rejected_by.or(added.duplicate_of)) {
        (Some(g), _) => PriceChange::Created(g),
        (None, Some(g)) => PriceChange::Kept(g),
        (None, None) => return Err(DomainError::conflict("the price database refused the price")),
    })
}

/// Update a price's time, type and value, and its source when given, keeping it otherwise
/// (`gnc_price_set_*`; moving its time re-applies the same-day rules, as `ops::prices::update_price` does).
fn set(book: &mut Book, guid: Guid, time: Time64, source: Option<&str>, value: Numeric, zone: &TimeZone) -> DomainResult<PriceChange> {
    let changes = PriceChanges {
        time: Some(time),
        source: source.map(str::to_owned),
        price_type: Some(Some(PRICE_TYPE_TRANSACTION.to_owned())),
        value: Some(value),
        ..PriceChanges::default()
    };
    prices::update_price(book, guid, &changes, zone)?;
    Ok(PriceChange::Updated(guid))
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
fn decide(book: &Book, x: &Transfer, leg: &Leg, zone: &TimeZone) -> DomainResult<(Numeric, &'static str, Numeric)> {
    let by_rate = |rate: Numeric| -> DomainResult<(Numeric, &'static str, Numeric)> {
        if !rate.is_positive() {
            return Err(DomainError::invalid("the exchange rate must be positive"));
        }
        Ok((rate, USER_PRICE_SOURCE, x.amount.mul(rate, leg.scu, How::round(Round::HalfUp))?))
    };
    let (rate, source, to_amount) = match x.rate {
        Rate::Rate(r) => by_rate(r)?,
        Rate::ToAmount(to) => (to.div(x.amount, crate::DENOM_AUTO, How::REDUCE_NEVER)?, XFER_DIALOG_SOURCE, to),
        Rate::FromPriceDb => by_rate(prefilled_rate(book, x, leg, zone)?)?,
    };
    if to_amount.is_zero() {
        return Err(DomainError::invalid("the amount received is zero"));
    }
    Ok((rate, source, to_amount))
}

/// The rate the dialog fills in (`gnc_xfer_dialog_set_price_auto`, `:367-404`; `gnc_xfer_dialog_update_price`,
/// `:306-334`).
fn prefilled_rate(book: &Book, x: &Transfer, leg: &Leg, zone: &TimeZone) -> DomainResult<Numeric> {
    if let (Some(f), Some(t)) = (euro_rate(&leg.from), euro_rate(&leg.to)) {
        return Ok(t.div(f, crate::DENOM_AUTO, How::REDUCE_NEVER)?);
    }
    let time = zone.neutral_time(x.date);
    let found = lookup(book, &leg.from, &leg.to, time, Nearest::SameDay, zone)
        .or_else(|| lookup(book, &leg.from, &leg.to, time, Nearest::Nearest, zone))
        .ok_or_else(|| DomainError::invalid(format!("no price of {} in {}; give a rate or the amount received", leg.from, leg.to)))?;
    Ok(if found.reversed { invert(found.value) } else { found.value })
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
        Some(decide(book, x, &leg, env.zone())?)
    };
    let to_amount = decided.map_or(x.amount, |(_, _, to)| to);
    let with_memo = |s: NewSplit| match &x.memo {
        Some(m) => s.with_memo(m.clone()),
        None => s,
    };
    // `gnc_set_num_action(trans, from_split, num, NULL)` (`:1514-1516`): with the num field source on, the num goes
    // to the from split's action.
    let num_on_split = uses_split_action_for_num(book);
    let from_split = with_memo(NewSplit::new(x.from, x.amount.neg()).with_amount(x.amount.neg()));
    let from_split = match &x.num {
        Some(n) if num_on_split => from_split.with_action(n.clone()),
        _ => from_split,
    };
    let new = NewTransaction {
        description: x.description.clone(),
        num: x.num.clone().filter(|_| !num_on_split),
        notes: x.notes.clone(),
        splits: vec![from_split, with_memo(NewSplit::new(x.to, x.amount).with_amount(to_amount))],
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
    let zone = env.zone().clone();
    let time = zone.neutral_time(x.date);
    if let Some(found) = lookup(book, &leg.from, &leg.to, time, Nearest::SameDay, &zone) {
        return update_price(book, leg, &found, rate, source, time, &zone);
    }
    let (mut commodity, mut currency, mut value) = (leg.from.clone(), leg.to.clone(), rate.abs());
    let default_first = x.default_currency.as_ref().is_some_and(|d| *d == leg.from && *d != leg.to);
    if (leg.from.is_currency() && !leg.to.is_currency()) || default_first {
        std::mem::swap(&mut commodity, &mut currency);
        value = invert(value);
    }
    add(book, env, commodity, currency, time, source, value)
}

/// The dialog's `update_price`: keep an existing price whose source is preferred or whose rounded value is the same;
/// otherwise set its time, type and *unrounded* value (inverted when it is stored the other way round), keeping its
/// source.
fn update_price(book: &mut Book, leg: &Leg, found: &Found, rate: Numeric, source: &str, time: Time64, zone: &TimeZone) -> DomainResult<PriceChange> {
    if found.rank < source_rank(Some(source)) {
        return Ok(PriceChange::Kept(found.guid));
    }
    let value = if found.reversed { invert(rate) } else { rate };
    let rounded_existing = round_price(book, &leg.from, &leg.to, found.value)?;
    if round_price(book, &leg.from, &leg.to, value)?.same_value(rounded_existing) {
        return Ok(PriceChange::Kept(found.guid));
    }
    set(book, found.guid, time, None, value, zone)
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
    let fraction = commodity_fraction(book, curr).unwrap_or(100);
    let zone = env.zone().clone();
    let Some(found) = lookup(book, comm, curr, t.date_posted, Nearest::SameDay, &zone) else {
        let value = value.to_denom(fraction * 10_000, Round::HalfUp)?;
        return add(book, env, comm.clone(), curr.clone(), t.date_posted, source, value);
    };
    let compared = if found.reversed { invert(value) } else { value };
    let register_over_dialog = found.rank == source_rank(Some(XFER_DIALOG_SOURCE)) && source == SPLIT_REGISTER_SOURCE;
    if compared.same_value(found.value) || (found.rank < source_rank(Some(source)) && !register_over_dialog) {
        return Ok(PriceChange::Kept(found.guid));
    }
    let fraction = if found.reversed { commodity_fraction(book, comm).unwrap_or(100) } else { fraction };
    set(book, found.guid, t.date_posted, Some(source), compared.to_denom(fraction * 10_000, Round::HalfUp)?, &zone)
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
        assert_eq!(p.source.as_deref(), Some(USER_PRICE_SOURCE));
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
}
