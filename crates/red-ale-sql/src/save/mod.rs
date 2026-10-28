//! Writing a save: the rows of every entity that differs between the Book as saved and the Book now, in one SQL
//! transaction the caller opened, then read back and compared before the caller commits
//! (`design/sql-fidelity.md` §4).
//!
//! Tables are written in GnuCash's order, each entity's additions and changes in the Book's order and its deletions
//! after them. Every statement's row count is checked. Rows of unchanged entities, rows red-ale doesn't model and
//! orphan rows are never touched, except the orphans an inserted entity would adopt.

mod entities;
pub(crate) mod sql;
mod transactions;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use red_ale_core::currencies::iso_currency;
use red_ale_core::diff::Change;
use red_ale_core::{Book, Commodity, CommodityRef, Env, Frame, Guid};

use crate::cell::{Cell, Table, parse_guid};
use crate::encode::{self, Refs, guid};
use crate::error::SqlError;
use crate::residue::SqlResidue;
use crate::tables;
use sql::{Faults, Statements, insert, select, update};

/// What the read-back checks once everything is written.
enum Check {
    /// The row keyed by the first cell's GUID holds exactly these cells.
    Row(&'static Table, Vec<Cell>),
    /// This query finds no row.
    Gone(String, Vec<Cell>),
    /// These slot rows, by owner in `id` order, are exactly the rows each owner has.
    Slots(Vec<Vec<Cell>>),
    /// The rows of `table` whose `column` is the owner are exactly these, in `rowid` order.
    Dependents(&'static Table, &'static str, Guid, Vec<Vec<Cell>>),
    /// The transaction's splits read back in its order.
    SplitOrder(red_ale_core::Transaction),
}

/// Writes one save.
pub(crate) struct Writer<'a, 'c> {
    st: Statements<'c>,
    residue: &'a mut SqlResidue,
    book: &'a Book,
    env: &'a mut dyn Env,
    checks: Vec<Check>,
    /// Lots a written split entered, left or changed in, to be marked `is_closed` unknown.
    touched_lots: BTreeSet<Guid>,
    /// Lots whose row this save wrote.
    written_lots: BTreeSet<Guid>,
}

/// Write the rows that differ between `saved` and `book`, then read them back. The caller commits only on success.
pub(crate) fn write(
    conn: &rusqlite::Connection,
    residue: &mut SqlResidue,
    saved: &Book,
    book: &Book,
    env: &mut dyn Env,
    faults: Faults,
) -> Result<(), SqlError> {
    let mut w = Writer {
        st: Statements::new(conn, faults),
        residue,
        book,
        env,
        checks: Vec::new(),
        touched_lots: BTreeSet::new(),
        written_lots: BTreeSet::new(),
    };
    w.commodities(saved)?;
    w.book_slots(saved)?;
    w.accounts(saved)?;
    w.budgets(saved)?;
    w.prices(saved)?;
    w.lots(saved)?;
    w.transactions(saved)?;
    w.mark_touched_lots()?;
    w.scheduled(saved)?;
    w.business(saved)?;
    if w.st.corrupt() {
        w.corrupt()?;
    }
    w.verify()
}

/// Whether anything differs between `saved` and `book` that a save would write.
pub(crate) fn has_changes(saved: &Book, book: &Book) -> bool {
    use red_ale_core::diff::changes;
    let b = (&saved.business, &book.business);
    saved.slots != book.slots
        || !btree_changes(&saved.commodities, &book.commodities).is_empty()
        || !changes(&saved.accounts, &book.accounts).is_empty()
        || !changes(&saved.templates.accounts, &book.templates.accounts).is_empty()
        || !changes(&saved.budgets, &book.budgets).is_empty()
        || !changes(&saved.prices, &book.prices).is_empty()
        || !changes(&saved.lots, &book.lots).is_empty()
        || !changes(&saved.transactions, &book.transactions).is_empty()
        || !changes(&saved.templates.transactions, &book.templates.transactions).is_empty()
        || !changes(&saved.scheduled, &book.scheduled).is_empty()
        || b.0 != b.1
}

/// Every entry added, changed or removed between two ordered maps: additions and changes in `new`'s order, then
/// removals in `old`'s order, as [`red_ale_core::diff::changes`] gives them for the Book's other maps.
pub(crate) fn btree_changes<'a, K: Ord, T: PartialEq>(old: &'a BTreeMap<K, Arc<T>>, new: &'a BTreeMap<K, Arc<T>>) -> Vec<Change<'a, T>> {
    let changed = new.iter().filter_map(|(k, n)| match old.get(k) {
        None => Some(Change::Added(&**n)),
        Some(o) if !Arc::ptr_eq(o, n) && **o != **n => Some(Change::Changed(&**o, &**n)),
        Some(_) => None,
    });
    let removed = old.iter().filter(|(k, _)| !new.contains_key(*k)).map(|(_, o)| Change::Removed(&**o));
    changed.chain(removed).collect()
}

impl Writer<'_, '_> {
    fn refs(&self) -> Refs<'_> {
        Refs {
            commodities: &self.residue.commodities,
        }
    }

