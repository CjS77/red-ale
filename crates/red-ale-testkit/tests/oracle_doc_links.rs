//! Document links, checked against GnuCash 5.17: GnuCash reads the transaction and document links red-ale wrote, and
//! no link where red-ale removed one, through `xaccTransGetDocLink` and `gncInvoiceGetDocLink` (`docLink` in
//! `oracle/dump.py`).

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{call, oracle, oracle_check, open};
use serde_json::{Value, json};

const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";
/// The posted customer invoice 000001 in `synthetic/business-editable.gnucash`.
const POSTED_INVOICE: &str = "aa210b7131de44c48b8513cd27c00e95";

fn post(d: &mut red_ale_rpc::Dispatcher, description: &str, link: &str) -> Value {
    let p = json!({ "datePosted": "2024-06-20", "description": description, "docLink": link, "splits": [
        { "account": CHECKING, "value": "-5" }, { "account": "Expenses:Food", "value": "5" }] });
    call(d, "transaction.post", &p)["guid"].clone()
}

#[test]
fn transaction_links_read_back_in_gnucash() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    post(&mut d, "relative", "receipts/june 2024.pdf");
    post(&mut d, "url", "https://example.com/r/1?x=1&y=2");
    let removed = post(&mut d, "removed", "file:///tmp/a.pdf");
    call(&mut d, "transaction.update", &json!({ "guid": removed, "docLink": "" }));
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let dump = oracle::dump(&book.path).unwrap();
    let link = |desc: &str| dump["transactions"].as_array().unwrap().iter().find(|t| t["description"] == desc).unwrap()["docLink"].clone();
    assert_eq!(link("relative"), "receipts/june 2024.pdf");
    assert_eq!(link("url"), "https://example.com/r/1?x=1&y=2");
    assert_eq!(link("removed"), Value::Null);
}

#[test]
fn document_links_read_back_in_gnucash() {
    let (book, mut d) = open("synthetic/business-editable.gnucash");
    call(&mut d, "invoice.update", &json!({ "invoice": POSTED_INVOICE, "docLink": "invoices/000001.pdf" }));
    let paper = call(&mut d, "vendor.list", &json!({ "id": "000001" }))["vendors"][0]["guid"].clone();
    let bill = call(&mut d, "invoice.create", &json!({ "owner": paper, "docLink": "https://example.com/bill/9" }));
    let gone = call(&mut d, "invoice.create", &json!({ "owner": paper, "docLink": "x.pdf" }));
    call(&mut d, "invoice.update", &json!({ "invoice": gone["guid"], "docLink": "" }));
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let dump = oracle::dump(&book.path).unwrap();
    let invoices = dump["business"]["invoices"].as_array().unwrap();
    let link = |kind: &str, id: &Value| invoices.iter().find(|i| i["type"] == kind && &i["id"] == id).unwrap()["docLink"].clone();
    assert_eq!(link("Invoice", &json!("000001")), "invoices/000001.pdf");
    assert_eq!(link("Bill", &bill["id"]), "https://example.com/bill/9");
    assert_eq!(link("Bill", &gone["id"]), Value::Null);
}
