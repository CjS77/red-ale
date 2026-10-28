//! Edits of each kind saved to an SQLite book read back as the Book saved, with no finding, and touch only the
//! rows of what changed (`design/sql-fidelity.md` §4, §9 "Edits").

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use red_ale_core::{Book, CommodityRef, FixedEnv, Guid, Numeric, SlotValue, Split, Time64, Transaction};
use red_ale_sql::{LockOwner, open, open_read_only};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sqlite").join(name)
}

/// Every row of the tables red-ale doesn't write in these edits, and every table's row count.
fn rows(path: &Path, table: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let columns: Vec<String> = conn
        .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let quoted = columns.iter().map(|c| format!("quote(\"{c}\")")).collect::<Vec<_>>().join(" || '|' || ");
    let sql = format!("SELECT rowid || '|' || {quoted} FROM \"{table}\" ORDER BY rowid");
    conn.prepare(&sql).unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

const TABLES: [&str; 23] = [
    "books",
    "commodities",
    "accounts",
    "budgets",
    "budget_amounts",
    "prices",
    "transactions",
    "splits",
    "slots",
    "recurrences",
    "schedxactions",
    "lots",
    "billterms",
    "customers",
    "employees",
    "entries",
    "invoices",
    "jobs",
    "orders",
    "taxtables",
    "taxtable_entries",
    "vendors",
    "versions",
];

/// Save `edit` of the fixture's Book; return the Book read back and the tables whose rows changed.
fn save(name: &str, edit: impl FnOnce(&mut Book)) -> (Book, Book, Vec<&'static str>) {
    save_after(name, "", edit)
}

/// [`save`], after running `setup` on the copy.
fn save_after(name: &str, setup: &str, edit: impl FnOnce(&mut Book)) -> (Book, Book, Vec<&'static str>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(fixture(name), &path).unwrap();
    rusqlite::Connection::open(&path).unwrap().execute_batch(setup).unwrap();
    let before: Vec<Vec<String>> = TABLES.iter().map(|t| rows(&path, t)).collect();
    let (book, mut opened, findings) = open(&path, &LockOwner::new("test", 1), &|_, f| f.is_empty()).unwrap();
    assert!(findings.is_empty());
    let mut edited = book.clone();
    edit(&mut edited);
    opened.save(&book, &edited, &mut FixedEnv::new(Time64(0), 99)).unwrap();
    drop(opened);
    let changed = TABLES.iter().zip(&before).filter(|(t, b)| &rows(&path, t) != *b).map(|(t, _)| *t).collect();
    let (read, _, findings) = open_read_only(&path).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    (edited, read, changed)
}

fn tx(book: &mut Book, i: usize) -> &mut Transaction {
    Arc::make_mut(book.transactions.get_index_mut(i).unwrap().1)
}

fn split(guid: u8, account: Guid, value: i64) -> Split {
    Split::new(Guid::from_bytes([guid; 16]), account, Numeric::new(value, 100), Numeric::new(value, 100))
}

#[test]
fn a_split_moved_to_another_transaction_keeps_its_slots() {
    let (edited, read, changed) = save("synthetic/rich.gnucash", |b| {
        let moved = tx(b, 1).splits.pop().unwrap();
        tx(b, 0).splits.push(moved);
    });
    assert_eq!(read, edited);
    assert!(changed.contains(&"splits") && changed.iter().all(|t| ["splits", "lots"].contains(t)), "{changed:?}");
}

#[test]
fn a_split_added_before_the_others_rewrites_the_transaction_s_splits_in_order() {
    let (edited, read, changed) = save("synthetic/rich.gnucash", |b| {
        let account = tx(b, 0).splits[1].account.unwrap();
        let t = tx(b, 0);
        t.splits.insert(0, split(0xab, account, 0));
        t.splits.push(split(0xac, account, -1));
    });
    assert_eq!(read, edited);
    assert_eq!(changed, ["splits"]);
}

#[test]
fn a_removed_split_takes_its_slot_rows_with_it() {
    let (edited, read, changed) = save("synthetic/rich.gnucash", |b| {
        let i = b.transactions.values().position(|t| t.splits.iter().any(|s| !s.slots.is_empty())).unwrap();
        let t = tx(b, i);
        let at = t.splits.iter().position(|s| !s.slots.is_empty()).unwrap();
        t.splits.remove(at);
    });
    assert_eq!(read, edited);
    assert!(changed.contains(&"slots") && changed.contains(&"splits"), "{changed:?}");
}

#[test]
fn a_new_transaction_in_a_currency_with_no_row_inserts_the_currency() {
    let (mut edited, read, changed) = save("synthetic/rich.gnucash", |b| {
        let account = b.transactions[0].splits[0].account.unwrap();
        let mut t = Transaction {
            guid: Guid::from_bytes([0xcd; 16]),
            currency: CommodityRef::currency("JPY"),
            num: None,
            date_posted: Time64(1_717_000_000),
            date_entered: Time64(1_717_000_000),
            description: Some("Yen".into()),
            slots: red_ale_core::Frame::new(),
            splits: vec![split(0xce, account, 500), split(0xcf, account, -500)],
        };
        t.slots.set("notes", SlotValue::String("a note".into()));
        b.transactions.insert(t.guid, Arc::new(t));
    });
    let jpy = read.commodities.get(&CommodityRef::currency("JPY")).unwrap();
    assert_eq!(jpy.fraction, 1);
    edited.commodities.insert(jpy.id.clone(), Arc::clone(jpy));
    assert_eq!(read, edited);
    assert_eq!(changed, ["commodities", "transactions", "splits", "slots"]);
}

#[test]
fn a_renamed_commodity_gets_a_new_row_and_its_users_follow() {
    let (edited, read, changed) = save("synthetic/rich.gnucash", |b| {
        let old = CommodityRef::new("NASDAQ", "ACME");
        let c = b.commodities.remove(&old).unwrap();
        let mut renamed = (*c).clone();
        renamed.id = CommodityRef::new("NYSE", "ACME");
        b.commodities.insert(renamed.id.clone(), Arc::new(renamed.clone()));
        b.accounts
            .values_mut()
            .filter(|a| a.commodity.as_ref() == Some(&old))
            .for_each(|a| Arc::make_mut(a).commodity = Some(renamed.id.clone()));
        b.prices.values_mut().filter(|p| p.commodity == old).for_each(|p| Arc::make_mut(p).commodity = renamed.id.clone());
    });
    assert_eq!(read, edited);
    assert!(changed.iter().all(|t| ["commodities", "accounts", "prices", "slots"].contains(t)), "{changed:?}");
}

#[test]
fn budget_amounts_are_rewritten_with_its_slots() {
    let (edited, read, changed) = save("synthetic/rich.gnucash", |b| {
        let account = b.accounts.values().find(|a| a.parent.is_some()).unwrap().guid;
        let budget = Arc::make_mut(b.budgets.get_index_mut(0).unwrap().1);
        budget.slots.set(&format!("{account}/3"), SlotValue::Numeric(Numeric::new(12_345, 100)));
    });
    assert_eq!(read, edited);
    assert_eq!(changed, ["budget_amounts", "slots"]);
}

#[test]
fn a_schedule_change_rewrites_its_recurrences() {
    let (edited, read, changed) = save("synthetic/rich.gnucash", |b| {
        let sx = Arc::make_mut(b.scheduled.get_index_mut(0).unwrap().1);
        let r = sx.schedule[0];
        sx.schedule.push(r);
        sx.name = "Twice monthly".into();
    });
    assert_eq!(read, edited);
    assert_eq!(changed, ["recurrences", "schedxactions"]);
}

#[test]
fn business_edits_write_their_rows() {
    let (edited, read, changed) = save("synthetic/business-editable.gnucash", |b| {
        let biz = &mut b.business;
        let c = Arc::make_mut(biz.customers.values_mut().next().unwrap());
        c.notes = "edited".into();
        let t = Arc::make_mut(biz.tax_tables.values_mut().next().unwrap());
        t.entries.reverse();
        let first = *biz.jobs.keys().next().unwrap();
        biz.jobs.remove(&first);
    });
    assert_eq!(read, edited);
    assert!(changed.iter().all(|t| ["customers", "taxtable_entries", "jobs", "slots"].contains(t)), "{changed:?}");
}

#[test]
fn a_split_changed_in_a_lot_marks_the_lot_unknown() {
    let (edited, read, changed) = save_after("synthetic/rich.gnucash", "UPDATE lots SET is_closed = 0", |b| {
        let i = b.transactions.values().position(|t| t.splits.iter().any(|s| s.lot.is_some())).unwrap();
        let s = tx(b, i).splits.iter_mut().find(|s| s.lot.is_some()).unwrap();
        s.memo = Some("in a lot".into());
    });
    assert_eq!(read, edited);
    assert_eq!(changed, ["splits", "lots"]);
}
