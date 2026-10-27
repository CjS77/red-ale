//! The rules GnuCash applies when a transaction is committed, shared by every operation that creates or edits one.
//!
//! GnuCash balances an unbalanced transaction at commit by adding an `Imbalance-<CUR>` split. red-ale refuses
//! unbalanced input instead; the imbalance scrub exists for callers who ask for GnuCash's behaviour. In a Book that uses
//! trading accounts, the commit also rebuilds the transaction's trading splits ([`trading::commit`]).
//! See `design/notes/engine-accounts-transactions.md` §1.5, §5 and §9.6.

use std::sync::Arc;

use crate::book_options::uses_trading_accounts;
use crate::currencies::iso_currency;
use crate::ops::trading;
use crate::transaction::slot_paths;
use crate::{
    Book, CommodityRef, Date, DomainError, DomainResult, Env, Guid, Numeric, ReconcileState, Round, SlotValue, Split, Time64, Transaction,
};

/// A split to create.
#[derive(Debug, Clone, PartialEq)]
pub struct NewSplit {
    /// The account it posts to.
    pub account: Guid,
    /// Value, in the transaction currency.
    pub value: Numeric,
    /// Amount, in the account's commodity. `None` means "equal to the value", which is allowed only when the
    /// account's commodity is the transaction currency.
    pub amount: Option<Numeric>,
    /// Memo.
    pub memo: Option<String>,
    /// Action.
    pub action: Option<String>,
    /// Reconcile state.
    pub reconcile_state: ReconcileState,
    /// When it was reconciled.
    pub reconcile_date: Option<Time64>,
    /// The lot it belongs to.
    pub lot: Option<Guid>,
}

impl NewSplit {
    /// A split of `value` to `account`, with the amount equal to the value.
    pub fn new(account: Guid, value: Numeric) -> Self {
        Self {
            account,
            value,
            amount: None,
            memo: None,
            action: None,
            reconcile_state: ReconcileState::New,
            reconcile_date: None,
            lot: None,
        }
    }

    /// The same split with an explicit amount.
    #[must_use]
    pub fn with_amount(mut self, amount: Numeric) -> Self {
        self.amount = Some(amount);
        self
    }

    /// The same split with a memo.
    #[must_use]
    pub fn with_memo(mut self, memo: impl Into<String>) -> Self {
        self.memo = Some(memo.into());
        self
    }

    /// The same split with an action.
    #[must_use]
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }
}

/// A transaction to create.
#[derive(Debug, Clone, PartialEq)]
pub struct NewTransaction {
    /// The transaction currency.
    pub currency: CommodityRef,
    /// The date it takes effect.
    pub date_posted: Date,
    /// When it was entered; `None` means now.
    pub date_entered: Option<Time64>,
    /// Check or reference number.
    pub num: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Notes; empty means none.
    pub notes: Option<String>,
    /// Its splits, in order.
    pub splits: Vec<NewSplit>,
}

impl NewTransaction {
    /// A transaction in `currency` on `date_posted` with no splits yet.
    pub fn new(currency: CommodityRef, date_posted: Date) -> Self {
        Self {
            currency,
            date_posted,
            date_entered: None,
            num: None,
            description: None,
            notes: None,
            splits: Vec::new(),
        }
    }
}

/// The smallest fraction of a commodity: the book's own definition, else the built-in ISO 4217 table.
pub fn commodity_fraction(book: &Book, id: &CommodityRef) -> Option<i64> {
    book.commodity(id).map(|c| c.fraction).or_else(|| {
        if id.is_currency() {
            iso_currency(&id.mnemonic).map(|c| c.fraction)
        } else {
            None
        }
    })
}

/// The fraction a transaction currency's values are kept in.
pub fn currency_fraction(book: &Book, currency: &CommodityRef) -> DomainResult<i64> {
    if !currency.is_currency() {
        return Err(DomainError::invalid(format!("{currency} is not a currency")));
    }
    commodity_fraction(book, currency).ok_or_else(|| DomainError::not_found("currency", currency))
}

/// The SCU an account's amounts are kept in.
pub fn account_scu(book: &Book, account: Guid) -> DomainResult<i64> {
    let acc = book.account(account).ok_or_else(|| DomainError::not_found("account", &account))?;
    let commodity = acc
        .commodity
        .as_ref()
        .ok_or_else(|| DomainError::invalid(format!("account {} has no commodity", acc.name)))?;
    let fraction = commodity_fraction(book, commodity).ok_or_else(|| DomainError::not_found("commodity", commodity))?;
    Ok(acc.scu(fraction))
}

/// Convert to `denom` with ROUND_HALF_UP, refusing a non-zero amount that would round to zero, as
/// `xaccSplitSetValue` does (`5.17:libgnucash/engine/Split.cpp:1279-1312`).
fn round_to(n: Numeric, denom: i64, what: &str) -> DomainResult<Numeric> {
    let rounded = n.to_denom(denom, Round::HalfUp)?;
    if rounded.is_zero() && !n.is_zero() {
        return Err(DomainError::invalid(format!(
            "{what} {} rounds to zero at 1/{denom}",
            n.to_decimal_string()
        )));
    }
    Ok(rounded)
}

