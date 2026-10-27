//! Business posting, payments, totals and due reminders against GnuCash 5.17.
//!
//! Each test builds a book with GnuCash (`oracle/business_posting.py make`), copies it, performs the same operations
//! with GnuCash on one copy and with red-ale's API on the other, and compares GnuCash-independent views of both
//! results (`view`: transactions, splits, lots, slots, documents, tax tables and billing terms, with GUIDs replaced by
//! names). It then checks that GnuCash reads red-ale's file as red-ale does (`oracle_check`).

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use red_ale_core::{Book, Guid};
use red_ale_rpc::Dispatcher;
use red_ale_testkit::{TempBook, call, diff, dispatcher, oracle, oracle_check};
use serde_json::{Value, json};

fn script() -> std::path::PathBuf {
    oracle::scripts_dir().join("business_posting.py")
}

fn run(args: &[&str]) -> String {
    oracle::run_script(&script(), args).unwrap_or_else(|e| panic!("oracle: {e}"))
}

/// A fresh scenario book made by GnuCash, and two copies of it.
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
    book.business
        .invoices
        .values()
        .find(|i| i.id == id)
        .unwrap_or_else(|| panic!("no document {id}"))
        .guid
        .to_string()
}

fn owner(book: &Book, name: &str) -> Value {
    let b = &book.business;
    let found: Option<(&str, Guid)> = b
        .customers
        .values()
        .find(|c| c.name == name)
        .map(|c| ("customer", c.guid))
        .or_else(|| b.vendors.values().find(|v| v.name == name).map(|v| ("vendor", v.guid)))
        .or_else(|| b.employees.values().find(|e| e.addr.name == name).map(|e| ("employee", e.guid)))
        .or_else(|| b.jobs.values().find(|j| j.name == name).map(|j| ("job", j.guid)));
    let (kind, guid) = found.unwrap_or_else(|| panic!("no owner {name}"));
    json!({ "type": kind, "id": guid.to_string() })
}

/// Perform one operation of the shared op language through red-ale's API.
fn red_ale_op(d: &mut Dispatcher, op: &Value) {
    let book = d.context().book().unwrap().clone();
    let inv = |key: &str| invoice_guid(&book, op[key].as_str().unwrap());
    let opt = |key: &str| op.get(key).cloned().unwrap_or(Value::Null);
    match op["op"].as_str().unwrap() {
        "post" => {
            let prices: Vec<Value> = op
                .get("prices")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|p| json!({ "commodity": p[0], "price": p[1] }))
                .collect();
            let params = json!({
                "invoice": inv("invoice"), "account": op["account"], "postDate": op["postDate"], "dueDate": opt("dueDate"),
                "memo": opt("memo"), "accumulateSplits": opt("accumulate"), "autoPay": op.get("autopay").cloned().unwrap_or(json!(false)),
                "prices": prices,
            });
            call(d, "invoice.post", &params);
        }
        "unpost" => {
            call(
                d,
                "invoice.unpost",
                &json!({ "invoice": inv("invoice"), "resetTaxTables": op.get("reset").cloned().unwrap_or(json!(false)) }),
            );
        }
        "pay" => {
            let apply: Vec<String> = op
                .get("applyTo")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|i| invoice_guid(&book, i.as_str().unwrap()))
                .collect();
            let params = json!({
                "owner": owner(&book, op["owner"].as_str().unwrap()), "account": op["account"], "transferAccount": opt("transfer"),
                "amount": op["amount"], "exchangeRate": opt("exch"), "date": op["date"], "memo": opt("memo"), "num": opt("num"),
                "applyTo": apply, "autoPay": op.get("autoPay").cloned().unwrap_or(json!(false)),
            });
            call(d, "payment.process", &params);
        }
        "payInvoice" => {
            let params = json!({
                "invoice": inv("invoice"), "transferAccount": op["transfer"], "amount": op["amount"], "exchangeRate": opt("exch"),
                "date": op["date"], "memo": opt("memo"), "num": opt("num"),
            });
            call(d, "invoice.pay", &params);
        }
        "numSource" => {
            call(d, "book.setOptions", &json!({ "splitActionForNum": op["on"] }));
        }
        other => panic!("unknown op {other}"),
    }
}

/// Run `ops` with GnuCash on one copy and red-ale on the other; both results must look the same.
fn compare(ops: &Value) {
    let (gnucash, ours) = scenario();
    let ops_file = gnucash.dir.path().join("ops.json");
    std::fs::write(&ops_file, ops.to_string()).unwrap();
    run(&["do", &gnucash.path_str(), &ops_file.display().to_string()]);
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": ours.path_str() }));
    ops.as_array().unwrap().iter().for_each(|op| red_ale_op(&mut d, op));
    let fidelity = oracle_check(&mut d, &ours, &[]);
    assert!(
        fidelity.is_empty(),
        "GnuCash reads red-ale's book differently:\n{}",
        fidelity.join("\n")
    );
    let differences = diff(&view(&gnucash.path), &view(&ours.path), &[]);
    assert!(
        differences.is_empty(),
        "red-ale's result differs from GnuCash's:\n{}",
        differences.join("\n")
    );
}

