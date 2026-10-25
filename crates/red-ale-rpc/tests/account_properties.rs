//! Account colours, balance limits, the opening balance account, associated accounts, cascading and the display
//! symbol through the dispatcher, and their round trip through a saved book.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

const OB_FEATURE: &str = "Use a dedicated opening balance account identified by an 'equity-type' slot";

struct Book {
    dir: tempfile::TempDir,
    d: Dispatcher,
}

fn dispatcher() -> Dispatcher {
    let mut d = Dispatcher::new(Settings::default());
    // 2024-07-01T12:00:00Z
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(1_719_835_200), 0xb0 << 120))));
    d
}

fn create() -> Book {
    let dir = tempfile::tempdir().unwrap();
    let mut d = dispatcher();
    d.call("book.create", json!({ "path": dir.path().join("new.gnucash").display().to_string() })).unwrap();
    Book { dir, d }
}

impl Book {
    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        match self.d.call(method, params.clone()) {
            Ok(v) => panic!("{method} {params} should fail, gave {v}"),
            Err(e) => e,
        }
    }

    fn add(&mut self, name: &str, ty: &str, parent: Option<&str>, commodity: &str) -> String {
        let mut p = json!({ "name": name, "type": ty, "commodity": commodity });
        if let Some(parent) = parent {
            p["parent"] = json!(parent);
        }
        self.ok("account.create", p)["guid"].as_str().unwrap().to_owned()
    }

    fn post(&mut self, debit: &str, credit: &str, amount: &str) {
        let splits = json!([{ "account": debit, "value": amount }, { "account": credit, "value": format!("-{amount}") }]);
        self.ok("transaction.post", json!({ "description": "x", "datePosted": "2024-06-01", "splits": splits }));
    }

    fn reopen(&mut self) {
        self.ok("book.close", json!({ "save": true }));
        self.ok("book.open", json!({ "path": self.dir.path().join("new.gnucash").display().to_string() }));
    }
}

fn amount(v: &Value) -> String {
    format!("{}/{}", v["num"].as_str().unwrap(), v["denom"].as_str().unwrap())
}

#[test]
fn colours_are_written_as_the_dialog_writes_them_and_survive_a_save() {
    let mut b = create();
    b.add("Assets", "ASSET", None, "USD");
    let a = b.ok("account.update", json!({ "account": "Assets", "color": "#1a2B3c" }));
    assert_eq!(a["color"], "rgb(26,43,60)");
    assert_eq!(b.err("account.update", json!({ "account": "Assets", "color": "teal" })).code, codes::INVALID);
    b.reopen();
    assert_eq!(b.ok("account.get", json!({ "account": "Assets" }))["color"], "rgb(26,43,60)");
    assert_eq!(b.ok("account.update", json!({ "account": "Assets", "color": "rgb(237,236,235)" }))["color"], Value::Null);
}

#[test]
fn balance_limits_are_set_checked_and_cleared() {
    let mut b = create();
    b.add("Bank", "BANK", None, "USD");
    b.add("Sub", "BANK", Some("Bank"), "USD");
    b.add("Income", "INCOME", None, "USD");
    b.post("Bank", "Income", "100.00");
    b.post("Bank:Sub", "Income", "30.00");
    let a = b.ok("account.update", json!({ "account": "Bank", "higherBalanceLimit": "120.00", "lowerBalanceLimit": "-10" }));
    assert_eq!((amount(&a["higherBalanceLimit"]), amount(&a["lowerBalanceLimit"])), ("12000/100".to_owned(), "-10/1".to_owned()));
    let check = b.ok("account.checkBalanceLimit", json!({ "account": "Bank" }));
    assert_eq!((amount(&check["balance"]), &check["state"], &check["asOf"]), ("10000/100".to_owned(), &Value::Null, &json!("2024-07-01")));
    b.ok("account.update", json!({ "account": "Bank", "includeSubaccountBalances": true }));
    let check = b.ok("account.checkBalanceLimit", json!({ "account": "Bank" }));
    assert_eq!((amount(&check["balance"]), &check["state"]), ("13000/100".to_owned(), &json!("above")));
    assert_eq!(b.ok("account.checkBalanceLimit", json!({ "account": "Bank", "asOf": "2024-05-31" }))["state"], Value::Null);
    let bad = b.err("account.update", json!({ "account": "Bank", "lowerBalanceLimit": "500" }));
    assert_eq!(bad.code, codes::INVALID);
    b.reopen();
    assert_eq!(b.ok("account.get", json!({ "account": "Bank" }))["includeSubaccountBalances"], true);
    let cleared = b.ok("account.update", json!({ "account": "Bank", "higherBalanceLimit": null, "lowerBalanceLimit": null }));
    assert_eq!((&cleared["higherBalanceLimit"], &cleared["includeSubaccountBalances"]), (&Value::Null, &json!(false)));
}