    /// Insert a row and check it reads back.
    fn insert_row(&mut self, table: &'static Table, cells: Vec<Cell>) -> Result<(), SqlError> {
        self.st.expect(&insert(table), &cells, 1)?;
        self.checks.push(Check::Row(table, cells));
        Ok(())
    }

    /// Update the row keyed by the first cell's GUID, which must exist, and check it reads back.
    fn update_row(&mut self, table: &'static Table, cells: Vec<Cell>) -> Result<(), SqlError> {
        let mut params = cells.clone();
        params.push(cells[0].clone());
        self.st.expect(&update(table), &params, 1)?;
        self.checks.push(Check::Row(table, cells));
        Ok(())
    }

    /// Update the row if its cells differ from what it held.
    fn update_if_changed(&mut self, table: &'static Table, old: &[Cell], new: Vec<Cell>) -> Result<bool, SqlError> {
        if old == new.as_slice() {
            return Ok(false);
        }
        self.update_row(table, new)?;
        Ok(true)
    }

    /// Delete the row with this GUID, which must exist, and check it is gone.
    fn delete_row(&mut self, table: &'static Table, key: Guid) -> Result<(), SqlError> {
        let sql = format!("DELETE FROM \"{}\" WHERE \"guid\" = ?1", table.name);
        self.st.expect(&sql, &[guid(key)], 1)?;
        self.gone(table, "guid", key);
        Ok(())
    }

    fn gone(&mut self, table: &Table, column: &str, key: Guid) {
        let sql = format!("SELECT rowid FROM \"{}\" WHERE \"{column}\" = ?1", table.name);
        self.checks.push(Check::Gone(sql, vec![guid(key)]));
    }

    /// Delete every row of `table` whose `column` is `owner`, which must be `rows` of them.
    fn delete_dependents(&mut self, table: &'static Table, column: &str, owner: Guid, rows: usize) -> Result<(), SqlError> {
        let sql = format!("DELETE FROM \"{}\" WHERE \"{column}\" = ?1", table.name);
        self.st.expect(&sql, &[guid(owner)], rows)
    }

    /// Insert an owner's dependent rows (recurrences, budget amounts, tax-table entries) and check they read back.
    fn insert_dependents(&mut self, table: &'static Table, column: &'static str, owner: Guid, rows: Vec<Vec<Cell>>) -> Result<(), SqlError> {
        rows.iter().try_for_each(|r| self.st.expect(&insert(table), r, 1))?;
        self.checks.push(Check::Dependents(table, column, owner, rows));
        Ok(())
    }

    /// Insert the slot rows of a new owner, with fresh frame and list GUIDs from the Env.
    fn insert_slots(&mut self, owner: Guid, frame: &Frame) -> Result<(), SqlError> {
        if frame.is_empty() {
            return Ok(());
        }
        let env = &mut *self.env;
        let rows = encode::slots::rows(owner, frame, &mut || env.new_guid())?;
        rows.iter().try_for_each(|r| self.st.expect(&insert(&tables::SLOTS), r, 1))?;
        self.residue.slot_rows.insert(owner, rows.len());
        self.checks.push(Check::Slots(rows));
        Ok(())
    }

    /// Delete an owner's slot rows, frames and lists recursively, which must be the rows its Slots were stored as.
    fn delete_slots(&mut self, owner: Guid) -> Result<(), SqlError> {
        self.delete_stored_slots(owner)?;
        self.gone(&tables::SLOTS, "obj_guid", owner);
        Ok(())
    }

