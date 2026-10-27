//! Creating an instance's transactions: one per template transaction, its amounts from the Formulas with the
//! instance's bindings (`create_each_transaction_helper`, `5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1087-1422`;
//! the note §3.6–§3.8, §4.6).

use std::sync::Arc;

use super::error::{AccountProblem, Refusal, ScheduledError, Side};
use super::template::{exchange_rate_variable, formulas, site, split_account, template_transactions, transaction_currency};
use crate::formula::{NumberFormat, Variables, evaluate};
use crate::ops::commit::{commodity_fraction, currency_fraction, set_date_posted, set_notes, sort_splits};
use crate::ops::scrub::{Repair, Scope, scrub_imbalance};
use crate::{
    Account, Book, CommodityRef, Date, DomainError, Env, Frame, Guid, How, Numeric, ReconcileState, Round, ScheduledTransaction, SlotValue, Split, Time64,
    Transaction,
};

/// The Slot of a created transaction holding the GUID of the scheduled transaction it came from (`GNC_SX_FROM`,
/// `5.17:libgnucash/engine/Transaction.cpp:186`, set at `gnc-sx-instance-model.c:1405-1409`).
pub const FROM_SCHEDULED: &str = "from-sched-xaction";

/// A split an instance would create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltSplit {
    /// The real account.
    pub account: Guid,
    /// Value in the transaction currency.
    pub value: Numeric,
    /// Amount in the account's commodity.
    pub amount: Numeric,
    /// The template split's memo.
    pub memo: Option<String>,
    /// The template split's action.
    pub action: Option<String>,
    /// The template split's reconcile state.
    pub reconcile_state: ReconcileState,
    /// The template split's reconcile date.
    pub reconcile_date: Option<Time64>,
}

/// A transaction an instance would create, before its commit balances it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltTransaction {
    /// The template transaction it comes from.
    pub template: Guid,
    /// Its currency (`get_transaction_currency`).
    pub currency: CommodityRef,
    /// The template's num.
    pub num: Option<String>,
    /// The template's description.
    pub description: Option<String>,
    /// The template's notes.
    pub notes: Option<String>,
    /// One split per template split, in template order.
    pub splits: Vec<BuiltSplit>,
    /// The value of the `Imbalance-<CUR>` split the commit will add when rounding left the values unbalanced.
    pub imbalance: Option<Numeric>,
}

/// An `Imbalance-<CUR>` split the commit added to a created transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImbalanceSplit {
    /// The transaction.
    pub transaction: Guid,
    /// The split.
    pub split: Guid,
    /// The Imbalance account.
    pub account: Guid,
    /// Its value.
    pub value: Numeric,
}

/// A transaction created for an instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    /// The transaction.
    pub transaction: Guid,
    /// The Imbalance split its commit added, if any.
    pub imbalance: Option<ImbalanceSplit>,
}

/// The transactions an instance with `bindings` would create, in the template account's split order, without
/// changing the Book. A template transaction with no splits creates nothing, as in GnuCash (`:1349-1356`).
///
/// Where GnuCash creates a transaction anyway, this refuses (deviation (b) and the placeholder row of
/// `design/api/scheduled-transactions.md`): a Formula that fails ([`ScheduledError::Formula`]); a split whose account
/// is unknown, the root, a placeholder or without a commodity ([`ScheduledError::Account`]); a transaction currency that
/// is not a currency; and a rate of zero for a split whose value is not zero.
pub fn build_instance(
    book: &Book,
    sx: &ScheduledTransaction,
    bindings: &Variables,
    format: NumberFormat,
) -> Result<Vec<BuiltTransaction>, ScheduledError> {
    template_transactions(book, sx)
        .into_iter()
        .filter(|t| !t.splits.is_empty())
        .map(|t| build_transaction(book, sx, t, bindings, format))
        .collect()
}

fn build_transaction(
    book: &Book,
    sx: &ScheduledTransaction,
    t: &Transaction,
    bindings: &Variables,
    format: NumberFormat,
) -> Result<BuiltTransaction, ScheduledError> {
    let currency = transaction_currency(book, sx, t)?;
    if !currency.is_currency() {
        let message = format!("scheduled transaction {} would create a transaction in {currency}, which is not a currency", sx.name);
        return Err(ScheduledError::refused(Refusal::NotACurrency, Vec::new(), message));
    }
    let fraction = currency_fraction(book, &currency)?;
    let splits = t
        .splits
        .iter()
        .map(|s| build_split(book, sx, t, s, &currency, fraction, bindings, format))
        .collect::<Result<Vec<_>, _>>()?;
    let total = Numeric::sum_fixed(splits.iter().map(|s| s.value))?;
    Ok(BuiltTransaction {
        template: t.guid,
        currency,
        num: t.num.clone(),
        description: t.description.clone(),
        notes: t.notes().map(str::to_owned),
        splits,
        imbalance: (!total.is_zero()).then(|| total.neg()),
    })
}

