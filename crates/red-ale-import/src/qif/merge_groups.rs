//! QIF's own duplicate finder: existing transactions an imported one may repeat.
//!
//! Translated from `5.17:gnucash/import-export/qif-imp/qif-merge-groups.scm` ("eliminate duplicate xtns in a new
//! (imported) account group"), Copyright 2001 Bill Gribble <grib@billgribble.com>. GPL-2.0-or-later; this
//! translation is GPL-3.0-or-later.
//!
//! GnuCash compares the import's account tree with the Book's (`gnc:account-tree-find-duplicates`, `:44-162`); the
//! user then ticks at most one candidate per imported transaction, and the ticked transactions are destroyed
//! (`gnc:prune-matching-transactions`, `:175-184`). red-ale's import is applied to the Book directly, so the
//! imported transactions are compared with those the Book had before; the Imbalance and orphan splits committing
//! added are left out, as GnuCash makes them outside the import's tree. The ticks are the caller's `skip` actions;
//! [`prune_matching_transactions`] removes those transactions. `gnc:account-tree-catenate-and-merge` is the account
//! merge [`super::to_gnc::accounts`] decides as each account is made.

use std::collections::{BTreeMap, BTreeSet};

use red_ale_core::book_options::uses_split_action_for_num;
use red_ale_core::ops::transactions::order::split_order;
use red_ale_core::{Book, DomainResult, Guid, Split, Transaction};

/// One week in seconds, `WeekSecs`. `qif-merge-groups.scm:70`.
pub const WEEK_SECS: i64 = 60 * 60 * 24 * 7;

/// An existing split an imported transaction may repeat, with its transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    /// The existing split.
    pub split: Guid,
    /// Its transaction.
    pub transaction: Guid,
}

/// Splits in the order a split query returns them (`xaccSplitOrder`).
fn sorted<'b>(book: &'b Book, splits: Vec<(&'b Transaction, &'b Split)>) -> Vec<(&'b Transaction, &'b Split)> {
    let num_action = uses_split_action_for_num(book);
    let mut splits = splits;
    splits.sort_by(|(ta, sa), (tb, sb)| split_order(ta, sa, tb, sb, num_action));
    splits
}

/// `gnc:account-tree-find-duplicates`: for each imported transaction, the existing splits whose value is the same,
/// whose account has the same full name, and whose transaction is posted at most a week apart, found from the
/// first of its splits (in split order) that has any. `old` is the Book before the import, `new` after it; `added`
/// are the imported transactions and `left_out` the splits committing added to them. `qif-merge-groups.scm:59-162`.
pub fn find_duplicates(old: &Book, new: &Book, added: &[Guid], left_out: &BTreeSet<Guid>) -> BTreeMap<Guid, Vec<Candidate>> {
    if !old.transactions.values().any(|t| t.splits.iter().any(|s| s.account.is_some())) {
        return BTreeMap::new();
    }
    let new_splits = sorted(
        new,
        added.iter().filter_map(|g| new.transaction(*g)).flat_map(|t| t.splits.iter().filter(|s| !left_out.contains(&s.guid)).map(move |s| (t, s))).collect(),
    );
    let (Some(min), Some(max)) = (new_splits.iter().map(|(t, _)| t.date_posted).min(), new_splits.iter().map(|(t, _)| t.date_posted).max()) else {
        return BTreeMap::new();
    };
    let in_range = |t: &Transaction| t.date_posted.0 >= min.0.saturating_sub(WEEK_SECS) && t.date_posted.0 <= max.0.saturating_add(WEEK_SECS);
    let old_splits = sorted(old, old.transactions.values().filter(|t| in_range(t)).flat_map(|t| t.splits.iter().map(move |s| (&**t, s))).collect());
    let full_name = |book: &Book, s: &Split| s.account.and_then(|a| book.full_name(a));
    let mut matches: BTreeMap<Guid, Vec<Candidate>> = BTreeMap::new();
    new_splits.iter().for_each(|(nt, ns)| {
        if matches.contains_key(&nt.guid) {
            return;
        }
        let name = full_name(new, ns);
        let candidates: Vec<Candidate> = old_splits
            .iter()
            .filter(|(ot, os)| {
                os.value.same_value(ns.value)
                    && name.is_some()
                    && full_name(old, os) == name
                    && (ot.date_posted.0 - nt.date_posted.0).abs() <= WEEK_SECS
            })
            .map(|(ot, os)| Candidate { split: os.guid, transaction: ot.guid })
            .collect();
        if !candidates.is_empty() {
            matches.insert(nt.guid, candidates);
        }
    });
    matches
}

