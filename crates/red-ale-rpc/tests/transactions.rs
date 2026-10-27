//! The transaction methods through the dispatcher, on copies of `synthetic/rich.gnucash`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";
const MISTAKE: &str = "7e04ac3fbb8d4eff83068196f09164f4";
const GROCERIES: &str = "439480360f6342f9abfd6df6f40cc5e9";
const NOW: Time64 = Time64(1_719_835_200);

struct Book {
    _dir: tempfile::TempDir,
    d: Dispatcher,
}

fn open() -> Book {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(
        format!("{}/../../fixtures/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR")),
        &path,
    )
    .unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(NOW, 0x00ed_0000_0000_0000_0000_0000_0000_0000))));
    d.call("book.open", json!({ "path": path.display().to_string() })).unwrap();
    Book { _dir: dir, d }
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
}

fn amount(v: &Value) -> (i64, i64) {
    (
        v["num"].as_str().unwrap().parse().unwrap(),
        v["denom"].as_str().unwrap().parse().unwrap(),
    )
}

fn rent(b: &mut Book) -> Value {
    b.call(
        "transaction.post",
        json!({
            "datePosted": "2024-06-20",
            "num": "102",
            "description": "Rent",
            "notes": "June",
            "splits": [
                { "account": CHECKING, "value": "-750.00", "memo": "cheque", "action": "Pay" },
                { "account": "Expenses:Food", "value": "750.00" }
            ]
        }),
    )
}