    fn delete_stored_slots(&mut self, owner: Guid) -> Result<(), SqlError> {
        let expected = self.residue.slot_rows.remove(&owner).unwrap_or(0);
        let deleted = self.delete_slot_tree(owner)?;
        if deleted == expected {
            Ok(())
        } else {
            Err(SqlError::Verification(format!(
                "the Slots of {owner} were stored as {expected} rows but {deleted} were found; nothing was written"
            )))
        }
    }

    /// Rewrite an owner's slot rows when its Slots changed, as GnuCash does on every commit
    /// (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:645-649`).
    fn rewrite_slots(&mut self, owner: Guid, old: &Frame, new: &Frame) -> Result<(), SqlError> {
        if old == new {
            return Ok(());
        }
        self.delete_stored_slots(owner)?;
        if new.is_empty() {
            self.gone(&tables::SLOTS, "obj_guid", owner);
        }
        self.insert_slots(owner, new)
    }

    /// Delete the slot rows `owner` owns and those under its frames and lists (`gnc_sql_slots_delete`,
    /// `5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:658-697`). Returns how many rows went.
    fn delete_slot_tree(&mut self, owner: Guid) -> Result<usize, SqlError> {
        let containers = self.st.rows(
            "SELECT guid_val FROM slots WHERE obj_guid = ?1 AND slot_type IN (8, 9) AND guid_val IS NOT NULL",
            &[guid(owner)],
        )?;
        let children: Vec<Guid> = containers
            .iter()
            .filter_map(|r| match r.first() {
                Some(Cell::Text(t)) => std::str::from_utf8(t).ok().and_then(parse_guid),
                _ => None,
            })
            .collect();
        let nested = children.into_iter().try_fold(0, |n, child| Ok::<_, SqlError>(n + self.delete_slot_tree(child)?))?;
        Ok(nested + self.st.run("DELETE FROM slots WHERE obj_guid = ?1", &[guid(owner)])?)
    }

    /// Delete the orphan rows keyed by a GUID an entity is about to be inserted with, since GnuCash would read them as
    /// the new entity's (`design/sql-fidelity.md` §4, "Orphans that would be adopted").
    fn adopt(&mut self, key: Guid) -> Result<(), SqlError> {
        let orphans = &mut self.residue.orphans;
        let found = [
            (orphans.slots.remove(&key), None),
            (orphans.recurrences.remove(&key), Some((&tables::RECURRENCES, "obj_guid"))),
            (orphans.budget_amounts.remove(&key), Some((&tables::BUDGET_AMOUNTS, "budget_guid"))),
            (orphans.taxtable_entries.remove(&key), Some((&tables::TAXTABLE_ENTRIES, "taxtable"))),
            (orphans.splits.remove(&key), Some((&tables::SPLITS, "tx_guid"))),
        ];
        found.into_iter().filter(|(was, _)| *was).try_for_each(|(_, table)| {
            match table {
                None => self.delete_slot_tree(key).map(drop),
                Some((table, column)) => self.st.run(&format!("DELETE FROM \"{}\" WHERE \"{column}\" = ?1", table.name), &[guid(key)]).map(drop),
            }
        })
    }

    /// Make sure a commodity a written row refers to has a row: from the Book, or the ISO table for a currency, as
    /// GnuCash's `save_commodity` does (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:884-893`).
    fn ensure(&mut self, id: &CommodityRef) -> Result<(), SqlError> {
        if self.residue.commodities.contains_key(id) {
            return Ok(());
        }
        let found: Option<Commodity> = self.book.commodities.get(id).map(|c| (**c).clone()).or_else(|| {
            id.is_currency().then(|| iso_currency(&id.mnemonic)).flatten().map(red_ale_core::currencies::IsoCurrency::to_commodity)
        });
        let commodity = found.ok_or_else(|| SqlError::Unstorable(format!("commodity {id} is used but is neither in the book nor a currency")))?;
        let row = self.env.new_guid();
        self.residue.commodities.insert(id.clone(), row);
        self.insert_row(&tables::COMMODITIES, encode::commodity(row, &commodity))?;
        self.insert_slots(row, &commodity.slots)
    }

