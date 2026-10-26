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
pub fn prune_matching_transactions(book: &mut Book, ticked: &[Guid]) -> DomainResult<()> {
    ticked.iter().try_for_each(|g| red_ale_core::ops::transactions::delete(book, *g))
}
