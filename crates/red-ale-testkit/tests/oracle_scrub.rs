//! Scrubs, checked against GnuCash 5.17: GnuCash scrubs one copy of a broken book itself (its load-time scrubs, then
//! the imbalance step of Check & Repair), red-ale scrubs another copy through its API, and GnuCash must see the same
//! book in both.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use red_ale_testkit::{TempBook, call, dispatcher, oracle, oracle_check};
use serde_json::{Value, json};

const ROOT: &str = "be129ab681d8447ab493a70949a6d837";
const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";
const FOOD: &str = "1feca2abf92347288c268ebf93753ec4";
const EQUITY: &str = "8839ad3eae7b4ca596db5bde593d8803";
const IMBALANCE: &str = "1111111111111111111111111111aaaa";

fn replace_once(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "{from:?} must occur once");
    text.replacen(from, to, 1)
}

/// `synthetic/rich.gnucash` broken four ways: an unbalanced transaction (Groceries), a split whose account does not
/// exist (the Opening equity split), a split whose amount differs from its value in a currency account (Sell ACME), and a
/// transaction whose existing Imbalance-USD split exactly absorbs its imbalance.
fn broken_book() -> TempBook {
    let book = TempBook::copy("synthetic/rich.gnucash");
    let mut text = std::fs::read_to_string(&book.path).unwrap();
    text = replace_once(
        &text,
        "<split:value>1234/100</split:value>\n      <split:quantity>1234/100</split:quantity>",
        "<split:value>1000/100</split:value>\n      <split:quantity>1000/100</split:quantity>",
    );
    // GnuCash drops a split with no account element, so an orphan is a split whose account does not exist.
    text = replace_once(&text, &format!("{EQUITY}</split:account>"), "2222222222222222222222222222cccc</split:account>");
    text = replace_once(
        &text,
        "<split:value>60000/100</split:value>\n      <split:quantity>60000/100</split:quantity>",
        "<split:value>60000/100</split:value>\n      <split:quantity>59000/100</split:quantity>",
    );
    let first_txn = text.find("<gnc:transaction").unwrap();
    text.insert_str(first_txn, &leftover_imbalance());
    std::fs::write(&book.path, text).unwrap();
    book
}

/// `synthetic/rich.gnucash` with two transactions whose currency is a security: Buy ACME, whose accounts all lead to
/// USD, and To euro account, whose EUR and USD splits tie.
fn currency_broken_book() -> TempBook {
    let book = TempBook::copy("synthetic/rich.gnucash");
    let mut text = std::fs::read_to_string(&book.path).unwrap();
    ["<trn:description>Buy ACME", "<trn:description>To euro account"].iter().for_each(|description| {
        let at = text.find(description).unwrap();
        let start = text[..at].rfind("<trn:currency>").unwrap();
        let usd = "<cmdty:space>CURRENCY</cmdty:space>\n    <cmdty:id>USD</cmdty:id>";
        let offset = start + text[start..].find(usd).unwrap();
        text.replace_range(offset..offset + usd.len(), "<cmdty:space>NASDAQ</cmdty:space>\n    <cmdty:id>ACME</cmdty:id>");
    });
    std::fs::write(&book.path, text).unwrap();
    book
}

fn split_xml(id: &str, cents: i64, account: &str) -> String {
    format!(
        "    <trn:split>\n      <split:id type=\"guid\">{id}</split:id>\n      <split:reconciled-state>n</split:reconciled-state>\n      \
         <split:value>{cents}/100</split:value>\n      <split:quantity>{cents}/100</split:quantity>\n      \
         <split:account type=\"guid\">{account}</split:account>\n    </trn:split>\n"
    )
}

fn leftover_imbalance() -> String {
    let usd = "    <cmdty:space>CURRENCY</cmdty:space>\n    <cmdty:id>USD</cmdty:id>\n";
    let account = format!(
        "<gnc:account version=\"2.0.0\">\n  <act:name>Imbalance-USD</act:name>\n  <act:id type=\"guid\">{IMBALANCE}</act:id>\n  \
         <act:type>BANK</act:type>\n  <act:commodity>\n{usd}  </act:commodity>\n  <act:commodity-scu>100</act:commodity-scu>\n  \
         <act:parent type=\"guid\">{ROOT}</act:parent>\n</gnc:account>\n"
    );
    let splits = [
        split_xml("1111111111111111111111111111bbb1", 700, FOOD),
        split_xml("1111111111111111111111111111bbb2", -700, CHECKING),
        split_xml("1111111111111111111111111111bbb3", 300, IMBALANCE),
    ]
    .concat();
    let transaction = format!(
        "<gnc:transaction version=\"2.0.0\">\n  <trn:id type=\"guid\">1111111111111111111111111111bbbb</trn:id>\n  \
         <trn:currency>\n{usd}  </trn:currency>\n  <trn:date-posted>\n    <ts:date>2024-06-08 10:59:00 +0000</ts:date>\n  \
         </trn:date-posted>\n  <trn:date-entered>\n    <ts:date>2024-06-30 12:00:00 +0000</ts:date>\n  </trn:date-entered>\n  \
         <trn:description>Leftover imbalance</trn:description>\n  <trn:splits>\n{splits}  </trn:splits>\n</gnc:transaction>\n"
    );
    account + transaction.as_str()
}

