//! Writing as GnuCash writes (`design/sql-fidelity.md` §9): red-ale and GnuCash 5.17 make the same edit to two copies
//! of each SQLite fixture, and the two files hold the same rows once what is legitimately arbitrary is normalised
//! ([`normalise`]): the same rows changed, with the same values.
//!
//! red-ale's side goes through the JSON-RPC dispatcher and `book.close` with a save; GnuCash's through its Python
//! bindings (`oracle/sql_edits.py`), where each commit writes through. The edits' targets are picked from the
//! fixture's rows by [`PICK`], so every fixture with a suitable target is covered.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use red_ale_rpc::Dispatcher;
use red_ale_testkit::{TempBook, call, dispatcher, fixtures_dir, oracle};
use serde_json::{Value, json};

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

/// The targets of the edits, picked from the book's rows with Python's `sqlite3` (no GnuCash):
///
/// - `pair`: the first transaction with exactly two splits, both unreconciled, in no lot, in accounts outside the
///   template tree that hold the transaction's currency, which no invoice posted, which is not read-only, and whose
///   slots hold no double or list. GnuCash rewrites every slot of an object it commits and its rewrite loses the digits
///   of a double past the twelfth decimal and garbles nested lists (`design/notes/sql-backend.md` §3.8, §9), where
///   red-ale keeps both: no equal dump exists for such an object.
/// - `account`: the first account outside the template tree, not the root, whose slots hold no double or list (as
///   above).
/// - `price`: the commodity and currency of the first price, if any.
const PICK: &str = r#"
import json, sqlite3, sys
db = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True)
q = lambda sql, *a: db.execute(sql, a).fetchall()
template = {r[0] for r in q("SELECT root_template_guid FROM books")}
def under_template(acct):
    while acct is not None:
        if acct in template:
            return True
        acct = (q("SELECT parent_guid FROM accounts WHERE guid = ?", acct) or [(None,)])[0][0]
    return False
def slot_types(owner):
    rows = q("SELECT slot_type, guid_val FROM slots WHERE obj_guid = ?", owner)
    return [t for t, _ in rows] + [u for t, g in rows if t in (8, 9) for u in slot_types(g)]
def simple(owner):
    return not any(t in (2, 8) for t in slot_types(owner))
def ident(guid):
    ns, mn = q("SELECT namespace, mnemonic FROM commodities WHERE guid = ?", guid)[0]
    return ns + "::" + mn
posted = {r[0] for r in q("SELECT post_txn FROM invoices WHERE post_txn IS NOT NULL")}
pair = None
for tx, currency in q("SELECT guid, currency_guid FROM transactions ORDER BY rowid"):
    splits = q("SELECT s.guid, s.account_guid, a.commodity_guid, s.reconcile_state, s.lot_guid, s.value_num,"
               " s.value_denom, s.quantity_num, s.quantity_denom"
               " FROM splits s JOIN accounts a ON a.guid = s.account_guid WHERE s.tx_guid = ? ORDER BY s.rowid", tx)
    if (len(splits) == 2 and tx not in posted and simple(tx)
            and not q("SELECT 1 FROM slots WHERE obj_guid = ? AND name = 'trans-read-only'", tx)
            and all(c == currency and r == "n" and lot is None and not under_template(a) and simple(g)
                    for g, a, c, r, lot, *_ in splits)):
        pair = {"tx": tx, "currency": ident(currency),
                "splits": [{"guid": g, "account": a, "value": [vn, vd], "amount": [qn, qd]}
                           for g, a, _, _, _, vn, vd, qn, qd in splits]}
        break
account = next((g for g, t in q("SELECT guid, account_type FROM accounts ORDER BY rowid")
                if t != "ROOT" and not under_template(g) and simple(g)), None)
price = q("SELECT commodity_guid, currency_guid FROM prices ORDER BY rowid LIMIT 1")
price = {"commodity": ident(price[0][0]), "currency": ident(price[0][1])} if price else None
db.close()
print(json.dumps({"pair": pair, "account": account, "price": price}))
"#;

