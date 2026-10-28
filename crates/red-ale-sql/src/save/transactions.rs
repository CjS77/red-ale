//! Transactions and their splits, the Book's and the template transactions alike (`design/sql-fidelity.md` §4).
//!
//! A transaction's row is updated only if its own columns changed; each split is inserted, updated or deleted as it
//! changed. New split rows go to the end of `splits`, so they read back last; when that would not give the Book's split
//! order, all the transaction's split rows are deleted and inserted again in order, keeping their GUIDs and their slot
//! rows. Removals come first, so a split moved to another transaction leaves its old row before it gets its new one.

use std::collections::{BTreeMap, BTreeSet};

use red_ale_core::diff::{Change, changes};
use red_ale_core::ops::commit::sort_splits;
use red_ale_core::{Book, Guid, Split, Transaction};

use super::Writer;
use super::sql::Statements;
use crate::cell::{Cell, parse_guid};
use crate::encode::{self, guid};
use crate::error::SqlError;
use crate::tables;

/// The splits of `tx` in the order GnuCash reads them: `rowid` order, then sorted as every commit sorts
/// (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:256-262`; `5.17:libgnucash/engine/Transaction.cpp:1508-1510`).
pub(super) fn read_order(st: &Statements<'_>, tx: &Transaction) -> Result<Vec<Guid>, SqlError> {
    Ok(sorted(tx, &read_stored(st, tx.guid)?))
}

/// `order`, the GUIDs of `tx`'s splits as stored, as GnuCash's load sorts them.
fn sorted(tx: &Transaction, order: &[Guid]) -> Vec<Guid> {
    let mut t = tx.clone();
    t.splits = order
        .iter()
        .filter_map(|g| tx.splits.iter().find(|s| s.guid == *g))
        .cloned()
        .collect();
    sort_splits(&mut t);
    t.splits.iter().map(|s| s.guid).collect()
}

fn splits_by_guid(t: &Transaction) -> BTreeMap<Guid, &Split> {
    t.splits.iter().map(|s| (s.guid, s)).collect()
}

/// Splits leaving one transaction and entering another in the same save, by GUID, as they were.
type Moved<'a> = BTreeMap<Guid, &'a Split>;

impl Writer<'_, '_> {
    pub(super) fn transactions(&mut self, saved: &Book) -> Result<(), SqlError> {
        let book = self.book;
        let mut all = changes(&saved.transactions, &book.transactions);
        all.extend(changes(&saved.templates.transactions, &book.templates.transactions));
        let moved = moved_splits(&all);
        all.iter().try_for_each(|c| match c {
            Change::Removed(o) => self.remove_transaction(o, &moved),
            Change::Changed(o, n) => self.remove_splits(o, n, &moved),
            Change::Added(_) => Ok(()),
        })?;
        all.iter().try_for_each(|c| match c {
            Change::Added(n) => self.add_transaction(n, &moved),
            Change::Changed(o, n) => self.change_transaction(o, n, &moved),
            Change::Removed(_) => Ok(()),
        })
    }

    /// A transaction removed with its splits and all their slot rows (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:512-532`).
    fn remove_transaction(&mut self, old: &Transaction, moved: &Moved<'_>) -> Result<(), SqlError> {
        self.delete_row(&tables::TRANSACTIONS, old.guid)?;
        self.delete_slots(old.guid)?;
        self.delete_dependents(&tables::SPLITS, "tx_guid", old.guid, old.splits.len())?;
        self.gone(&tables::SPLITS, "tx_guid", old.guid);
        old.splits.iter().try_for_each(|s| {
            self.touched_lots.extend(s.lot);
            if moved.contains_key(&s.guid) {
                Ok(())
            } else {
                self.delete_slots(s.guid)
            }
        })
    }

    /// The splits a changed transaction lost, with their slot rows, which GnuCash would leave as orphans
    /// (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:575-578`).
    fn remove_splits(&mut self, old: &Transaction, new: &Transaction, moved: &Moved<'_>) -> Result<(), SqlError> {
        let kept = splits_by_guid(new);
        old.splits.iter().filter(|s| !kept.contains_key(&s.guid)).try_for_each(|s| {
            let sql = "DELETE FROM splits WHERE guid = ?1 AND tx_guid = ?2";
            self.st.expect(sql, &[guid(s.guid), guid(old.guid)], 1)?;
            self.checks.push(super::Check::Gone(
                "SELECT rowid FROM splits WHERE guid = ?1 AND tx_guid = ?2".into(),
                vec![guid(s.guid), guid(old.guid)],
            ));
            self.touched_lots.extend(s.lot);
            if moved.contains_key(&s.guid) {
                Ok(())
            } else {
                self.delete_slots(s.guid)
            }
        })
    }

    fn add_transaction(&mut self, new: &Transaction, moved: &Moved<'_>) -> Result<(), SqlError> {
        self.ensure(&new.currency)?;
        self.adopt(new.guid)?;
        self.insert_row(&tables::TRANSACTIONS, encode::transaction(new, self.refs()))?;
        self.insert_slots(new.guid, &new.slots)?;
        new.splits
            .iter()
            .try_for_each(|s| self.add_split(new.guid, s, moved.get(&s.guid).copied()))?;
        self.checks.push(super::Check::SplitOrder(new.clone()));
        Ok(())
    }

