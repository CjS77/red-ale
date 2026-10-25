//! Domain reads: account balances as GnuCash's engine computes them.
//!
//! GnuCash walks an account's splits in split order (`xaccSplitOrder`) and keeps running totals of their amounts
//! (`xaccAccountRecomputeBalance`, `5.17:libgnucash/engine/Account.cpp:2263-2335`). See
//! `design/notes/engine-accounts-transactions.md` §2.7.

use std::collections::BTreeMap;

use crate::book_options::uses_split_action_for_num;
use crate::ops::commit::{account_scu, commodity_fraction};
use crate::ops::transactions::order::split_order;
use crate::{Book, CommodityRef, DomainError, DomainResult, Guid, How, Numeric, ReconcileState, Round, Split, Time64, Transaction};

/// An account's balances, in its commodity (amounts, not values).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Balances {
    /// Every split.
    pub balance: Numeric,
    /// Splits not in state `n`.
    pub cleared: Numeric,
    /// Splits in state `y` or `f`.
    pub reconciled: Numeric,
}

impl Balances {
    /// All zero.
    pub const ZERO: Self = Self { balance: Numeric::ZERO, cleared: Numeric::ZERO, reconciled: Numeric::ZERO };
}

/// An account's splits with their transactions, in split order: GnuCash's `xaccAccountGetSplits`. With the Book's
/// num field source on, split actions stand in for transaction nums (`5.17:libgnucash/engine/Split.cpp:1548-1557`).
/// An unknown account has none.
pub fn account_splits(book: &Book, account: Guid) -> Vec<(&Transaction, &Split)> {
    let num_action = uses_split_action_for_num(book);
    let mut splits: Vec<(&Transaction, &Split)> = book.splits_of(account).collect();
    splits.sort_by(|(ta, sa), (tb, sb)| split_order(ta, sa, tb, sb, num_action));
    splits
}

fn is_stock_split(s: &Split) -> bool {
    s.slots.get_str("split-type") == Some("stock-split")
}

/// One step of `xaccAccountRecomputeBalance` (`5.17:libgnucash/engine/Account.cpp:2263-2335`). A stock split is
/// skipped while the balance is zero or when it would make it zero.
fn step(running: &mut Balances, s: &Split, amount: Numeric) -> DomainResult<()> {
    if is_stock_split(s) {
        if running.balance.is_zero() {
            return Ok(());
        }
        let next = running.balance.add_fixed(amount)?;
        if next.is_zero() {
            return Ok(());
        }
        running.balance = next;
    } else {
        running.balance = running.balance.add_fixed(amount)?;
    }
    if s.reconcile_state != ReconcileState::New {
        running.cleared = running.cleared.add_fixed(amount)?;
    }
    if matches!(s.reconcile_state, ReconcileState::Reconciled | ReconcileState::Frozen) {
        running.reconciled = running.reconciled.add_fixed(amount)?;
    }
    Ok(())
}

/// An account's balances, or, with `before`, the running balances at its last split whose transaction was posted
/// strictly before that instant (`GetBalanceAsOfDate`, `5.17:libgnucash/engine/Account.cpp:3506`). Amounts are taken
/// at the account's SCU, as GnuCash holds them once loaded.
pub fn balances(book: &Book, guid: Guid, before: Option<Time64>) -> DomainResult<Balances> {
    book.account(guid).ok_or_else(|| DomainError::not_found("account", &guid))?;
    let scu = account_scu(book, guid).ok();
    let mut running = Balances::ZERO;
    let mut at = Balances::ZERO;
    account_splits(book, guid).iter().try_for_each(|(t, s)| -> DomainResult<()> {
        let amount = match scu {
            Some(scu) => s.amount.to_denom(scu, Round::HalfUp)?,
            None => s.amount,
        };
        step(&mut running, s, amount)?;
        if before.is_none_or(|cut| t.date_posted < cut) {
            at = running;
        }
        Ok(())
    })?;
    Ok(at)
}