#[test]
fn the_opening_balance_account_is_unique_per_currency_and_sets_the_feature() {
    let mut b = create();
    b.add("Equity", "EQUITY", None, "USD");
    let named = b.add("Opening Balances", "EQUITY", Some("Equity"), "USD");
    let found = b.ok("account.openingBalanceAccount", json!({ "currency": "USD" }));
    assert_eq!((&found["account"]["guid"], &found["foundBy"], &found["featureUsed"]), (&json!(named), &json!("name"), &json!(false)));
    let ob = b.ok("account.create", json!({ "name": "OB", "type": "EQUITY", "parent": "Equity", "openingBalance": true }));
    assert_eq!(ob["openingBalance"], true);
    let found = b.ok("account.openingBalanceAccount", json!({ "currency": "USD" }));
    assert_eq!((&found["account"]["guid"], &found["foundBy"], &found["featureUsed"]), (&ob["guid"], &json!("equityType"), &json!(true)));
    assert_eq!(b.err("account.update", json!({ "account": named, "openingBalance": true })).code, codes::CONFLICT);
    b.add("Bank", "BANK", None, "USD");
    assert_eq!(b.err("account.update", json!({ "account": "Bank", "openingBalance": true })).code, codes::INVALID);
    assert_eq!(b.err("account.openingBalanceAccount", json!({ "currency": "NASDAQ::NONE" })).code, codes::NOT_FOUND);
    b.reopen();
    let features = b.ok("book.info", json!({}))["features"].to_string();
    assert!(features.contains(OB_FEATURE), "{features}");
    assert_eq!(b.ok("book.info", json!({}))["readOnly"], false);
    assert_eq!(b.ok("account.get", json!({ "account": "Equity:OB" }))["openingBalance"], true);
}

#[test]
fn associated_accounts_by_tag() {
    let mut b = create();
    b.add("Broker", "ASSET", None, "USD");
    let div = b.add("Dividends", "INCOME", None, "USD");
    let a = b.ok("account.update", json!({ "account": "Broker", "associatedAccounts": { "stock-dividends": "Dividends", "stock-capgains": div } }));
    assert_eq!(a["associatedAccounts"], json!({ "stock-capgains": div, "stock-dividends": div }));
    b.reopen();
    let a = b.ok("account.update", json!({ "account": "Broker", "associatedAccounts": { "stock-capgains": null } }));
    assert_eq!(a["associatedAccounts"], json!({ "stock-dividends": div }));
    assert_eq!(b.err("account.update", json!({ "account": "Broker", "associatedAccounts": { "x": "Nowhere" } })).code, codes::NOT_FOUND);
    assert_eq!(b.err("account.update", json!({ "account": "Broker", "associatedAccounts": { "": "Dividends" } })).code, codes::INVALID);
}

#[test]
fn cascade_applies_to_every_descendant() {
    let mut b = create();
    let top = b.add("Top", "ASSET", None, "USD");
    let mid = b.add("Mid", "ASSET", Some("Top"), "USD");
    let leaf = b.add("Leaf", "ASSET", Some("Top:Mid"), "USD");
    b.ok("account.update", json!({ "account": "Top:Mid", "color": "#00ff00" }));
    let r = b.ok("account.cascade", json!({ "account": "Top", "color": "#ff0000", "hidden": true }));
    assert_eq!(r["changed"], json!([top, mid, leaf]));
    let colors: Vec<Value> = ["Top", "Top:Mid", "Top:Mid:Leaf"].iter().map(|n| b.ok("account.get", json!({ "account": n }))["color"].clone()).collect();
    assert_eq!(colors, [json!("rgb(255,0,0)"), json!("rgb(0,255,0)"), json!("rgb(255,0,0)")]);
    let r = b.ok("account.cascade", json!({ "account": "Top", "color": "#ff0000", "replaceColors": true }));
    assert_eq!(r["changed"], json!([mid]));
    assert_eq!(b.err("account.cascade", json!({ "account": "Top:Mid:Leaf", "hidden": false })).code, codes::INVALID);
}

#[test]
fn display_symbols() {
    let mut b = create();
    assert_eq!(b.ok("commodity.get", json!({ "commodity": "USD" }))["displaySymbol"], "$");
    assert_eq!(b.ok("commodity.get", json!({ "commodity": "AFN" }))["displaySymbol"], "AFN");
    b.ok("commodity.create", json!({ "namespace": "NASDAQ", "mnemonic": "ACME", "fullname": "Acme" }));
    assert_eq!(b.ok("commodity.get", json!({ "commodity": "NASDAQ::ACME" }))["displaySymbol"], "ACME");
    let c = b.ok("commodity.update", json!({ "commodity": "NASDAQ::ACME", "userSymbol": "Ⓐ" }));
    assert_eq!(c["displaySymbol"], "Ⓐ");
}