#[allow(clippy::too_many_arguments)] // The template split, its transaction and the instance's context.
fn build_split(
    book: &Book,
    sx: &ScheduledTransaction,
    t: &Transaction,
    split: &Split,
    currency: &CommodityRef,
    fraction: i64,
    bindings: &Variables,
    format: NumberFormat,
) -> Result<BuiltSplit, ScheduledError> {
    let (account, commodity) = check_account(book, sx, t, split)?;
    let difference = formula_difference(sx, t, split, bindings, format)?;
    let value = split_value(split.value, difference, fraction)?;
    let scu = account.scu(commodity_fraction(book, commodity).ok_or_else(|| DomainError::not_found("commodity", commodity))?);
    let amount = if commodity == currency {
        amount_as_value(split.amount, value, scu.min(fraction))?.to_denom(scu, Round::HalfUp)?
    } else {
        let rate = bindings.get(&exchange_rate_variable(commodity, currency)).copied().unwrap_or(Numeric::ONE);
        exchanged(book, sx, split.amount, difference, rate, commodity)?.to_denom(scu, Round::HalfUp)?
    };
    Ok(BuiltSplit {
        account: account.guid,
        value,
        amount,
        memo: split.memo.clone(),
        action: split.action.clone(),
        reconcile_state: split.reconcile_state,
        reconcile_date: split.reconcile_date,
    })
}

/// The real account of a template split, refused when it is unknown, has no commodity, or is the root or a
/// placeholder, which red-ale's commit rules refuse a split in.
pub(crate) fn check_account<'a>(
    book: &'a Book,
    sx: &ScheduledTransaction,
    t: &Transaction,
    split: &Split,
) -> Result<(&'a Account, &'a CommodityRef), ScheduledError> {
    let (account, commodity) = split_account(book, sx, t, split)?;
    let problem = if account.guid == book.root {
        AccountProblem::Root(account.guid)
    } else if account.is_placeholder() {
        AccountProblem::Placeholder(account.guid)
    } else {
        return Ok((account, commodity));
    };
    Err(ScheduledError::Account {
        site: site(sx, t, split),
        problem,
        index: None,
    })
}

/// Debit minus credit, exact (`split_apply_formulas`, `gnc-sx-instance-model.c:1169-1194`): each formula evaluated with
/// the bindings, an empty one 0 (`_get_sx_formula_value`, `:1087-1148`; the stored numerics are never used, since the
/// bindings always hold `i`).
fn formula_difference(
    sx: &ScheduledTransaction,
    t: &Transaction,
    split: &Split,
    bindings: &Variables,
    format: NumberFormat,
) -> Result<Numeric, ScheduledError> {
    let side = |wanted: Side| -> Result<Numeric, ScheduledError> {
        formulas(split).into_iter().find(|(s, _)| *s == wanted).map_or(Ok(Numeric::ZERO), |(side, text)| {
            evaluate(text, format, bindings).map_err(|e| ScheduledError::formula(site(sx, t, split), side, text, e))
        })
    };
    Ok(side(Side::Debit)?.sub_fixed(side(Side::Credit)?)?)
}

/// `xaccSplitSetValue` (`5.17:libgnucash/engine/Split.cpp:1280-1300`): half up to the currency fraction, keeping the
/// value as cloned from the template when a non-zero one would round to zero.
fn split_value(template: Numeric, value: Numeric, fraction: i64) -> Result<Numeric, ScheduledError> {
    let rounded = value.to_denom(fraction, Round::HalfUp)?;
    Ok(if rounded.is_zero() && !value.is_zero() { template } else { rounded })
}

/// `xaccSplitScrub` for a split in the transaction currency (`5.17:libgnucash/engine/Scrub.cpp:236-347`): the amount
/// cloned from the template becomes the value unless the two are the same at the smaller of the SCU and the fraction.
fn amount_as_value(template: Numeric, value: Numeric, denom: i64) -> Result<Numeric, ScheduledError> {
    let same = template.to_denom(denom, Round::HalfUp)? == value.to_denom(denom, Round::HalfUp)?;
    Ok(if same { template } else { value })
}

