//! SQLite books red-ale writes, checked against GnuCash 5.17 (`design/sql-fidelity.md` §4–§9,
//! `design/milestone4.md` "Compatibility in v0.4"): edited, created and backed-up books open in GnuCash with nothing
//! logged and read the same; rows red-ale doesn't model are kept; each refuses a book the other holds; and a save
//! after GnuCash broke red-ale's lock is refused.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use red_ale_core::commodity::TEMPLATE_NAMESPACE;
use red_ale_core::{Account, AccountType, Book, FixedEnv, Frame, SlotValue, Time64};
use red_ale_rpc::codes;
use red_ale_sql::LockOwner;
use red_ale_testkit::{TempBook, call, call_err, diff, dispatcher, fixtures_dir, model, oracle};
use serde_json::{Value, json};

const RICH: &str = "sqlite/synthetic/rich.gnucash";
const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";
/// "To euro account", which no edit has made read-only.
const TO_EURO: &str = "bf5e53a96f4749dcb43fd23fe9479cab";

/// Bill term and tax table refcounts, which GnuCash's SQL load inflates (see `oracle_sqlite.rs`).
const LOAD_INFLATED: [&str; 2] = ["business.billTerms.*.refcount", "business.taxTables.*.refcount"];

fn uri(path: &Path) -> PathBuf {
    PathBuf::from(format!("sqlite3://{}", path.display()))
}

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

/// How GnuCash reads the book differs from how red-ale reads it.
fn differences(path: &Path) -> Vec<String> {
    let gnucash = oracle::dump(&uri(path)).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    diff(&gnucash, &model::dump(&read(path)), &LOAD_INFLATED)
}

/// What GnuCash logs when it opens the book for editing (`oracle/opened.py`).
fn opened(path: &Path) -> Value {
    let out = oracle::run_script(&oracle::scripts_dir().join("opened.py"), &[&uri(path).display().to_string()]).unwrap();
    serde_json::from_str(&out).unwrap()
}

fn clean() -> Value {
    json!({ "unknownFeatures": null, "logged": [] })
}

/// Every table's rows (`rowid` and each column's `repr`) and `sqlite_master`, read with Python's `sqlite3`.
const ROWS: &str = r#"
import json, sqlite3, sys
db = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True)
out = {"sqlite_master": [repr(r) for r in db.execute("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY rowid")]}
for (t,) in db.execute("SELECT name FROM sqlite_master WHERE type = 'table'").fetchall():
    out[t] = [repr(r) for r in db.execute('SELECT rowid, * FROM "%s" ORDER BY rowid' % t)]
db.close()
print(json.dumps(out))
"#;

fn rows(path: &Path) -> BTreeMap<String, Vec<String>> {
    serde_json::from_str(&oracle::run_code(ROWS, &[&path.display().to_string()]).unwrap()).unwrap()
}

/// Run SQL on the book with Python's `sqlite3`, as another program would.
fn execute(path: &Path, sql: &str) {
    let code = "import sqlite3, sys\ndb = sqlite3.connect(sys.argv[1])\ndb.executescript(sys.argv[2])\ndb.commit()\ndb.close()\n";
    oracle::run_code(code, &[&path.display().to_string(), sql]).unwrap();
}

/// What GnuCash's `SESSION_NORMAL_OPEN` reports: the names of the errors it raised, empty when it opened the book.
const PROBE: &str = r#"
import json, sys
from gnucash import Session, SessionOpenMode, GnuCashBackendException, gnucash_core_c as c
try:
    s = Session("sqlite3://" + sys.argv[1], SessionOpenMode.SESSION_NORMAL_OPEN)
    s.end()
    print(json.dumps([]))
except GnuCashBackendException as e:
    print(json.dumps(sorted(n for n in dir(c) if n.startswith("ERR_") and getattr(c, n) in e.errors)))
"#;

fn gnucash_open_errors(path: &Path) -> Vec<String> {
    oracle::check_version().unwrap();
    serde_json::from_str(&oracle::run_code(PROBE, &[&path.display().to_string()]).unwrap()).unwrap()
}

/// GnuCash opens the book as the GUI does, says so on standard output, and holds it until its standard input closes.
const HOLD: &str = r#"
import sys
from gnucash import Session, SessionOpenMode
s = Session("sqlite3://" + sys.argv[1], SessionOpenMode.SESSION_NORMAL_OPEN)
try:
    print("held", flush=True)
    sys.stdin.read()