#[test]
fn post_get_and_list() {
    let mut b = open();
    let t = rent(&mut b);
    assert_eq!(t["currency"], "CURRENCY::USD", "the first split's account currency");
    assert_eq!(t["datePosted"], "2024-06-20");
    assert_eq!(t["datePostedTime"], "2024-06-20T10:59:00Z");
    assert_eq!(t["dateEntered"], "2024-07-01T12:00:00Z", "date entered is now");
    assert_eq!(t["notes"], "June");
    assert_eq!(t["splits"][0]["accountName"], "Expenses:Food", "debits sort first");
    assert_eq!(amount(&t["splits"][1]["value"]), (-75000, 100));
    assert_eq!(t["splits"][1]["memo"], "cheque");
    assert_eq!(t["splits"][1]["action"], "Pay");
    assert_eq!(t["splits"][1]["reconcileState"], "n");
    let got = b.call("transaction.get", json!({ "guid": t["guid"] }));
    assert_eq!(got, t);
    let listed = b.call(
        "transaction.list",
        json!({ "account": "Expenses:Food", "from": "2024-06-02", "to": "2024-06-30" }),
    );
    let descriptions: Vec<&str> = listed["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["description"].as_str().unwrap())
        .collect();
    assert_eq!(descriptions.len(), 3, "groceries, the voided mistake and rent: {descriptions:?}");
    assert_eq!(descriptions[2], "Rent");
}

#[test]
fn post_refuses_bad_input_atomically() {
    let mut b = open();
    let before = b.call("book.info", json!({}))["counts"]["transactions"].clone();
    let unbalanced = json!({ "splits": [{ "account": CHECKING, "value": "-1" }, { "account": "Expenses:Food", "value": "2" }] });
    assert_eq!(b.err("transaction.post", unbalanced).code, codes::UNBALANCED);
    let placeholder = json!({ "splits": [{ "account": "Assets", "value": "-1" }, { "account": "Expenses:Food", "value": "1" }] });
    assert_eq!(b.err("transaction.post", placeholder).code, codes::INVALID);
    let foreign = json!({ "splits": [{ "account": CHECKING, "value": "-1" }, { "account": "Assets:Euro Bank", "value": "1" }] });
    assert_eq!(
        b.err("transaction.post", foreign).code,
        codes::INVALID,
        "a EUR split needs an amount"
    );
    let float = json!({ "splits": [{ "account": CHECKING, "value": -1.5 }] });
    assert_eq!(b.err("transaction.post", float).code, codes::INVALID_PARAMS);
    assert_eq!(b.call("book.info", json!({}))["counts"]["transactions"], before);
}

#[test]
fn multi_currency_post_keeps_value_and_amount() {
    let mut b = open();
    let t = b.call(
        "transaction.post",
        json!({
            "currency": "USD",
            "datePosted": "2024-06-21",
            "splits": [
                { "account": CHECKING, "value": "-55.00" },
                { "account": "Assets:Euro Bank", "value": "55.00", "amount": "50.00" }
            ],
            "recordPrice": true
        }),
    );
    let eur = &t["splits"][0];
    assert_eq!((amount(&eur["value"]), amount(&eur["amount"])), ((5500, 100), (5000, 100)));
    assert_eq!(eur["commodity"], "CURRENCY::EUR");
    // A BANK account is not priced, so recordPrice adds nothing (xaccAccountIsPriced).
    assert_eq!(b.call("book.info", json!({}))["counts"]["prices"], 4);
    let stock = b.call(
        "transaction.post",
        json!({
            "datePosted": "2024-06-21",
            "splits": [
                { "account": CHECKING, "value": "-1300.00" },
                { "account": "Assets:Brokerage:ACME", "value": "1300.00", "amount": "1", "action": "Buy" }
            ],
            "recordPrice": true
        }),
    );
    assert_eq!(
        amount(&stock["splits"][0]["amount"]),
        (10000, 10000),
        "rounded to the account's SCU"
    );
    assert_eq!(
        b.call("book.info", json!({}))["counts"]["prices"],
        5,
        "a STOCK split records its price"
    );
}

#[test]
fn update_edits_fields_and_splits() {
    let mut b = open();
    let t = rent(&mut b);
    let food = t["splits"][0]["guid"].clone();
    let bank = t["splits"][1]["guid"].clone();
    let u = b.call(
        "transaction.update",
        json!({
            "guid": t["guid"],
            "description": "Rent and fees",
            "notes": "",
            "num": "",
            "datePosted": "2024-06-22",
            "dateEntered": "2024-06-23T08:00:00Z",
            "changeSplits": [{ "guid": bank, "value": "-760.00", "memo": "" }],
            "addSplits": [{ "account": "Expenses:Fees", "value": "10.00", "memo": "late fee" }]
        }),
    );
    assert_eq!(u["description"], "Rent and fees");
    assert!(u["notes"].is_null() && u["num"].is_null());
    assert_eq!(u["datePosted"], "2024-06-22");
    assert_eq!(u["dateEntered"], "2024-06-23T08:00:00Z");
    assert_eq!(u["splits"].as_array().unwrap().len(), 3);
    let bank_split = u["splits"].as_array().unwrap().iter().find(|s| s["guid"] == bank).unwrap();
    assert_eq!(
        amount(&bank_split["amount"]),
        (-76000, 100),
        "the amount follows the value in the currency"
    );
    assert!(bank_split["memo"].is_null());
    let err = b.err("transaction.update", json!({ "guid": t["guid"], "removeSplits": [food] }));
    assert_eq!(err.code, codes::UNBALANCED);
    assert_eq!(
        b.call("transaction.get", json!({ "guid": t["guid"] })),
        u,
        "a refused update changes nothing"
    );
    let err = b.err("transaction.update", json!({ "guid": MISTAKE, "description": "x" }));
    assert_eq!(err.code, codes::CONFLICT, "a voided transaction is read-only");
}

#[test]
fn currency_change_revalues_splits() {
    let mut b = open();
    let t = b.call(
        "transaction.post",
        json!({
            "currency": "USD",
            "splits": [
                { "account": CHECKING, "value": "-110.00" },
                { "account": "Assets:Euro Bank", "value": "110.00", "amount": "100.00" }
            ]
        }),
    );
    let u = b.call("transaction.update", json!({ "guid": t["guid"], "currency": "EUR" }));
    assert_eq!(u["currency"], "CURRENCY::EUR");
    let values: Vec<(i64, i64)> = u["splits"].as_array().unwrap().iter().map(|s| amount(&s["value"])).collect();
    assert_eq!(values, [(10000, 100), (-10000, 100)]);
}

#[test]
fn delete_is_refused_for_read_only_transactions() {
    let mut b = open();
    let t = rent(&mut b);
    b.call("transaction.delete", json!({ "guid": t["guid"] }));
    assert_eq!(b.err("transaction.get", json!({ "guid": t["guid"] })).code, codes::NOT_FOUND);
    assert_eq!(b.err("transaction.delete", json!({ "guid": MISTAKE })).code, codes::CONFLICT);
}

#[test]
fn void_unvoid_and_reverse() {
    let mut b = open();
    let fixture_void = b.call("transaction.get", json!({ "guid": MISTAKE }));
    assert_eq!(fixture_void["void"], true);
    assert_eq!(fixture_void["voidReason"], "entered twice");
    let original = b.call("transaction.get", json!({ "guid": GROCERIES }));
    let v = b.call("transaction.void", json!({ "guid": GROCERIES, "reason": "duplicate" }));
    assert_eq!(v["void"], true);
    assert_eq!(v["voidTime"], "2024-07-01T12:00:00Z");
    assert_eq!(v["notes"], "Voided transaction");
    assert_eq!(v["readOnly"], "Transaction Voided");
    assert!(
        v["splits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["reconcileState"] == "v" && s["value"]["num"] == "0")
    );
    assert_eq!(b.err("transaction.void", json!({ "guid": GROCERIES })).code, codes::CONFLICT);
    let u = b.call("transaction.unvoid", json!({ "guid": GROCERIES }));
    assert_eq!(u, original);
    assert_eq!(b.err("transaction.unvoid", json!({ "guid": GROCERIES })).code, codes::CONFLICT);
    let r = b.call("transaction.reverse", json!({ "guid": GROCERIES, "date": "2024-06-30" }));
    assert_eq!(r["datePosted"], "2024-06-30");
    assert_eq!(r["description"], original["description"]);
    assert_eq!(r["notes"], original["notes"]);
    assert_eq!(b.call("transaction.get", json!({ "guid": GROCERIES }))["reversedBy"], r["guid"]);
    let net: i64 = r["splits"]
        .as_array()
        .unwrap()
        .iter()
        .chain(original["splits"].as_array().unwrap())
        .map(|s| amount(&s["value"]).0)
        .sum();
    assert_eq!(net, 0);
    assert_eq!(b.err("transaction.reverse", json!({ "guid": GROCERIES })).code, codes::CONFLICT);
    assert_eq!(
        b.err("transaction.reverse", json!({ "guid": MISTAKE })).code,
        codes::CONFLICT,
        "a void transaction"
    );
    b.call("transaction.void", json!({ "guid": r["guid"], "reason": "" }));
    b.call("book.save", json!({}));
}

#[test]
fn transfer_records_a_price() {
    let mut b = open();
    let out = b.call(
        "transaction.transfer",
        json!({ "from": CHECKING, "to": "Assets:Euro Bank", "amount": "220.00", "toAmount": "200.00", "date": "2024-06-25", "memo": "fx" }),
    );
    assert_eq!(amount(&out["toAmount"]), (20000, 100));
    assert_eq!(amount(&out["rate"]), (10, 11));
    assert_eq!(out["price"]["action"], "created");
    assert_eq!(out["transaction"]["currency"], "CURRENCY::USD");
    assert_eq!(b.call("book.info", json!({}))["counts"]["prices"], 5);
    // The rich book has EUR in USD from the dialog (user:xfer-dialog) on 2024-06-03. A rate typed that day
    // (user:price, preferred) updates it in place, inverted to its direction.
    let same_day = b.call(
        "transaction.transfer",
        json!({ "from": CHECKING, "to": "Assets:Euro Bank", "amount": "100.00", "rate": "0.8", "date": "2024-06-03" }),
    );
    assert_eq!(same_day["price"]["action"], "updated");
    assert_eq!(same_day["price"]["guid"], "8318eee98d264a3e85b01e608ef2026f");
    assert_eq!(amount(&same_day["toAmount"]), (8000, 100));
    // The same rate again, as an amount received, leaves it: the rounded values are equal.
    let again = b.call(
        "transaction.transfer",
        json!({ "from": CHECKING, "to": "Assets:Euro Bank", "amount": "100.00", "toAmount": "80.00", "date": "2024-06-03" }),
    );
    assert_eq!(again["price"]["action"], "kept", "1/0.8 rounds to the same price");
    assert_eq!(b.call("book.info", json!({}))["counts"]["prices"], 5);
    b.call("book.save", json!({}));
}

#[test]
fn transfer_refusals() {
    let mut b = open();
    let both = json!({ "from": CHECKING, "to": "Assets:Euro Bank", "amount": "1", "rate": "1", "toAmount": "1" });
    assert_eq!(b.err("transaction.transfer", both).code, codes::INVALID);
    let stock = json!({ "from": "Assets:Brokerage:ACME", "to": CHECKING, "amount": "1" });
    assert_eq!(b.err("transaction.transfer", stock).code, codes::INVALID);
    let zero = json!({ "from": CHECKING, "to": "Assets:Cash", "amount": "0" });
    assert_eq!(b.err("transaction.transfer", zero).code, codes::INVALID);
    let same = b.call(
        "transaction.transfer",
        json!({ "from": CHECKING, "to": "Assets:Cash", "amount": "5" }),
    );
    assert!(same["price"].is_null() && same["rate"].is_null());
}

#[test]
fn find_with_the_dialogs_criteria() {
    let mut b = open();
    rent(&mut b);
    let hits = |b: &mut Book, q: Value| -> Vec<String> {
        b.call("transaction.find", json!({ "query": q }))["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["split"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(hits(&mut b, json!({ "memo": { "text": "CHEQUE" } })).len(), 1);
    assert!(hits(&mut b, json!({ "memo": { "text": "CHEQUE", "matchCase": true } })).is_empty());
    assert_eq!(
        hits(
            &mut b,
            json!({ "description": { "op": "matchesRegex", "text": "^(Rent|Opening)$" } })
        )
        .len(),
        4
    );
    assert_eq!(
        hits(&mut b, json!({ "reconcile": { "states": ["v"] } })).len(),
        2,
        "the voided transaction's splits"
    );
    assert_eq!(hits(&mut b, json!({ "void": true })).len(), 2);
    let big_debits = json!({ "all": [{ "value": { "op": "ge", "amount": "750", "sign": "debit" } }, { "datePosted": { "op": "onOrAfter", "date": "2024-06-02" } }] });
    assert_eq!(hits(&mut b, big_debits).len(), 2, "rent and the ACME purchase");
    let all = hits(&mut b, json!({ "all": [] }));
    let last: Vec<String> = b.call("transaction.find", json!({ "query": { "all": [] }, "limit": 2 }))["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["split"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(last, all[all.len() - 2..], "a limit keeps the last matches");
    let both = json!({ "accounts": { "accounts": [CHECKING, "Expenses:Food"], "matching": "all" } });
    assert_eq!(hits(&mut b, both).len(), 6, "groceries, mistake and rent, two splits each");
    assert_eq!(
        b.err("transaction.find", json!({ "query": { "memo": { "text": "" } } })).code,
        codes::INVALID
    );
    assert_eq!(
        b.err(
            "transaction.find",
            json!({ "query": { "memo": { "text": "(", "op": "matchesRegex" } } })
        )
        .code,
        codes::INVALID
    );
    assert_eq!(
        b.err("transaction.find", json!({ "query": { "nope": true } })).code,
        codes::INVALID_PARAMS
    );
}

#[test]
fn schema_lists_every_transaction_method() {
    let b = &mut open();
    let schema = b.call("api.schema", json!({}));
    let names: Vec<&str> = schema["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .filter(|n| n.starts_with("transaction."))
        .collect();
    assert_eq!(names.len(), 10, "{names:?}");
    let delete = schema["methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "transaction.delete")
        .unwrap();
    assert_eq!(delete["effect"], "destructive");
}
