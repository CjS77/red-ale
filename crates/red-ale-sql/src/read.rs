//! Reading a whole book: every table red-ale models into one Book, with the SQL Residue and the findings that make it
//! read-only (`design/sql-fidelity.md` §3).
//!
//! Every table is read with explicit column names and `ORDER BY rowid`, so entities enter the Book's maps in the
//! file's order, and rows are decoded as they are stepped and not kept. A column the book lacks reads as NULL: the
//! schema check has already made that a finding.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use indexmap::IndexMap;
use red_ale_core::business::Business;
use red_ale_core::commodity::TEMPLATE_NAMESPACE;
use red_ale_core::{
    Account, Book, Budget, Commodity, CommodityRef, FixedEnv, Frame, Guid, Lot, Price, Recurrence, ScheduledTransaction, SlotValue,
    Templates, Time64, Transaction,
};
use rusqlite::Connection;
use rusqlite::types::ValueRef;

use crate::Findings;
use crate::cell::{Cell, Row, Table};
use crate::error::SqlError;
use crate::residue::{Orphans, SqlResidue};
use crate::rows::{self, AccountRow, AmountRow, BookRow, BudgetRow, Lookup, business};
use crate::schema::{self, VERSIONS_TABLE};
use crate::slots::{self, Owner};
use crate::tables;

fn cell(value: ValueRef<'_>) -> Cell {
    match value {
        ValueRef::Null => Cell::Null,
        ValueRef::Integer(i) => Cell::Integer(i),
        ValueRef::Real(r) => Cell::Real(r),
        ValueRef::Text(t) => Cell::Text(t.to_vec()),
        ValueRef::Blob(b) => Cell::Blob(b.to_vec()),
    }
}

/// The columns a table has, empty when it does not exist.
fn existing_columns(conn: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM pragma_table_xinfo(?1)")?;
    stmt.query_map([table], |r| r.get(0))?.collect()
}

/// Step through a table's rows in `rowid` order, handing each to `f`. A table the book lacks has no rows.
fn each_row(conn: &Connection, table: &Table, mut f: impl FnMut(Row)) -> Result<(), SqlError> {
    let existing = existing_columns(conn, table.name)?;
    if existing.is_empty() {
        return Ok(());
    }
    let columns: Vec<String> = table
        .columns
        .iter()
        .map(|c| if existing.iter().any(|e| e == c) { format!("\"{c}\"") } else { "NULL".to_owned() })
        .collect();
    let sql = format!("SELECT rowid, {} FROM \"{}\" ORDER BY rowid", columns.join(", "), table.name);
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let rowid = r.get(0)?;
        let cells = (1..=table.columns.len()).map(|i| r.get_ref(i).map(cell)).collect::<Result<_, _>>()?;
        f(Row { rowid, cells });
    }
    Ok(())
}

/// Decode every row of a table with `decode`, keeping those that enter the Book.
fn decode_all<T>(
    conn: &Connection,
    table: &Table,
    findings: &mut Vec<String>,
    mut decode: impl FnMut(&Row, &mut Vec<String>) -> Option<T>,
) -> Result<Vec<T>, SqlError> {
    let mut out = Vec::new();
    each_row(conn, table, |row| out.extend(decode(&row, findings)))?;
    Ok(out)
}

/// Read the book on `conn`, which the caller has opened and put in one read transaction.
pub(crate) fn read(conn: &Connection) -> Result<(Book, SqlResidue, Findings), SqlError> {
    let mut findings = Findings {
        unrecognised: schema::check(conn).map_err(|e| match e {
            SqlError::Sqlite(e) => SqlError::from_sqlite(e),
            other => other,
        })?,
        schema_versions: Vec::new(),
    };
    let mut version_rows = Vec::new();
    each_row(conn, &VERSIONS_TABLE, |row| version_rows.push(row))?;
    let (differ, versions) = schema::check_versions(&version_rows, &mut findings.unrecognised);
    findings.schema_versions = differ;
    let mut reader = Reader::new(conn, findings.unrecognised);
    let (book, residue) = reader.book(versions)?;
    findings.unrecognised = reader.findings;
    Ok((book, residue, findings))
}

/// Entities by GUID, in `rowid` order.
type Entities<T> = IndexMap<Guid, Arc<T>>;

/// The Book's commodities, and each one's row GUID in `rowid` order.
type CommodityRows = (std::collections::BTreeMap<CommodityRef, Arc<Commodity>>, IndexMap<CommodityRef, Guid>);

