//! The scheduled transaction methods on `fixtures/synthetic/rich.gnucash` (`design/api/scheduled-transactions.md`,
//! "Methods", "Schedule from transaction" and "The loan assistant"): create, read, change and delete a scheduled
//! transaction, see its occurrences, make one from a transaction and one from a loan, and save and reopen the book.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

/// 2026-01-15 12:00 UTC.
const JAN_15: i64 = 1_768_478_400;
const RENT: &str = "52f96837087e9670c933f9421b0ac626";
const FOOD: &str = "1feca2abf92347288c268ebf93753ec4";
const CASH: &str = "c080059e1aec4c0b9ad4671e80965686";
const OPENING: &str = "f485efcd743943ddb6eede20c1d22c05";

struct Book {
    _dir: tempfile::TempDir,
    path: String,
    d: Dispatcher,
}

impl Book {
    fn open() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.gnucash");
        std::fs::copy(format!("{}/../../fixtures/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR")), &path).unwrap();
        let path = path.display().to_string();
        let d = dispatcher();
        let mut book = Self { _dir: dir, path, d };
        let path = book.path.clone();
        book.ok("book.open", json!({ "path": path }));
        book
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.d
            .call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        match self.d.call(method, params.clone()) {
            Ok(v) => panic!("{method} {params} should fail but returned {v}"),
            Err(e) => e,
        }
    }
}

fn dispatcher() -> Dispatcher {
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(JAN_15), 0x5c << 100))));
    d
}

fn reason(e: &RpcError) -> (i32, Value) {
    (e.code, e.data.clone().unwrap_or_default()["reason"].clone())
}

fn rent_params() -> Value {
    json!({
        "name": "Rent",
        "startDate": "2026-02-01",
        "schedule": [{ "mult": 1, "periodType": "month" }],
        "end": { "occurrences": 12 },
        "templateTransactions": [{
            "description": "Rent",
            "num": "CHK",
            "splits": [
                { "account": "Expenses:Food", "memo": "rent", "debitFormula": "750.00" },
                { "account": "Assets:Cash", "creditFormula": "750.00" },
            ],
        }],
    })
}

/// `create` writes the scheduled transaction with the editor's defaults and its template; `get` reads it back with its
/// template splits, numerics and variables; `list` names both; `occurrences` steps it; saving and reopening the book
/// reads it the same.
#[test]
fn create_get_list_occurrences_and_reopen() {
    let mut b = Book::open();
    let created = b.ok("scheduledTransaction.create", rent_params());
    let guid = created["guid"].as_str().unwrap().to_owned();
    assert_eq!(created["warnings"], json!([]));
    assert_eq!(json!([created["enabled"], created["autoCreate"], created["notify"], created["instanceCount"]]), json!([true, false, false, 1]));
    assert_eq!(created["end"], json!({ "occurrences": 12, "remaining": 12 }));
    assert_eq!(created["schedule"], json!([{ "mult": 1, "periodType": "month", "start": "2026-02-01", "weekendAdjust": "none" }]));
    assert_eq!(json!([created["lastOccurrence"], created["nextOccurrence"]]), json!([null, "2026-02-01"]));
    assert_eq!(created["variables"], json!(["i"]));
    let template = &created["templateTransactions"][0];
    assert_eq!(json!([template["description"], template["num"], template["currency"]]), json!(["Rent", "CHK", "CURRENCY::USD"]));
    let food = &template["splits"][0];
    assert_eq!(json!([food["account"], food["accountName"], food["memo"]]), json!([FOOD, "Expenses:Food", "rent"]));
    assert_eq!(json!([food["debitFormula"], food["creditFormula"]]), json!(["750.00", ""]));
    assert_eq!(json!([food["debitNumeric"], food["creditNumeric"]]), json!([{ "num": "750", "denom": "1" }, { "num": "0", "denom": "1" }]));

    let got = b.ok("scheduledTransaction.get", json!({ "scheduledTransaction": guid }));
    assert_eq!(got.get("warnings"), None, "only writes report warnings");
    let listed = b.ok("scheduledTransaction.list", json!({}));
    let names: Vec<&Value> = listed["scheduledTransactions"].as_array().unwrap().iter().map(|s| &s["name"]).collect();
    assert_eq!(names, [&json!("Monthly rent"), &json!("Rent")], "book order, new ones last");
    assert!(listed["scheduledTransactions"][1].get("templateTransactions").is_none());
    let cash = b.ok("scheduledTransaction.list", json!({ "account": "Assets:Cash" }));
    assert_eq!(cash["scheduledTransactions"].as_array().map(Vec::len), Some(1));
    let disabled = b.ok("scheduledTransaction.list", json!({ "enabled": false }));
    assert_eq!(disabled["scheduledTransactions"], json!([]));

    let occurrences = b.ok("scheduledTransaction.occurrences", json!({ "scheduledTransaction": guid, "count": 3, "after": "2026-02-01" }));
    let expected = json!([
        { "date": "2026-03-01", "instanceCount": 2 }, { "date": "2026-04-01", "instanceCount": 3 }, { "date": "2026-05-01", "instanceCount": 4 },
    ]);
    assert_eq!(occurrences["occurrences"], expected);
    let all = b.ok("scheduledTransaction.occurrences", json!({ "scheduledTransaction": guid, "count": 1000 }));
    assert_eq!(all["occurrences"].as_array().map(Vec::len), Some(12), "the count ends it");

    b.ok("book.close", json!({ "save": true }));
    let mut reopened = dispatcher();
    reopened.call("book.open", json!({ "path": b.path, "readOnly": true })).unwrap();
    let read = reopened.call("scheduledTransaction.get", json!({ "scheduledTransaction": guid })).unwrap();
    assert_eq!(read, got);
}