/// `split_apply_exchange_rate` (`gnc-sx-instance-model.c:1196-1231`), on the unrounded difference: for a currency
/// value × rate to 1/1000, for any other commodity value ÷ rate to the commodity's fraction, both half up.
fn exchanged(
    book: &Book,
    sx: &ScheduledTransaction,
    template: Numeric,
    difference: Numeric,
    rate: Numeric,
    commodity: &CommodityRef,
) -> Result<Numeric, ScheduledError> {
    if rate.is_zero() {
        if difference.is_zero() {
            return Ok(template);
        }
        let message = format!("scheduled transaction {} has an exchange rate of 0 for {}", sx.name, commodity.mnemonic);
        return Err(ScheduledError::refused(Refusal::ZeroRate, Vec::new(), message));
    }
    let half_up = How::round(Round::HalfUp);
    Ok(if commodity.is_currency() {
        difference.mul(rate, 1000, half_up)?
    } else {
        let fraction = commodity_fraction(book, commodity).ok_or_else(|| DomainError::not_found("commodity", commodity))?;
        difference.div(rate, fraction, half_up)?
    })
}

/// Create an instance's transactions on `date` from what [`build_instance`] built: each posted on the date at the
/// Day-neutral time with the `date-posted` Slot, entered now, with the template's num, description and notes, the
/// `from-sched-xaction` Slot naming the scheduled transaction and no other Slot (the note §4.6). Each is committed as
/// GnuCash commits it: the imbalance scrub adds an `Imbalance-<CUR>` split where rounding left it unbalanced, creating
/// the account if needed, and rebuilds trading splits in a Book that uses them (`Transaction.cpp:1549-1558`); then
/// its splits are sorted debits first.
pub fn create_built(
    book: &mut Book,
    env: &mut dyn Env,
    scheduled: Guid,
    date: Date,
    built: Vec<BuiltTransaction>,
) -> Result<Vec<Created>, ScheduledError> {
    built.into_iter().map(|b| create_one(book, env, scheduled, date, b)).collect()
}

fn create_one(book: &mut Book, env: &mut dyn Env, scheduled: Guid, date: Date, b: BuiltTransaction) -> Result<Created, ScheduledError> {
    let splits = b
        .splits
        .into_iter()
        .map(|s| Split {
            memo: s.memo,
            action: s.action,
            reconcile_state: s.reconcile_state,
            reconcile_date: s.reconcile_date,
            ..Split::new(env.new_guid(), s.account, s.value, s.amount)
        })
        .collect();
    let mut t = Transaction {
        guid: env.new_guid(),
        currency: b.currency,
        num: b.num,
        date_posted: Time64(0),
        date_entered: env.now(),
        description: b.description,
        slots: Frame::new(),
        splits,
    };
    set_notes(&mut t, b.notes.as_deref());
    set_date_posted(&mut t, date, env.zone());
    t.slots.set(FROM_SCHEDULED, SlotValue::Guid(scheduled));
    let guid = t.guid;
    book.transactions.insert(guid, Arc::new(t));
    let repairs = scrub_imbalance(book, env, Scope::Transaction(guid))?;
    if let Some(t) = book.transactions.get_mut(&guid) {
        sort_splits(Arc::make_mut(t));
    }
    let imbalance = repairs.iter().find_map(|r| match *r {
        Repair::BalanceSplitAdded {
            transaction,
            split,
            account,
            value,
        } => Some(ImbalanceSplit {
            transaction,
            split,
            account,
            value,
        }),
        _ => None,
    });
    Ok(Created { transaction: guid, imbalance })
}

/// Create the transactions of the instance of `scheduled` on `date` with `bindings`: [`build_instance`], then
/// [`create_built`]. On an error the Book may hold some of them; callers run it on a copy, as every operation.
pub fn create_instance(
    book: &mut Book,
    env: &mut dyn Env,
    scheduled: Guid,
    date: Date,
    bindings: &Variables,
    format: NumberFormat,
) -> Result<Vec<Created>, ScheduledError> {
    let sx = book.scheduled.get(&scheduled).ok_or_else(|| DomainError::not_found("scheduledTransaction", &scheduled))?;
    let built = build_instance(book, sx, bindings, format)?;
    create_built(book, env, scheduled, date, built)
}

#[cfg(test)]
mod tests;