/// A dump with GUIDs of accounts replaced by full names and splits identified by content, so books scrubbed separately
/// (whose new accounts and splits have different GUIDs) can be compared.
fn normalised(dump: &Value) -> Value {
    let accounts: BTreeMap<String, &Value> =
        dump["accounts"].as_array().unwrap().iter().map(|a| (a["guid"].as_str().unwrap().to_owned(), a)).collect();
    let full_name = |guid: &Value| -> Value {
        let mut parts = Vec::new();
        let mut current = guid.as_str().map(str::to_owned);
        while let Some(g) = current {
            let a = accounts[&g];
            parts.push(a["name"].as_str().unwrap().to_owned());
            current = a["parent"].as_str().map(str::to_owned);
        }
        parts.reverse();
        if parts.is_empty() { Value::Null } else { json!(parts.join(":")) }
    };
    let mut account_list: Vec<Value> = accounts
        .values()
        .map(|a| {
            let mut a = (*a).clone();
            a["parent"] = full_name(&a["parent"]);
            a["guid"] = full_name(&a["guid"]);
            a
        })
        .collect();
    account_list.sort_by_key(|a| a["guid"].to_string());
    let transactions: Vec<Value> = dump["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            let mut t = t.clone();
            let mut splits: Vec<Value> = t["splits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| {
                    let mut s = s.clone();
                    s["account"] = full_name(&s["account"]);
                    s.as_object_mut().unwrap().remove("guid");
                    s
                })
                .collect();
            splits.sort_by_key(ToString::to_string);
            t["splits"] = json!(splits);
            t
        })
        .collect();
    json!({ "accounts": account_list, "transactions": transactions, "commodities": dump["commodities"] })
}

fn gnucash_scrubbed() -> Value {
    gnucash_scrub(&broken_book())
}

fn gnucash_scrub(book: &TempBook) -> Value {
    oracle::check_version().unwrap();
    oracle::run_script(&oracle::scripts_dir().join("scrub.py"), &[&book.path_str()]).unwrap_or_else(|e| panic!("{e}"));
    normalised(&oracle::dump(&book.path).unwrap())
}

/// Scrub a broken book through red-ale's API and save it; GnuCash's view of the result, and the method's result.
/// With `whole`, the book must come out with nothing left for GnuCash's load-time scrubs to change, so GnuCash must
/// read it exactly as red-ale does.
fn red_ale_scrubbed(method: &str, params: &Value, whole: bool) -> (Value, Value) {
    red_ale_scrub(&broken_book(), method, params, whole)
}

fn red_ale_scrub(book: &TempBook, method: &str, params: &Value, whole: bool) -> (Value, Value) {
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    let result = call(&mut d, method, params);
    if whole {
        let diffs = oracle_check(&mut d, book, &[]);
        assert!(diffs.is_empty(), "GnuCash reads red-ale's scrubbed book differently:\n{}", diffs.join("\n"));
    } else {
        call(&mut d, "book.close", &json!({ "save": true }));
    }
    (normalised(&oracle::dump(&book.path).unwrap()), result)
}

fn kinds(result: &Value) -> Vec<&str> {
    result["repairs"].as_array().unwrap().iter().map(|r| r["kind"].as_str().unwrap()).collect()
}

#[test]
fn check_and_repair_matches_gnucash() {
    let theirs = gnucash_scrubbed();
    let (ours, result) = red_ale_scrubbed("scrub.checkAndRepair", &json!({}), true);
    let diffs = red_ale_testkit::diff(&theirs, &ours, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    assert_eq!(
        kinds(&result),
        ["accountCreated", "orphanAssigned", "balanceSplitRemoved", "balanceSplitAdded", "amountSetToValue"],
        "{result}"
    );
}

#[test]
fn imbalance_scrub_of_the_tree_matches_gnucash() {
    let theirs = gnucash_scrubbed();
    let (ours, _) = red_ale_scrubbed("scrub.imbalance", &json!({ "account": "Assets", "subaccounts": true }), true);
    let diffs = red_ale_testkit::diff(&theirs, &ours, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}

#[test]
fn orphan_scrub_matches_gnucash_load_time_view() {
    // GnuCash fixes orphans (and split amounts) when it opens a book; red-ale's orphan scrub must give the same
    // accounts and orphan splits.
    let book = broken_book();
    let theirs = normalised(&oracle::dump(&book.path).unwrap());
    let (ours, result) = red_ale_scrubbed("scrub.orphans", &json!({}), false);
    assert_eq!(kinds(&result), ["accountCreated", "orphanAssigned"]);
    let orphan = |v: &Value| v["accounts"].as_array().unwrap().iter().find(|a| a["name"] == "Orphan-USD").cloned();
    assert_eq!(orphan(&theirs), orphan(&ours));
    let opening = |v: &Value| v["transactions"].as_array().unwrap().iter().find(|t| t["description"] == "Opening").cloned();
    assert_eq!(opening(&theirs), opening(&ours));
}

#[test]
fn currency_scrub_matches_gnucash() {
    let theirs = gnucash_scrub(&currency_broken_book());
    let (ours, result) = red_ale_scrub(&currency_broken_book(), "scrub.checkAndRepair", &json!({}), true);
    let diffs = red_ale_testkit::diff(&theirs, &ours, &[]);
    assert!(diffs.is_empty(), "{}\n{result}", diffs.join("\n"));
    assert!(kinds(&result).contains(&"currencySet"), "{result}");
}