fn post(invoice: &str, account: &str, date: &str) -> Value {
    json!({ "op": "post", "invoice": invoice, "account": account, "postDate": date, "memo": "post memo" })
}

#[test]
fn totals_match_gnucash() {
    let (book, _) = scenario();
    let expected: Value = serde_json::from_str(&run(&["totals", &book.path_str()])).unwrap();
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str(), "readOnly": true }));
    let snapshot = d.context().book().unwrap().clone();
    let reduce = |v: &Value| {
        let n: i64 = v["num"].as_str().unwrap().parse().unwrap();
        red_ale_testkit::model::frac(red_ale_core::Numeric::new(n, v["denom"].as_str().unwrap().parse().unwrap()))
    };
    let ours: serde_json::Map<String, Value> = snapshot
        .business
        .invoices
        .values()
        .map(|inv| {
            let t = call(&mut d, "invoice.totals", &json!({ "invoice": inv.guid.to_string() }));
            let mut entries: Vec<Value> = t["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    let desc = snapshot.business.entries[&Guid::parse(e["entry"].as_str().unwrap()).unwrap()].description.clone();
                    json!({ "description": desc, "value": reduce(&e["value"]), "discount": reduce(&e["discount"]), "tax": reduce(&e["tax"]) })
                })
                .collect();
            entries.sort_by(|a, b| a["description"].as_str().cmp(&b["description"].as_str()));
            let v = json!({ "subtotal": reduce(&t["subtotal"]), "tax": reduce(&t["tax"]), "total": reduce(&t["total"]), "entries": entries });
            (inv.id.clone(), v)
        })
        .collect();
    assert_eq!(expected.as_object().map(serde_json::Map::len), Some(8), "every scenario document");
    let differences = diff(&expected, &Value::Object(ours), &[]);
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn post_invoices_bills_and_vouchers() {
    compare(&json!([
        post("INV1", "Assets:Receivable", "2024-03-05"),
        { "op": "post", "invoice": "BILL1", "account": "Liabilities:Payable", "postDate": "2024-03-21", "accumulate": false, "memo": "bill" },
        post("EXP1", "Liabilities:Payable", "2024-03-06"),
        { "op": "post", "invoice": "INV3", "account": "Assets:Receivable", "postDate": "2024-03-07", "dueDate": "2024-04-30" },
    ]));
}

#[test]
fn post_a_foreign_currency_line() {
    compare(&json!([
        { "op": "post", "invoice": "BILL2", "account": "Liabilities:Payable", "postDate": "2024-03-08", "prices": [["CURRENCY::EUR", "11/10"]] },
    ]));
}

#[test]
fn payments_partial_over_and_autopay_at_post() {
    compare(&json!([
        post("INV1", "Assets:Receivable", "2024-03-05"),
        { "op": "payInvoice", "invoice": "INV1", "transfer": "Assets:Bank", "amount": "100", "date": "2024-03-10", "memo": "partial", "num": "CHK-1" },
        { "op": "pay", "owner": "Acme Ltd", "account": "Assets:Receivable", "transfer": "Assets:Bank", "amount": "900", "date": "2024-03-20",
          "num": "CHK-2", "autoPay": true },
        { "op": "post", "invoice": "INV2", "account": "Assets:Receivable", "postDate": "2024-03-22", "autopay": true },
    ]));
}

#[test]
fn opposite_documents_are_linked_and_unposting_relinks() {
    // NEG1 links to INV2; NEG2 extends that link (one transaction, three documents); unposting INV2 deletes the link;
    // a zero payment then links NEG1, INV3 and NEG2 again, merging INV3's two link splits.
    compare(&json!([
        post("INV2", "Assets:Receivable", "2024-03-06"),
        { "op": "post", "invoice": "NEG1", "account": "Assets:Receivable", "postDate": "2024-03-08", "autopay": true },
        { "op": "post", "invoice": "NEG2", "account": "Assets:Receivable", "postDate": "2024-03-09", "autopay": true },
        post("INV3", "Assets:Receivable", "2024-03-09"),
        { "op": "unpost", "invoice": "INV2" },
        { "op": "pay", "owner": "Acme Ltd", "account": "Assets:Receivable", "amount": "0", "date": "2024-03-10", "applyTo": ["NEG1", "INV3", "NEG2"] },
    ]));
}