/// A value converted to the transaction currency's fraction (`xaccSplitSetValue`).
pub fn round_value(book: &Book, currency: &CommodityRef, value: Numeric) -> DomainResult<Numeric> {
    round_to(value, currency_fraction(book, currency)?, "value")
}

/// An amount converted to the account's SCU (`xaccSplitSetAmount`, `5.17:libgnucash/engine/Split.cpp:1242-1267`).
pub fn round_amount(book: &Book, account: Guid, amount: Numeric) -> DomainResult<Numeric> {
    round_to(amount, account_scu(book, account)?, "amount")
}

/// Set the posted date as GnuCash's dialogs do: the day-neutral time in the local time zone plus the `date-posted`
/// gdate slot (`xaccTransSetDatePostedGDate`, `5.17:libgnucash/engine/Transaction.cpp:1914-1926`).
pub fn set_date_posted(t: &mut Transaction, date: Date, zone: &crate::TimeZone) {
    t.date_posted = zone.neutral_time(date);
    t.slots.set(slot_paths::DATE_POSTED, SlotValue::Date(date));
}

/// Set or clear the notes; an empty string deletes the slot (`5.17:libgnucash/engine/Transaction.cpp:2042-2046`).
pub fn set_notes(t: &mut Transaction, notes: Option<&str>) {
    t.slots.set_opt_str(slot_paths::NOTES, notes.filter(|n| !n.is_empty()));
}

/// Stable sort putting splits with value ≥ 0 before those with value < 0 (`xaccTransSortSplits`,
/// `5.17:libgnucash/engine/Transaction.cpp:540-553`).
pub fn sort_splits(t: &mut Transaction) {
    t.splits.sort_by_key(|s| s.value.is_negative());
}

/// Build a split for a transaction in `currency`, applying the rounding rules.
pub fn build_split(book: &Book, env: &mut dyn Env, currency: &CommodityRef, new: NewSplit) -> DomainResult<Split> {
    let account = book
        .account(new.account)
        .ok_or_else(|| DomainError::not_found("account", &new.account))?;
    if account.guid == book.root {
        return Err(DomainError::invalid("cannot post to the root account"));
    }
    let amount = match new.amount {
        Some(a) => a,
        None if account.commodity.as_ref() == Some(currency) => new.value,
        None => {
            let commodity = account.commodity.as_ref().map_or_else(String::new, ToString::to_string);
            return Err(DomainError::invalid(format!(
                "split to {} needs an amount in {commodity}",
                account.name
            )));
        }
    };
    let mut split = Split::new(
        env.new_guid(),
        new.account,
        round_value(book, currency, new.value)?,
        round_amount(book, new.account, amount)?,
    );
    split.memo = new.memo;
    split.action = new.action;
    split.reconcile_state = new.reconcile_state;
    split.reconcile_date = new.reconcile_date;
    split.lot = new.lot;
    Ok(split)
}

/// Check a transaction's values sum to zero.
pub fn check_balanced(t: &Transaction) -> DomainResult<()> {
    let imbalance = t.imbalance()?;
    if imbalance.is_zero() {
        Ok(())
    } else {
        Err(DomainError::Unbalanced {
            imbalance: imbalance.to_decimal_string(),
            currency: t.currency.mnemonic.clone(),
        })
    }
}

/// Check a committed transaction is balanced as the Book defines it: its values sum to zero, and, when the Book uses
/// trading accounts, its trading values and each commodity's amounts do too (`xaccTransIsBalanced`,
/// `5.17:libgnucash/engine/Transaction.cpp:1078-1121`).
pub fn check_committed(book: &Book, t: &Transaction) -> DomainResult<()> {
    check_balanced(t)?;
    if uses_trading_accounts(book) {
        trading::check(book, t)
    } else {
        Ok(())
    }
}

/// Re-apply the commit rules to an edited transaction: values to the currency fraction, amounts to each account's
/// SCU, trading splits rebuilt when the Book uses them, splits sorted, and the result balanced.
pub fn commit(book: &mut Book, env: &mut dyn Env, t: &mut Transaction) -> DomainResult<()> {
    let currency = t.currency.clone();
    t.splits.iter_mut().try_for_each(|s| -> DomainResult<()> {
        s.value = round_value(book, &currency, s.value)?;
        if let Some(account) = s.account {
            s.amount = round_amount(book, account, s.amount)?;
        }
        Ok(())
    })?;
    trading::commit(book, env, t)?;
    sort_splits(t);
    check_committed(book, t)
}

