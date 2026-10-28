//! The row encoders write what GnuCash 5.17 wrote: every row of every SQLite fixture, decoded into the Book and
//! encoded again, gives back the cells GnuCash stored (`design/sql-fidelity.md` §4, "Which rows an entity writes").

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use red_ale_core::{Book, Guid};
use red_ale_sql::cell::{Cell, Table};
use red_ale_sql::encode::{self, Refs};
use red_ale_sql::{open_read_only, tables};
use rusqlite::types::ValueRef;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sqlite")
}

fn sqlite_fixtures() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ["gnucash", "piecash", "reports", "synthetic"]
        .iter()
        .flat_map(|dir| std::fs::read_dir(fixtures().join(dir)).unwrap().flatten().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "gnucash"))
        .collect();
    out.sort();
    out
}

fn cell(v: ValueRef<'_>) -> Cell {
    match v {
        ValueRef::Null => Cell::Null,
        ValueRef::Integer(i) => Cell::Integer(i),
        ValueRef::Real(r) => Cell::Real(r),
        ValueRef::Text(t) => Cell::Text(t.to_vec()),
        ValueRef::Blob(b) => Cell::Blob(b.to_vec()),
    }
}

/// Every row of a table, in `rowid` order, with its columns as `table` lists them, the `id` of an `AUTOINCREMENT`
/// table left out.
fn rows(conn: &rusqlite::Connection, table: &Table) -> Vec<Vec<Cell>> {
    let columns = written_columns(table);
    let sql = format!("SELECT {} FROM {} ORDER BY rowid", columns.join(", "), table.name);
    let mut stmt = conn.prepare(&sql).unwrap();
    let n = columns.len();
    stmt.query_map([], |r| Ok((0..n).map(|i| cell(r.get_ref(i).unwrap())).collect())).unwrap().map(Result::unwrap).collect()
}

/// The columns a row is written with: all but the `id` of an `AUTOINCREMENT` table, which is its first.
fn written_columns(table: &Table) -> Vec<&'static str> {
    let skip = usize::from(table.columns[0] == "id");
    table.columns[skip..].to_vec()
}

fn show(c: &Cell) -> String {
    match c {
        Cell::Text(t) => format!("{:?}", String::from_utf8_lossy(t)),
        other => format!("{other:?}"),
    }
}

/// Rows keyed by their first cell, the GUID.
fn by_key(rows: Vec<Vec<Cell>>) -> BTreeMap<Vec<u8>, Vec<Cell>> {
    rows.into_iter()
        .map(|r| {
            let Cell::Text(k) = &r[0] else { panic!("{r:?}") };
            (k.clone(), r)
        })
        .collect()
}

fn key(g: Guid) -> Vec<u8> {
    g.to_string().into_bytes()
}

struct Check<'a> {
    name: String,
    failures: &'a mut Vec<String>,
}

impl Check<'_> {
    fn same(&mut self, table: &Table, stored: Option<&Vec<Cell>>, encoded: &[Cell]) {
        let Some(stored) = stored else {
            self.failures.push(format!("{}: {} has no row for {}", self.name, table.name, show(&encoded[0])));
            return;
        };
        let columns = written_columns(table);
        stored.iter().zip(encoded).zip(&columns).filter(|((s, e), _)| s != e).for_each(|((s, e), c)| {
            self.failures.push(format!("{}: {}.{c} of {}: stored {}, encoded {}", self.name, table.name, show(&encoded[0]), show(s), show(e)));
        });
    }
}

