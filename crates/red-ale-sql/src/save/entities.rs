//! The rows of every entity but transactions: one row per entity, its slot rows, and the dependent rows some own
//! (`design/sql-fidelity.md` §4, "Which rows an entity writes").

use red_ale_core::business::{BillTerm, Customer, Employee, Entry, Invoice, Job, Order, TaxTable, Vendor};
use red_ale_core::diff::{Change, changes};
use red_ale_core::{Account, Book, Budget, CommodityRef, Frame, Guid, Lot, Price, ScheduledTransaction};

use super::{Writer, btree_changes};
use crate::cell::{Cell, Table};
use crate::encode::{self, Refs, business as biz};
use crate::error::SqlError;
use crate::tables;

/// How an entity is stored: its table, its key, its Slots, the commodities its row refers to, and its row.
pub(super) struct Kind<T> {
    pub(super) table: &'static Table,
    pub(super) guid: fn(&T) -> Guid,
    pub(super) slots: fn(&T) -> Option<&Frame>,
    pub(super) uses: fn(&T) -> Vec<&CommodityRef>,
    pub(super) encode: fn(&T, Refs<'_>) -> Vec<Cell>,
}

fn none<T>(_: &T) -> Vec<&CommodityRef> {
    Vec::new()
}

/// The `budget_amounts` rows of a budget, in GnuCash's order: the Account tree's descendants depth first, periods
/// ascending, set amounts only (`5.17:libgnucash/backend/sql/gnc-budget-sql.cpp:251-289`).
fn amount_rows(book: &Book, b: &Budget) -> Vec<Vec<Cell>> {
    book.descendants(book.root)
        .iter()
        .flat_map(|a| (0..b.num_periods).filter_map(move |p| b.amount(a.guid, p).map(|n| encode::budget_amount(b.guid, a.guid, p, n))))
        .collect()
}

impl Writer<'_, '_> {
    /// Write one kind of entity's changes: additions and changes in order, then removals.
    pub(super) fn entities<T: PartialEq>(&mut self, kind: &Kind<T>, changes: &[Change<'_, T>]) -> Result<(), SqlError> {
        changes.iter().try_for_each(|c| match c {
            Change::Added(n) => self.add(kind, n),
            Change::Changed(o, n) => self.change(kind, o, n),
            Change::Removed(_) => Ok(()),
        })?;
        changes.iter().try_for_each(|c| match c {
            Change::Removed(o) => self.remove(kind, o),
            _ => Ok(()),
        })
    }

    fn add<T>(&mut self, kind: &Kind<T>, new: &T) -> Result<(), SqlError> {
        self.ensure_all((kind.uses)(new))?;
        let key = (kind.guid)(new);
        self.adopt(key)?;
        self.insert_row(kind.table, (kind.encode)(new, self.refs()))?;
        (kind.slots)(new).map_or(Ok(()), |f| self.insert_slots(key, f))
    }

    fn change<T>(&mut self, kind: &Kind<T>, old: &T, new: &T) -> Result<(), SqlError> {
        self.ensure_all((kind.uses)(new))?;
        let cells = ((kind.encode)(old, self.refs()), (kind.encode)(new, self.refs()));
        self.update_if_changed(kind.table, &cells.0, cells.1)?;
        match ((kind.slots)(old), (kind.slots)(new)) {
            (Some(o), Some(n)) => self.rewrite_slots((kind.guid)(new), o, n),
            _ => Ok(()),
        }
    }

    fn remove<T>(&mut self, kind: &Kind<T>, old: &T) -> Result<(), SqlError> {
        let key = (kind.guid)(old);
        self.delete_row(kind.table, key)?;
        (kind.slots)(old).map_or(Ok(()), |_| self.delete_slots(key))
    }

    /// The Account tree and the template accounts. The two roots' rows are never inserted or deleted: GnuCash writes
    /// both in every book, and the Book holds the Template root only while it has children.
    pub(super) fn accounts(&mut self, saved: &Book) -> Result<(), SqlError> {
        let kind = Kind::<Account> {
            table: &tables::ACCOUNTS,
            guid: |a| a.guid,
            slots: |a| Some(&a.slots),
            uses: |a| a.commodity.iter().collect(),
            encode: encode::account,
        };
        let book = self.book;
        let template_root = self.residue.template_root;
        if book.templates.root.is_some_and(|r| r != template_root) {
            return Err(SqlError::Unstorable(format!("the book's template root is not {template_root}, the one its books row names")));
        }
        let mut all = changes(&saved.accounts, &book.accounts);
        all.extend(changes(&saved.templates.accounts, &book.templates.accounts));
        let (roots, others): (Vec<_>, Vec<_>) = all.into_iter().partition(|c| {
            let guid = c.after().map_or_else(|| removed_guid(c), |a| a.guid);
            guid == template_root || guid == book.root
        });
        self.entities(&kind, &others)?;
        roots.iter().try_for_each(|c| match c {
            Change::Added(n) => {
                self.adopt(n.guid)?;
                self.update_row(&tables::ACCOUNTS, encode::account(n, self.refs()))?;
                self.insert_slots(n.guid, &n.slots)
            }
            Change::Changed(o, n) => self.change(&kind, o, n),
            Change::Removed(_) => Ok(()),
        })
    }

    pub(super) fn budgets(&mut self, saved: &Book) -> Result<(), SqlError> {
        let kind = Kind::<Budget> {
            table: &tables::BUDGETS,
            guid: |b| b.guid,
            slots: |b| Some(&b.slots),
            uses: none,
            encode: |b, _| encode::budget(b),
        };
        let changes = changes(&saved.budgets, &self.book.budgets);
        self.entities(&kind, &changes)?;
        changes.iter().try_for_each(|c| self.budget_dependents(saved, c))
    }

    /// A budget's one recurrence and its `budget_amounts`, deleted and inserted again whole when they change.
    fn budget_dependents(&mut self, saved: &Book, change: &Change<'_, Budget>) -> Result<(), SqlError> {
        let (old, new) = match change {
            Change::Added(n) => (None, Some(*n)),
            Change::Changed(o, n) => (Some(*o), Some(*n)),
            Change::Removed(o) => (Some(*o), None),
        };
        let key = new.or(old).map_or(Guid::NULL, |b| b.guid);
        if old.map(|b| &b.recurrence) != new.map(|b| &b.recurrence) {
            if old.is_some() {
                self.delete_dependents(&tables::RECURRENCES, "obj_guid", key, 1)?;
            }
            let rows = new.map(|b| encode::recurrence(b.guid, &b.recurrence)).into_iter().collect();
            self.insert_dependents(&tables::RECURRENCES, "obj_guid", key, rows)?;
        }
        let amounts_changed = old.map(|b| (&b.slots, b.num_periods)) != new.map(|b| (&b.slots, b.num_periods));
        if amounts_changed {
            let before = old.map_or(0, |b| amount_rows(saved, b).len());
            self.delete_dependents(&tables::BUDGET_AMOUNTS, "budget_guid", key, before)?;
            let rows = new.map(|b| amount_rows(self.book, b)).unwrap_or_default();
            self.insert_dependents(&tables::BUDGET_AMOUNTS, "budget_guid", key, rows)?;
        }
        Ok(())
    }

    pub(super) fn prices(&mut self, saved: &Book) -> Result<(), SqlError> {
        let kind = Kind::<Price> {
            table: &tables::PRICES,
            guid: |p| p.guid,
            slots: |_| None,
            uses: |p| vec![&p.commodity, &p.currency],
            encode: encode::price,
        };
        self.entities(&kind, &changes(&saved.prices, &self.book.prices))
    }

    pub(super) fn lots(&mut self, saved: &Book) -> Result<(), SqlError> {
        let kind = Kind::<Lot> {
            table: &tables::LOTS,
            guid: |l| l.guid,
            slots: |l| Some(&l.slots),
            uses: none,
            encode: |l, _| encode::lot(l),
        };
        let changes = changes(&saved.lots, &self.book.lots);
        self.written_lots.extend(changes.iter().filter_map(Change::after).map(|l| l.guid));
        self.entities(&kind, &changes)
    }

    pub(super) fn scheduled(&mut self, saved: &Book) -> Result<(), SqlError> {
        let kind = Kind::<ScheduledTransaction> {
            table: &tables::SCHEDXACTIONS,
            guid: |s| s.guid,
            slots: |s| Some(&s.slots),
            uses: none,
            encode: |s, _| encode::scheduled(s),
        };
        let changes = changes(&saved.scheduled, &self.book.scheduled);
        self.entities(&kind, &changes)?;
        changes.iter().try_for_each(|c| {
            let (old, new) = (before(c).map(|s| &s.schedule), c.after().map(|s| &s.schedule));
            let key = c.after().or(before(c)).map_or(Guid::NULL, |s| s.guid);
            if old == new {
                return Ok(());
            }
            self.delete_dependents(&tables::RECURRENCES, "obj_guid", key, old.map_or(0, Vec::len))?;
            let rows = new.into_iter().flatten().map(|r| encode::recurrence(key, r)).collect();
            self.insert_dependents(&tables::RECURRENCES, "obj_guid", key, rows)
        })
    }

    pub(super) fn business(&mut self, saved: &Book) -> Result<(), SqlError> {
        let (old, new) = (&saved.business, &self.book.business);
        macro_rules! kind {
            ($t:ty, $table:expr, $encode:expr, $uses:expr) => {
                Kind::<$t> {
                    table: &$table,
                    guid: |x| x.guid,
                    slots: |x| Some(&x.slots),
                    uses: $uses,
                    encode: $encode,
                }
            };
        }
        self.entities(&kind!(BillTerm, tables::BILLTERMS, |x, _| biz::bill_term(x), none), &btree_changes(&old.bill_terms, &new.bill_terms))?;
        self.entities(&kind!(Customer, tables::CUSTOMERS, biz::customer, customer_uses), &btree_changes(&old.customers, &new.customers))?;
        self.entities(&kind!(Employee, tables::EMPLOYEES, biz::employee, employee_uses), &btree_changes(&old.employees, &new.employees))?;
        self.entities(&kind!(Entry, tables::ENTRIES, |x, _| biz::entry(x), none), &btree_changes(&old.entries, &new.entries))?;
        self.entities(&kind!(Invoice, tables::INVOICES, biz::invoice, invoice_uses), &btree_changes(&old.invoices, &new.invoices))?;
        self.entities(&kind!(Job, tables::JOBS, |x, _| biz::job(x), none), &btree_changes(&old.jobs, &new.jobs))?;
        self.entities(&kind!(Order, tables::ORDERS, |x, _| biz::order(x), none), &btree_changes(&old.orders, &new.orders))?;
        let tax_tables = btree_changes(&old.tax_tables, &new.tax_tables);
        self.entities(&kind!(TaxTable, tables::TAXTABLES, |x, _| biz::tax_table(x), none), &tax_tables)?;
        tax_tables.iter().try_for_each(|c| {
            let (old_entries, new_entries) = (before(c).map(|t| &t.entries), c.after().map(|t| &t.entries));
            let key = c.after().or(before(c)).map_or(Guid::NULL, |t| t.guid);
            if old_entries == new_entries {
                return Ok(());
            }
            self.delete_dependents(&tables::TAXTABLE_ENTRIES, "taxtable", key, old_entries.map_or(0, Vec::len))?;
            let rows = new_entries.into_iter().flatten().map(|e| biz::tax_table_entry(key, e)).collect();
            self.insert_dependents(&tables::TAXTABLE_ENTRIES, "taxtable", key, rows)
        })?;
        self.entities(&kind!(Vendor, tables::VENDORS, biz::vendor, vendor_uses), &btree_changes(&old.vendors, &new.vendors))
    }
}

/// The entity before a change, unless it was added.
fn before<'a, T>(c: &Change<'a, T>) -> Option<&'a T> {
    match c {
        Change::Changed(o, _) | Change::Removed(o) => Some(o),
        Change::Added(_) => None,
    }
}

fn customer_uses(x: &Customer) -> Vec<&CommodityRef> {
    x.currency.iter().collect()
}

fn employee_uses(x: &Employee) -> Vec<&CommodityRef> {
    x.currency.iter().collect()
}

fn invoice_uses(x: &Invoice) -> Vec<&CommodityRef> {
    x.currency.iter().collect()
}

fn vendor_uses(x: &Vendor) -> Vec<&CommodityRef> {
    x.currency.iter().collect()
}

fn removed_guid(c: &Change<'_, Account>) -> Guid {
    match c {
        Change::Removed(o) | Change::Changed(o, _) => o.guid,
        Change::Added(n) => n.guid,
    }
}
