//! The reconcile methods through the dispatcher, on a copy of `synthetic/rich.gnucash`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

/// 2024-07-01 12:00:00 UTC.
const NOW: Time64 = Time64(1_719_835_200);
const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";
const OPENING: &str = "be0e9ff1864d40848698b6dc8313515e";
const GROCERIES: &str = "aff7bc7c47b848f6814da21a590796b7";
const EURO: &str = "c9cde06da6274e87981afb424c59f17d";
const ACME_BUY: &str = "45a2c80dfeec49758ef32c715050677c";
const ACME_SELL: &str = "fc4e6f9a523e438a9beab3d97ed51df0";
const VOIDED: &str = "823f78ac78fd44c0956c238ae7c0d28d";

fn open() -> (tempfile::TempDir, Dispatcher) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(format!("{}/../../fixtures/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR")), &path).unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(NOW, 1 << 100))));
    d.call("book.open", json!({ "path": path.display().to_string() })).unwrap();
    (dir, d)
}

fn call(d: &mut Dispatcher, method: &str, params: Value) -> Value {
    let shown = params.to_string();
    d.call(method, params).unwrap_or_else(|e| panic!("{method} {shown}: {e} {:?}", e.data))
}

fn err(d: &mut Dispatcher, method: &str, params: Value) -> i32 {
    d.call(method, params).unwrap_err().code
}

fn amount(num: i64, denom: i64) -> Value {
    json!({ "num": num.to_string(), "denom": denom.to_string() })
}

fn same(a: &Value, num: i64, denom: i64) -> bool {
    let n: i128 = a["num"].as_str().unwrap().parse().unwrap();
    let d: i128 = a["denom"].as_str().unwrap().parse().unwrap();
    n * i128::from(denom) == i128::from(num) * d
}

fn split_ids(view: &Value) -> Vec<String> {
    view["splits"].as_array().unwrap().iter().map(|s| s["split"].as_str().unwrap().to_owned()).collect()
}

#[test]
fn start_uses_gnucash_defaults() {
    let (_dir, mut d) = open();
    let info = call(&mut d, "reconcile.info", json!({ "account": CHECKING }));
    assert_eq!(info["lastDate"], "2024-01-31T00:00:00Z");
    assert_eq!(info["lastInterval"], Value::Null);
    let view = call(&mut d, "reconcile.start", json!({ "account": CHECKING }));
    assert_eq!(view["statementDate"], "2024-02-29", "last date plus one month, tracking the month end");
    assert_eq!(view["statementTime"], "2024-02-29T23:59:59Z");
    assert!(same(&view["endingBalance"], 0, 1), "balance at the statement date");
    assert!(same(&view["startingBalance"], 5000, 1), "the reconciled opening split");
    assert_eq!(split_ids(&view), [GROCERIES, EURO, ACME_BUY, ACME_SELL], "n and c splits, any date; voided and reconciled left out");
    assert_eq!(view["ticked"], json!([]));
    assert!(same(&view["difference"], -5000, 1));

    let east = call(&mut d, "reconcile.start", json!({ "account": CHECKING, "statementDate": "2024-06-30", "utcOffsetMinutes": 120 }));
    assert_eq!(east["statementTime"], "2024-06-30T21:59:59Z");
    assert_eq!(err(&mut d, "reconcile.start", json!({ "account": CHECKING, "utcOffsetMinutes": 5000 })), codes::INVALID_PARAMS);
    assert_eq!(err(&mut d, "reconcile.start", json!({ "account": "Assets", "includeSubaccounts": true })), codes::INVALID);
    assert_eq!(err(&mut d, "reconcile.start", json!({ "account": "Nope" })), codes::NOT_FOUND);
}

#[test]
fn tick_and_finish() {
    let (_dir, mut d) = open();
    let set = call(&mut d, "reconcile.setState", json!({ "split": GROCERIES, "state": "c" }));
    assert_eq!(set["state"], "c");
    let start = json!({ "account": CHECKING, "statementDate": "2024-06-30" });
    let view = call(&mut d, "reconcile.start", start);
    assert!(same(&view["endingBalance"], 446_771, 100), "{}", view["endingBalance"]);
    assert_eq!(view["ticked"], json!([GROCERIES]), "cleared splits by the statement date start ticked");

    let base = json!({ "account": CHECKING, "statementDate": "2024-06-30", "endingBalance": "4467.71" });
    let mut status = base.clone();
    status["mark"] = json!([EURO, ACME_BUY, ACME_SELL]);
    let view = call(&mut d, "reconcile.status", status.clone());
    assert!(same(&view["difference"], 0, 1));
    status["unmark"] = json!([EURO]);
    assert!(same(&call(&mut d, "reconcile.status", status)["difference"], -110, 1));

    let mut fin = base.clone();
    fin["ticked"] = json!([GROCERIES, EURO]);
    assert_eq!(err(&mut d, "reconcile.finish", fin.clone()), codes::INVALID, "the difference is not zero");
    fin["ticked"] = json!([OPENING]);
    assert_eq!(err(&mut d, "reconcile.finish", fin.clone()), codes::INVALID, "already reconciled");
    fin["ticked"] = json!([GROCERIES, EURO, ACME_BUY, ACME_SELL]);
    let done = call(&mut d, "reconcile.finish", fin);
    assert_eq!(done["reconciled"].as_array().unwrap().len(), 4);
    assert_eq!(done["lastInterval"], json!({ "months": 5, "days": 0 }));
    let info = call(&mut d, "reconcile.info", json!({ "account": CHECKING }));
    assert_eq!(info["lastDate"], "2024-06-30T23:59:59Z");
    assert_eq!(info["includeSubaccounts"], false);
    let next = call(&mut d, "reconcile.start", json!({ "account": CHECKING }));
    assert_eq!(next["statementDate"], "2024-07-01", "five months on, clamped to today");
    assert!(next["splits"].as_array().unwrap().is_empty());
    assert!(same(&next["startingBalance"], 446_771, 100));
}

