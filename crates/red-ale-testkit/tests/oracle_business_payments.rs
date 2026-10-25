//! Credit notes, payment matching and payments from transactions against GnuCash 5.17.
//!
//! Each test builds a book with GnuCash (`oracle/business_payments.py make`: the posting scenario plus credit notes),
//! performs the same operations with GnuCash on one copy and with red-ale's API on the other, and compares the
//! GUID-free views of both. GnuCash must read red-ale's file as red-ale does, and GnuCash's business scrub must find
//! nothing to change in it.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use red_ale_core::{Book, Guid};
use red_ale_rpc::Dispatcher;
use red_ale_testkit::{TempBook, call, diff, dispatcher, oracle, oracle_check};
use serde_json::{Value, json};

fn run(args: &[&str]) -> String {
    oracle::run_script(&oracle::scripts_dir().join("business_payments.py"), args).unwrap_or_else(|e| panic!("oracle: {e}"))
}

fn scenario() -> (TempBook, TempBook) {
    oracle::check_version().unwrap();
    let base = TempBook::new_path();
    run(&["make", &base.path_str()]);
    let copy = || {
        let t = TempBook::new_path();
        std::fs::copy(&base.path, &t.path).unwrap();
        t
    };
    (copy(), copy())
}

fn view(path: &Path) -> Value {
    serde_json::from_str(&run(&["view", &path.display().to_string()])).unwrap()
}

fn invoice_guid(book: &Book, id: &str) -> String {
    book.business.invoices.values().find(|i| i.id == id).unwrap_or_else(|| panic!("no document {id}")).guid.to_string()
}

fn owner(book: &Book, name: &str) -> Value {
    let b = &book.business;
    let found: Option<(&str, Guid)> = b
        .customers
        .values()
        .find(|c| c.name == name)
        .map(|c| ("customer", c.guid))
        .or_else(|| b.vendors.values().find(|v| v.name == name).map(|v| ("vendor", v.guid)))
        .or_else(|| b.employees.values().find(|e| e.addr.name == name).map(|e| ("employee", e.guid)));
    let (kind, guid) = found.unwrap_or_else(|| panic!("no owner {name}"));
    json!({ "type": kind, "id": guid.to_string() })
}

fn txn_by_num(book: &Book, num: &str) -> String {
    book.transactions.values().find(|t| t.num.as_deref() == Some(num)).unwrap_or_else(|| panic!("no transaction {num}")).guid.to_string()
}

