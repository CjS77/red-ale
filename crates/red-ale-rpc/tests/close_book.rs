//! Closing the books through the dispatcher.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

fn open(name: &str) -> (tempfile::TempDir, Dispatcher) {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("book.gnucash");
    std::fs::copy(format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR")), &dest).unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(1_719_835_200), 0x00ed_0000_0000_0000_0000_0000_0000_0000))));
    d.call("book.open", json!({ "path": dest.display().to_string() })).unwrap();
    (dir, d)
}

fn call(d: &mut Dispatcher, method: &str, params: Value) -> Value {
    d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
}

fn balance(d: &mut Dispatcher, account: &str, date: &str) -> String {
    let b = call(d, "account.balance", json!({ "account": account, "asOf": date }));
    let (n, den) = (b["balances"]["balance"]["num"].as_str().unwrap(), b["balances"]["balance"]["denom"].as_str().unwrap());
    format!("{n}/{den}")
}

#[test]
fn closing_zeroes_income_and_expense_into_equity() {
    let (_dir, mut d) = open("synthetic/rich.gnucash");
    let params = json!({ "date": "2024-12-31", "incomeTotalAccount": "Equity", "expenseTotalAccount": "Equity", "description": "Year end" });
    let out = call(&mut d, "book.closeBooks", params.clone());
    let txns = out["transactions"].as_array().unwrap();
    assert!(!txns.is_empty());
    assert!(txns.iter().all(|t| t["currency"] == "CURRENCY::USD" && t["equityAccount"] == txns[0]["equityAccount"]));
    assert_eq!(out["createdAccounts"], json!([]));
    ["Income", "Income:Salary", "Expenses:Food", "Expenses:Fees"].iter().for_each(|a| {
        assert!(balance(&mut d, a, "2025-01-01").starts_with("0/"), "{a} is closed");
    });
    let t = call(&mut d, "transaction.get", json!({ "guid": txns[0]["guid"] }));
    assert_eq!(t["description"], "Year end");
    assert_eq!(t["datePosted"], "2024-12-31");
    assert_eq!(call(&mut d, "book.closeBooks", params)["transactions"], json!([]), "nothing is left to close");
}

#[test]
fn closing_refuses_accounts_that_are_not_equity() {
    let (_dir, mut d) = open("synthetic/rich.gnucash");
    let err = d
        .call("book.closeBooks", json!({ "date": "2024-12-31", "incomeTotalAccount": "Assets:Cash", "expenseTotalAccount": "Equity" }))
        .unwrap_err();
    assert_eq!(err.code, codes::INVALID);
    let err = d
        .call("book.closeBooks", json!({ "date": "2024-12-31", "incomeTotalAccount": "Nope", "expenseTotalAccount": "Equity" }))
        .unwrap_err();
    assert_eq!(err.code, codes::NOT_FOUND);
    assert_eq!(call(&mut d, "book.info", json!({}))["dirty"], false, "a refused close changes nothing");
}