/// `gnc:prune-matching-transactions`: destroy the imported transactions ticked as duplicates.
/// `qif-merge-groups.scm:175-184`.
///
/// A voided transaction is read-only, so GnuCash's `xaccTransDestroy` refuses it and the duplicate stays in the
/// book (`5.17:libgnucash/engine/Transaction.cpp:1363-1374`); red-ale removes it as ticked. The transactions were
/// made by this import a moment before, so nothing else refers to them.
pub fn prune_matching_transactions(book: &mut Book, ticked: &[Guid]) -> DomainResult<()> {
    ticked.iter().try_for_each(|g| {
        book.transactions.shift_remove(g).map(|_| ()).ok_or_else(|| red_ale_core::DomainError::not_found("transaction", g))
    })
}

#[cfg(test)]
mod tests {
    //! `test-gnc:account-tree-find-duplicates`, `5.17:gnucash/import-export/qif-imp/test/test-qif-merge-groups.scm:39-109`,
    //! over the transactions of `create-test-data` (`5.17:bindings/guile/test/test-engine-extras.scm:389-470`) that
    //! fall within a week of the imported ones, and a closing one that does not. GnuCash builds the imported tree
    //! beside the old one in the same book; red-ale applies the import to the Book, so the imported transactions go
    //! into a copy, into the accounts of the same full names.
    use std::sync::Arc;

    use red_ale_core::ops::accounts::{AccountEdit, create_account};
    use red_ale_core::{AccountType, CommodityRef, Date, Env, FixedEnv, Frame, Numeric, ReconcileState, Time64, TimeZone};

    use super::*;

    fn account(book: &mut Book, env: &mut FixedEnv, parent: Guid, name: &str, account_type: AccountType) -> Guid {
        let edit = AccountEdit {
            name: Some(name.into()),
            account_type: Some(account_type),
            commodity: Some(CommodityRef::currency("USD")),
            ..AccountEdit::default()
        };
        create_account(book, env, parent, &edit).unwrap()
    }

    /// `env-create-multisplit-transaction`: the splits' values, at 10:59 UTC on the day.
    fn post(book: &mut Book, env: &mut FixedEnv, (d, m, y): (u32, u32, i32), splits: &[(Guid, i64)]) -> Guid {
        let guid = env.new_guid();
        let date = TimeZone::utc().instant(Date::from_ymd(y, m, d).unwrap(), 10, 59, 0).unwrap();
        let splits = splits
            .iter()
            .map(|(account, value)| Split {
                guid: env.new_guid(),
                memo: None,
                action: None,
                reconcile_state: ReconcileState::New,
                reconcile_date: None,
                value: Numeric::from_int(*value),
                amount: Numeric::from_int(*value),
                account: Some(*account),
                lot: None,
                slots: Frame::default(),
            })
            .collect();
        let t = Transaction {
            guid,
            currency: CommodityRef::currency("USD"),
            num: None,
            date_posted: date,
            date_entered: date,
            description: None,
            slots: Frame::default(),
            splits,
        };
        book.transactions.insert(guid, Arc::new(t));
        guid
    }

    /// `env-transfer`: `amount` from one account to another.
    fn transfer(book: &mut Book, env: &mut FixedEnv, day: (u32, u32, i32), from: Guid, to: Guid, amount: i64) -> Guid {
        post(book, env, day, &[(from, -amount), (to, amount)])
    }