/// Builds the Book table by table.
struct Reader<'c> {
    conn: &'c Connection,
    findings: Vec<String>,
    lookup: Lookup,
    orphans: Orphans,
}

/// The two account trees, as read.
struct Trees {
    accounts: IndexMap<Guid, Arc<Account>>,
    templates: IndexMap<Guid, Arc<Account>>,
    /// Every account row, for the hidden and placeholder check.
    rows: Vec<AccountRow>,
}

impl<'c> Reader<'c> {
    fn new(conn: &'c Connection, findings: Vec<String>) -> Self {
        Self {
            conn,
            findings,
            lookup: Lookup::default(),
            orphans: Orphans::default(),
        }
    }

    fn book(&mut self, versions: Vec<(String, i64)>) -> Result<(Book, SqlResidue), SqlError> {
        let ids = self.book_row()?;
        let (commodities, residue_commodities) = self.commodities()?;
        let trees = self.accounts(&ids)?;
        let lots = self.lots(&trees)?;
        let (transactions, template_transactions) = self.transactions(&trees, &lots)?;
        let prices = self.prices()?;
        let mut recurrences = self.recurrences()?;
        let budgets = self.budgets(&mut recurrences)?;
        let scheduled = self.scheduled(&mut recurrences, &trees)?;
        self.orphans.recurrences.extend(recurrences.into_keys());
        let business = self.business()?;
        let mut book = Book {
            guid: ids.guid,
            slots: Frame::new(),
            commodities,
            root: ids.root,
            accounts: trees.accounts,
            transactions,
            prices,
            lots,
            business,
            budgets,
            templates: Templates {
                root: trees.templates.contains_key(&ids.template_root).then_some(ids.template_root),
                accounts: trees.templates,
                transactions: template_transactions,
            },
            scheduled,
        };
        let slot_rows = self.slots(&mut book, &residue_commodities)?;
        self.check_flags(&book, &trees.rows);
        self.check_budget_amounts(&book)?;
        add_template_commodity(&mut book);
        let residue = SqlResidue {
            commodities: residue_commodities,
            template_root: ids.template_root,
            slot_rows,
            orphans: std::mem::take(&mut self.orphans),
            versions,
        };
        Ok((book, residue))
    }

    fn book_row(&mut self) -> Result<BookRow, SqlError> {
        let books = decode_all(self.conn, &tables::BOOKS, &mut self.findings, rows::book)?;
        if books.len() > 1 {
            self.findings.push(format!("books has {} rows; GnuCash reads one", books.len()));
        }
        books.into_iter().next().ok_or_else(|| SqlError::NotABook("the books table has no row".into()))
    }

    fn commodities(&mut self) -> Result<CommodityRows, SqlError> {
        let read = decode_all(self.conn, &tables::COMMODITIES, &mut self.findings, rows::commodity)?;
        let mut commodities = std::collections::BTreeMap::new();
        let mut guids = IndexMap::new();
        for (guid, c) in read {
            if guids.insert(c.id.clone(), guid).is_some() {
                self.findings.push(format!("commodities: {} has more than one row", c.id));
            }
            self.lookup.commodities.insert(guid, c.id.clone());
            commodities.insert(c.id.clone(), Arc::new(c));
        }
        Ok((commodities, guids))
    }

    /// The Account tree from the root and the template accounts from the Template root, each in `rowid` order. The
    /// Template root is kept even with no children: the `books` row names it (`5.17:libgnucash/backend/sql/
    /// gnc-book-sql.cpp:67-69`, `:119-136`), so the first scheduled transaction goes under it rather than under a
    /// new root the row doesn't name. An XML book writes the root only with children (`Templates::has_descendants`).
    fn accounts(&mut self, ids: &BookRow) -> Result<Trees, SqlError> {
        let lookup = self.lookup.clone();
        let rows = decode_all(self.conn, &tables::ACCOUNTS, &mut self.findings, |row, f| rows::account(row, &lookup, f))?;
        let main = reachable(&rows, ids.root);
        let template = reachable(&rows, ids.template_root);
        if !main.contains(&ids.root) {
            return Err(SqlError::NotABook(format!("the root account {} has no row", ids.root)));
        }
        rows.iter()
            .filter(|r| !main.contains(&r.account.guid) && !template.contains(&r.account.guid))
            .for_each(|r| self.findings.push(format!("accounts: {} is reachable from neither root", r.account.guid)));
        let pick = |set: &HashSet<Guid>| -> IndexMap<Guid, Arc<Account>> {
            rows.iter()
                .filter(|r| set.contains(&r.account.guid))
                .map(|r| (r.account.guid, Arc::new(r.account.clone())))
                .collect()
        };
        let accounts = pick(&main);
        let templates = pick(&template);
        Ok(Trees { accounts, templates, rows })
    }