fn check_book(name: String, conn: &rusqlite::Connection, book: &Book, guids: &indexmap::IndexMap<red_ale_core::CommodityRef, Guid>, failures: &mut Vec<String>) {
    let refs = Refs { commodities: guids };
    let mut c = Check { name, failures };
    let stored = by_key(rows(conn, &tables::COMMODITIES));
    guids.iter().for_each(|(id, g)| c.same(&tables::COMMODITIES, stored.get(&key(*g)), &encode::commodity(*g, &book.commodities[id])));
    let stored = by_key(rows(conn, &tables::ACCOUNTS));
    book.accounts
        .values()
        .chain(book.templates.accounts.values())
        .for_each(|a| c.same(&tables::ACCOUNTS, stored.get(&key(a.guid)), &encode::account(a, refs)));
    let stored = by_key(rows(conn, &tables::TRANSACTIONS));
    let splits = by_key(rows(conn, &tables::SPLITS));
    book.transactions.values().chain(book.templates.transactions.values()).for_each(|t| {
        c.same(&tables::TRANSACTIONS, stored.get(&key(t.guid)), &encode::transaction(t, refs));
        t.splits.iter().for_each(|s| c.same(&tables::SPLITS, splits.get(&key(s.guid)), &encode::split(t.guid, s)));
    });
    let stored = by_key(rows(conn, &tables::LOTS));
    book.lots.values().for_each(|l| {
        let mut e = encode::lot(l);
        // `is_closed` is the engine's cache; red-ale always writes it unknown.
        e[2] = stored.get(&key(l.guid)).map_or(Cell::Null, |r| r[2].clone());
        c.same(&tables::LOTS, stored.get(&key(l.guid)), &e);
    });
    let stored = by_key(rows(conn, &tables::PRICES));
    book.prices.values().for_each(|p| c.same(&tables::PRICES, stored.get(&key(p.guid)), &encode::price(p, refs)));
    let stored = by_key(rows(conn, &tables::BUDGETS));
    book.budgets.values().for_each(|b| c.same(&tables::BUDGETS, stored.get(&key(b.guid)), &encode::budget(b)));
    let stored = by_key(rows(conn, &tables::SCHEDXACTIONS));
    book.scheduled.values().for_each(|s| c.same(&tables::SCHEDXACTIONS, stored.get(&key(s.guid)), &encode::scheduled(s)));
    let recurrences: Vec<Vec<Cell>> = book
        .budgets
        .values()
        .map(|b| encode::recurrence(b.guid, &b.recurrence))
        .chain(book.scheduled.values().flat_map(|s| s.schedule.iter().map(|r| encode::recurrence(s.guid, r))))
        .collect();
    let stored = rows(conn, &tables::RECURRENCES);
    recurrences.iter().for_each(|e| c.same(&tables::RECURRENCES, stored.iter().find(|s| s == &e).or(stored.iter().find(|s| s[0] == e[0])), e));
    check_business(&mut c, conn, book, refs);
    check_slots(&mut c, conn, book, guids);
    check_budget_amounts(&mut c, conn, book);
}

fn check_business(c: &mut Check<'_>, conn: &rusqlite::Connection, book: &Book, refs: Refs<'_>) {
    use encode::business as b;
    let biz = &book.business;
    let t = |table: &Table| by_key(rows(conn, table));
    let s = t(&tables::BILLTERMS);
    biz.bill_terms.values().for_each(|x| c.same(&tables::BILLTERMS, s.get(&key(x.guid)), &b::bill_term(x)));
    let s = t(&tables::CUSTOMERS);
    biz.customers.values().for_each(|x| c.same(&tables::CUSTOMERS, s.get(&key(x.guid)), &b::customer(x, refs)));
    let s = t(&tables::EMPLOYEES);
    biz.employees.values().for_each(|x| c.same(&tables::EMPLOYEES, s.get(&key(x.guid)), &b::employee(x, refs)));
    let s = t(&tables::ENTRIES);
    biz.entries.values().for_each(|x| c.same(&tables::ENTRIES, s.get(&key(x.guid)), &b::entry(x)));
    let s = t(&tables::INVOICES);
    biz.invoices.values().for_each(|x| c.same(&tables::INVOICES, s.get(&key(x.guid)), &b::invoice(x, refs)));
    let s = t(&tables::JOBS);
    biz.jobs.values().for_each(|x| c.same(&tables::JOBS, s.get(&key(x.guid)), &b::job(x)));
    let s = t(&tables::ORDERS);
    biz.orders.values().for_each(|x| c.same(&tables::ORDERS, s.get(&key(x.guid)), &b::order(x)));
    let s = t(&tables::TAXTABLES);
    biz.tax_tables.values().for_each(|x| c.same(&tables::TAXTABLES, s.get(&key(x.guid)), &b::tax_table(x)));
    let s = t(&tables::VENDORS);
    biz.vendors.values().for_each(|x| c.same(&tables::VENDORS, s.get(&key(x.guid)), &b::vendor(x, refs)));
    let mut stored = rows(conn, &tables::TAXTABLE_ENTRIES);
    let mut encoded: Vec<Vec<Cell>> = biz.tax_tables.values().flat_map(|x| x.entries.iter().map(|e| b::tax_table_entry(x.guid, e))).collect();
    let by_table = |r: &Vec<Cell>| show(&r[0]);
    stored.sort_by_key(by_table);
    encoded.sort_by_key(by_table);
    if stored != encoded {
        c.failures.push(format!("{}: taxtable_entries: stored {stored:?}, encoded {encoded:?}", c.name));
    }
}

