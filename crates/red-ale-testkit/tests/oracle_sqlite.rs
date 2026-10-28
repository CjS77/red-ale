//! SQLite books read the same in GnuCash 5.17 and red-ale (`design/sql-fidelity.md` §9): every SQLite fixture, the
//! fixtures rebuilt from their XML, and a book whose split rows are out of transaction order.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use red_ale_core::Book;
use red_ale_testkit::{TempBook, diff, fixtures_dir, model, oracle};

/// The SQLite fixtures, as paths under `fixtures/`, sorted.
fn sqlite_fixtures() -> Vec<String> {
    let root = fixtures_dir().join("sqlite");
    let mut out: Vec<String> = ["gnucash", "piecash", "reports", "synthetic"]
        .iter()
        .flat_map(|dir| {
            std::fs::read_dir(root.join(dir))
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".gnucash"))
                .map(|name| format!("sqlite/{dir}/{name}"))
                .collect::<Vec<_>>()
        })
        .collect();
    out.sort();
    out
}

fn read(path: &Path) -> Book {
    let (book, _, findings) = red_ale_sql::open_read_only(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert!(findings.is_empty(), "{}: {findings:?}", path.display());
    book
}

fn without_childless_template_root(mut book: Book) -> Book {
    if !book.templates.has_descendants() {
        book.templates.root = None;
        book.templates.accounts.clear();
    }
    book
}

fn uri(path: &Path) -> PathBuf {
    PathBuf::from(format!("sqlite3://{}", path.display()))
}

/// Bill term and tax table refcounts as GnuCash's SQL load leaves them: the stored count plus one for each customer,
/// vendor or invoice that sets the term or table while loading (`5.17:libgnucash/engine/gncCustomer.c:409-417`,
/// `5.17:libgnucash/engine/gncBillTerm.c:420-428`). The XML load recounts them afterwards
/// (`5.17:libgnucash/backend/xml/gnc-bill-term-xml-v2.cpp:689-721`); the SQL load does not. red-ale reads the stored
/// count, which is what the XML book holds.
const LOAD_INFLATED: [&str; 2] = ["business.billTerms.*.refcount", "business.taxTables.*.refcount"];

#[test]
fn every_sqlite_fixture_reads_as_gnucash_reads_it() {
    let names = sqlite_fixtures();
    assert_eq!(names.len(), 25);
    let failures: Vec<String> = names
        .iter()
        .flat_map(|name| {
            let book = TempBook::copy(name);
            let gnucash = oracle::dump(&uri(&book.path)).unwrap_or_else(|e| panic!("{name}: {e}"));
            let ours = model::dump(&read(&book.path));
            let ignore: Vec<&str> = model::NOT_YET_COMPUTED.iter().chain(&LOAD_INFLATED).copied().collect();
            diff(&gnucash, &ours, &ignore)
                .into_iter()
                .take(10)
                .map(move |d| format!("{name}: {d}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Rebuilding the fixtures with `scripts/sql-fixtures/build.py` gives books that read to the same Books: only the
/// GUIDs GnuCash makes afresh differ. red-ale reads neither commodities' nor slot frames' into the Book; a template
/// root with no children, also made afresh by each Save As, is left out of the comparison.
#[test]
fn the_fixtures_are_what_the_build_script_makes() {
    let out = tempfile::tempdir().unwrap();
    let script = fixtures_dir().join("../scripts/sql-fixtures/build.py");
    let status = Command::new("python3")
        .arg(&script)
        .arg(out.path())
        .env("RED_ALE_ORACLE_PYTHONPATH", oracle::python_path())
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
    let failures: Vec<String> = sqlite_fixtures()
        .iter()
        .filter_map(|name| {
            let committed = TempBook::copy(name);
            let rebuilt = out.path().join(name.trim_start_matches("sqlite/"));
            let (a, b) = (
                without_childless_template_root(read(&committed.path)),
                without_childless_template_root(read(&rebuilt)),
            );
            (format!("{a:?}") != format!("{b:?}")).then(|| name.clone())
        })
        .collect();
    assert!(failures.is_empty(), "rebuilt books differ: {failures:?}");
}

/// Each transaction's splits, in the order GnuCash 5.17 loads them, by transaction GUID.
const SPLIT_ORDER: &str = r#"
import json, sys
from gnucash import Session, SessionOpenMode
session = Session("sqlite3://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    out = {}
    for account in session.book.get_root_account().get_descendants():
        for split in account.GetSplitList():
            tx = split.GetParent()
            out[tx.GetGUID().to_string()] = [s.GetGUID().to_string() for s in tx.GetSplitList()]
finally:
    session.end()
print(json.dumps(out))
"#;

/// Move the first split row of each transaction with several to the end of the table, so `rowid` order is no
/// longer the order GnuCash wrote, and print each transaction's splits in the new `rowid` order.
const MOVE_SPLITS: &str = r#"
import json, sqlite3, sys
db = sqlite3.connect(sys.argv[1])
firsts = db.execute("SELECT min(rowid) FROM splits GROUP BY tx_guid HAVING count(*) > 1").fetchall()
for (rowid,) in firsts:
    row = db.execute("SELECT * FROM splits WHERE rowid = ?", (rowid,)).fetchone()
    db.execute("DELETE FROM splits WHERE rowid = ?", (rowid,))
    db.execute("INSERT INTO splits VALUES (%s)" % ",".join("?" * len(row)), row)
db.commit()
out = {}
for tx, split in db.execute("SELECT tx_guid, guid FROM splits ORDER BY rowid"):
    out.setdefault(tx, []).append(split)
db.close()
print(json.dumps(out))
"#;

/// GnuCash loads a transaction's splits in `rowid` order (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:256-262`
/// joins with no `ORDER BY`) and then sorts them, as every commit does (`5.17:libgnucash/engine/Transaction.cpp:1508-1510`:
/// value ≥ 0 first, stably). red-ale reads them the same way.
#[test]
fn splits_load_in_rowid_order_then_sorted_as_in_gnucash() {
    let book = TempBook::copy("sqlite/synthetic/rich.gnucash");
    let path = book.path_str();
    let by_rowid: BTreeMap<String, Vec<String>> = serde_json::from_str(&oracle::run_code(MOVE_SPLITS, &[&path]).unwrap()).unwrap();
    let gnucash: BTreeMap<String, Vec<String>> = serde_json::from_str(&oracle::run_code(SPLIT_ORDER, &[&path]).unwrap()).unwrap();
    let ours: BTreeMap<String, Vec<String>> = read(&book.path)
        .transactions
        .iter()
        .map(|(guid, tx)| (guid.to_string(), tx.splits.iter().map(|s| s.guid.to_string()).collect()))
        .collect();
    assert_eq!(gnucash, ours);
    let moved = gnucash.iter().filter(|(tx, splits)| by_rowid.get(*tx) != Some(splits)).count();
    assert!(
        moved > 0,
        "no transaction's GnuCash order differs from its rowid order, so the sort went untested"
    );
}
