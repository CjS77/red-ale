//! Export accounts checked against GnuCash 5.17 (`design/api/export.md`, "Export accounts"): GnuCash opens the file
//! `export.accounts` wrote with nothing logged and no unknown feature, and finds the source book's accounts (GUIDs,
//! names, types, commodities, SCUs, parents and every property kept in slots) and commodities, and no transactions,
//! prices or lots.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use red_ale_testkit::{call, diff, open, oracle};
use serde_json::{Value, json};

fn gnucash(script: &str, args: &[&str]) -> Value {
    oracle::check_version().unwrap();
    let out = oracle::run_script(&oracle::scripts_dir().join(script), args).unwrap_or_else(|e| panic!("{e}"));
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"))
}

/// The commodities GnuCash lists for a book with no transactions or prices: every security, and the currencies its
/// accounts use (`oracle/dump.py` lists only the currencies something uses).
fn commodities_of_the_tree(dump: &Value) -> Vec<Value> {
    let used: BTreeSet<&str> = dump["accounts"].as_array().unwrap().iter().filter_map(|a| a["commodity"].as_str()).collect();
    let keep = |c: &&Value| c["id"].as_str().is_some_and(|id| !id.starts_with("CURRENCY::") || used.contains(id));
    dump["commodities"].as_array().unwrap().iter().filter(keep).cloned().collect()
}

fn exported_tree_matches(fixture: &str) {
    let (book, mut d) = open(fixture);
    let target = book.dir.path().join("chart-of-accounts");
    let written = call(&mut d, "export.accounts", &json!({ "path": target.display().to_string() }));
    let path = written["path"].as_str().unwrap().to_owned();
    assert!(path.ends_with("chart-of-accounts.gnucash"), "{written}");
    assert_eq!(call(&mut d, "book.info", &json!({}))["dirty"], false);
    call(&mut d, "book.close", &json!({}));

    assert_eq!(gnucash("opened.py", &[&path]), json!({ "unknownFeatures": null, "logged": [] }), "{fixture}");
    let source = oracle::dump(&book.path).unwrap();
    let exported = oracle::dump(std::path::Path::new(&path)).unwrap();
    let differences = diff(&source["accounts"], &exported["accounts"], &["*.balance"]);
    assert!(differences.is_empty(), "{fixture}: accounts differ:\n{}", differences.join("\n"));
    assert_eq!(commodities_of_the_tree(&source), exported["commodities"].as_array().unwrap().clone(), "{fixture}");
    ["transactions", "prices", "lots"].iter().for_each(|k| assert_eq!(exported[*k], json!([]), "{fixture}: {k}"));
    assert_eq!(written["accounts"].as_u64().unwrap(), 1 + source["accounts"].as_array().unwrap().len() as u64, "the root is written too");
}

#[test]
fn gnucash_reads_the_exported_tree_of_a_rich_book() {
    exported_tree_matches("synthetic/rich.gnucash");
}

/// A book with lots, business objects and a book-level feature: the lots are left out, and so is every book slot.
#[test]
fn gnucash_reads_the_exported_tree_of_a_business_book() {
    exported_tree_matches("synthetic/business.gnucash");
}

/// A book GnuCash itself wrote, with securities and a price database.
#[test]
fn gnucash_reads_the_exported_tree_of_an_investment_book() {
    exported_tree_matches("piecash/investment.gnucash");
}
