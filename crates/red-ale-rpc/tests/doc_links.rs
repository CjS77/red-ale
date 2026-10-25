//! Document links on transactions and business documents, and the path head, through the dispatcher.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";
/// The posted customer invoice 000001 in `synthetic/business-editable.gnucash`.
const POSTED_INVOICE: &str = "aa210b7131de44c48b8513cd27c00e95";

fn open(name: &str) -> (tempfile::TempDir, Dispatcher) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR")), &path).unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(1_719_835_200), 1 << 100))));
    d.call("book.open", json!({ "path": path.display().to_string() })).unwrap();
    (dir, d)
}

fn call(d: &mut Dispatcher, method: &str, params: Value) -> Value {
    let shown = params.to_string();
    d.call(method, params).unwrap_or_else(|e| panic!("{method} {shown}: {e} {:?}", e.data))
}

fn post(d: &mut Dispatcher, link: &str) -> Value {
    let p = json!({ "datePosted": "2024-06-20", "description": "Receipt", "docLink": link, "splits": [
        { "account": CHECKING, "value": "-5" }, { "account": "Expenses:Food", "value": "5" }] });
    call(d, "transaction.post", p)
}

#[test]
fn transactions_take_change_and_drop_links() {
    let (_dir, mut d) = open("synthetic/rich.gnucash");
    let t = post(&mut d, "receipts/june.pdf");
    let guid = t["guid"].clone();
    assert_eq!(t["docLink"], "receipts/june.pdf");
    let plain = post(&mut d, "");
    assert_eq!(plain["docLink"], Value::Null, "an empty link stores nothing");

    let changed = call(&mut d, "transaction.update", json!({ "guid": guid, "docLink": "https://example.com/r/1" }));
    assert_eq!(changed["docLink"], "https://example.com/r/1");
    let kept = call(&mut d, "transaction.update", json!({ "guid": guid, "description": "Receipt 1" }));
    assert_eq!(kept["docLink"], "https://example.com/r/1", "absent leaves it");
    let dropped = call(&mut d, "transaction.update", json!({ "guid": guid, "docLink": "" }));
    assert_eq!(dropped["docLink"], Value::Null);

    call(&mut d, "transaction.update", json!({ "guid": guid, "docLink": "receipts/june.pdf" }));
    call(&mut d, "transaction.void", json!({ "guid": guid, "reason": "wrong" }));
    let e = d.call("transaction.update", json!({ "guid": guid, "docLink": "x.pdf" })).unwrap_err();
    assert_eq!(e.code, codes::CONFLICT, "a read-only transaction keeps its link");
}

#[test]
fn listings_resolve_relative_links_against_the_path_head() {
    let (_dir, mut d) = open("synthetic/rich.gnucash");
    post(&mut d, "receipts/june.pdf");
    post(&mut d, "https://example.com/r/1");
    post(&mut d, "file:old/receipt.pdf");
    let before = call(&mut d, "docLink.transactions", json!({}));
    assert_eq!(before["pathHead"], Value::Null);
    let links = before["links"].as_array().unwrap();
    assert_eq!(links.len(), 3);
    assert_eq!((links[0]["relative"].as_bool(), &links[0]["uri"]), (Some(true), &Value::Null), "no path head: unresolved");

    let head = call(&mut d, "docLink.setPathHead", json!({ "pathHead": "/srv/docs" }));
    assert_eq!(head["pathHead"], "file:///srv/docs/");
    assert_eq!(call(&mut d, "docLink.pathHead", json!({}))["pathHead"], "file:///srv/docs/");
    assert_eq!(d.call("docLink.setPathHead", json!({ "pathHead": "docs" })).unwrap_err().code, codes::INVALID);

    let after = call(&mut d, "docLink.transactions", json!({}));
    let uris: Vec<&str> = after["links"].as_array().unwrap().iter().map(|l| l["uri"].as_str().unwrap()).collect();
    assert_eq!(uris, ["file:///srv/docs/receipts/june.pdf", "https://example.com/r/1", "file:///srv/docs/old/receipt.pdf"]);
    assert_eq!(after["links"][2]["docLink"], "file:old/receipt.pdf", "the listing does not rewrite the old form");
    assert_eq!(after["links"][0]["description"], "Receipt");

    call(&mut d, "docLink.setPathHead", json!({ "pathHead": null }));
    assert_eq!(call(&mut d, "docLink.pathHead", json!({}))["pathHead"], Value::Null);
}

#[test]
fn documents_take_links_even_when_posted() {
    let (_dir, mut d) = open("synthetic/business-editable.gnucash");
    let posted = call(&mut d, "invoice.update", json!({ "invoice": POSTED_INVOICE, "docLink": "invoices/000001.pdf" }));
    assert_eq!((posted["posted"].as_bool(), posted["docLink"].as_str()), (Some(true), Some("invoices/000001.pdf")));
    let e = d.call("invoice.update", json!({ "invoice": POSTED_INVOICE, "notes": "changed" })).unwrap_err();
    assert_eq!(e.code, codes::CONFLICT, "other fields of a posted document stay frozen");

    let acme = call(&mut d, "customer.list", json!({ "id": "000001" }))["customers"][0]["guid"].clone();
    let fresh = call(&mut d, "invoice.create", json!({ "owner": acme, "docLink": "https://example.com/po/9" }));
    assert_eq!(fresh["docLink"], "https://example.com/po/9");
    let listed = call(&mut d, "docLink.documents", json!({}));
    let ids: Vec<(&str, &str)> =
        listed["links"].as_array().unwrap().iter().map(|l| (l["id"].as_str().unwrap(), l["type"].as_str().unwrap())).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&("000001", "invoice")) && ids.contains(&("000003", "invoice")));

    let cleared = call(&mut d, "invoice.update", json!({ "invoice": POSTED_INVOICE, "docLink": "" }));
    assert_eq!(cleared["docLink"], Value::Null);
}
