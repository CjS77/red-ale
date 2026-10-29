//! The book options methods, and how transactions follow the num field source and the auto read-only threshold, on
//! copies of `synthetic/rich.gnucash`. Today is 2024-07-01.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";
const NOW: Time64 = Time64(1_719_835_200);

struct Book {
    dir: tempfile::TempDir,
    d: Dispatcher,
}

fn dispatcher() -> Dispatcher {
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(NOW, 0x00ed_0000_0000_0000_0000_0000_0000_0000))));
    d
}

fn open() -> Book {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(
        format!("{}/../../fixtures/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR")),
        &path,
    )
    .unwrap();
    let mut d = dispatcher();
    d.call("book.open", json!({ "path": path.display().to_string() })).unwrap();
    Book { dir, d }
}

impl Book {
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.d
            .call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        self.d
            .call(method, params.clone())
            .err()
            .unwrap_or_else(|| panic!("{method} {params} should fail"))
    }

    fn post(&mut self, date: &str, num: &str, action: &str) -> Value {
        self.call(
            "transaction.post",
            json!({
                "datePosted": date, "num": num, "description": format!("on {date}"),
                "splits": [
                    { "account": CHECKING, "value": "-10.00", "action": action },
                    { "account": "Expenses:Food", "value": "10.00" }
                ]
            }),
        )
    }

    fn reopen(&mut self) {
        self.call("book.close", json!({ "save": true }));
        let path = self.dir.path().join("book.gnucash");
        self.d = dispatcher();
        self.call("book.open", json!({ "path": path.display().to_string() }));
    }
}

fn assert_auto_read_only(e: &RpcError, code: i32, date: &str) {
    assert_eq!(e.code, code, "{e}");
    assert_eq!(
        e.data,
        Some(json!({
            "reason": "auto-read-only", "date": date, "threshold": "2024-06-21",
            "help": format!("error/{}", codes::name(code).unwrap()),
        })),
        "{e}"
    );
}