    fn lots(&mut self, trees: &Trees) -> Result<IndexMap<Guid, Arc<Lot>>, SqlError> {
        let read = decode_all(self.conn, &tables::LOTS, &mut self.findings, rows::lot)?;
        Ok(read
            .into_iter()
            .filter(|lot| {
                let known = trees.accounts.contains_key(&lot.account);
                if !known {
                    self.findings.push(format!("lots: {} is in account {}, which is not in the Account tree", lot.guid, lot.account));
                }
                known
            })
            .map(|lot| (lot.guid, Arc::new(lot)))
            .collect())
    }

    /// Transactions with their splits, in `rowid` order, split into the Book's and the template transactions. Each
    /// transaction's splits come in `rowid` order, then sorted as GnuCash's commit sorts them.
    fn transactions(
        &mut self,
        trees: &Trees,
        lots: &IndexMap<Guid, Arc<Lot>>,
    ) -> Result<(Entities<Transaction>, Entities<Transaction>), SqlError> {
        let lookup = self.lookup.clone();
        let read = decode_all(self.conn, &tables::TRANSACTIONS, &mut self.findings, |row, f| rows::transaction(row, &lookup, f))?;
        let mut all: IndexMap<Guid, Transaction> = read.into_iter().map(|t| (t.guid, t)).collect();
        let splits = decode_all(self.conn, &tables::SPLITS, &mut self.findings, rows::split)?;
        for (tx, split) in splits {
            match all.get_mut(&tx) {
                Some(t) => t.splits.push(split),
                None => {
                    self.orphans.splits.insert(tx);
                }
            }
        }
        let (mut main, mut template) = (IndexMap::new(), IndexMap::new());
        for (guid, mut t) in all {
            // GnuCash loads a transaction's splits in `rowid` order, then commits it, which sorts them as every commit
            // does (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:297`, `:397`;
            // `5.17:libgnucash/engine/Transaction.cpp:1508-1510`).
            red_ale_core::ops::commit::sort_splits(&mut t);
            self.check_splits(&t, trees, lots);
            let in_templates = |s: &red_ale_core::Split| s.account.is_some_and(|a| trees.templates.contains_key(&a));
            let target = if !t.splits.is_empty() && t.splits.iter().all(in_templates) {
                &mut template
            } else {
                if t.splits.iter().any(in_templates) {
                    self.findings.push(format!("transactions: {guid} has splits in both the Account tree and the template accounts"));
                }
                &mut main
            };
            target.insert(guid, Arc::new(t));
        }
        Ok((main, template))
    }

    fn check_splits(&mut self, t: &Transaction, trees: &Trees, lots: &IndexMap<Guid, Arc<Lot>>) {
        for s in &t.splits {
            if let Some(a) = s.account.filter(|a| !trees.accounts.contains_key(a) && !trees.templates.contains_key(a)) {
                self.findings.push(format!("splits: {} refers to account {a}, which has no row in either tree", s.guid));
            }
            if let Some(l) = s.lot.filter(|l| !lots.contains_key(l)) {
                self.findings.push(format!("splits: {} refers to lot {l}, which has no row", s.guid));
            }
        }
    }

    fn prices(&mut self) -> Result<IndexMap<Guid, Arc<Price>>, SqlError> {
        let lookup = self.lookup.clone();
        let read = decode_all(self.conn, &tables::PRICES, &mut self.findings, |row, f| rows::price(row, &lookup, f))?;
        Ok(read.into_iter().map(|p| (p.guid, Arc::new(p))).collect())
    }

    /// Recurrences by owner, each owner's in `rowid` order, which is the Schedule's order
    /// (`5.17:libgnucash/backend/sql/gnc-recurrence-sql.cpp:348-361`).
    fn recurrences(&mut self) -> Result<IndexMap<Guid, Vec<Recurrence>>, SqlError> {
        let read = decode_all(self.conn, &tables::RECURRENCES, &mut self.findings, rows::recurrence)?;
        let mut by_owner: IndexMap<Guid, Vec<Recurrence>> = IndexMap::new();
        read.into_iter().for_each(|(owner, r)| by_owner.entry(owner).or_default().push(r));
        Ok(by_owner)
    }