/// Every table's rows in `rowid` order, `rowid` first, and `sqlite_master`, read with Python's `sqlite3`. A REAL is
/// `{"real": repr}`, so it never compares equal to an INTEGER or loses digits; a BLOB is `{"blob": hex}`.
const ROWS: &str = r#"
import json, sqlite3, sys
db = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True)
def cell(v):
    if isinstance(v, float):
        return {"real": repr(v)}
    if isinstance(v, bytes):
        return {"blob": v.hex()}
    return v
out = {"sqlite_master": [[cell(c) for c in r] for r in db.execute("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY rowid")]}
for (t,) in db.execute("SELECT name FROM sqlite_master WHERE type = 'table'").fetchall():
    out[t] = [[cell(c) for c in r] for r in db.execute('SELECT rowid, * FROM "%s" ORDER BY rowid' % t)]
db.close()
print(json.dumps(out))
"#;

type Rows = BTreeMap<String, Vec<Vec<Value>>>;

fn rows(path: &Path) -> Rows {
    serde_json::from_str(&oracle::run_code(ROWS, &[&path.display().to_string()]).unwrap()).unwrap()
}

fn pick(path: &Path) -> Value {
    serde_json::from_str(&oracle::run_code(PICK, &[&path.display().to_string()]).unwrap()).unwrap()
}

fn uri(path: &Path) -> PathBuf {
    PathBuf::from(format!("sqlite3://{}", path.display()))
}

/// What GnuCash logs when it opens the book for editing (`oracle/opened.py`).
fn opened(path: &Path) -> Value {
    let out = oracle::run_script(&oracle::scripts_dir().join("opened.py"), &[&uri(path).display().to_string()]).unwrap();
    serde_json::from_str(&out).unwrap()
}

/// GnuCash makes `edit` to the book (`oracle/sql_edits.py`); returns what it logged at warning or above.
fn gnucash_edit(path: &Path, edit: &str, targets: &Value) -> Vec<String> {
    let script = oracle::scripts_dir().join("sql_edits.py");
    let out = oracle::run_script(&script, &[&path.display().to_string(), edit, &targets.to_string()]).unwrap();
    serde_json::from_str(&out).unwrap()
}

/// red-ale makes an edit through the dispatcher, opened on the book; `book.close` saves it.
type RedAleEdit = fn(&mut Dispatcher, &Value);

fn red_ale_edit(path: &Path, edit: RedAleEdit, targets: &Value) {
    let mut d = dispatcher();
    let info = call(&mut d, "book.open", &json!({ "path": path.display().to_string() }));
    assert_eq!(
        json!([info["format"], info["readOnly"]]),
        json!(["sqlite", false]),
        "{}",
        path.display()
    );
    edit(&mut d, targets);
    call(&mut d, "book.close", &json!({ "save": true }));
}

/// A rational as the wire takes it.
fn rational(pair: &Value, factor: i64) -> Value {
    json!({ "num": (pair[0].as_i64().unwrap() * factor).to_string(), "denom": pair[1].as_i64().unwrap().to_string() })
}

// The red-ale side of each edit in `oracle/sql_edits.py`, field for field.