    fn ensure_all<'r>(&mut self, ids: impl IntoIterator<Item = &'r CommodityRef>) -> Result<(), SqlError> {
        ids.into_iter().try_for_each(|id| self.ensure(id))
    }

    fn commodities(&mut self, saved: &Book) -> Result<(), SqlError> {
        let changes = btree_changes(&saved.commodities, &self.book.commodities);
        changes.iter().try_for_each(|change| match change {
            Change::Added(n) => self.write_commodity(None, n),
            Change::Changed(o, n) => self.write_commodity(Some(o), n),
            Change::Removed(_) => Ok(()),
        })?;
        changes.iter().try_for_each(|change| match change {
            Change::Removed(o) => match self.residue.commodities.shift_remove(&o.id) {
                Some(row) => {
                    self.delete_row(&tables::COMMODITIES, row)?;
                    self.delete_slots(row)
                }
                None => Ok(()),
            },
            _ => Ok(()),
        })
    }

    /// A commodity added or changed: updated if it has a row, else inserted with a new GUID. A renamed commodity is a
    /// removal and an addition, so its new row has a new GUID.
    fn write_commodity(&mut self, old: Option<&Commodity>, new: &Commodity) -> Result<(), SqlError> {
        let Some(row) = self.residue.commodities.get(&new.id).copied() else { return self.ensure(&new.id) };
        let empty = Frame::new();
        let (old_cells, old_slots) = old.map_or((Vec::new(), &empty), |o| (encode::commodity(row, o), &o.slots));
        self.update_if_changed(&tables::COMMODITIES, &old_cells, encode::commodity(row, new))?;
        self.rewrite_slots(row, old_slots, &new.slots)
    }

    /// The Book's own Slots; the `books` row never changes.
    fn book_slots(&mut self, saved: &Book) -> Result<(), SqlError> {
        self.rewrite_slots(self.book.guid, &saved.slots, &self.book.slots)
    }

    /// Mark every lot a written split entered, left or changed in as `is_closed` unknown, so GnuCash recomputes it.
    fn mark_touched_lots(&mut self) -> Result<(), SqlError> {
        let lots: Vec<Guid> = self.touched_lots.difference(&self.written_lots).copied().collect();
        lots.into_iter().filter_map(|g| self.book.lots.get(&g).cloned()).try_for_each(|lot| self.update_row(&tables::LOTS, encode::lot(&lot)))
    }

    /// Change one written cell behind the save's back, for the read-back to find ([`Faults::corrupt`]).
    fn corrupt(&mut self) -> Result<(), SqlError> {
        let Some(Check::Row(table, cells)) = self.checks.iter().find(|c| matches!(c, Check::Row(..))) else { return Ok(()) };
        let column = sql::written_columns(table).get(1).copied().unwrap_or("guid");
        let sql = format!("UPDATE \"{}\" SET \"{column}\" = 'corrupted' WHERE guid = ?1", table.name);
        let key = cells[0].clone();
        self.st.run(&sql, &[key]).map(drop)
    }

    /// Read back everything written and compare it with what was meant.
    fn verify(&self) -> Result<(), SqlError> {
        self.checks.iter().try_for_each(|check| self.verify_one(check))
    }

    fn verify_one(&self, check: &Check) -> Result<(), SqlError> {
        let mismatch = |what: String| Err(SqlError::Verification(format!("{what} did not read back as written; the save was rolled back")));
        match check {
            Check::Row(table, cells) => {
                let found = self.st.rows(&select(table, "guid"), &cells[..1])?;
                if found != [cells.clone()] {
                    return mismatch(format!("the {} row {:?}", table.name, cells[0]));
                }
            }
            Check::Gone(sql, params) => {
                if !self.st.rows(sql, params)?.is_empty() {
                    return mismatch(format!("a deleted row ({sql})"));
                }
            }
            Check::Slots(rows) => {
                let mut owners: Vec<&Cell> = rows.iter().map(|r| &r[0]).collect();
                owners.dedup();
                for owner in owners {
                    let want: Vec<Vec<Cell>> = rows.iter().filter(|r| &r[0] == owner).cloned().collect();
                    if self.st.rows(&select(&tables::SLOTS, "obj_guid"), std::slice::from_ref(owner))? != want {
                        return mismatch(format!("the slot rows of {owner:?}"));
                    }
                }
            }
            Check::Dependents(table, column, owner, rows) => {
                if &self.st.rows(&select(table, column), &[guid(*owner)])? != rows {
                    return mismatch(format!("the {} rows of {owner}", table.name));
                }
            }
            Check::SplitOrder(t) => {
                if transactions::read_order(&self.st, t)? != t.splits.iter().map(|s| s.guid).collect::<Vec<_>>() {
                    return mismatch(format!("the split order of transaction {}", t.guid));
                }
            }
        }
        Ok(())
    }
}