    /// Budgets, each with exactly one Recurrence: GnuCash warns at none or several and takes the first
    /// (`5.17:libgnucash/backend/sql/gnc-recurrence-sql.cpp:323-345`).
    fn budgets(&mut self, recurrences: &mut IndexMap<Guid, Vec<Recurrence>>) -> Result<IndexMap<Guid, Arc<Budget>>, SqlError> {
        let read: Vec<BudgetRow> = decode_all(self.conn, &tables::BUDGETS, &mut self.findings, rows::budget)?;
        Ok(read
            .into_iter()
            .filter_map(|b| {
                let found = recurrences.shift_remove(&b.guid).unwrap_or_default();
                if found.len() != 1 {
                    self.findings.push(format!("budgets: {} has {} recurrences; GnuCash reads one", b.guid, found.len()));
                }
                let recurrence = found.into_iter().next()?;
                let budget = Budget {
                    guid: b.guid,
                    name: b.name,
                    description: b.description,
                    num_periods: b.num_periods,
                    recurrence,
                    slots: Frame::new(),
                };
                Some((budget.guid, Arc::new(budget)))
            })
            .collect())
    }

    fn scheduled(
        &mut self,
        recurrences: &mut IndexMap<Guid, Vec<Recurrence>>,
        trees: &Trees,
    ) -> Result<IndexMap<Guid, Arc<ScheduledTransaction>>, SqlError> {
        let read = decode_all(self.conn, &tables::SCHEDXACTIONS, &mut self.findings, rows::scheduled)?;
        Ok(read
            .into_iter()
            .map(|mut sx| {
                sx.schedule = recurrences.shift_remove(&sx.guid).unwrap_or_default();
                if let Some(a) = sx.template_account.filter(|a| !trees.templates.contains_key(a)) {
                    self.findings.push(format!("schedxactions: {} names template account {a}, which has no row", sx.guid));
                }
                (sx.guid, Arc::new(sx))
            })
            .collect())
    }

    fn business(&mut self) -> Result<Business, SqlError> {
        let (conn, lookup) = (self.conn, self.lookup.clone());
        let f = &mut self.findings;
        let mut terms = decode_all(conn, &tables::BILLTERMS, f, business::bill_term)?;
        set_children(&mut terms, |t| t.guid, |t| t.parent, |t, c| t.child = Some(c));
        let mut b = Business {
            bill_terms: keyed(terms, |t| t.guid),
            customers: keyed(decode_all(conn, &tables::CUSTOMERS, f, |r, f| business::customer(r, &lookup, f))?, |c| c.guid),
            employees: keyed(decode_all(conn, &tables::EMPLOYEES, f, |r, f| business::employee(r, &lookup, f))?, |e| e.guid),
            entries: keyed(decode_all(conn, &tables::ENTRIES, f, business::entry)?, |e| e.guid),
            invoices: keyed(decode_all(conn, &tables::INVOICES, f, |r, f| business::invoice(r, &lookup, f))?, |i| i.guid),
            jobs: keyed(decode_all(conn, &tables::JOBS, f, business::job)?, |j| j.guid),
            orders: keyed(decode_all(conn, &tables::ORDERS, f, business::order)?, |o| o.guid),
            tax_tables: std::collections::BTreeMap::new(),
            vendors: keyed(decode_all(conn, &tables::VENDORS, f, |r, f| business::vendor(r, &lookup, f))?, |v| v.guid),
        };
        let mut tax_tables = decode_all(conn, &tables::TAXTABLES, f, business::tax_table)?;
        let entries = decode_all(conn, &tables::TAXTABLE_ENTRIES, f, business::tax_table_entry)?;
        for (table, entry) in entries {
            match tax_tables.iter_mut().find(|t| t.guid == table) {
                Some(t) => t.entries.push(entry),
                None => {
                    self.orphans.taxtable_entries.insert(table);
                }
            }
        }
        set_children(&mut tax_tables, |t| t.guid, |t| t.parent, |t, c| t.child = Some(c));
        b.tax_tables = keyed(tax_tables, |t| t.guid);
        Ok(b)
    }
}

/// Business objects by GUID, the order the Book keeps them in.
fn keyed<T>(items: Vec<T>, guid: impl Fn(&T) -> Guid) -> std::collections::BTreeMap<Guid, Arc<T>> {
    items.into_iter().map(|t| (guid(&t), Arc::new(t))).collect()
}