/// The first scheduled transaction of an XML book that has none makes the template root, with no commodity, as
/// `sxtg_book_begin` makes it (`5.17:libgnucash/engine/SX-book.cpp:113-125`). GnuCash gives a template account read
/// without one the template commodity (`5.17:libgnucash/backend/xml/gnc-schedxaction-xml-v2.cpp:796-818`), so the
/// save's read-back finds the root so; the save is not refused, and the book reopens with its scheduled transaction.
#[test]
fn the_first_scheduled_transaction_of_a_new_book_saves_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.gnucash").display().to_string();
    let mut d = dispatcher();
    let ok = |d: &mut Dispatcher, method: &str, params: Value| {
        d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    };
    ok(&mut d, "book.create", json!({ "path": path }));
    ok(&mut d, "account.create", json!({ "name": "Expenses", "type": "EXPENSE", "commodity": "CURRENCY::USD" }));
    ok(&mut d, "account.create", json!({ "name": "Food", "type": "EXPENSE", "parent": "Expenses" }));
    ok(&mut d, "account.create", json!({ "name": "Assets", "type": "ASSET", "commodity": "CURRENCY::USD" }));
    ok(&mut d, "account.create", json!({ "name": "Cash", "type": "CASH", "parent": "Assets" }));
    let guid = ok(&mut d, "scheduledTransaction.create", rent_params())["guid"].clone();
    let got = ok(&mut d, "scheduledTransaction.get", json!({ "scheduledTransaction": guid }));
    ok(&mut d, "book.close", json!({ "save": true }));
    let mut reopened = dispatcher();
    ok(&mut reopened, "book.open", json!({ "path": path, "readOnly": true }));
    assert_eq!(ok(&mut reopened, "scheduledTransaction.get", json!({ "scheduledTransaction": guid })), got);
}