finally:
    s.end()
"#;

/// GnuCash takes over the book's lock ("open anyway") and changes a transaction's description.
const BREAK_AND_EDIT: &str = r#"
import sys
from gnucash import Session, SessionOpenMode
s = Session("sqlite3://" + sys.argv[1], SessionOpenMode.SESSION_BREAK_LOCK)
try:
    for a in s.book.get_root_account().get_descendants():
        for sp in a.GetSplitList():
            t = sp.GetParent()
            if t.GetGUID().to_string() == sys.argv[2]:
                t.BeginEdit()
                t.SetDescription("Changed by GnuCash")
                t.CommitEdit()
finally:
    s.end()
"#;

/// GnuCash 5.17 creates an empty SQLite book: `SESSION_NEW_STORE`, then a save. A book without a root account is
/// "empty" and its save writes nothing, not even the schema (`5.17:libgnucash/engine/qofsession.cpp:439`,
/// `5.17:libgnucash/engine/qofbook.cpp:375-378`); asking for the root creates it and marks the book dirty, as a new
/// book in GnuCash's GUI has one.
const GNUCASH_CREATE: &str = r#"
import sys
from gnucash import Session, SessionOpenMode
s = Session("sqlite3://" + sys.argv[1], SessionOpenMode.SESSION_NEW_STORE)
try:
    s.book.get_root_account()
    s.save()
    errors = s.pop_all_errors()
finally:
    s.end()
if errors:
    sys.exit(f"saving: {errors}")
"#;

fn spawn(code: &str, args: &[&str]) -> (tempfile::TempDir, Child) {
    oracle::check_version().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("script.py");
    std::fs::write(&script, code).unwrap();
    let child = Command::new("python3")
        .arg(&script)
        .args(args)
        .env("PYTHONPATH", oracle::python_path())
        .env("TZ", "UTC")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    (dir, child)
}

/// A spread of edits through the API: new and voided transactions, an account, a commodity, a price and a customer.
fn edit(d: &mut red_ale_rpc::Dispatcher) {
    let splits = json!([
        { "account": CHECKING, "value": "-750.00", "memo": "cheque", "action": "Pay" },
        { "account": "Expenses:Food", "value": "750.00", "reconcileState": "c" }
    ]);
    let rent = json!({ "datePosted": "2024-06-20", "num": "102", "description": "Rent", "notes": "June", "splits": splits });
    call(d, "transaction.post", &rent);
    call(d, "transaction.update", &json!({ "guid": TO_EURO, "description": "To the euro account" }));
    call(d, "transaction.void", &json!({ "guid": "439480360f6342f9abfd6df6f40cc5e9", "reason": "duplicate" }));
    call(d, "account.create", &json!({ "name": "Savings", "type": "BANK", "parent": "Assets", "commodity": "EUR" }));
    call(d, "commodity.create", &json!({ "namespace": "NYSE", "mnemonic": "IBM", "fullname": "Intl Business Machines", "fraction": 100 }));
    call(d, "price.add", &json!({ "commodity": "NYSE::IBM", "currency": "USD", "value": "171.25", "date": "2024-06-08" }));
    call(d, "customer.create", &json!({ "name": "Widget Co", "currency": "EUR", "notes": "new" }));
}

/// Edits through the API and a save give a book GnuCash opens with nothing logged, and reads as red-ale does.
#[test]
fn an_edited_sqlite_book_opens_in_gnucash_with_nothing_logged_and_reads_the_same() {
    let book = TempBook::copy(RICH);
    let mut d = dispatcher();
    let info = call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    assert_eq!(json!([info["format"], info["readOnly"]]), json!(["sqlite", false]));
    edit(&mut d);
    call(&mut d, "book.close", &json!({ "save": true }));
    let diffs = differences(&book.path);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    assert_eq!(opened(&book.path), clean());
}