/// Balances of an account and its sub-accounts, as `xaccAccountGetXxxBalanceInCurrencyRecursive` sums them
/// (`5.17:libgnucash/engine/Account.cpp:3669-3740`): each sub-account's balance is added rounded half up to the
/// commodity's fraction. GnuCash converts a sub-account in another commodity at the latest price; red-ale v0.1 adds
/// nothing for it (as GnuCash does when no price exists) and reports its balances unconverted, by commodity.
pub fn balances_with_subaccounts(book: &Book, guid: Guid, before: Option<Time64>) -> DomainResult<(Balances, BTreeMap<CommodityRef, Balances>)> {
    let acc = book.account(guid).ok_or_else(|| DomainError::not_found("account", &guid))?;
    let own = balances(book, guid, before)?;
    let Some(commodity) = acc.commodity.clone() else { return Ok((own, BTreeMap::new())) };
    let fraction = commodity_fraction(book, &commodity).ok_or_else(|| DomainError::not_found("commodity", &commodity))?;
    let add = |a: Numeric, b: Numeric| a.add(b, fraction, How::round(Round::HalfUp));
    let mut unconverted: BTreeMap<CommodityRef, Balances> = BTreeMap::new();
    let total = book.descendants(guid).iter().try_fold(own, |total, sub| -> DomainResult<Balances> {
        let b = balances(book, sub.guid, before)?;
        let same = sub.commodity.as_ref() == Some(&commodity) || b.balance.is_zero() && b.cleared.is_zero() && b.reconciled.is_zero();
        let counted = if same {
            b
        } else {
            let other = sub.commodity.clone().unwrap_or_else(|| commodity.clone());
            let entry = unconverted.entry(other).or_insert(Balances::ZERO);
            *entry = Balances {
                balance: entry.balance.add_exact(b.balance)?,
                cleared: entry.cleared.add_exact(b.cleared)?,
                reconciled: entry.reconciled.add_exact(b.reconciled)?,
            };
            Balances::ZERO
        };
        Ok(Balances {
            balance: add(total.balance, counted.balance)?,
            cleared: add(total.cleared, counted.cleared)?,
            reconciled: add(total.reconciled, counted.reconciled)?,
        })
    })?;
    Ok((total, unconverted))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::ops::accounts::{AccountEdit, create_account};
    use crate::ops::commit::{NewSplit, NewTransaction, add_transaction};
    use crate::{AccountType, Date, FixedEnv};

    struct Fx {
        book: Book,
        env: FixedEnv,
    }

    impl Fx {
        fn new() -> Self {
            let mut env = FixedEnv::new(Time64(1_700_000_000), 0);
            let book = Book::new_empty(&mut env);
            Self { book, env }
        }

        fn add(&mut self, parent: Option<Guid>, name: &str, t: AccountType) -> Guid {
            let edit = AccountEdit {
                name: Some(name.into()),
                account_type: Some(t),
                commodity: Some(CommodityRef::currency("USD")),
                ..AccountEdit::default()
            };
            let parent = parent.unwrap_or(self.book.root);
            create_account(&mut self.book, &mut self.env, parent, &edit).unwrap()
        }

        fn post(&mut self, a: Guid, b: Guid, cents: i64, day: u32) -> Guid {
            let mut t = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, 1, day).unwrap());
            t.splits = vec![NewSplit::new(a, Numeric::new(cents, 100)), NewSplit::new(b, Numeric::new(-cents, 100))];
            add_transaction(&mut self.book, &mut self.env, t).unwrap()
        }
    }

    #[test]
    fn balances_follow_reconcile_states_and_dates() {
        let mut f = Fx::new();
        let bank = f.add(None, "Bank", AccountType::Bank);
        let sub = f.add(Some(bank), "Sub", AccountType::Bank);
        let food = f.add(None, "Food", AccountType::Expense);
        f.post(bank, food, 1000, 2);
        let t = f.post(bank, food, 250, 5);
        f.post(sub, food, 5, 6);
        let tx = Arc::make_mut(f.book.transactions.get_mut(&t).unwrap());
        tx.splits.iter_mut().for_each(|s| s.reconcile_state = ReconcileState::Reconciled);
        let all = balances(&f.book, bank, None).unwrap();
        assert_eq!((all.balance, all.cleared, all.reconciled), (Numeric::new(1250, 100), Numeric::new(250, 100), Numeric::new(250, 100)));
        let before = balances(&f.book, bank, Some(Date::from_ymd(2024, 1, 5).unwrap().start_utc())).unwrap();
        assert_eq!(before.balance, Numeric::new(1000, 100));
        let (total, other) = balances_with_subaccounts(&f.book, bank, None).unwrap();
        assert_eq!(total.balance, Numeric::new(1255, 100));
        assert!(other.is_empty());
    }
}
