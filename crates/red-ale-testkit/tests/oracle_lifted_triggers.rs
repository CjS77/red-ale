//! The four read-only triggers v0.2 lifts, together (`design/milestone2.md`, "Compatibility in v0.2"): a book with
//! credit notes opens for editing, red-ale turns on the num field source, the auto read-only threshold and trading
//! accounts, posts a transaction and saves. GnuCash 5.17 then reads everything red-ale holds, opens the book with no
//! unknown feature and nothing logged, finds every transaction balanced with nothing for its scrub to fix, and sees
//! both book features red-ale relies on.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, oracle, oracle_check, open};
use serde_json::{Value, json};

fn gnucash(script: &str, args: &[&str]) -> Value {
    oracle::check_version().unwrap();
    let out = oracle::run_script(&oracle::scripts_dir().join(script), args).unwrap_or_else(|e| panic!("{e}"));
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"))
}

fn edited_with_every_trigger_on() -> TempBook {
    let (book, mut d) = open("synthetic/business.gnucash");
    let info = call(&mut d, "book.info", &json!({}));
    assert_eq!(info["readOnly"], false, "a credit-note book opens for editing: {info}");
    let options = json!({ "splitActionForNum": true, "autoReadOnlyDays": 30, "useTradingAccounts": true });
    call(&mut d, "book.setOptions", &options);
    let splits = json!([{ "account": "Expenses", "value": "12.34" }, { "account": "Assets:Bank", "value": "-12.34" }]);
    call(&mut d, "transaction.post", &json!({ "description": "after the triggers", "datePosted": "2099-01-01", "splits": splits }));
    let differences = oracle_check(&mut d, &book, &[]);
    assert!(differences.is_empty(), "GnuCash reads what red-ale holds:\n{}", differences.join("\n"));
    book
}

#[test]
fn gnucash_opens_a_book_with_every_lifted_trigger_without_complaint() {
    let book = edited_with_every_trigger_on();
    let path = book.path_str();
    assert_eq!(gnucash("opened.py", &[&path]), json!({ "unknownFeatures": null, "logged": [] }));
    let found = gnucash("trading.py", &["check", &path]);
    assert_eq!(found, json!({ "unbalanced": [], "changed": [], "accountsAdded": 0, "logged": [] }), "{found}");
    let features = gnucash("book_options.py", &["slots", &path])["features"].clone();
    ["Credit Notes", "Number Field Source"].iter().for_each(|f| assert!(features.get(f).is_some(), "{f} in {features}"));
}