/// Slot rows by owner, each owner's in `id` order.
type SlotRows = BTreeMap<Vec<u8>, Vec<Vec<Cell>>>;

fn group(rows: Vec<Vec<Cell>>) -> SlotRows {
    let mut out = SlotRows::new();
    rows.into_iter().for_each(|r| {
        let Cell::Text(owner) = &r[0] else { panic!("{r:?}") };
        out.entry(owner.clone()).or_default().push(r);
    });
    out
}

/// An owner's rows depth first, with the frame and list GUIDs, which GnuCash makes afresh on every write, left out.
fn flatten(owner: &[u8], rows: &SlotRows, out: &mut Vec<Vec<Cell>>) {
    rows.get(owner).into_iter().flatten().for_each(|r| {
        let mut row = r.clone();
        row[0] = Cell::Null;
        let container = matches!(r[2], Cell::Integer(8 | 9));
        if container {
            row[7] = Cell::Null;
        }
        out.push(row);
        if let (true, Cell::Text(g)) = (container, &r[7]) {
            flatten(g, rows, out);
        }
    });
}

fn check_slots(c: &mut Check<'_>, conn: &rusqlite::Connection, book: &Book, guids: &indexmap::IndexMap<red_ale_core::CommodityRef, Guid>) {
    let stored = group(rows(conn, &tables::SLOTS));
    let mut n = 0_u128;
    let mut next = || {
        n += 1;
        Guid::from_bytes(n.to_be_bytes())
    };
    let b = &book.business;
    let commodities = guids.iter().map(|(id, g)| (*g, &book.commodities[id].slots));
    let transactions = book.transactions.values().chain(book.templates.transactions.values());
    let owners: Vec<(Guid, &red_ale_core::Frame)> = std::iter::once((book.guid, &book.slots))
        .chain(commodities)
        .chain(book.accounts.values().chain(book.templates.accounts.values()).map(|a| (a.guid, &a.slots)))
        .chain(book.lots.values().map(|l| (l.guid, &l.slots)))
        .chain(transactions.flat_map(|t| std::iter::once((t.guid, &t.slots)).chain(t.splits.iter().map(|s| (s.guid, &s.slots)))))
        .chain(book.budgets.values().map(|x| (x.guid, &x.slots)))
        .chain(book.scheduled.values().map(|x| (x.guid, &x.slots)))
        .chain(b.customers.values().map(|x| (x.guid, &x.slots)))
        .chain(b.vendors.values().map(|x| (x.guid, &x.slots)))
        .chain(b.invoices.values().map(|x| (x.guid, &x.slots)))
        .chain(b.entries.values().map(|x| (x.guid, &x.slots)))
        .chain(b.jobs.values().map(|x| (x.guid, &x.slots)))
        .collect();
    owners.into_iter().for_each(|(owner, frame)| {
        let written = group(encode::slots::rows(owner, frame, &mut next).unwrap());
        let (mut want, mut got) = (Vec::new(), Vec::new());
        flatten(&key(owner), &stored, &mut want);
        flatten(&key(owner), &written, &mut got);
        if want != got {
            c.failures.push(format!("{}: slots of {owner}: stored {want:?}, encoded {got:?}", c.name));
        }
    });
}

fn check_budget_amounts(c: &mut Check<'_>, conn: &rusqlite::Connection, book: &Book) {
    let stored = rows(conn, &tables::BUDGET_AMOUNTS);
    let accounts = book.descendants(book.root);
    let encoded: Vec<Vec<Cell>> = book
        .budgets
        .values()
        .flat_map(|b| {
            accounts.iter().flat_map(move |a| {
                (0..b.num_periods).filter_map(move |p| b.amount(a.guid, p).map(|n| encode::budget_amount(b.guid, a.guid, p, n)))
            })
        })
        .collect();
    if stored != encoded {
        c.failures.push(format!("{}: budget_amounts: stored {stored:?}, encoded {encoded:?}", c.name));
    }
}

#[test]
fn every_row_gnucash_wrote_is_what_the_encoders_write() {
    let mut failures = Vec::new();
    for path in sqlite_fixtures() {
        let name = path.strip_prefix(fixtures()).unwrap().display().to_string();
        let (book, opened, _) = open_read_only(&path).unwrap();
        let conn = rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        check_book(name, &conn, &book, &opened.residue().commodities, &mut failures);
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.iter().take(60).cloned().collect::<Vec<_>>().join("\n"));
}