/// One operation of the shared op language, through red-ale's API.
fn red_ale_op(d: &mut Dispatcher, op: &Value) {
    let book = d.context().book().unwrap().clone();
    let inv = |key: &str| invoice_guid(&book, op[key].as_str().unwrap());
    let opt = |key: &str| op.get(key).cloned().unwrap_or(Value::Null);
    let docs = |key: &str| -> Option<Vec<String>> {
        op.get(key).and_then(Value::as_array).map(|a| a.iter().map(|i| invoice_guid(&book, i.as_str().unwrap())).collect())
    };
    match op["op"].as_str().unwrap() {
        "post" => {
            let params = json!({
                "invoice": inv("invoice"), "account": op["account"], "postDate": op["postDate"], "memo": opt("memo"),
                "autoPay": op.get("autopay").cloned().unwrap_or(json!(false)),
            });
            call(d, "invoice.post", &params);
        }
        "unpost" => {
            call(d, "invoice.unpost", &json!({ "invoice": inv("invoice"), "resetTaxTables": op.get("reset").cloned().unwrap_or(json!(false)) }));
        }
        "pay" => {
            let params = json!({
                "owner": owner(&book, op["owner"].as_str().unwrap()), "account": op["account"], "transferAccount": opt("transfer"),
                "amount": op["amount"], "date": op["date"], "memo": opt("memo"), "num": opt("num"),
                "applyTo": docs("applyTo").unwrap_or_default(), "autoPay": op.get("autoPay").cloned().unwrap_or(json!(false)),
            });
            call(d, "payment.process", &params);
        }
        "payInvoice" => {
            let params = json!({
                "invoice": inv("invoice"), "transferAccount": op["transfer"], "amount": op["amount"], "date": op["date"], "memo": opt("memo"),
                "num": opt("num"),
            });
            call(d, "invoice.pay", &params);
        }
        "txn" => {
            let splits: Vec<Value> = op["splits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| json!({ "account": s[0], "value": s[1], "reconcileState": s[2], "action": s.get(3) }))
                .collect();
            let params = json!({
                "currency": "USD", "description": op["description"], "num": op["num"], "datePosted": op["date"], "notes": opt("notes"), "splits": splits,
            });
            call(d, "transaction.post", &params);
        }
        "fromTxn" => {
            let mut params = json!({ "transaction": txn_by_num(&book, op["find"].as_str().unwrap()), "owner": owner(&book, op["owner"].as_str().unwrap()) });
            let p = params.as_object_mut().unwrap();
            if let Some(apply) = docs("applyTo") {
                p.insert("applyTo".into(), json!(apply));
            }
            for (from, to) in [("amount", "amount"), ("date", "date"), ("memo", "memo"), ("num", "num"), ("autoPay", "autoPay"), ("account", "account")] {
                if let Some(v) = op.get(from) {
                    p.insert(to.into(), v.clone());
                }
            }
            call(d, "payment.fromTransaction", &params);
        }
        "numSource" => {
            call(d, "book.setOptions", &json!({ "splitActionForNum": op["on"] }));
        }
        other => panic!("unknown op {other}"),
    }
}

/// GnuCash's business scrub on a copy of `book` changes nothing.
fn assert_scrub_clean(book: &Path) {
    let copy = TempBook::new_path();
    std::fs::copy(book, &copy.path).unwrap();
    run(&["scrub", &copy.path_str()]);
    let differences = diff(&view(book), &view(&copy.path), &[]);
    assert!(differences.is_empty(), "GnuCash's business scrub changed red-ale's book:\n{}", differences.join("\n"));
}

/// Run `ops` with GnuCash on one copy and red-ale on the other; both must look the same.
fn compare(ops: &Value) {
    let (gnucash, ours) = scenario();
    let ops_file = gnucash.dir.path().join("ops.json");
    std::fs::write(&ops_file, ops.to_string()).unwrap();
    run(&["do", &gnucash.path_str(), &ops_file.display().to_string()]);
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": ours.path_str() }));
    ops.as_array().unwrap().iter().for_each(|op| red_ale_op(&mut d, op));
    let fidelity = oracle_check(&mut d, &ours, &[]);
    assert!(fidelity.is_empty(), "GnuCash reads red-ale's book differently:\n{}", fidelity.join("\n"));
    let differences = diff(&view(&gnucash.path), &view(&ours.path), &[]);
    assert!(differences.is_empty(), "red-ale's result differs from GnuCash's:\n{}", differences.join("\n"));
    assert_scrub_clean(&ours.path);
}

fn post(invoice: &str, account: &str, date: &str) -> Value {
    json!({ "op": "post", "invoice": invoice, "account": account, "postDate": date, "memo": "post memo" })
}

const AR: &str = "Assets:Receivable";
const AP: &str = "Liabilities:Payable";

#[test]
fn credit_note_totals_match_gnucash() {
    let (book, _) = scenario();
    let expected: Value = serde_json::from_str(&run(&["totals", &book.path_str()])).unwrap();
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str(), "readOnly": true }));
    let snapshot = d.context().book().unwrap().clone();
    let reduce = |v: &Value| {
        let n: i64 = v["num"].as_str().unwrap().parse().unwrap();
        red_ale_testkit::model::frac(red_ale_core::Numeric::new(n, v["denom"].as_str().unwrap().parse().unwrap()))
    };
    for id in ["CN1", "VCN1", "ECN1"] {
        let t = call(&mut d, "invoice.totals", &json!({ "invoice": invoice_guid(&snapshot, id) }));
        let ours = json!({ "subtotal": reduce(&t["subtotal"]), "tax": reduce(&t["tax"]), "total": reduce(&t["total"]) });
        let theirs = json!({ "subtotal": expected[id]["subtotal"], "tax": expected[id]["tax"], "total": expected[id]["total"] });
        assert_eq!(ours, theirs, "{id}");
        assert!(!t["total"]["num"].as_str().unwrap().starts_with('-'), "{id} totals are printed positive");
    }
}

