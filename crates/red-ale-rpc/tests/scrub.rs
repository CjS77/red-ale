//! The scrub methods through the dispatcher, on broken copies of `synthetic/rich.gnucash`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

const OPENING: &str = "f485efcd743943ddb6eede20c1d22c05";
const GROCERIES: &str = "439480360f6342f9abfd6df6f40cc5e9";
const EQUITY: &str = "8839ad3eae7b4ca596db5bde593d8803";
const FOOD: &str = "1feca2abf92347288c268ebf93753ec4";

/// A copy of rich.gnucash with Groceries unbalanced by 2.34 and the Opening equity split's account missing.
fn open_broken(read_only: bool) -> (tempfile::TempDir, Dispatcher) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    let text = std::fs::read_to_string(format!("{}/../../fixtures/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let text = text
        .replacen(
            "<split:value>1234/100</split:value>\n      <split:quantity>1234/100</split:quantity>",
            "<split:value>1000/100</split:value>\n      <split:quantity>1000/100</split:quantity>",
            1,
        )
        .replacen(&format!("{EQUITY}</split:account>"), "2222222222222222222222222222cccc</split:account>", 1);
    std::fs::write(&path, text).unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(1_719_835_200), 1 << 100))));
    d.call("book.open", json!({ "path": path.display().to_string(), "readOnly": read_only })).unwrap();
    (dir, d)
}

fn call(d: &mut Dispatcher, method: &str, params: &Value) -> Value {
    d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
}

fn err(d: &mut Dispatcher, method: &str, params: &Value) -> i32 {
    d.call(method, params.clone()).unwrap_err().code
}

fn kinds(result: &Value) -> Vec<String> {
    result["repairs"].as_array().unwrap().iter().map(|r| r["kind"].as_str().unwrap().to_owned()).collect()
}

#[test]
fn imbalance_of_one_transaction() {
    let (_dir, mut d) = open_broken(false);
    let result = call(&mut d, "scrub.imbalance", &json!({ "transaction": GROCERIES }));
    assert_eq!(kinds(&result), ["accountCreated", "balanceSplitAdded"]);
    let repairs = result["repairs"].as_array().unwrap();
    assert_eq!(repairs[0]["name"], "Imbalance-USD");
    assert_eq!(repairs[0]["commodity"], "CURRENCY::USD");
    assert_eq!(repairs[1]["value"], json!({ "num": "234", "denom": "100" }));
    assert_eq!(repairs[1]["transaction"], GROCERIES);
    assert!(kinds(&call(&mut d, "scrub.imbalance", &json!({ "transaction": GROCERIES }))).is_empty(), "nothing left to repair");
    let info = call(&mut d, "book.info", &json!({}));
    assert_eq!(info["dirty"], true);
}

#[test]
fn account_scopes() {
    let (_dir, mut d) = open_broken(false);
    let result = call(&mut d, "scrub.imbalance", &json!({ "account": "Income" }));
    assert!(kinds(&result).is_empty(), "no transactions in Income");
    let result = call(&mut d, "scrub.orphans", &json!({ "account": "Expenses", "subaccounts": true }));
    assert!(kinds(&result).is_empty(), "the orphan is in Opening, which does not touch Expenses");
    let result = call(&mut d, "scrub.orphans", &json!({ "account": FOOD }));
    assert!(kinds(&result).is_empty());
    let result = call(&mut d, "scrub.orphans", &json!({}));
    assert_eq!(kinds(&result), ["accountCreated", "orphanAssigned"]);
    assert_eq!(result["repairs"][1]["transaction"], OPENING);
    let result = call(&mut d, "scrub.checkAndRepair", &json!({ "account": "Expenses", "subaccounts": true, "lots": true }));
    assert_eq!(kinds(&result), ["accountCreated", "balanceSplitAdded"], "Groceries, through Food");
    assert!(kinds(&call(&mut d, "scrub.checkAndRepair", &json!({ "lots": true }))).is_empty());
    assert!(kinds(&call(&mut d, "scrub.currency", &json!({}))).is_empty());
}

#[test]
fn refusals() {
    let (_dir, mut d) = open_broken(false);
    assert_eq!(err(&mut d, "scrub.imbalance", &json!({ "transaction": GROCERIES, "account": "Expenses" })), codes::INVALID_PARAMS);
    assert_eq!(err(&mut d, "scrub.imbalance", &json!({ "subaccounts": true })), codes::INVALID_PARAMS);
    assert_eq!(err(&mut d, "scrub.imbalance", &json!({ "transaction": GROCERIES, "subaccounts": true })), codes::INVALID_PARAMS);
    assert_eq!(err(&mut d, "scrub.orphans", &json!({ "account": "Nowhere" })), codes::NOT_FOUND);
    assert_eq!(err(&mut d, "scrub.imbalance", &json!({ "transaction": "0123456789abcdef0123456789abcdef" })), codes::NOT_FOUND);
    assert_eq!(err(&mut d, "scrub.checkAndRepair", &json!({ "transaction": GROCERIES, "lots": true })), codes::INVALID);
    assert_eq!(call(&mut d, "book.info", &json!({}))["dirty"], false, "refusals change nothing");
    let (_dir, mut d) = open_broken(true);
    assert_eq!(err(&mut d, "scrub.checkAndRepair", &json!({})), codes::READ_ONLY);
}

#[test]
fn reconciling_an_unbalanced_transaction_is_refused_until_scrubbed() {
    let (_dir, mut d) = open_broken(false);
    let split = "117dda4092564917b4eb353ecb4e5d62";
    assert_eq!(err(&mut d, "reconcile.setState", &json!({ "split": split, "state": "c" })), codes::UNBALANCED);
    call(&mut d, "scrub.imbalance", &json!({ "transaction": GROCERIES }));
    call(&mut d, "reconcile.setState", &json!({ "split": split, "state": "c" }));
}

#[test]
fn methods_declare_their_effects() {
    let d = Dispatcher::new(Settings::default());
    let schema = d.schema();
    let effect = |name: &str| schema["methods"].as_array().unwrap().iter().find(|m| m["name"] == name).unwrap()["effect"].clone();
    let scrubs = ["scrub.imbalance", "scrub.orphans", "scrub.currency", "scrub.checkAndRepair"];
    scrubs.iter().for_each(|m| assert_eq!(effect(m), "destructive"));
    assert_eq!(effect("reconcile.start"), "read");
    assert_eq!(effect("reconcile.finish"), "write");
}
