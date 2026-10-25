//! Trading accounts through the dispatcher: the book option, and the trading splits every Mutation keeps.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

fn call(d: &mut Dispatcher, method: &str, params: &Value) -> Value {
    d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
}

/// A new book with USD checking, a EUR account and a stock, as in the oracle runs of
/// `design/notes/engine-accounts-transactions.md` §9.6; trading accounts on when `trading`.
fn book(trading: bool) -> (tempfile::TempDir, Dispatcher) {
    let dir = tempfile::tempdir().unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(1_719_835_200), 1 << 100))));
    call(&mut d, "book.create", &json!({ "path": dir.path().join("t.gnucash").display().to_string() }));
    call(&mut d, "commodity.create", &json!({ "namespace": "NASDAQ", "mnemonic": "ACME", "fullname": "Acme", "fraction": 1000 }));
    [
        json!({ "name": "Assets", "type": "ASSET", "commodity": "CURRENCY::USD" }),
        json!({ "name": "Checking", "type": "BANK", "parent": "Assets", "commodity": "CURRENCY::USD" }),
        json!({ "name": "Euro", "type": "BANK", "parent": "Assets", "commodity": "CURRENCY::EUR" }),
        json!({ "name": "ACME", "type": "STOCK", "parent": "Assets", "commodity": "NASDAQ::ACME" }),
        json!({ "name": "Income", "type": "INCOME", "commodity": "CURRENCY::USD" }),
    ]
    .iter()
    .for_each(|a| {
        call(&mut d, "account.create", a);
    });
    if trading {
        call(&mut d, "book.setOptions", &json!({ "useTradingAccounts": true }));
    }
    (dir, d)
}

fn exchange(d: &mut Dispatcher) -> Value {
    call(
        d,
        "transaction.post",
        &json!({ "datePosted": "2024-03-01", "currency": "CURRENCY::USD", "splits": [
            { "account": "Assets:Euro", "value": "110.00", "amount": "100.00" },
            { "account": "Assets:Checking", "value": "-110.00" },
        ]}),
    )
}

/// `(accountName, value, amount)` of each split, values and amounts as decimal-free `num/denom`.
fn rows(t: &Value) -> Vec<(String, String, String)> {
    let frac = |n: &Value| format!("{}/{}", n["num"].as_str().unwrap(), n["denom"].as_str().unwrap());
    let row = |s: &Value| (s["accountName"].as_str().unwrap().to_owned(), frac(&s["value"]), frac(&s["amount"]));
    t["splits"].as_array().unwrap().iter().map(row).collect()
}

fn row(a: &str, v: &str, q: &str) -> (String, String, String) {
    (a.to_owned(), v.to_owned(), q.to_owned())
}

#[test]
fn the_option_is_read_and_set() {
    let (_dir, mut d) = book(false);
    assert_eq!(call(&mut d, "book.options", &json!({}))["useTradingAccounts"], false);
    assert_eq!(call(&mut d, "book.setOptions", &json!({ "useTradingAccounts": true }))["useTradingAccounts"], true);
    assert_eq!(call(&mut d, "book.options", &json!({}))["useTradingAccounts"], true);
    assert_eq!(call(&mut d, "book.info", &json!({}))["readOnly"], false);
    assert_eq!(call(&mut d, "book.setOptions", &json!({ "useTradingAccounts": false }))["useTradingAccounts"], false);
    assert_eq!(d.call("book.setOptions", json!({ "useTradingAccounts": "yes" })).unwrap_err().code, codes::INVALID_PARAMS);
}