#[test]
fn unpost_with_payments_and_reset_tax_tables() {
    compare(&json!([
        post("INV1", "Assets:Receivable", "2024-03-05"),
        post("INV3", "Assets:Receivable", "2024-03-05"),
        { "op": "payInvoice", "invoice": "INV1", "transfer": "Assets:Bank", "amount": "50", "date": "2024-03-10" },
        { "op": "unpost", "invoice": "INV1", "reset": true },
        { "op": "unpost", "invoice": "INV3" },
        { "op": "post", "invoice": "INV1", "account": "Assets:Receivable", "postDate": "2024-03-12", "autopay": true },
    ]));
}

#[test]
fn vendor_payment_from_a_foreign_bank() {
    compare(&json!([
        post("BILL1", "Liabilities:Payable", "2024-03-21"),
        { "op": "pay", "owner": "Paper Co", "account": "Liabilities:Payable", "transfer": "Assets:Euro Bank", "amount": "100", "exch": "9/10",
          "date": "2024-03-25", "num": "W1", "applyTo": ["BILL1"] },
    ]));
}

/// With the num field source on, posting swaps the document ID and the type word between the transaction's num and
/// the splits' actions, and a payment's num becomes its transfer split's action (`design/api/book-options.md`).
#[test]
fn posting_and_paying_follow_the_num_field_source() {
    compare(&json!([
        { "op": "numSource", "on": true },
        post("INV1", "Assets:Receivable", "2024-03-05"),
        { "op": "post", "invoice": "BILL1", "account": "Liabilities:Payable", "postDate": "2024-03-21", "accumulate": false, "memo": "bill" },
        post("EXP1", "Liabilities:Payable", "2024-03-06"),
        { "op": "payInvoice", "invoice": "INV1", "transfer": "Assets:Bank", "amount": "100", "date": "2024-03-10", "num": "CHK-1" },
        { "op": "pay", "owner": "Paper Co", "account": "Liabilities:Payable", "transfer": "Assets:Bank", "amount": "40", "date": "2024-03-25",
          "num": "", "applyTo": ["BILL1"] },
        post("NEG1", "Assets:Receivable", "2024-03-08"),
        post("INV2", "Assets:Receivable", "2024-03-09"),
        { "op": "pay", "owner": "Acme Ltd", "account": "Assets:Receivable", "amount": "0", "date": "2024-03-10", "applyTo": ["NEG1", "INV2"] },
    ]));
}

/// GnuCash's reminder query has no binding, so GnuCash supplies its inputs (posted state, due dates, lot balances)
/// and the test applies `gnc_invoice_show_docs_due`'s predicate to them.
#[test]
fn documents_due_match_gnucash_dates_and_balances() {
    let ops = json!([
        post("INV1", "Assets:Receivable", "2024-03-05"),
        post("INV2", "Assets:Receivable", "2024-03-25"),
        post("BILL1", "Liabilities:Payable", "2024-03-21"),
        post("EXP1", "Liabilities:Payable", "2024-03-06"),
        { "op": "payInvoice", "invoice": "INV2", "transfer": "Assets:Bank", "amount": "50", "date": "2024-03-26" },
    ]);
    let (gnucash, ours) = scenario();
    let ops_file = gnucash.dir.path().join("ops.json");
    std::fs::write(&ops_file, ops.to_string()).unwrap();
    run(&["do", &gnucash.path_str(), &ops_file.display().to_string()]);
    let dump = oracle::dump(&gnucash.path).unwrap();
    let now = 1_712_102_400_i64; // 2024-04-03T00:00:00Z
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": ours.path_str() }));
    ops.as_array().unwrap().iter().for_each(|op| red_ale_op(&mut d, op));
    for (kind, customer) in [("invoices", true), ("bills", false)] {
        let mut expected: Vec<(i64, String)> = dump["business"]["invoices"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| {
                i["paid"] == json!(false) && (i["type"] == "Invoice") == customer && i["dateDue"].as_i64().unwrap() <= now + 7 * 86_400
            })
            .map(|i| (i["dateDue"].as_i64().unwrap(), i["id"].as_str().unwrap().to_owned()))
            .collect();
        expected.sort();
        let got = call(&mut d, "invoice.due", &json!({ "kind": kind, "now": "2024-04-03T00:00:00Z" }));
        let got: Vec<(i64, String)> = got
            .as_array()
            .unwrap()
            .iter()
            .map(|x| (chrono_secs(x["due"].as_str().unwrap()), x["id"].as_str().unwrap().to_owned()))
            .collect();
        assert_eq!(got, expected, "{kind}");
        assert!(!got.is_empty());
    }
}

fn chrono_secs(rfc3339: &str) -> i64 {
    let t: red_ale_rpc::wire::Timestamp = serde_json::from_value(json!(rfc3339)).unwrap();
    t.0.0
}
