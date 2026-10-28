//! An SQLite book whose `books` row names a template root that has no `accounts` row. GnuCash makes the template
//! root in memory and gives it the GUID the row names (`5.17:libgnucash/backend/sql/gnc-book-sql.cpp:119-136`), so a
//! scheduled transaction it adds puts its template account under that GUID and no row is written for the root: an
//! account commit writes only its own row (`5.17:libgnucash/backend/sql/gnc-account-sql.cpp:278-344`), and
//! `gnc_account_append_child` marks the parent dirty without committing it
//! (`5.17:libgnucash/engine/Account.cpp:2824-2879`). Reopened, the template account finds its parent by that GUID.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_rpc::Dispatcher;
use red_ale_testkit::{TempBook, call, dispatcher};
use serde_json::{Value, json};

/// A new SQLite book with an expense and an asset account, saved, and then its template root's `accounts` row
/// deleted. Returns the GUID the `books` row still names.
fn rootless_book(d: &mut Dispatcher, book: &TempBook) -> String {
    call(d, "book.create", &json!({ "path": book.path_str(), "format": "sqlite" }));
    [("Expenses", "EXPENSE"), ("Assets", "ASSET")].iter().for_each(|(name, kind)| {
        call(
            d,
            "account.create",
            &json!({ "name": name, "type": kind, "commodity": "CURRENCY::USD" }),
        );
    });
    call(d, "book.close", &json!({ "save": true }));
    let conn = rusqlite::Connection::open(&book.path).unwrap();
    let root: String = conn
        .query_row("SELECT root_template_guid FROM books", [], |r| r.get(0))
        .unwrap();
    assert_eq!(conn.execute("DELETE FROM accounts WHERE guid = ?1", [&root]).unwrap(), 1);
    root
}

fn daily() -> Value {
    json!({
        "name": "Daily", "startDate": "2024-06-01", "schedule": [{ "mult": 1, "periodType": "day" }],
        "end": { "occurrences": 5 },
        "templateTransactions": [{
            "description": "Scheduled",
            "splits": [
                { "account": "Expenses", "debitFormula": "12.34" },
                { "account": "Assets", "creditFormula": "12.34" },
            ],
        }],
    })
}

/// `(guid, parent_guid)` of every `accounts` row that is the template root or a child of it.
fn template_rows(book: &TempBook, root: &str) -> Vec<(String, Option<String>)> {
    let conn = rusqlite::Connection::open_with_flags(&book.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut st = conn
        .prepare("SELECT guid, parent_guid FROM accounts WHERE guid = ?1 OR parent_guid = ?1 ORDER BY rowid")
        .unwrap();
    st.query_map([root], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// Add the [`daily`] scheduled transaction to a [`rootless_book`], save it, and return what `get` gave for it. One
/// dispatcher serves every session, so none makes a GUID another made.
fn first_schedule_of_a_rootless_book(d: &mut Dispatcher, book: &TempBook) -> (String, Value) {
    let root = rootless_book(d, book);
    call(d, "book.open", &json!({ "path": book.path_str() }));
    let made = call(d, "scheduledTransaction.create", &daily());
    let info = call(d, "scheduledTransaction.get", &json!({ "scheduledTransaction": made["guid"] }));
    call(d, "book.close", &json!({ "save": true }));
    (root, info)
}

/// The first scheduled transaction goes under the template root the `books` row names, as in GnuCash: its template
/// account's row has that parent, no row is written for the root, and the book reopens with the scheduled
/// transaction as it was made.
#[test]
fn the_first_scheduled_transaction_goes_under_a_template_root_with_no_row() {
    let book = TempBook::new_path();
    let mut d = dispatcher();
    let (root, info) = first_schedule_of_a_rootless_book(&mut d, &book);
    let rows = template_rows(&book, &root);
    assert_eq!(rows.len(), 1, "one template account and no root row: {rows:?}");
    assert_eq!(rows[0].1.as_deref(), Some(root.as_str()));
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    let again = call(&mut d, "scheduledTransaction.get", &json!({ "scheduledTransaction": info["guid"] }));
    assert_eq!(again, info);
    call(&mut d, "book.close", &json!({}));
}

/// GnuCash 5.17 reads the scheduled transaction red-ale saved, with its template splits.
#[cfg(feature = "oracle")]
#[test]
fn gnucash_reads_the_first_scheduled_transaction_of_a_book_whose_template_root_has_no_row() {
    use red_ale_testkit::oracle;
    oracle::check_version().unwrap();
    let book = TempBook::new_path();
    let (_, info) = first_schedule_of_a_rootless_book(&mut dispatcher(), &book);
    let script = oracle::scripts_dir().join("scheduled.py");
    let dump = oracle::run_script(&script, &["dump", &book.path_str()]).unwrap_or_else(|e| panic!("oracle: {e}"));
    let gnucash: Value = serde_json::from_str(&dump).unwrap();
    let sx = &gnucash[info["guid"].as_str().unwrap()];
    assert_eq!(sx["name"], "Daily", "{gnucash}");
    assert_eq!(sx["splits"].as_array().map(Vec::len), Some(2), "{sx}");
}
