//! Duplicating documents, checked against GnuCash 5.17: the same documents are duplicated by red-ale on one copy of a
//! book and by GnuCash (`gncInvoiceCopy` with the resets of `gnc_ui_invoice_duplicate` replayed, `oracle/duplicate.py`)
//! on another, and GnuCash's readings of the two books agree field by field, the new GUIDs aside.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, diff, open, oracle, oracle_check};
use serde_json::{Value, json};

/// In `synthetic/business-editable.gnucash`: the posted invoice 000001 (two lines, a frozen billing term), the posted
/// expense voucher 000001, the posted bill 000001 and the unposted job invoice 000002.
const DOCUMENTS: [&str; 4] = [
    "aa210b7131de44c48b8513cd27c00e95",
    "645f06f442be44ecaaee148b5233ffdf",
    "95e2e7416db94eb3a7de7433f3e7875f",
    "71803cac5d8e4dceabfd0d56ec5e7521",
];
const DATE: &str = "2024-08-15";

/// GnuCash's reading of a book with the new documents' and lines' GUIDs taken out, so two books that differ only in
/// them compare equal: each invoice's `entries` becomes its lines' dumps, and the lines list goes.
fn without_new_guids(mut dump: Value) -> Value {
    let business = dump["business"].as_object_mut().unwrap();
    let lines: Vec<Value> = business.remove("entries").unwrap().as_array().unwrap().clone();
    let line = |g: &Value| -> Value {
        let mut l = lines.iter().find(|l| &l["guid"] == g).unwrap().clone();
        l.as_object_mut().unwrap().remove("guid");
        l
    };
    business["invoices"].as_array_mut().unwrap().iter_mut().for_each(|inv| {
        let mut own: Vec<Value> = inv["entries"].as_array().unwrap().iter().map(line).collect();
        own.sort_by_key(Value::to_string);
        inv["entries"] = Value::Array(own);
    });
    dump
}

#[test]
fn duplicates_match_gnucash_field_by_field() {
    let theirs = TempBook::copy("synthetic/business-editable.gnucash");
    oracle::check_version().unwrap();
    let mut args = vec![theirs.path_str(), DATE.to_owned()];
    args.extend(DOCUMENTS.iter().map(|g| (*g).to_owned()));
    let script = oracle::scripts_dir().join("duplicate.py");
    oracle::run_script(&script, &args.iter().map(String::as_str).collect::<Vec<_>>()).unwrap_or_else(|e| panic!("{e}"));

    let (ours, mut d) = open("synthetic/business-editable.gnucash");
    let ids: Vec<Value> = DOCUMENTS
        .iter()
        .map(|g| call(&mut d, "invoice.duplicate", &json!({ "invoice": g, "date": DATE }))["id"].clone())
        .collect();
    assert_eq!(ids, [json!("000003"), json!("000002"), json!("000002"), json!("000004")]);
    let diffs = oracle_check(&mut d, &ours, &[]);
    assert!(diffs.is_empty(), "GnuCash reads red-ale's duplicates back:\n{}", diffs.join("\n"));

    let gnucash = without_new_guids(oracle::dump(&theirs.path).unwrap());
    let red_ale = without_new_guids(oracle::dump(&ours.path).unwrap());
    let diffs = diff(&gnucash, &red_ale, &[]);
    assert!(
        diffs.is_empty(),
        "duplicated by GnuCash != duplicated by red-ale:\n{}",
        diffs.join("\n")
    );
    let invoices = red_ale["business"]["invoices"].as_array().unwrap();
    assert_eq!(
        invoices.iter().filter(|i| i["dateOpened"] == 1_723_719_540).count(),
        4,
        "2024-08-15 10:59:00 UTC"
    );
}
