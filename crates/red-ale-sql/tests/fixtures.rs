//! Every SQLite fixture opens read-only, leaves its file untouched, and reads to the same Book as the XML book it was
//! built from (`design/sql-fidelity.md` §9, "One book, two formats" and "Read-only round-trip").

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use red_ale_core::{Book, Frame, SlotValue};
use red_ale_sql::open_read_only;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// The SQLite fixtures, as paths relative to `fixtures/sqlite`, sorted.
fn sqlite_fixtures() -> Vec<String> {
    let root = fixtures().join("sqlite");
    let mut out: Vec<String> = ["gnucash", "piecash", "reports", "synthetic"]
        .iter()
        .flat_map(|dir| {
            std::fs::read_dir(root.join(dir))
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".gnucash"))
                .map(|name| format!("{dir}/{name}"))
                .collect::<Vec<_>>()
        })
        .collect();
    out.sort();
    out
}

/// A copy of a fixture in its own temporary directory.
fn copy(from: &Path) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(from, &path).unwrap();
    (dir, path)
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn every_fixture_opens_read_only_and_is_left_byte_for_byte() {
    for name in sqlite_fixtures() {
        let (dir, path) = copy(&fixtures().join("sqlite").join(&name));
        let before = std::fs::read(&path).unwrap();
        let (book, _, findings) = open_read_only(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(findings.is_empty(), "{name}: {findings:?}");
        assert!(!book.accounts.is_empty(), "{name}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "{name} changed");
        assert_eq!(listing(dir.path()), ["book.gnucash"], "{name}: a file appeared beside it");
    }
}

/// What GnuCash's own Save As (`sync`) does to a double: it writes 12 fixed decimals and SQLite stores the REAL that
/// text gives (`5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.hpp:403-416`).
fn as_gnucash_stores_it(text: &str) -> String {
    let stored: f64 = format!("{:.12}", text.parse::<f64>().unwrap()).parse().unwrap();
    red_ale_sql::double::format_double(stored.to_bits())
}

fn doubles_as_stored(frame: &Frame) -> Frame {
    frame.iter().map(|(k, v)| (k.clone(), double_as_stored(v))).collect()
}

fn double_as_stored(value: &SlotValue) -> SlotValue {
    match value {
        SlotValue::Double(text) => SlotValue::Double(as_gnucash_stores_it(text)),
        SlotValue::Frame(f) => SlotValue::Frame(doubles_as_stored(f)),
        SlotValue::List(items) => SlotValue::List(items.iter().map(double_as_stored).collect()),
        other => other.clone(),
    }
}

/// The XML Book as an SQL book can hold it: what GnuCash's own conversion loses (`design/sql-fidelity.md` §8, §9).
fn as_sql_holds_it(mut xml: Book) -> Book {
    // No column for deferred instances.
    xml.scheduled.values_mut().for_each(|sx| Arc::make_mut(sx).deferred.clear());
    // No place for an address's Slots.
    let b = &mut xml.business;
    b.customers.values_mut().for_each(|c| {
        let c = Arc::make_mut(c);
        c.addr.slots = Frame::new();
        c.ship_addr.slots = Frame::new();
    });
    b.vendors.values_mut().for_each(|v| Arc::make_mut(v).addr.slots = Frame::new());
    b.employees.values_mut().for_each(|e| Arc::make_mut(e).addr.slots = Frame::new());
    // Doubles keep 12 decimals.
    xml.accounts.values_mut().for_each(|a| {
        let a = Arc::make_mut(a);
        a.slots = doubles_as_stored(&a.slots);
    });
    // Save As writes a commodity only when an account, transaction, price, invoice, employee or vendor uses it
    // (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:884-893` and its callers); the template commodity, which
    // every book has, the reader adds.
    let used = used_commodities(&xml);
    xml.commodities.retain(|id, _| id.namespace == red_ale_core::commodity::TEMPLATE_NAMESPACE || used.contains(id));
    xml
}

fn used_commodities(book: &Book) -> std::collections::BTreeSet<red_ale_core::CommodityRef> {
    let accounts = book.accounts.values().chain(book.templates.accounts.values()).filter_map(|a| a.commodity.clone());
    let transactions = book.transactions.values().chain(book.templates.transactions.values()).map(|t| t.currency.clone());
    let prices = book.prices.values().flat_map(|p| [p.commodity.clone(), p.currency.clone()]);
    let b = &book.business;
    let business = b
        .invoices
        .values()
        .filter_map(|i| i.currency.clone())
        .chain(b.employees.values().filter_map(|e| e.currency.clone()))
        .chain(b.vendors.values().filter_map(|v| v.currency.clone()));
    accounts.chain(transactions).chain(prices).chain(business).collect()
}

/// The ISO-dates Book feature, which the builder's one read-only open adds as every GnuCash load does
/// (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:879-880`).
const ISO_DATES: &str = "ISO-8601 formatted date strings in SQLite3 databases.";

fn without_iso_dates_feature(mut sql: Book) -> Book {
    sql.slots.remove_in(&["features", ISO_DATES]);
    sql.slots.remove_in_if_empty(&["features"]);
    sql
}

/// Commodities the XML reader does not see: currencies GnuCash's XML writer leaves out (no quotes, no Slots), which
/// the SQL backend writes because something uses them.
fn without_unwritten_currencies(mut sql: Book) -> Book {
    sql.commodities.retain(|_, c| !c.is_currency() || c.quote_flag || !c.slots.is_empty());
    sql
}

/// The Template root with no children, which an SQLite book keeps because its `books` row names it and an XML book
/// leaves out (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1179-1199`).
fn without_childless_template_root(mut sql: Book) -> Book {
    if !sql.templates.has_descendants() {
        sql.templates.root = None;
        sql.templates.accounts.clear();
    }
    sql
}

/// Where two Books differ, entity by entity.
fn differences(xml: &Book, sql: &Book) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |what: &str, same: bool| {
        if !same {
            out.push(what.to_owned());
        }
    };
    check("guid", xml.guid == sql.guid);
    check("root", xml.root == sql.root);
    check("book slots", xml.slots == sql.slots);
    xml.commodities.iter().for_each(|(k, v)| {
        let other = sql.commodities.get(k);
        check(&format!("commodity {k}: {v:?} / {other:?}"), other == Some(v));
    });
    sql.commodities.keys().filter(|k| !xml.commodities.contains_key(*k)).for_each(|k| check(&format!("extra commodity {k}"), false));
    macro_rules! map {
        ($field:ident $(. $sub:ident)*) => {
            xml.$field$(.$sub)*.iter().for_each(|(k, v)| {
                let other = sql.$field$(.$sub)*.get(k);
                // Debug text, not `==`: numerics compare by value, and their denominators must be the same too.
                let same = other.map(|o| format!("{o:?}")) == Some(format!("{v:?}"));
                check(&format!("{} {k}: {v:?}\n  sql: {other:?}", stringify!($field$(.$sub)*)), same);
            });
            check(&format!("{} count", stringify!($field$(.$sub)*)), xml.$field$(.$sub)*.len() == sql.$field$(.$sub)*.len());
        };
    }
    map!(accounts);
    map!(transactions);
    map!(prices);
    map!(lots);
    map!(budgets);
    map!(scheduled);
    map!(templates.accounts);
    map!(templates.transactions);
    map!(business.bill_terms);
    map!(business.customers);
    map!(business.employees);
    map!(business.entries);
    map!(business.invoices);
    map!(business.jobs);
    map!(business.orders);
    map!(business.tax_tables);
    map!(business.vendors);
    check("template root", xml.templates.root == sql.templates.root);
    // Save As writes accounts, transactions and prices in the engine's order, which is the XML file's. Lots are not:
    // XML holds each account's lots in GUID order, SQL in the order the account lists them; GnuCash finds lots by
    // GUID, so neither order is data.
    check("account order", xml.accounts.keys().eq(sql.accounts.keys()));
    check("transaction order", xml.transactions.keys().eq(sql.transactions.keys()));
    check("template order", xml.templates.accounts.keys().eq(sql.templates.accounts.keys()));
    check("price order", xml.prices.keys().eq(sql.prices.keys()));
    out
}

#[test]
fn every_fixture_reads_to_the_book_of_its_xml_original() {
    let failures: Vec<String> = sqlite_fixtures()
        .iter()
        .flat_map(|name| {
            let (_dir, path) = copy(&fixtures().join("sqlite").join(name));
            let (sql, _, _) = open_read_only(&path).unwrap();
            let (xml, _) = red_ale_xml::read_book(&std::fs::read(fixtures().join(name)).unwrap()).unwrap();
            assert!(sql.slots.get_in(&["features", ISO_DATES]).is_some(), "{name}: no ISO-dates feature");
            let xml = without_iso_dates_feature(as_sql_holds_it(xml));
            differences(&xml, &without_childless_template_root(without_iso_dates_feature(without_unwritten_currencies(sql))))
                .into_iter()
                .map(move |d| format!("{name}: {d}"))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