#[test]
fn post_update_void_unvoid_and_reverse_keep_trading_splits() {
    let (_dir, mut d) = book(true);
    let t = exchange(&mut d);
    assert_eq!(
        rows(&t),
        [
            row("Assets:Euro", "11000/100", "10000/100"),
            row("Trading:CURRENCY:USD", "11000/100", "11000/100"),
            row("Assets:Checking", "-11000/100", "-11000/100"),
            row("Trading:CURRENCY:EUR", "-11000/100", "-10000/100"),
        ]
    );
    assert_eq!(t["balanced"], true);
    let guid = t["guid"].clone();
    let euro_split = t["splits"][0]["guid"].clone();
    let t = call(&mut d, "transaction.update", &json!({ "guid": guid, "changeSplits": [{ "guid": euro_split, "amount": "90.00" }] }));
    assert!(rows(&t).contains(&row("Trading:CURRENCY:EUR", "-11000/100", "-9000/100")), "{t}");
    assert_eq!(t["balanced"], true);
    let voided = call(&mut d, "transaction.void", &json!({ "guid": guid, "reason": "typo" }));
    assert_eq!(voided["splits"].as_array().unwrap().len(), 4);
    assert_eq!(voided["balanced"], true);
    let back = call(&mut d, "transaction.unvoid", &json!({ "guid": guid }));
    assert_eq!(rows(&back), rows(&t));
    let reversed = call(&mut d, "transaction.reverse", &json!({ "guid": guid, "date": "2024-03-02" }));
    let unsigned = |t: &Value| {
        let strip = |x: &str| x.trim_start_matches('-').to_owned();
        let mut r: Vec<_> = rows(t).into_iter().map(|(a, v, q)| (a, strip(&v), strip(&q))).collect();
        r.sort();
        r
    };
    assert_eq!(unsigned(&reversed), unsigned(&t));
    assert_eq!(reversed["balanced"], true);
}

#[test]
fn a_transfer_and_a_stock_purchase_get_trading_splits() {
    let (_dir, mut d) = book(true);
    let transfer =
        json!({ "from": "Assets:Checking", "to": "Assets:Euro", "amount": "110.00", "toAmount": "100.00", "date": "2024-03-01" });
    let x = call(&mut d, "transaction.transfer", &transfer);
    assert_eq!(x["transaction"]["splits"].as_array().unwrap().len(), 4);
    let buy = call(
        &mut d,
        "transaction.post",
        &json!({ "datePosted": "2024-03-01", "currency": "CURRENCY::USD", "splits": [
            { "account": "Assets:ACME", "value": "500.00", "amount": "10.000" },
            { "account": "Assets:Checking", "value": "-500.00" },
        ]}),
    );
    assert_eq!(
        rows(&buy),
        [
            row("Assets:ACME", "50000/100", "10000/1000"),
            row("Trading:CURRENCY:USD", "50000/100", "50000/100"),
            row("Assets:Checking", "-50000/100", "-50000/100"),
            row("Trading:NASDAQ:ACME", "-50000/100", "-10000/1000"),
        ]
    );
    let tree = call(&mut d, "account.get", &json!({ "account": "Trading:NASDAQ" }));
    assert_eq!((tree["type"].as_str(), tree["placeholder"].as_bool()), (Some("TRADING"), Some(true)));
}

#[test]
fn a_transaction_from_before_the_option_gets_trading_splits_when_next_changed() {
    let (_dir, mut d) = book(false);
    let t = exchange(&mut d);
    assert_eq!(t["splits"].as_array().unwrap().len(), 2);
    call(&mut d, "book.setOptions", &json!({ "useTradingAccounts": true }));
    let guid = t["guid"].clone();
    let still = call(&mut d, "transaction.get", &json!({ "guid": guid }));
    let split_count = still["splits"].as_array().unwrap().len();
    assert_eq!((split_count, &still["balanced"]), (2, &json!(false)), "turning the option on changes nothing");
    call(&mut d, "reconcile.setState", &json!({ "split": t["splits"][0]["guid"], "state": "c" }));
    let after = call(&mut d, "transaction.get", &json!({ "guid": guid }));
    assert_eq!(after["splits"].as_array().unwrap().len(), 4, "{after}");
    assert_eq!(after["balanced"], true);
}

#[test]
fn unbalanced_values_are_refused_in_a_trading_book() {
    let (_dir, mut d) = book(true);
    let e = d
        .call(
            "transaction.post",
            json!({ "datePosted": "2024-03-01", "currency": "CURRENCY::USD", "splits": [
                { "account": "Assets:Euro", "value": "110.00", "amount": "100.00" },
                { "account": "Assets:Checking", "value": "-100.00" },
            ]}),
        )
        .unwrap_err();
    assert_eq!(e.code, codes::UNBALANCED);
    let accounts = call(&mut d, "account.list", &json!({}));
    assert!(accounts["accounts"].as_array().unwrap().iter().all(|a| a["type"] != "TRADING"), "nothing is left behind");
}