fn edit_transaction(d: &mut Dispatcher, t: &Value) {
    let pair = &t["pair"];
    let splits: Vec<Value> = pair["splits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| json!({ "guid": s["guid"], "value": rational(&s["value"], 2), "amount": rational(&s["amount"], 2) }))
        .collect();
    call(
        d,
        "transaction.update",
        &json!({ "guid": pair["tx"], "description": "Edited description", "changeSplits": splits }),
    );
}

fn add_transaction(d: &mut Dispatcher, t: &Value) {
    let pair = &t["pair"];
    let (first, second) = (&pair["splits"][0]["account"], &pair["splits"][1]["account"]);
    let (debit, credit) = (json!({ "num": "-750", "denom": "100" }), json!({ "num": "750", "denom": "100" }));
    let splits = json!([
        { "account": first, "value": debit, "amount": debit, "memo": "cheque", "action": "Pay" },
        { "account": second, "value": credit, "amount": credit, "reconcileState": "c" },
    ]);
    let params = json!({
        "currency": pair["currency"], "datePosted": "2024-06-20", "dateEntered": "2024-07-01T12:00:00Z", "num": "102",
        "description": "Rent", "splits": splits,
    });
    call(d, "transaction.post", &params);
}

fn delete_transaction(d: &mut Dispatcher, t: &Value) {
    call(d, "transaction.delete", &json!({ "guid": t["pair"]["tx"] }));
}

fn edit_account(d: &mut Dispatcher, t: &Value) {
    let params = json!({ "account": t["account"], "name": "Renamed account", "notes": "Account notes", "taxUsCode": "N123" });
    call(d, "account.update", &params);
}

fn add_price(d: &mut Dispatcher, t: &Value) {
    let price = &t["price"];
    let params = json!({
        "commodity": price["commodity"], "currency": price["currency"], "value": { "num": "12345", "denom": "1000" },
        "date": "2024-06-15", "source": "user:price-editor", "type": "last",
    });
    call(d, "price.add", &params);
}

fn set_transaction_slots(d: &mut Dispatcher, t: &Value) {
    let params = json!({ "guid": t["pair"]["tx"], "notes": "Edited notes", "docLink": "https://example.com/receipt.pdf" });
    call(d, "transaction.update", &params);
}

/// The column of `slots` rows: `rowid`, then GnuCash's columns (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:119-153`).
const SLOT_ID: usize = 1;
const SLOT_OWNER: usize = 2;
const SLOT_TYPE: usize = 4;
const SLOT_GUID: usize = 9;

/// A frame (9) or list (8) slot, whose `guid_val` its children name as their `obj_guid`.
fn is_container(row: &[Value]) -> bool {
    matches!(row[SLOT_TYPE].as_i64(), Some(8 | 9))
}

/// The rows of both books made comparable. Only what is arbitrary changes:
///
/// - **Slot `id`s** are dropped, and with them `sqlite_sequence`'s count for `slots`: they are `AUTOINCREMENT`
///   numbers, and GnuCash takes new ones for every slot of every object it commits
///   (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:645-649`), where red-ale rewrites only the Slots that changed
///   (`design/sql-fidelity.md` §4, "Slots").
/// - **Slot rows are grouped by the object that owns them**, frames and lists with their owner, each group in `id`
///   order, the groups in order of their owner's GUID. A rewrite moves an object's slots to the end of the table, so
///   GnuCash, which rewrites the slots of an object whose Slots did not change (a transaction whose description
///   changed, say), moves rows red-ale leaves in place. Within an object the order is kept: it is a list's order.
/// - **Frame and list GUIDs** are numbered in order of appearance: GnuCash gives each a new random GUID on every
///   write (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:583`, `:601`), red-ale one from its session's `Env`.
/// - **GUIDs of new entities** (any GUID the original book doesn't hold) are numbered in order of appearance, the
///   other tables first, in name order, then `slots`: GnuCash's are random, red-ale's from its `Env`.
/// - **Each transaction's split rows** are put in the order GnuCash loads them ([`load_order`]): GnuCash writes a new
///   transaction's splits unsorted, red-ale sorted, and both read back sorted.
fn normalise(mut book: Rows, original: &BTreeSet<String>) -> Rows {
    book.get_mut("splits").into_iter().for_each(|splits| load_order(splits));
    let slots = book.remove("slots").unwrap_or_default();
    let frames = slots
        .iter()
        .filter(|r| is_container(r))
        .filter_map(|r| r[SLOT_GUID].as_str())
        .map(|g| (g.to_owned(), None));
    let mut names = Names {
        frames: frames.collect(),
        ..Names::default()
    };
    book.values_mut().flatten().flatten().for_each(|cell| names.entity(cell, original));
    let slots = group_slots(slots, &names)
        .into_iter()
        .map(|mut row| {
            names.slot_row(&mut row, original);
            row.split_off(SLOT_ID + 1)
        })
        .collect();
    book.insert("slots".into(), slots);
    book.get_mut("sqlite_sequence")
        .into_iter()
        .flatten()
        .filter(|r| r[1] == "slots")
        .for_each(|r| r[2] = Value::Null);
    book
}

/// The column of `splits` rows: `rowid`, then GnuCash's columns (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:104-122`).
const SPLIT_TX: usize = 2;
const SPLIT_VALUE_NUM: usize = 8;

/// Put each transaction's split rows in the order GnuCash loads them: their `rowid` order, then stably those of value
/// ≥ 0 first, as the load's commit sorts them (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:395-397`;
/// `xaccTransSortSplits`, `5.17:libgnucash/engine/Transaction.cpp:540-553`, `:1510`). The rows keep the places and
/// `rowid`s the transaction's rows had.
///
/// GnuCash writes a new transaction's splits in the order they joined it: the backend writes the transaction row,
/// then `trans_cleanup_commit` commits each split, and only then sorts them
/// (`5.17:libgnucash/engine/qofinstance.cpp:1004`, `:1028-1029`; `5.17:libgnucash/engine/Transaction.cpp:1460-1510`).
/// red-ale writes the Book's order, which is the sorted one (`red_ale_core::ops::commit::sort_splits`). Both load in
/// the sorted order, so nothing of the rows' order within a transaction is seen but what the sort keeps: the order
/// within each sign.
fn load_order(splits: &mut [Vec<Value>]) {
    let mut places: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    splits
        .iter()
        .enumerate()
        .for_each(|(i, r)| places.entry(r[SPLIT_TX].to_string()).or_default().push(i));
    places.into_values().for_each(|at| {
        let mut rows: Vec<Vec<Value>> = at.iter().map(|&i| splits[i].clone()).collect();
        rows.sort_by_key(|r| r[SPLIT_VALUE_NUM].as_i64().is_some_and(i64::is_negative));
        at.iter().zip(rows).for_each(|(&i, mut row)| {
            row[0] = splits[i][0].clone();
            splits[i] = row;
        });
    });
}

/// The names [`normalise`] gives GUIDs: new entities' by appearance, and every frame's and list's.
#[derive(Default)]
struct Names {
    entities: BTreeMap<String, String>,
    /// Every frame and list GUID in the book, with its number once it has appeared.
    frames: BTreeMap<String, Option<String>>,
}

impl Names {
    /// A GUID the original book doesn't hold becomes `new-<n>`.
    fn entity(&mut self, cell: &mut Value, original: &BTreeSet<String>) {
        let Some(text) = cell.as_str().filter(|s| is_guid(s) && !original.contains(*s)) else {
            return;
        };
        let next = format!("new-{}", self.entities.len() + 1);
        *cell = Value::String(self.entities.entry(text.to_owned()).or_insert(next).clone());
    }

    /// A frame or list GUID becomes `frame-<n>`.
    fn frame(&mut self, cell: &mut Value) {
        let numbered = self.frames.values().flatten().count();
        let Some(slot) = cell.as_str().and_then(|text| self.frames.get_mut(text)) else {
            return;
        };
        *cell = Value::String(slot.get_or_insert_with(|| format!("frame-{}", numbered + 1)).clone());
    }

    fn slot_row(&mut self, row: &mut [Value], original: &BTreeSet<String>) {
        self.frame(&mut row[SLOT_OWNER]);
        if is_container(row) {
            self.frame(&mut row[SLOT_GUID]);
        }
        row.iter_mut().for_each(|cell| self.entity(cell, original));
    }

    fn renamed(&self, guid: &str) -> String {
        self.entities.get(guid).cloned().unwrap_or_else(|| guid.to_owned())
    }
}

fn is_guid(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Slot rows grouped by the object that owns them, frames and lists with their owner (see [`normalise`]).
fn group_slots(slots: Vec<Vec<Value>>, names: &Names) -> Vec<Vec<Value>> {
    let parent: BTreeMap<String, String> = slots
        .iter()
        .filter(|r| is_container(r))
        .filter_map(|r| Some((r[SLOT_GUID].as_str()?.to_owned(), r[SLOT_OWNER].as_str()?.to_owned())))
        .collect();
    let root = |mut owner: String| {
        while let Some(up) = parent.get(&owner) {
            owner.clone_from(up);
        }
        owner
    };
    let mut groups: BTreeMap<String, Vec<Vec<Value>>> = BTreeMap::new();
    slots.into_iter().for_each(|row| {
        let owner = root(row[SLOT_OWNER].as_str().unwrap_or_default().to_owned());
        groups.entry(names.renamed(&owner)).or_default().push(row);
    });
    groups.into_values().flatten().collect()
}

/// Every GUID-shaped string in the book.
fn guids(book: &Rows) -> BTreeSet<String> {
    book.values()
        .flatten()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|s| is_guid(s))
        .map(str::to_owned)
        .collect()
}

/// Where two normalised books differ: per table, the rows only GnuCash wrote and the rows only red-ale wrote, or
/// that the same rows are in another order.
fn row_differences(gnucash: &Rows, red_ale: &Rows) -> Vec<String> {
    let tables: BTreeSet<&String> = gnucash.keys().chain(red_ale.keys()).collect();
    let empty = Vec::new();
    tables
        .into_iter()
        .flat_map(|t| {
            let (a, b) = (gnucash.get(t).unwrap_or(&empty), red_ale.get(t).unwrap_or(&empty));
            let only = |x: &Vec<Vec<Value>>, y: &Vec<Vec<Value>>, who: &str| -> Vec<String> {
                x.iter()
                    .filter(|r| !y.contains(r))
                    .map(|r| format!("{t}: only {who} wrote {}", Value::from(r.clone())))
                    .collect()
            };
            let mut out = only(a, b, "GnuCash");
            out.extend(only(b, a, "red-ale"));
            if out.is_empty() && a != b {
                out.push(format!("{t}: the same rows in another order"));
            }
            out
        })
        .collect()
}

/// What GnuCash logs when it opens a book that it reads cleanly.
fn clean() -> Value {
    json!({ "unknownFeatures": null, "logged": [] })
}

/// Make `edit` to a copy of every fixture `applies` to, with GnuCash and with red-ale, and list where the two books'
/// rows differ and what GnuCash logged, editing its copy or opening red-ale's. Also returns how many fixtures were
/// edited.
fn same_rows_as_gnucash(edit: &str, applies: fn(&Value) -> bool, red_ale: RedAleEdit) -> (Vec<String>, usize) {
    oracle::check_version().unwrap();
    let mut edited = 0;
    let failures = sqlite_fixtures()
        .iter()
        .flat_map(|name| {
            let (theirs, ours) = (TempBook::copy(name), TempBook::copy(name));
            let targets = pick(&theirs.path);
            if !applies(&targets) {
                return Vec::new();
            }
            edited += 1;
            let original = guids(&rows(&theirs.path));
            let logged = gnucash_edit(&theirs.path, edit, &targets);
            red_ale_edit(&ours.path, red_ale, &targets);
            let opened = opened(&ours.path);
            let (a, b) = (normalise(rows(&theirs.path), &original), normalise(rows(&ours.path), &original));
            let mut out = row_differences(&a, &b);
            out.extend(logged.into_iter().map(|l| format!("GnuCash logged editing: {l}")));
            out.extend((opened != clean()).then(|| format!("GnuCash logged opening red-ale's book: {opened}")));
            out.into_iter().map(|d| format!("{name} {edit}: {d}")).collect()
        })
        .collect();
    (failures, edited)
}

/// [`same_rows_as_gnucash`] finds no difference, on at least `at_least` fixtures.
fn check(edit: &str, applies: fn(&Value) -> bool, red_ale: RedAleEdit, at_least: usize) {
    let (failures, edited) = same_rows_as_gnucash(edit, applies, red_ale);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(edited >= at_least, "{edit}: only {edited} fixtures had a target");
}

fn has_pair(t: &Value) -> bool {
    !t["pair"].is_null()
}

#[test]
fn a_transaction_and_its_splits_changed_as_gnucash_writes_them() {
    check("transaction", has_pair, edit_transaction, 15);
}

#[test]
fn a_transaction_added_as_gnucash_writes_it() {
    check("new-transaction", has_pair, add_transaction, 15);
}

#[test]
fn a_transaction_deleted_as_gnucash_deletes_it() {
    check("delete-transaction", has_pair, delete_transaction, 15);
}

#[test]
fn an_account_renamed_with_new_notes_as_gnucash_writes_it() {
    check("account", |t| !t["account"].is_null(), edit_account, 20);
}

#[test]
fn a_price_added_as_gnucash_writes_it() {
    check("price", |t| !t["price"].is_null(), add_price, 5);
}

#[test]
fn a_transaction_given_slots_as_gnucash_writes_them() {
    check("transaction-slots", has_pair, set_transaction_slots, 15);
}