/// Every SQLite fixture, opened for writing, given a new account and saved, still reads the same in GnuCash.
#[test]
fn every_sqlite_fixture_edited_and_saved_reads_the_same_in_gnucash() {
    let failures: Vec<String> = sqlite_fixtures()
        .iter()
        .flat_map(|name| {
            let book = TempBook::copy(name);
            let mut d = dispatcher();
            let info = call(&mut d, "book.open", &json!({ "path": book.path_str() }));
            assert_eq!(info["readOnly"], false, "{name}: {info}");
            call(&mut d, "account.create", &json!({ "name": "red-ale probe", "type": "EQUITY", "commodity": "USD", "placeholder": true }));
            call(&mut d, "book.close", &json!({ "save": true }));
            let logged = opened(&book.path);
            let logged = (logged != clean()).then(|| format!("{name}: GnuCash logged {logged}"));
            differences(&book.path).into_iter().take(10).map(|d| format!("{name}: {d}")).chain(logged).collect::<Vec<_>>()
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Each fixture's Book written whole into a book red-ale created reads in GnuCash exactly as the fixture GnuCash
/// wrote does.
#[test]
fn every_sqlite_fixture_written_whole_by_red_ale_reads_in_gnucash_as_the_original() {
    let failures: Vec<String> = sqlite_fixtures()
        .iter()
        .flat_map(|name| {
            let original = TempBook::copy(name);
            let rebuilt = TempBook::new_path();
            rebuild(&original.path, &rebuilt.path);
            let expected = oracle::dump(&uri(&original.path)).unwrap();
            let actual = oracle::dump(&uri(&rebuilt.path)).unwrap();
            let logged = opened(&rebuilt.path);
            let logged = (logged != clean()).then(|| format!("{name}: GnuCash logged {logged}"));
            diff(&expected, &actual, &[]).into_iter().take(10).map(|d| format!("{name}: {d}")).chain(logged).collect::<Vec<_>>()
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Create a book at `dest` from the root alone, then save the whole of `source`'s Book into it.
fn rebuild(source: &Path, dest: &Path) {
    let mut env = FixedEnv::new(Time64(1_717_412_340), 7);
    let owner = LockOwner::new("red-ale-oracle", 4242);
    let (original, residue, _) = red_ale_sql::open_read_only(source).unwrap();
    let mut skeleton = Book::new_empty(&mut env);
    skeleton.guid = original.guid;
    skeleton.root = original.root;
    let root = Account::new(original.root, "Root Account", AccountType::Root, None, None);
    skeleton.accounts = [(original.root, Arc::new(root))].into_iter().collect();
    skeleton.slots = Frame::new();
    skeleton.templates.root = Some(residue.residue().template_root);
    let mut made = red_ale_sql::create(dest, &skeleton, &owner, &mut env).unwrap();
    skeleton.templates.root = None;
    made.save(&skeleton, &original, &mut env).unwrap();
}

/// Rows red-ale doesn't model (a slot no entity owns) and every row the edit doesn't touch are kept as they were,
/// byte for byte. (A table outside GnuCash's schema makes the book open read-only: `design/sql-fidelity.md` §1.)
#[test]
fn rows_red_ale_does_not_model_or_change_are_kept() {
    let book = TempBook::copy(RICH);
    execute(
        &book.path,
        "INSERT INTO slots (obj_guid, name, slot_type, int64_val, string_val, double_val, timespec_val, guid_val,
                            numeric_val_num, numeric_val_denom, gdate_val)
                    VALUES ('0123456789abcdef0123456789abcdef', 'nobody/owns', 4, 0, 'kept', 0.0, NULL, NULL, 0, 1, NULL);",
    );
    let before = rows(&book.path);
    let mut d = dispatcher();
    let info = call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    assert_eq!(info["readOnly"], false, "{info}");
    call(&mut d, "transaction.update", &json!({ "guid": TO_EURO, "description": "To the euro account" }));
    call(&mut d, "book.close", &json!({ "save": true }));
    let after = rows(&book.path);

    let changed: Vec<&String> = before.keys().filter(|t| before[*t] != after[*t]).collect();
    assert_eq!(changed, ["transactions"]);
    let gone: Vec<&String> = before["transactions"].iter().filter(|r| !after["transactions"].contains(r)).collect();
    let new: Vec<&String> = after["transactions"].iter().filter(|r| !before["transactions"].contains(r)).collect();
    assert_eq!((gone.len(), new.len()), (1, 1), "{gone:?} {new:?}");
    assert_eq!(gone[0].replace("'To euro account'", "'To the euro account'"), *new[0]);
    assert!(after["slots"].iter().any(|r| r.contains("nobody/owns") && r.contains("'kept'")));
    assert_eq!(opened(&book.path), clean());
}

/// GnuCash refuses a book red-ale holds; red-ale refuses one GnuCash holds, naming GnuCash's host and process; each
/// can open it once the other lets go.
#[test]
fn each_refuses_a_book_the_other_holds() {
    let book = TempBook::copy(RICH);
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    assert_eq!(gnucash_open_errors(&book.path), ["ERR_BACKEND_LOCKED"]);
    call(&mut d, "book.close", &json!({}));
    assert_eq!(gnucash_open_errors(&book.path), Vec::<String>::new());

    let (_dir, mut gnucash) = spawn(HOLD, &[&book.path_str()]);
    let mut line = String::new();
    BufReader::new(gnucash.stdout.take().unwrap()).read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "held");
    let err = call_err(&mut d, "book.open", &json!({ "path": book.path_str() }));
    assert_eq!(err.code, codes::BOOK_LOCKED);
    assert_eq!(err.data.unwrap()["holder"]["pid"], json!(gnucash.id()));
    drop(gnucash.stdin.take());
    assert!(gnucash.wait().unwrap().success());
    let info = call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    assert_eq!(info["readOnly"], false);
    call(&mut d, "book.close", &json!({}));
}

/// When GnuCash opens a book red-ale holds with "open anyway" and edits it, red-ale's next save is refused with
/// `book-changed` and GnuCash's edit stays.
#[test]
fn a_save_after_gnucash_broke_the_lock_is_refused() {
    let book = TempBook::copy(RICH);
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    oracle::check_version().unwrap();
    oracle::run_code(BREAK_AND_EDIT, &[&book.path_str(), TO_EURO]).unwrap();
    call(&mut d, "transaction.update", &json!({ "guid": TO_EURO, "description": "Edited by red-ale" }));
    let before = rows(&book.path);
    let err = call_err(&mut d, "book.save", &json!({}));
    assert_eq!(err.code, codes::BOOK_CHANGED, "{err}");
    assert_eq!(rows(&book.path), before, "nothing was written");
    call(&mut d, "book.close", &json!({ "discard": true }));
    let descriptions: Vec<String> = read(&book.path).transactions.values().filter_map(|t| t.description.clone()).collect();
    assert!(descriptions.iter().any(|d| d == "Changed by GnuCash"), "{descriptions:?}");
    assert!(!descriptions.iter().any(|d| d == "Edited by red-ale"));
}

/// A new book's account rows without their GUIDs: the root and the template root, as GnuCash's `sync` writes them.
const ROOT_ROWS: &str = r#"
import json, sqlite3, sys
db = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True)
print(json.dumps({
    "accounts": [list(r) for r in db.execute(
        "SELECT name, account_type, commodity_guid, commodity_scu, non_std_scu, parent_guid, code, description, hidden,"
        " placeholder FROM accounts ORDER BY rowid")],
    "commodities": db.execute("SELECT count(*) FROM commodities").fetchone()[0],
}))
db.close()
"#;

fn root_rows(path: &Path) -> Value {
    serde_json::from_str(&oracle::run_code(ROOT_ROWS, &[&path.display().to_string()]).unwrap()).unwrap()
}

/// The schema of a new book: `sqlite_master`, sorted, and the `versions` rows.
fn schema(path: &Path) -> (Vec<String>, Vec<String>) {
    let mut all = rows(path);
    let mut master = all.remove("sqlite_master").unwrap();
    master.sort();
    let mut versions: Vec<String> = all.remove("versions").unwrap().iter().map(|r| r.split_once(", ").unwrap().1.to_owned()).collect();
    versions.sort();
    (master, versions)
}

/// A book made by `book.create` with `format: "sqlite"` opens in GnuCash with no upgrade and nothing logged, and has
/// the schema, `versions` and root account rows GnuCash 5.17 gives a book it creates.
#[test]
fn a_created_sqlite_book_opens_in_gnucash_and_has_its_schema() {
    let book = TempBook::new_path();
    let mut d = dispatcher();
    let info = call(&mut d, "book.create", &json!({ "path": book.path_str(), "format": "sqlite" }));
    assert_eq!(info["format"], "sqlite");
    call(&mut d, "book.close", &json!({}));
    let made = rows(&book.path);
    assert_eq!(gnucash_open_errors(&book.path), Vec::<String>::new());
    assert_eq!(opened(&book.path), clean());
    assert_eq!(rows(&book.path), made, "GnuCash's open upgraded or added nothing");
    let diffs = differences(&book.path);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));

    let theirs = TempBook::new_path();
    oracle::run_code(GNUCASH_CREATE, &[&theirs.path_str()]).unwrap();
    assert_eq!(schema(&book.path), schema(&theirs.path));
    assert_eq!(root_rows(&book.path), root_rows(&theirs.path));
}

/// The backup red-ale makes before its first save opens in GnuCash, unlocked, as the book was before the save.
#[test]
fn the_backup_opens_in_gnucash_as_the_book_was() {
    let book = TempBook::copy(RICH);
    let expected = oracle::dump(&uri(&book.path)).unwrap();
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    edit(&mut d);
    let saved = call(&mut d, "book.save", &json!({}));
    let backup = PathBuf::from(saved["backup"].as_str().unwrap());
    assert_eq!(gnucash_open_errors(&backup), Vec::<String>::new(), "the backup holds no lock");
    assert_eq!(opened(&backup), clean());
    let diffs = diff(&expected, &oracle::dump(&uri(&backup)).unwrap(), &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    call(&mut d, "book.close", &json!({}));
}

/// Open `path` for writing with `red-ale-sql` itself, save `edit` of its Book, and return the Book saved.
fn save_edit(path: &Path, edit: impl FnOnce(&Book) -> Book) -> Book {
    let mut env = FixedEnv::new(Time64(1_717_412_340), 7);
    let owner = LockOwner::new("red-ale-oracle", 4242);
    let (book, mut opened, findings) = red_ale_sql::open(path, &owner, &|_, f| f.is_empty()).unwrap();
    assert!(findings.is_empty(), "{}: {findings:?}", path.display());
    let edited = edit(&book);
    assert_eq!(red_ale_sql::SqliteBook::unstorable(&book, &edited), None);
    opened.save(&book, &edited, &mut env).unwrap();
    edited
}

fn probe() -> SlotValue {
    SlotValue::String("red-ale probe".into())
}

/// Add a slot to `slots` and return the text to append to a field.
fn mark(slots: &mut Frame) -> &'static str {
    slots.set("red-ale/probe", probe());
    " (edited)"
}

/// Every entity of every kind changed: a column of its own row and a new slot, so each kind's `UPDATE` and slot
/// rewrite are written.
fn touch_all(book: &Book) -> Book {
    let mut b = book.clone();
    b.slots.set("red-ale/probe", probe());
    b.commodities.values_mut().map(Arc::make_mut).filter(|c| !c.id.is_currency() && c.id.namespace != TEMPLATE_NAMESPACE).for_each(|c| {
        let tail = mark(&mut c.slots);
        c.fullname = Some(format!("{}{tail}", c.fullname.clone().unwrap_or_default()));
    });
    let root = b.root;
    b.accounts.values_mut().filter(|a| a.guid != root).map(Arc::make_mut).for_each(touch_account);
    b.templates.accounts.values_mut().map(Arc::make_mut).for_each(touch_account);
    b.transactions.values_mut().map(Arc::make_mut).for_each(touch_transaction);
    b.templates.transactions.values_mut().map(Arc::make_mut).for_each(touch_transaction);
    b.prices.values_mut().map(Arc::make_mut).for_each(|p| p.source = Some("user:price-editor".into()));
    b.lots.values_mut().map(Arc::make_mut).for_each(|l| {
        mark(&mut l.slots);
    });
    b.budgets.values_mut().map(Arc::make_mut).for_each(|x| x.description.push_str(mark(&mut x.slots)));
    b.scheduled.values_mut().map(Arc::make_mut).for_each(|x| x.name.push_str(mark(&mut x.slots)));
    touch_business(&mut b.business);
    b
}

fn touch_account(a: &mut Account) {
    let tail = mark(&mut a.slots);
    a.description = Some(format!("{}{tail}", a.description.clone().unwrap_or_default()));
}

fn touch_transaction(t: &mut red_ale_core::Transaction) {
    let tail = mark(&mut t.slots);
    t.description = Some(format!("{}{tail}", t.description.clone().unwrap_or_default()));
    t.splits.iter_mut().for_each(|s| {
        let tail = mark(&mut s.slots);
        s.memo = Some(format!("{}{tail}", s.memo.clone().unwrap_or_default()));
    });
}

fn touch_business(b: &mut red_ale_core::business::Business) {
    b.customers.values_mut().map(Arc::make_mut).for_each(|x| x.notes.push_str(mark(&mut x.slots)));
    b.vendors.values_mut().map(Arc::make_mut).for_each(|x| x.notes.push_str(mark(&mut x.slots)));
    b.employees.values_mut().map(Arc::make_mut).for_each(|x| x.username.push_str(mark(&mut x.slots)));
    b.jobs.values_mut().map(Arc::make_mut).for_each(|x| x.name.push_str(mark(&mut x.slots)));
    b.orders.values_mut().map(Arc::make_mut).for_each(|x| x.notes.push_str(mark(&mut x.slots)));
    b.invoices.values_mut().map(Arc::make_mut).for_each(|x| x.notes.push_str(mark(&mut x.slots)));
    b.entries.values_mut().map(Arc::make_mut).for_each(|x| x.description.push_str(mark(&mut x.slots)));
    b.bill_terms.values_mut().map(Arc::make_mut).for_each(|x| x.description.push_str(mark(&mut x.slots)));
    b.tax_tables.values_mut().map(Arc::make_mut).for_each(|x| x.name.push_str(" 2"));
}

/// Everything removed but the root account, the commodity it uses and the template commodity, which the template
/// root's row refers to and no Mutation removes (`red_ale_core::ops::commodities::delete_commodity`): every kind's
/// `DELETE`s, dependent rows included.
fn remove_all(book: &Book) -> Book {
    let mut b = book.clone();
    let root = b.root;
    b.accounts.retain(|g, _| *g == root);
    let kept = b.accounts.get(&root).and_then(|a| a.commodity.clone());
    b.commodities.retain(|id, _| Some(id) == kept.as_ref() || id.namespace == TEMPLATE_NAMESPACE);
    b.transactions.clear();
    b.prices.clear();
    b.lots.clear();
    b.budgets.clear();
    b.scheduled.clear();
    // A Book has a template root only while it has template accounts, as red-ale reads one.
    b.templates = red_ale_core::scheduled::Templates::default();
    b.business = red_ale_core::business::Business::default();
    b
}

/// A pass of [`every_kind_edited_or_removed_reads_the_same_in_gnucash`]: its name, the edit, and the dump paths to skip.
type Pass = (&'static str, fn(&Book) -> Book, &'static [&'static str]);

/// Each fixture with every entity edited, then with every entity removed: GnuCash opens each result with nothing
/// logged and reads what red-ale saved, and red-ale reads back the Book it saved.
#[test]
fn every_kind_edited_or_removed_reads_the_same_in_gnucash() {
    // `dump.py` lists a currency only when an account under the root uses it, so the root's own currency, all a
    // removed book has left, is in red-ale's dump alone; the read-back comparison covers it.
    let kinds: [Pass; 2] = [("edited", touch_all, &[]), ("removed", remove_all, &["commodities"])];
    let failures: Vec<String> = sqlite_fixtures()
        .iter()
        .flat_map(|name| {
            kinds.into_iter().flat_map(move |(what, edit, ignore)| {
                let book = TempBook::copy(name);
                let saved = save_edit(&book.path, edit);
                let back = (read(&book.path) != saved).then(|| format!("{name} {what}: red-ale reads back another Book"));
                let logged = opened(&book.path);
                let logged = (logged != clean()).then(|| format!("{name} {what}: GnuCash logged {logged}"));
                let gnucash = oracle::dump(&uri(&book.path)).unwrap();
                let skip: Vec<&str> = LOAD_INFLATED.iter().chain(ignore).copied().collect();
                let diffs = diff(&gnucash, &model::dump(&saved), &skip);
                diffs.into_iter().take(10).map(move |d| format!("{name} {what}: {d}")).chain(back).chain(logged).collect::<Vec<_>>()
            })
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