/// Set each parent's child to the last of its children in `rowid` order, as loading each child sets it
/// (`5.17:libgnucash/backend/sql/gnc-bill-term-sql.cpp:154-175`, `gnc-tax-table-sql.cpp:175-197`).
fn set_children<T>(items: &mut [T], guid: impl Fn(&T) -> Guid, parent: impl Fn(&T) -> Option<Guid>, set: impl Fn(&mut T, Guid)) {
    let links: Vec<(Guid, Guid)> = items.iter().filter_map(|t| parent(t).map(|p| (p, guid(t)))).collect();
    links.into_iter().for_each(|(p, child)| {
        if let Some(parent) = items.iter_mut().find(|t| guid(t) == p) {
            set(parent, child);
        }
    });
}

/// The GUIDs reachable from `root` through `parent_guid`, `root` included if it has a row.
fn reachable(rows: &[AccountRow], root: Guid) -> HashSet<Guid> {
    let mut children: HashMap<Guid, Vec<Guid>> = HashMap::new();
    rows.iter()
        .filter_map(|r| r.account.parent.map(|p| (p, r.account.guid)))
        .for_each(|(p, c)| children.entry(p).or_default().push(c));
    let mut seen = HashSet::new();
    let mut stack: Vec<Guid> = rows.iter().filter(|r| r.account.guid == root).map(|r| r.account.guid).collect();
    while let Some(g) = stack.pop() {
        if seen.insert(g) {
            stack.extend(children.get(&g).into_iter().flatten().copied());
        }
    }
    seen
}

/// Every book has the `template` commodity; GnuCash writes its row only once a template account uses it
/// (`5.17:libgnucash/engine/SchedXaction.cpp:398-401`), so without one the reader adds it as `Book::new_empty` does.
fn add_template_commodity(book: &mut Book) {
    if book.commodities.keys().any(|c| c.namespace == TEMPLATE_NAMESPACE) {
        return;
    }
    let empty = Book::new_empty(&mut FixedEnv::new(Time64(0), 0));
    book.commodities.extend(empty.commodities);
}

/// True when an account Slot says yes, as `xaccAccountGetHidden` and `GetPlaceholder` read it: the string `true` or a
/// non-zero integer (`5.17:libgnucash/engine/Account.cpp:2563-2577`).
fn slot_truth(slots: &Frame, key: &str) -> bool {
    match slots.get_key(key) {
        Some(SlotValue::String(s)) => s == "true",
        Some(SlotValue::Integer(i)) => *i != 0,
        _ => false,
    }
}

impl Reader<'_> {
    /// Read the slot rows into every entity's Slots; returns each entity's row count.
    fn slots(
        &mut self,
        book: &mut Book,
        commodity_guids: &IndexMap<CommodityRef, Guid>,
    ) -> Result<std::collections::BTreeMap<Guid, usize>, SqlError> {
        let read = decode_all(self.conn, &tables::SLOTS, &mut self.findings, rows::slot)?;
        let entities = entity_guids(book, commodity_guids);
        let prices: HashSet<Guid> = book.prices.keys().copied().collect();
        let owner = |g: Guid| {
            if entities.contains(&g) {
                Owner::Entity
            } else if prices.contains(&g) {
                Owner::NoSlots
            } else {
                Owner::Unknown
            }
        };
        let built = slots::build(&read, &owner, &mut self.findings);
        self.orphans.slots = built.orphans;
        let mut frames = built.frames;
        assign_slots(book, commodity_guids, &mut |g| frames.remove(&g).unwrap_or_default());
        Ok(built.rows)
    }

    /// `accounts.hidden` and `placeholder` must agree with the account's Slots, which the engine keeps
    /// (`design/sql-fidelity.md` §3, "Columns that duplicate a Slot").
    fn check_flags(&mut self, book: &Book, rows: &[AccountRow]) {
        for r in rows {
            let guid = r.account.guid;
            let Some(account) = book.accounts.get(&guid).or_else(|| book.templates.accounts.get(&guid)) else { continue };
            for (name, column) in [("hidden", r.hidden), ("placeholder", r.placeholder)] {
                if column.unwrap_or(false) != slot_truth(&account.slots, name) {
                    self.findings.push(format!("accounts: {guid}'s {name} column disagrees with its {name} Slot"));
                }
            }
        }
    }

    /// `budget_amounts` must hold exactly one row per amount Slot of an account in the Account tree and a period
    /// below `num_periods`, with the same numerator and denominator, as GnuCash writes them
    /// (`5.17:libgnucash/backend/sql/gnc-budget-sql.cpp:251-289`).
    fn check_budget_amounts(&mut self, book: &Book) -> Result<(), SqlError> {
        let rows: Vec<AmountRow> = decode_all(self.conn, &tables::BUDGET_AMOUNTS, &mut self.findings, rows::budget_amount)?;
        let mut stored: HashMap<Guid, BTreeSet<(Guid, i64, i64, i64)>> = HashMap::new();
        for r in &rows {
            if book.budgets.contains_key(&r.budget) {
                stored.entry(r.budget).or_default().insert((r.account, r.period, r.amount.num(), r.amount.denom()));
            } else {
                self.orphans.budget_amounts.insert(r.budget);
            }
        }
        let accounts = book.descendants(book.root);
        for budget in book.budgets.values() {
            let set = |a: Guid, p: u32| budget.amount(a, p).map(|n| (a, i64::from(p), n.num(), n.denom()));
            let expected: BTreeSet<(Guid, i64, i64, i64)> =
                accounts.iter().flat_map(|a| (0..budget.num_periods).filter_map(move |p| set(a.guid, p))).collect();
            if stored.remove(&budget.guid).unwrap_or_default() != expected {
                self.findings.push(format!("budget_amounts: the rows of budget {} disagree with its amount Slots", budget.guid));
            }
        }
        Ok(())
    }
}