    #[test]
    fn duplicates_within_a_week_of_the_same_value_in_the_same_account() {
        let mut env = FixedEnv::new(Time64(0), 1);
        let mut old = Book::new_empty(&mut env);
        let root = old.root;
        let old_root = account(&mut old, &mut env, root, "Root", AccountType::Asset);
        let asset = account(&mut old, &mut env, old_root, "Asset", AccountType::Asset);
        let bank = account(&mut old, &mut env, asset, "Bank", AccountType::Asset);
        let wallet = account(&mut old, &mut env, asset, "Wallet", AccountType::Asset);
        // GnuCash's test puts income and expense accounts under an asset account, which red-ale refuses; the finder
        // reads only names and values, so they are asset accounts here.
        let income = account(&mut old, &mut env, old_root, "Income", AccountType::Asset);
        let expense = account(&mut old, &mut env, old_root, "Expenses", AccountType::Asset);
        let equity = account(&mut old, &mut env, old_root, "Equity", AccountType::Asset);
        transfer(&mut old, &mut env, (1, 1, 1970), bank, expense, 5);
        transfer(&mut old, &mut env, (31, 12, 1969), income, bank, 10);
        transfer(&mut old, &mut env, (1, 2, 1970), bank, expense, 15);
        post(&mut old, &mut env, (14, 2, 1971), &[(bank, -100), (expense, 80), (wallet, 20)]);
        transfer(&mut old, &mut env, (31, 12, 1977), expense, equity, 111);
        // `old-txn2`, which the test adds to the old book.
        transfer(&mut old, &mut env, (13, 2, 1971), bank, expense, 100);

        // The new structure: Root > Asset > Bank, Wallet, Income, Expenses. Bank's full name is the old one's, so the
        // import uses it; Expenses is under Asset, so its splits match nothing.
        let mut new = old.clone();
        let new_expenses = account(&mut new, &mut env, asset, "Expenses", AccountType::Asset);
        let txn1 = transfer(&mut new, &mut env, (1, 1, 1970), bank, new_expenses, 5);
        let txn2 = transfer(&mut new, &mut env, (20, 2, 1971), bank, new_expenses, 100);
        let txn3 = transfer(&mut new, &mut env, (22, 2, 1971), bank, new_expenses, 100);
        let added = [txn1, txn2, txn3];

        let matches = find_duplicates(&old, &new, &added, &BTreeSet::new());
        assert_eq!(matches.len(), 2, "2 txns matched");
        assert_eq!(matches[&txn1].len(), 1, "1st txn matches 1");
        assert_eq!(matches[&txn2].len(), 2, "2nd txn matches 2: the multisplit's bank split and old-txn2");
        assert!(!matches.contains_key(&txn3), "3rd txn matches none: more than a week from both");

        prune_matching_transactions(&mut new, &[txn2]).unwrap();
        let matches = find_duplicates(&old, &new, &added, &BTreeSet::new());
        assert_eq!(matches[&txn1].len(), 1, "1st txn matches 1");
        assert!(!matches.contains_key(&txn2), "2nd txn destroyed");
    }

    #[test]
    fn splits_committing_made_and_an_empty_book_match_nothing() {
        let mut env = FixedEnv::new(Time64(0), 1);
        let mut old = Book::new_empty(&mut env);
        let mut new = old.clone();
        let root = new.root;
        let bank = account(&mut new, &mut env, root, "Bank", AccountType::Bank);
        let t = post(&mut new, &mut env, (1, 1, 2000), &[(bank, 5)]);
        assert!(find_duplicates(&old, &new, &[t], &BTreeSet::new()).is_empty(), "the old book has no transactions");

        let old_bank = account(&mut old, &mut env, root, "Bank", AccountType::Bank);
        let other = account(&mut old, &mut env, root, "Other", AccountType::Bank);
        transfer(&mut old, &mut env, (2, 1, 2000), other, old_bank, 5);
        let split = new.transaction(t).unwrap().splits[0].guid;
        assert_eq!(find_duplicates(&old, &new, &[t], &BTreeSet::new()).len(), 1);
        assert!(find_duplicates(&old, &new, &[t], &BTreeSet::from([split])).is_empty(), "an Imbalance split is no part of the import");
    }
}