#[test]
fn postpone_then_resume() {
    let (_dir, mut d) = open();
    call(&mut d, "reconcile.setState", json!({ "split": GROCERIES, "state": "c" }));
    let params = json!({ "account": CHECKING, "statementDate": "2024-06-03", "endingBalance": "4890", "ticked": [EURO] });
    let done = call(&mut d, "reconcile.postpone", params);
    let changed: Vec<(&str, &str)> =
        done["changed"].as_array().unwrap().iter().map(|c| (c["split"].as_str().unwrap(), c["to"].as_str().unwrap())).collect();
    assert_eq!(changed, [(GROCERIES, "n"), (EURO, "c")], "unticked cleared splits by the date are un-cleared");
    let info = call(&mut d, "reconcile.info", json!({ "account": CHECKING }));
    assert_eq!(info["postponeDate"], "2024-06-03T23:59:59Z");
    assert!(same(&info["postponeBalance"], 4890, 1));
    let view = call(&mut d, "reconcile.start", json!({ "account": CHECKING }));
    assert_eq!(view["resumedPostponed"], true);
    assert_eq!(view["statementDate"], "2024-06-03");
    assert!(same(&view["endingBalance"], 4890, 1));
    assert_eq!(view["ticked"], json!([EURO]));
    let fin = json!({ "account": CHECKING, "statementDate": "2024-06-03", "endingBalance": "4890", "ticked": [EURO] });
    call(&mut d, "reconcile.finish", fin);
    let info = call(&mut d, "reconcile.info", json!({ "account": CHECKING }));
    assert_eq!((&info["postponeDate"], &info["postponeBalance"]), (&Value::Null, &Value::Null));
}

#[test]
fn split_state_rules() {
    let (_dir, mut d) = open();
    let y = call(&mut d, "reconcile.setState", json!({ "split": EURO, "state": "y" }));
    assert_eq!(y["reconcileDate"], "2024-07-01T12:00:00Z");
    let y = call(&mut d, "reconcile.setState", json!({ "split": EURO, "state": "y", "date": "2024-06-30T23:59:59Z" }));
    assert_eq!(y["reconcileDate"], "2024-06-30T23:59:59Z");
    let f = call(&mut d, "reconcile.setState", json!({ "split": EURO, "state": "f" }));
    assert_eq!((&f["state"], &f["reconcileDate"]), (&json!("f"), &json!("2024-06-30T23:59:59Z")));
    assert_eq!(err(&mut d, "reconcile.setState", json!({ "split": EURO, "state": "v" })), codes::INVALID);
    assert_eq!(err(&mut d, "reconcile.setState", json!({ "split": EURO, "state": "x" })), codes::INVALID_PARAMS);
    assert_eq!(err(&mut d, "reconcile.setState", json!({ "split": EURO, "state": "c", "date": "2024-06-30T00:00:00Z" })), codes::INVALID);
    assert_eq!(err(&mut d, "reconcile.setState", json!({ "split": VOIDED, "state": "c" })), codes::CONFLICT);
    assert_eq!(err(&mut d, "reconcile.setState", json!({ "split": "0123456789abcdef0123456789abcdef", "state": "c" })), codes::NOT_FOUND);
}

#[test]
fn set_info() {
    let (_dir, mut d) = open();
    let p = json!({
        "account": CHECKING,
        "lastInterval": { "months": 0, "days": 14 },
        "postpone": { "date": "2024-06-15T23:59:59Z", "balance": amount(-12, 1) },
        "includeSubaccounts": true,
    });
    let info = call(&mut d, "reconcile.setInfo", p);
    assert_eq!(info["lastDate"], "2024-01-31T00:00:00Z", "absent fields are left alone");
    assert_eq!(info["lastInterval"], json!({ "months": 0, "days": 14 }));
    assert_eq!(info["postponeDate"], "2024-06-15T23:59:59Z");
    assert_eq!(info["includeSubaccounts"], true);
    let info = call(&mut d, "reconcile.setInfo", json!({ "account": CHECKING, "lastDate": null, "postpone": null, "lastInterval": null }));
    assert_eq!((&info["lastDate"], &info["postponeDate"], &info["lastInterval"]), (&Value::Null, &Value::Null, &Value::Null));
    let bad = json!({ "account": CHECKING, "lastInterval": { "months": -1, "days": 0 } });
    assert_eq!(err(&mut d, "reconcile.setInfo", bad), codes::INVALID);
}