/// Every GUID that may own Slots.
fn entity_guids(book: &Book, commodity_guids: &IndexMap<CommodityRef, Guid>) -> HashSet<Guid> {
    let b = &book.business;
    let transactions = book.transactions.values().chain(book.templates.transactions.values());
    std::iter::once(book.guid)
        .chain(commodity_guids.values().copied())
        .chain(book.accounts.keys().chain(book.templates.accounts.keys()).copied())
        .chain(book.lots.keys().copied())
        .chain(transactions.flat_map(|t| std::iter::once(t.guid).chain(t.splits.iter().map(|s| s.guid))))
        .chain(book.budgets.keys().chain(book.scheduled.keys()).copied())
        .chain(b.bill_terms.keys().chain(b.customers.keys()).chain(b.employees.keys()).chain(b.entries.keys()).copied())
        .chain(b.invoices.keys().chain(b.jobs.keys()).chain(b.orders.keys()).chain(b.tax_tables.keys()).copied())
        .chain(b.vendors.keys().copied())
        .collect()
}

/// Give each entity the Frame `take` yields for its GUID.
fn assign_slots(book: &mut Book, commodity_guids: &IndexMap<CommodityRef, Guid>, take: &mut dyn FnMut(Guid) -> Frame) {
    book.slots = take(book.guid);
    for (id, c) in &mut book.commodities {
        if let Some(g) = commodity_guids.get(id) {
            Arc::make_mut(c).slots = take(*g);
        }
    }
    book.accounts.values_mut().chain(book.templates.accounts.values_mut()).for_each(|a| {
        let a = Arc::make_mut(a);
        a.slots = take(a.guid);
    });
    book.lots.values_mut().for_each(|l| Arc::make_mut(l).slots = take(l.guid));
    book.transactions.values_mut().chain(book.templates.transactions.values_mut()).for_each(|t| {
        let t = Arc::make_mut(t);
        t.slots = take(t.guid);
        t.splits.iter_mut().for_each(|s| s.slots = take(s.guid));
    });
    book.budgets.values_mut().for_each(|x| Arc::make_mut(x).slots = take(x.guid));
    book.scheduled.values_mut().for_each(|x| Arc::make_mut(x).slots = take(x.guid));
    let b = &mut book.business;
    b.bill_terms.values_mut().for_each(|x| Arc::make_mut(x).slots = take(x.guid));
    b.customers.values_mut().for_each(|x| Arc::make_mut(x).slots = take(x.guid));
    b.employees.values_mut().for_each(|x| Arc::make_mut(x).slots = take(x.guid));
    b.entries.values_mut().for_each(|x| Arc::make_mut(x).slots = take(x.guid));
    b.invoices.values_mut().for_each(|x| Arc::make_mut(x).slots = take(x.guid));
    b.jobs.values_mut().for_each(|x| Arc::make_mut(x).slots = take(x.guid));
    b.orders.values_mut().for_each(|x| Arc::make_mut(x).slots = take(x.guid));
    b.tax_tables.values_mut().for_each(|x| Arc::make_mut(x).slots = take(x.guid));
    b.vendors.values_mut().for_each(|x| Arc::make_mut(x).slots = take(x.guid));
}