    /// Insert a split's row: a new split adopts nothing and gets its slot rows; a moved one keeps its slot rows
    /// unless its Slots changed.
    fn add_split(&mut self, tx: Guid, s: &Split, was: Option<&Split>) -> Result<(), SqlError> {
        self.touched_lots.extend(s.lot);
        self.insert_row(&tables::SPLITS, encode::split(tx, s))?;
        if let Some(old) = was {
            self.rewrite_slots(s.guid, &old.slots, &s.slots)
        } else {
            self.adopt(s.guid)?;
            self.insert_slots(s.guid, &s.slots)
        }
    }

    fn change_transaction(&mut self, old: &Transaction, new: &Transaction, moved: &Moved<'_>) -> Result<(), SqlError> {
        self.ensure(&new.currency)?;
        let rows = (encode::transaction(old, self.refs()), encode::transaction(new, self.refs()));
        self.update_if_changed(&tables::TRANSACTIONS, &rows.0, rows.1)?;
        self.rewrite_slots(new.guid, &old.slots, &new.slots)?;
        let before = splits_by_guid(old);
        let stored = read_stored(&self.st, new.guid)?;
        let added: Vec<Guid> = new
            .splits
            .iter()
            .filter(|s| !before.contains_key(&s.guid))
            .map(|s| s.guid)
            .collect();
        let predicted: Vec<Guid> = stored.iter().chain(&added).copied().collect();
        if sorted(new, &predicted) == new.splits.iter().map(|s| s.guid).collect::<Vec<_>>() {
            new.splits.iter().try_for_each(|s| match before.get(&s.guid) {
                Some(o) => self.change_split(new.guid, o, s),
                None => self.add_split(new.guid, s, moved.get(&s.guid).copied()),
            })?;
        } else {
            self.reinsert_splits(new, &before, &stored, moved)?;
        }
        self.checks.push(super::Check::SplitOrder(new.clone()));
        Ok(())
    }

    fn change_split(&mut self, tx: Guid, old: &Split, new: &Split) -> Result<(), SqlError> {
        if self.update_if_changed(&tables::SPLITS, &encode::split(tx, old), encode::split(tx, new))? {
            self.touched_lots.extend(old.lot.into_iter().chain(new.lot));
        }
        self.rewrite_slots(new.guid, &old.slots, &new.slots)
    }

    /// Delete every split row of `new` and insert them again in the Book's order, keeping their slot rows, which are
    /// keyed by GUID.
    fn reinsert_splits(
        &mut self,
        new: &Transaction,
        before: &BTreeMap<Guid, &Split>,
        stored: &[Guid],
        moved: &Moved<'_>,
    ) -> Result<(), SqlError> {
        self.delete_dependents(&tables::SPLITS, "tx_guid", new.guid, stored.len())?;
        new.splits.iter().try_for_each(|s| match before.get(&s.guid) {
            Some(o) => {
                let cells = encode::split(new.guid, s);
                if cells != encode::split(new.guid, o) {
                    self.touched_lots.extend(o.lot.into_iter().chain(s.lot));
                }
                self.insert_row(&tables::SPLITS, cells)?;
                self.rewrite_slots(s.guid, &o.slots, &s.slots)
            }
            None => self.add_split(new.guid, s, moved.get(&s.guid).copied()),
        })
    }
}

/// The GUIDs of a transaction's split rows, in `rowid` order.
fn read_stored(st: &Statements<'_>, tx: Guid) -> Result<Vec<Guid>, SqlError> {
    let rows = st.rows("SELECT guid FROM splits WHERE tx_guid = ?1 ORDER BY rowid", &[guid(tx)])?;
    Ok(rows
        .iter()
        .filter_map(|r| match r.first() {
            Some(Cell::Text(t)) => std::str::from_utf8(t).ok().and_then(parse_guid),
            _ => None,
        })
        .collect())
}

/// Splits that leave a transaction and enter another in these changes.
fn moved_splits<'a>(all: &[Change<'a, Transaction>]) -> Moved<'a> {
    let entering: BTreeSet<Guid> = all
        .iter()
        .flat_map(|c| match c {
            Change::Added(n) => n.splits.iter().map(|s| s.guid).collect::<Vec<_>>(),
            Change::Changed(o, n) => {
                let had = splits_by_guid(o);
                n.splits.iter().filter(|s| !had.contains_key(&s.guid)).map(|s| s.guid).collect()
            }
            Change::Removed(_) => Vec::new(),
        })
        .collect();
    all.iter()
        .flat_map(|c| match c {
            Change::Removed(o) => o.splits.iter().collect::<Vec<_>>(),
            Change::Changed(o, n) => {
                let kept = splits_by_guid(n);
                o.splits.iter().filter(|s| !kept.contains_key(&s.guid)).collect()
            }
            Change::Added(_) => Vec::new(),
        })
        .filter(|s| entering.contains(&s.guid))
        .map(|s| (s.guid, s))
        .collect()
}