/// `update` sets one end and clears the other and replaces the template transactions, keeping the GUIDs given; the
/// temporal state stays. `delete` removes the scheduled transaction with its template.
#[test]
fn update_and_delete() {
    let mut b = Book::open();
    let created = b.ok("scheduledTransaction.create", rent_params());
    let guid = created["guid"].clone();
    let template = &created["templateTransactions"][0];
    let kept = template["guid"].clone();
    let updated = b.ok("scheduledTransaction.update", json!({
        "scheduledTransaction": guid,
        "name": "Monthly rent",
        "end": { "date": "2026-12-31" },
        "autoCreate": true,
        "templateTransactions": [{
            "guid": kept,
            "description": "Rent, raised",
            "splits": [
                { "guid": template["splits"][0]["guid"], "account": FOOD, "debitFormula": "800" },
                { "account": CASH, "creditFormula": "800" },
            ],
        }],
    }));
    assert_eq!(updated["warnings"], json!([{ "code": "duplicate-name", "message": "a scheduled transaction with the name \"Monthly rent\" already exists" }]));
    assert_eq!(updated["end"], json!({ "date": "2026-12-31" }));
    assert_eq!(json!([updated["autoCreate"], updated["notify"], updated["instanceCount"]]), json!([true, true, 1]));
    let t = &updated["templateTransactions"][0];
    assert_eq!(json!([t["guid"], t["description"], t["num"]]), json!([kept, "Rent, raised", null]));
    assert_eq!(t["splits"][0]["guid"], template["splits"][0]["guid"]);
    assert_ne!(t["splits"][1]["guid"], template["splits"][1]["guid"]);

    assert_eq!(b.ok("scheduledTransaction.delete", json!({ "scheduledTransaction": guid })), json!({ "deleted": true }));
    let gone = b.err("scheduledTransaction.get", json!({ "scheduledTransaction": guid }));
    assert_eq!((gone.code, gone.data.unwrap()["kind"].clone()), (codes::NOT_FOUND, json!("scheduledTransaction")));
    b.ok("scheduledTransaction.delete", json!({ "scheduledTransaction": RENT }));
    assert_eq!(b.ok("scheduledTransaction.list", json!({})), json!({ "scheduledTransactions": [] }));
}

/// The editor's refusals are `invalid` with a reason; a formula error is `formula` (6000) with where it is; a
/// template that provably doesn't balance is `unbalanced`; a read-only book refuses every write.
#[test]
fn refusals() {
    let mut b = Book::open();
    let with = |change: fn(&mut Value)| {
        let mut p = rent_params();
        change(&mut p);
        p
    };
    let e = b.err("scheduledTransaction.create", with(|p| p["name"] = json!("")));
    assert_eq!(reason(&e), (codes::INVALID, json!("name")));
    let e = b.err("scheduledTransaction.create", with(|p| p["end"] = json!({ "never": true, "date": "2026-12-31" })));
    assert_eq!(reason(&e), (codes::INVALID, json!("end")));
    let e = b.err("scheduledTransaction.create", with(|p| p["notify"] = json!(true)));
    assert_eq!(reason(&e), (codes::INVALID, json!("notify")));
    let e = b.err("scheduledTransaction.create", with(|p| p["templateTransactions"][0]["splits"][0]["debitFormula"] = json!("pmt(r : 360)")));
    assert_eq!(reason(&e), (codes::FORMULA, json!("arguments")));
    assert_eq!(e.data.unwrap()["side"], "debit");
    let e = b.err("scheduledTransaction.create", with(|p| p["templateTransactions"][0]["splits"][0]["debitFormula"] = json!("700")));
    assert_eq!(e.code, codes::UNBALANCED);
    let e = b.err("scheduledTransaction.create", with(|p| {
        p["autoCreate"] = json!(true);
        p["templateTransactions"][0]["splits"][0]["debitFormula"] = json!("rent");
        p["templateTransactions"][0]["splits"][1]["creditFormula"] = json!("rent");
    }));
    assert_eq!(reason(&e), (codes::INVALID, json!("auto-create")));
    let e = b.err("scheduledTransaction.occurrences", json!({ "scheduledTransaction": RENT, "schedule": [] }));
    assert_eq!(e.code, codes::INVALID_PARAMS);
    let listed = b.ok("scheduledTransaction.list", json!({}));
    assert_eq!(listed["scheduledTransactions"].as_array().map(Vec::len), Some(1), "nothing was created");

    let mut ro = dispatcher();
    ro.call("book.open", json!({ "path": b.path, "readOnly": true })).unwrap();
    let e = ro.call("scheduledTransaction.create", rent_params()).unwrap_err();
    assert_eq!(e.code, codes::READ_ONLY);
    let e = ro.call("scheduledTransaction.delete", json!({ "scheduledTransaction": RENT })).unwrap_err();
    assert_eq!(e.code, codes::READ_ONLY);
}