#[test]
fn options_are_off_by_default_and_set_as_the_dialog_sets_them() {
    let mut b = open();
    assert_eq!(
        b.call("book.options", json!({})),
        json!({ "splitActionForNum": false, "autoReadOnlyDays": 0, "useTradingAccounts": false, "defaultBudget": null })
    );
    let set = b.call("book.setOptions", json!({ "splitActionForNum": true, "autoReadOnlyDays": 10 }));
    assert_eq!(
        set,
        json!({ "splitActionForNum": true, "autoReadOnlyDays": 10, "readOnlyThreshold": "2024-06-21", "useTradingAccounts": false, "defaultBudget": null })
    );
    let info = b.call("book.info", json!({}));
    let feature = info["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "Number Field Source")
        .expect("the feature is set");
    assert_eq!(feature["supported"], true);
    assert_eq!(info["readOnly"], false);
    b.reopen();
    assert_eq!(
        b.call("book.options", json!({}))["autoReadOnlyDays"],
        10,
        "the options survive a save"
    );
    assert_eq!(b.call("book.info", json!({}))["readOnly"], false, "and the book opens for editing");
    let off = b.call(
        "book.setOptions",
        json!({ "splitActionForNum": false, "autoReadOnlyDays": 0, "useTradingAccounts": false, "defaultBudget": null }),
    );
    assert_eq!(
        off,
        json!({ "splitActionForNum": false, "autoReadOnlyDays": 0, "useTradingAccounts": false, "defaultBudget": null })
    );
    let info = b.call("book.info", json!({}));
    assert!(
        info["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["name"] == "Number Field Source"),
        "GnuCash never removes it"
    );
}

#[test]
fn days_outside_the_dialog_range_are_refused() {
    let mut b = open();
    assert_eq!(b.err("book.setOptions", json!({ "autoReadOnlyDays": 3651 })).code, codes::INVALID);
    assert_eq!(b.err("book.setOptions", json!({ "autoReadOnlyDays": -1 })).code, codes::INVALID);
    assert_eq!(b.call("book.options", json!({}))["autoReadOnlyDays"], 0, "nothing changed");
}

#[test]
fn transactions_before_the_threshold_cannot_be_changed() {
    let mut b = open();
    let old = b.post("2024-06-20", "1", "")["guid"].clone();
    let edge = b.post("2024-06-21", "2", "")["guid"].clone();
    b.call("book.setOptions", json!({ "autoReadOnlyDays": 10 }));

    let e = b.err("transaction.update", json!({ "guid": old, "description": "late" }));
    assert_auto_read_only(&e, codes::CONFLICT, "2024-06-20");
    assert_auto_read_only(&b.err("transaction.delete", json!({ "guid": old })), codes::CONFLICT, "2024-06-20");
    assert_auto_read_only(
        &b.err("transaction.void", json!({ "guid": old, "reason": "x" })),
        codes::CONFLICT,
        "2024-06-20",
    );
    assert_auto_read_only(&b.err("transaction.unvoid", json!({ "guid": old })), codes::CONFLICT, "2024-06-20");

    let e = b.err("transaction.update", json!({ "guid": edge, "datePosted": "2024-06-20" }));
    assert_auto_read_only(&e, codes::INVALID, "2024-06-20");
    let e = b.err(
        "transaction.post",
        json!({ "datePosted": "2024-06-01", "splits": [{ "account": CHECKING, "value": "-1" }, { "account": "Expenses:Food", "value": "1" }] }),
    );
    assert_auto_read_only(&e, codes::INVALID, "2024-06-01");

    b.call("transaction.update", json!({ "guid": edge, "description": "on the threshold" }));
    b.call("transaction.void", json!({ "guid": edge, "reason": "x" }));
    b.call("transaction.unvoid", json!({ "guid": edge }));
    b.call("transaction.delete", json!({ "guid": edge }));
}

#[test]
fn reverse_and_transfer_ignore_the_threshold_as_gnucash_does() {
    let mut b = open();
    let old = b.post("2024-06-01", "1", "")["guid"].clone();
    b.call("book.setOptions", json!({ "autoReadOnlyDays": 10 }));
    let reversal = b.call("transaction.reverse", json!({ "guid": old, "date": "2024-06-02" }));
    assert_eq!(reversal["datePosted"], "2024-06-02");
    let t = b.call(
        "transaction.transfer",
        json!({ "from": CHECKING, "to": "Expenses:Food", "amount": "5", "date": "2024-05-01" }),
    );
    assert_eq!(t["transaction"]["datePosted"], "2024-05-01");
}

#[test]
fn the_number_is_the_action_with_the_num_field_source_on() {
    let mut b = open();
    let t = b.post("2024-06-25", "77", "A-1");
    let checking = |t: &Value| {
        t["splits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["account"] == CHECKING)
            .unwrap()
            .clone()
    };
    assert_eq!(checking(&t)["number"], "77");
    b.call("book.setOptions", json!({ "splitActionForNum": true }));
    let t = b.call("transaction.get", json!({ "guid": t["guid"] }));
    assert_eq!(checking(&t)["number"], "A-1");
    assert_eq!(t["num"], "77", "num stays the transaction's num");
}

#[test]
fn transfers_put_the_num_on_the_from_split_with_the_num_field_source_on() {
    let mut b = open();
    b.call("book.setOptions", json!({ "splitActionForNum": true }));
    let t = b.call(
        "transaction.transfer",
        json!({ "from": CHECKING, "to": "Expenses:Food", "amount": "5", "date": "2024-06-25", "num": "T9" }),
    );
    let tx = &t["transaction"];
    assert_eq!(tx.get("num"), Some(&Value::Null));
    let from = tx["splits"].as_array().unwrap().iter().find(|s| s["account"] == CHECKING).unwrap();
    assert_eq!(from["action"], "T9");
    assert_eq!(from["number"], "T9");
}

#[test]
fn find_orders_same_day_splits_by_action_with_the_num_field_source_on() {
    let mut b = open();
    let first = b.post("2024-06-25", "1", "B")["guid"].clone();
    let second = b.post("2024-06-25", "2", "A")["guid"].clone();
    let query =
        json!({ "query": { "all": [{ "datePosted": { "op": "on", "date": "2024-06-25" } }, { "accounts": { "accounts": [CHECKING] } }] } });
    let order = |b: &mut Book| -> Vec<Value> {
        b.call("transaction.find", query.clone())["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["transaction"].clone())
            .collect()
    };
    assert_eq!(order(&mut b), vec![first.clone(), second.clone()], "by num");
    b.call("book.setOptions", json!({ "splitActionForNum": true }));
    assert_eq!(order(&mut b), vec![second, first], "by action");
}
