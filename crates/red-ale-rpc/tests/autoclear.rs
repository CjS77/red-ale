//! Auto-clear through the dispatcher, on a copy of `synthetic/rich.gnucash`. Its checking account holds a reconciled
//! opening balance of 5000.00 and four new splits: -12.34, -110.00, -1009.95 and +600.00.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";
const GROCERIES: &str = "aff7bc7c47b848f6814da21a590796b7";
const EURO: &str = "c9cde06da6274e87981afb424c59f17d";

fn open() -> (tempfile::TempDir, Dispatcher) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(format!("{}/../../fixtures/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR")), &path).unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(1_719_835_200), 1 << 100))));
    d.call("book.open", json!({ "path": path.display().to_string() })).unwrap();
    (dir, d)
}

fn call(d: &mut Dispatcher, method: &str, params: Value) -> Value {
    let shown = params.to_string();
    d.call(method, params).unwrap_or_else(|e| panic!("{method} {shown}: {e} {:?}", e.data))
}

fn find(d: &mut Dispatcher, figure: &str) -> Value {
    call(d, "reconcile.findAutoClear", json!({ "account": CHECKING, "clearedBalance": figure }))
}

fn states(d: &mut Dispatcher) -> Vec<(String, String)> {
    let list = call(d, "transaction.list", json!({ "account": CHECKING }));
    list["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| t["splits"].as_array().unwrap().clone())
        .filter(|s| s["account"] == CHECKING)
        .map(|s| (s["guid"].as_str().unwrap().to_owned(), s["reconcileState"].as_str().unwrap().to_owned()))
        .collect()
}

#[test]
fn finds_the_one_set_of_splits_and_reports_failures() {
    let (_dir, mut d) = open();
    let found = find(&mut d, "4877.66");
    assert_eq!(found["outcome"], "found");
    assert_eq!(found["currentClearedBalance"], json!({ "num": "500000", "denom": "100" }));
    let picked: Vec<&str> = found["splits"].as_array().unwrap().iter().map(|s| s["split"].as_str().unwrap()).collect();
    assert_eq!(picked, [GROCERIES, EURO], "in account order");
    assert_eq!(found["splits"][1]["datePosted"], "2024-06-03");
    assert_eq!(found["message"], Value::Null);

    let already = find(&mut d, "5000");
    assert_eq!((already["outcome"].as_str(), already["splits"].as_array().map(Vec::len)), (Some("alreadyCleared"), Some(0)));
    assert_eq!(already["message"], "Account is already at Auto-Clear Balance.");
    assert_eq!(find(&mut d, "4500")["outcome"], "unreachable");

    // Two more payments of the same size make any figure needing just one of them ambiguous.
    let pay = json!({ "datePosted": "2024-06-10", "splits": [
        { "account": CHECKING, "value": "-50" }, { "account": "Expenses:Food", "value": "50" }] });
    call(&mut d, "transaction.post", pay.clone());
    call(&mut d, "transaction.post", pay);
    assert_eq!(find(&mut d, "4950")["outcome"], "ambiguous");
    assert_eq!(find(&mut d, "4900")["outcome"], "found");
}

#[test]
fn auto_clear_marks_the_splits_cleared_or_changes_nothing() {
    let (_dir, mut d) = open();
    let before = states(&mut d);
    let e = d.call("reconcile.autoClear", json!({ "account": CHECKING, "clearedBalance": "4500" })).unwrap_err();
    assert_eq!((e.code, e.data.unwrap()["reason"].as_str()), (codes::INVALID, Some("unreachable")));
    assert_eq!(states(&mut d), before);

    let done = call(&mut d, "reconcile.autoClear", json!({ "account": CHECKING, "clearedBalance": "4877.66" }));
    assert_eq!(done["outcome"], "found");
    let after = states(&mut d);
    let state = |g: &str| after.iter().find(|(s, _)| s == g).map(|(_, st)| st.as_str());
    assert_eq!((state(GROCERIES), state(EURO)), (Some("c"), Some("c")));
    assert_eq!(after.iter().filter(|(_, st)| st == "n").count(), 2);
    assert_eq!(find(&mut d, "4877.66")["outcome"], "alreadyCleared");
    let info = call(&mut d, "book.info", json!({}));
    assert_eq!(info["dirty"], true);
}
