//! Reconciliation, checked against GnuCash 5.17: GnuCash reads back the states, dates and reconcile-info slots
//! red-ale writes, and computes the same balances the reconcile window starts from.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, open, oracle, oracle_check};
use serde_json::{Value, json};

const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";
const GROCERIES: &str = "aff7bc7c47b848f6814da21a590796b7";
const EURO: &str = "c9cde06da6274e87981afb424c59f17d";
const ACME_BUY: &str = "45a2c80dfeec49758ef32c715050677c";
const ACME_SELL: &str = "fc4e6f9a523e438a9beab3d97ed51df0";
/// 2024-06-30 23:59:59 UTC.
const JUNE_30_END: i64 = 1_719_791_999;

/// GnuCash's reading of the account's reconciliation (`oracle/reconcile.py`).
fn gnucash(book: &TempBook, time: i64) -> Value {
    oracle::check_version().unwrap();
    let script = oracle::scripts_dir().join("reconcile.py");
    let out = oracle::run_script(&script, &[&book.path_str(), CHECKING, &time.to_string()]).unwrap_or_else(|e| panic!("{e}"));
    serde_json::from_str(&out).unwrap()
}

/// A wire amount as the oracle prints it: a reduced `num/denom`.
fn frac(a: &Value) -> String {
    let n: i64 = a["num"].as_str().unwrap().parse().unwrap();
    let d: i64 = a["denom"].as_str().unwrap().parse().unwrap();
    let g = gcd(n.unsigned_abs(), d.unsigned_abs()).max(1);
    let g = i64::try_from(g).unwrap();
    format!("{}/{}", n / g, d / g)
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[test]
fn start_balances_match_gnucash() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    let view = call(
        &mut d,
        "reconcile.start",
        &json!({ "account": CHECKING, "statementDate": "2024-06-30" }),
    );
    let theirs = gnucash(&book, JUNE_30_END);
    assert_eq!(frac(&view["endingBalance"]), theirs["balanceAsOf"], "default ending balance");
    assert_eq!(frac(&view["startingBalance"]), theirs["reconciledBalance"]);
    assert_eq!(frac(&view["clearedBalance"]), theirs["clearedBalance"]);
    assert_eq!(view["statementTime"], "2024-06-30T23:59:59Z");
    let defaults = call(&mut d, "reconcile.start", &json!({ "account": CHECKING }));
    let theirs = gnucash(&book, 1_709_251_199); // 2024-02-29 23:59:59, the default GnuCash derives from last-date
    assert_eq!(defaults["statementTime"], "2024-02-29T23:59:59Z");
    assert_eq!(frac(&defaults["endingBalance"]), theirs["balanceAsOf"]);
}

#[test]
fn finish_reads_back_in_gnucash() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    call(&mut d, "reconcile.setState", &json!({ "split": GROCERIES, "state": "c" }));
    let fin = json!({
        "account": CHECKING, "statementDate": "2024-06-30", "endingBalance": "4467.71",
        "ticked": [GROCERIES, EURO, ACME_BUY, ACME_SELL],
    });
    let done = call(&mut d, "reconcile.finish", &fin);
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let theirs = gnucash(&book, JUNE_30_END);
    assert_eq!(theirs["lastDate"], JUNE_30_END);
    assert_eq!(
        theirs["lastInterval"],
        json!([done["lastInterval"]["months"], done["lastInterval"]["days"]])
    );
    assert_eq!((&theirs["postponeDate"], &theirs["postponeBalance"]), (&Value::Null, &Value::Null));
    assert_eq!(theirs["includeChildren"], false);
    assert_eq!(theirs["reconciledBalance"], "446771/100");
}

#[test]
fn postpone_reads_back_in_gnucash() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    call(&mut d, "reconcile.setState", &json!({ "split": GROCERIES, "state": "c" }));
    let p = json!({ "account": CHECKING, "statementDate": "2024-06-03", "endingBalance": "-4890", "ticked": [EURO] });
    call(&mut d, "reconcile.postpone", &p);
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let theirs = gnucash(&book, JUNE_30_END);
    assert_eq!(theirs["postponeDate"], 1_717_459_199); // 2024-06-03 23:59:59
    assert_eq!(theirs["postponeBalance"], "-4890/1");
    assert_eq!(theirs["lastDate"], 1_706_659_200, "postponing leaves the last reconcile date");
}

#[test]
fn split_states_and_info_read_back_in_gnucash() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    call(
        &mut d,
        "reconcile.setState",
        &json!({ "split": GROCERIES, "state": "y", "date": "2024-06-15T23:59:59Z" }),
    );
    call(&mut d, "reconcile.setState", &json!({ "split": EURO, "state": "f" }));
    call(&mut d, "reconcile.setState", &json!({ "split": ACME_BUY, "state": "c" }));
    let info = json!({
        "account": CHECKING, "lastDate": "2024-05-31T23:59:59Z", "lastInterval": { "months": 0, "days": 14 },
        "includeSubaccounts": true,
    });
    call(&mut d, "reconcile.setInfo", &info);
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let theirs = gnucash(&book, JUNE_30_END);
    assert_eq!(theirs["lastDate"], 1_717_199_999);
    assert_eq!(theirs["lastInterval"], json!([0, 14]));
    assert_eq!(theirs["includeChildren"], true);
    assert_eq!(theirs["reconciledBalance"], "243883/50", "y and f count as reconciled");
}