#[test]
fn credit_notes_post_link_and_unpost() {
    compare(&json!([
        post("INV2", AR, "2024-03-06"),
        { "op": "post", "invoice": "CN1", "account": AR, "postDate": "2024-03-11", "autopay": true },
        post("BILL1", AP, "2024-03-21"),
        { "op": "post", "invoice": "VCN1", "account": AP, "postDate": "2024-03-22", "autopay": true },
        post("ECN1", AP, "2024-03-12"),
        { "op": "unpost", "invoice": "CN1" },
        { "op": "post", "invoice": "CN1", "account": AR, "postDate": "2024-03-13" },
    ]));
}

#[test]
fn payments_with_chosen_documents_credit_notes_and_prepayments() {
    compare(&json!([
        post("INV1", AR, "2024-03-05"),
        post("INV2", AR, "2024-03-06"),
        post("CN1", AR, "2024-03-07"),
        // A refund-free settlement: the credit note and INV2 cancel out with no money moving.
        { "op": "pay", "owner": "Acme Ltd", "account": AR, "amount": "0", "date": "2024-03-08", "applyTo": ["CN1", "INV2"] },
        // A payment for part of INV1 and more, leaving a pre-payment.
        { "op": "pay", "owner": "Acme Ltd", "account": AR, "transfer": "Assets:Bank", "amount": "200", "date": "2024-03-09", "num": "C1" },
        { "op": "pay", "owner": "Acme Ltd", "account": AR, "transfer": "Assets:Bank", "amount": "600", "date": "2024-03-10", "num": "C2",
          "applyTo": ["INV1"] },
        post("INV3", AR, "2024-03-11"),
        { "op": "pay", "owner": "Acme Ltd", "account": AR, "amount": "0", "date": "2024-03-12", "autoPay": true },
    ]));
}

#[test]
fn auto_applied_payments_use_credit_notes() {
    compare(&json!([
        post("BILL1", AP, "2024-03-21"),
        post("VCN1", AP, "2024-03-22"),
        { "op": "pay", "owner": "Paper Co", "account": AP, "transfer": "Assets:Bank", "amount": "50", "date": "2024-03-25", "autoPay": true },
        post("EXP1", AP, "2024-03-06"),
        post("ECN1", AP, "2024-03-07"),
        { "op": "pay", "owner": "Jane Doe", "account": AP, "transfer": "Assets:Bank", "amount": "100", "date": "2024-03-26", "autoPay": true },
    ]));
}

#[test]
fn a_transaction_becomes_a_payment() {
    compare(&json!([
        post("INV1", AR, "2024-03-05"),
        post("INV2", AR, "2024-03-06"),
        { "op": "txn", "description": "Deposit from Acme", "num": "D1", "date": "2024-03-20", "notes": "kept",
          "splits": [["Assets:Bank", "120", "c"], ["Income:Sales", "-120", "n"]] },
        { "op": "fromTxn", "find": "D1", "owner": "Acme Ltd", "account": AR, "applyTo": ["INV2", "INV1"] },
    ]));
}