/// Commit every transaction a Mutation added or changed, as GnuCash's `xaccTransCommitEdit` follows every setter
/// (`5.17:libgnucash/engine/Transaction.cpp:1521-1574`): with trading accounts on, each one that is not balanced gets
/// its trading splits rebuilt ([`trading::commit`]). Transactions are taken in book order. Operations that commit
/// through [`commit`] or [`add_transaction`] have already done this, so it changes nothing for them.
pub fn commit_changed(before: &Book, after: &mut Book, env: &mut dyn Env) -> DomainResult<()> {
    if !uses_trading_accounts(after) {
        return Ok(());
    }
    let changed: Vec<Guid> = crate::diff::changes(&before.transactions, &after.transactions)
        .iter()
        .filter_map(crate::diff::Change::after)
        .map(|t| t.guid)
        .collect();
    changed.into_iter().try_for_each(|guid| {
        let Some(mut t) = after.transaction(guid).cloned() else {
            return Ok(());
        };
        if trading::commit(after, env, &mut t)?.is_some() {
            sort_splits(&mut t);
            after.transactions.insert(guid, Arc::new(t));
        }
        Ok(())
    })
}

/// Create a transaction and add it to the book. Its splits are rounded and sorted as GnuCash commits them; an
/// unbalanced or empty transaction is refused.
pub fn add_transaction(book: &mut Book, env: &mut dyn Env, new: NewTransaction) -> DomainResult<Guid> {
    if new.splits.is_empty() {
        return Err(DomainError::invalid("a transaction needs at least one split"));
    }
    currency_fraction(book, &new.currency)?;
    let currency = new.currency.clone();
    let splits = new
        .splits
        .into_iter()
        .map(|s| build_split(book, env, &currency, s))
        .collect::<DomainResult<Vec<_>>>()?;
    let date_entered = match new.date_entered {
        Some(t) => t,
        None => env.now(),
    };
    let mut t = Transaction {
        guid: env.new_guid(),
        currency,
        num: new.num,
        date_posted: env.zone().neutral_time(new.date_posted),
        date_entered,
        description: new.description,
        slots: crate::Frame::new(),
        splits,
    };
    set_date_posted(&mut t, new.date_posted, env.zone());
    set_notes(&mut t, new.notes.as_deref());
    check_balanced(&t)?;
    trading::commit(book, env, &mut t)?;
    sort_splits(&mut t);
    check_committed(book, &t)?;
    let guid = t.guid;
    book.transactions.insert(guid, Arc::new(t));
    Ok(guid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Account, AccountType, FixedEnv};

    fn book() -> (Book, FixedEnv, Guid, Guid) {
        let mut env = FixedEnv::new(Time64(1_700_000_000), 0);
        let mut book = Book::new_empty(&mut env);
        let usd = CommodityRef::currency("USD");
        let mut add = |name: &str, ty| {
            let a = Account::new(env.new_guid(), name, ty, Some(usd.clone()), Some(book.root));
            let g = a.guid;
            book.accounts.insert(g, Arc::new(a));
            g
        };
        let bank = add("Bank", AccountType::Bank);
        let food = add("Food", AccountType::Expense);
        (book, env, bank, food)
    }

    #[test]
    fn rounds_sorts_and_stamps() {
        let (mut book, mut env, bank, food) = book();
        let date = Date::from_ymd(2024, 1, 15).unwrap();
        let mut new = NewTransaction::new(CommodityRef::currency("USD"), date);
        new.notes = Some(String::new());
        new.splits = vec![
            NewSplit::new(bank, Numeric::new(-12345, 1000)),
            NewSplit::new(food, Numeric::new(12345, 1000)),
        ];
        let guid = add_transaction(&mut book, &mut env, new).unwrap();
        let t = book.transaction(guid).unwrap();
        assert!(t.splits[0].value.same_repr(Numeric::new(1235, 100)), "debit first, rounded half up");
        assert!(t.splits[1].value.same_repr(Numeric::new(-1235, 100)));
        assert_eq!(t.date_posted.to_file_string(), "2024-01-15 10:59:00 +0000");
        assert_eq!(t.slots.get(slot_paths::DATE_POSTED), Some(&SlotValue::Date(date)));
        assert_eq!(t.date_entered, Time64(1_700_000_000));
        assert!(t.notes().is_none());
    }

    #[test]
    fn refuses_unbalanced_and_missing_amounts() {
        let (mut book, mut env, bank, food) = book();
        let date = Date::from_ymd(2024, 1, 15).unwrap();
        let mut new = NewTransaction::new(CommodityRef::currency("USD"), date);
        new.splits = vec![
            NewSplit::new(bank, Numeric::new(-100, 100)),
            NewSplit::new(food, Numeric::new(99, 100)),
        ];
        assert!(matches!(
            add_transaction(&mut book, &mut env, new.clone()),
            Err(DomainError::Unbalanced { .. })
        ));
        new.currency = CommodityRef::currency("EUR");
        new.splits[1].value = Numeric::new(100, 100);
        assert!(matches!(add_transaction(&mut book, &mut env, new), Err(DomainError::Invalid(_))));
        assert!(book.transactions.is_empty());
    }
}