/// A Schedule not yet saved, as the editor's example calendar shows it: from its start date, as never run.
#[test]
fn occurrences_of_an_unsaved_schedule() {
    let mut b = Book::open();
    let got = b.ok("scheduledTransaction.occurrences", json!({
        "schedule": [{ "mult": 2, "periodType": "week" }],
        "startDate": "2026-01-03",
        "end": { "date": "2026-01-31" },
    }));
    let dates: Vec<&Value> = got["occurrences"].as_array().unwrap().iter().map(|o| &o["date"]).collect();
    assert_eq!(dates, [&json!("2026-01-03"), &json!("2026-01-17"), &json!("2026-01-31")]);
    assert_eq!(got["occurrences"][0]["instanceCount"], 1);
}

/// Schedule from transaction on the opening balance (5000.00 into Cash on 2024-06-01): monthly from 2024-07-01, its
/// splits' values as formulas without grouping.
#[test]
fn from_transaction() {
    let mut b = Book::open();
    let made = b.ok("scheduledTransaction.fromTransaction", json!({ "transaction": OPENING, "frequency": "monthly" }));
    assert_eq!(json!([made["name"], made["startDate"], made["instanceCount"]]), json!(["Opening", "2024-07-01", 1]));
    assert_eq!(made["schedule"], json!([{ "mult": 1, "periodType": "month", "start": "2024-07-01", "weekendAdjust": "none" }]));
    let splits = &made["templateTransactions"][0]["splits"];
    assert_eq!(json!([splits[0]["account"], splits[0]["debitFormula"], splits[0]["creditFormula"]]), json!([CASH, "5000.00", ""]));
    assert_eq!(json!([splits[1]["debitFormula"], splits[1]["creditFormula"]]), json!(["", "5000.00"]));
    let e = b.err("scheduledTransaction.fromTransaction", json!({ "transaction": RENT }));
    assert_eq!((e.code, e.data.unwrap()["kind"].clone()), (codes::NOT_FOUND, json!("transaction")));
}

/// The loan assistant: 100,000 at 6 % over 30 years from Cash, principal to Liabilities, interest to Fees; the
/// payment is 599.550525153, and the repayment's schedule and formulas are GnuCash's.
#[test]
fn loan() {
    let mut b = Book::open();
    let made = b.ok("scheduledTransaction.loan", json!({
        "loanAccount": "Liabilities",
        "principal": "100000",
        "rate": "6",
        "startDate": "2025-01-01",
        "length": { "years": 30 },
        "repayment": { "from": "Assets:Cash", "interestTo": "Expenses:Fees" },
    }));
    assert_eq!(made["payment"], json!({ "num": "599550525153", "denom": "1000000000" }));
    let sx = &made["scheduledTransaction"];
    assert_eq!(json!([sx["name"], sx["startDate"], sx["lastOccurrence"], sx["instanceCount"]]), json!(["Loan", "2025-01-01", "2024-12-01", 14]), "13 months from the start to the 15th");
    assert_eq!(json!([sx["end"], sx["nextOccurrence"]]), json!([{ "date": "2053-11-01" }, "2025-01-01"]));
    let splits = &sx["templateTransactions"][0]["splits"];
    assert_eq!(splits[0]["creditFormula"], "pmt( 0.06000 / 12.00 : 360.00 : 100000.00 : 0 : 0 )");
    assert_eq!(splits[1]["debitFormula"], "ppmt( 0.06000 / 12.00 : i : 360.00 : 100000.00 : 0 : 0 )");
    assert_eq!(sx["variables"], json!(["i"]));
    let e = b.err("scheduledTransaction.loan", json!({
        "loanAccount": "Assets:Cash", "principal": "1", "rate": "6", "length": { "months": 12 },
        "repayment": { "from": "Assets:Cash", "interestTo": "Expenses:Fees" },
    }));
    assert_eq!(reason(&e), (codes::INVALID, json!("account-type")));
}