#[test]
fn an_existing_payment_is_rematched() {
    compare(&json!([
        post("INV1", AR, "2024-03-05"),
        post("INV2", AR, "2024-03-06"),
        { "op": "payInvoice", "invoice": "INV2", "transfer": "Assets:Bank", "amount": "80", "date": "2024-03-10", "num": "P1" },
        // The dialog's defaults: INV2 stays chosen.
        { "op": "fromTxn", "find": "P1", "owner": "Acme Ltd", "amount": "60" },
        { "op": "pay", "owner": "Acme Ltd", "account": AR, "transfer": "Assets:Bank", "amount": "500", "date": "2024-03-11", "num": "P2",
          "applyTo": ["INV1"] },
        // Matched to nothing: P2 becomes a pre-payment, then INV1 is paid by P1's new amount.
        { "op": "fromTxn", "find": "P2", "owner": "Acme Ltd", "applyTo": [] },
        { "op": "fromTxn", "find": "P1", "owner": "Acme Ltd", "applyTo": ["INV1"], "date": "2024-03-12", "memo": "moved" },
    ]));
}

#[test]
fn open_items_come_in_the_dialogs_order() {
    let ops = json!([
        post("INV1", AR, "2024-03-05"),
        post("INV2", AR, "2024-03-06"),
        post("INV3", AR, "2024-03-04"),
        post("CN1", AR, "2024-03-07"),
        { "op": "pay", "owner": "Acme Ltd", "account": AR, "transfer": "Assets:Bank", "amount": "25", "date": "2024-03-02", "num": "PRE" },
        { "op": "payInvoice", "invoice": "INV1", "transfer": "Assets:Bank", "amount": "10", "date": "2024-03-09" },
    ]);
    let (gnucash, ours) = scenario();
    let ops_file = gnucash.dir.path().join("ops.json");
    std::fs::write(&ops_file, ops.to_string()).unwrap();
    run(&["do", &gnucash.path_str(), &ops_file.display().to_string()]);
    let expected: Value = serde_json::from_str(&run(&["openitems", &gnucash.path_str(), "Acme Ltd", AR])).unwrap();
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": ours.path_str() }));
    ops.as_array().unwrap().iter().for_each(|op| red_ale_op(&mut d, op));
    let book = d.context().book().unwrap().clone();
    let items = call(&mut d, "payment.openItems", &json!({ "owner": owner(&book, "Acme Ltd"), "account": AR }));
    let reduce = |v: &Value| {
        let n: i64 = v["num"].as_str().unwrap().parse().unwrap();
        red_ale_testkit::model::frac(red_ale_core::Numeric::new(n, v["denom"].as_str().unwrap().parse().unwrap()))
    };
    let got: Vec<Value> = items["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            let t: red_ale_rpc::wire::Timestamp = serde_json::from_value(i["sortDate"].clone()).unwrap();
            json!({ "id": i["id"], "balance": reduce(&i["balance"]), "sortDate": t.0.0 })
        })
        .collect();
    assert_eq!(Value::Array(got), expected);
    assert_eq!(expected.as_array().unwrap().len(), 5, "four documents and a pre-payment");
}

#[test]
fn payments_from_transactions_with_the_num_field_source_on() {
    // The payment is made before the option is turned on, so its number is still the transaction's num and can find it.
    compare(&json!([
        post("INV1", AR, "2024-03-05"),
        { "op": "txn", "description": "Cheque in", "num": "D7", "date": "2024-03-20",
          "splits": [["Assets:Bank", "120", "n", "CHK-77"], ["Income:Sales", "-120", "n"]] },
        { "op": "pay", "owner": "Acme Ltd", "account": AR, "transfer": "Assets:Bank", "amount": "30", "date": "2024-03-21", "num": "N2" },
        { "op": "numSource", "on": true },
        { "op": "fromTxn", "find": "D7", "owner": "Acme Ltd", "account": AR, "applyTo": ["INV1"] },
        { "op": "fromTxn", "find": "N2", "owner": "Acme Ltd", "applyTo": ["INV1"], "num": "N3" },
    ]));
}
