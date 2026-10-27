//! Auto-clear, checked against GnuCash 5.17: GnuCash's own `gnc_account_get_autoclear_splits`, run on the book
//! red-ale saved (`oracle/autoclear.py`), picks the same splits or fails the same way, and GnuCash reads back the
//! cleared states `reconcile.autoClear` wrote.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, create, open, oracle, oracle_check};
use serde_json::{Value, json};

const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";

/// GnuCash's result for each target, given as `num/denom`.
fn gnucash(book: &TempBook, account: &str, targets: &[&str]) -> Vec<Value> {
    oracle::check_version().unwrap();
    let script = oracle::scripts_dir().join("autoclear.py");
    let path = book.path_str();
    let args: Vec<&str> = [path.as_str(), account].into_iter().chain(targets.iter().copied()).collect();
    let out = oracle::run_script(&script, &args).unwrap_or_else(|e| panic!("{e}"));
    serde_json::from_str::<Vec<Value>>(&out).unwrap()
}

/// red-ale's result in the oracle's shape: GnuCash's `Autoclear` code, and the split GUIDs sorted.
fn ours(d: &mut red_ale_rpc::Dispatcher, account: &str, target: &str) -> Value {
    let r = call(
        d,
        "reconcile.findAutoClear",
        &json!({ "account": account, "clearedBalance": target }),
    );
    let code = match r["outcome"].as_str().unwrap() {
        "found" => 0,
        "alreadyCleared" => 1,
        "ambiguous" => 2,
        "limit" => 3,
        _ => 4,
    };
    let mut splits: Vec<&str> = r["splits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["split"].as_str().unwrap())
        .collect();
    splits.sort_unstable();
    json!({ "code": code, "message": r["message"], "splits": splits })
}

/// Save the book, then compare red-ale's and GnuCash's answers for every target.
fn compare(d: &mut red_ale_rpc::Dispatcher, book: &TempBook, account: &str, targets: &[&str]) {
    call(d, "book.save", &json!({}));
    let theirs = gnucash(book, account, targets);
    targets
        .iter()
        .zip(theirs)
        .for_each(|(t, g)| assert_eq!(ours(d, account, t), g, "target {t}"));
}

#[test]
fn rich_book_clears_as_gnucash_does() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    compare(
        &mut d,
        &book,
        CHECKING,
        &["487766/100", "500000/100", "450000/100", "548766/100", "-500/100"],
    );
    let pay = json!({ "datePosted": "2024-06-10", "splits": [
        { "account": CHECKING, "value": "-50" }, { "account": "Expenses:Food", "value": "50" }] });
    call(&mut d, "transaction.post", &pay);
    call(&mut d, "transaction.post", &pay);
    compare(&mut d, &book, CHECKING, &["495000/100", "490000/100", "477766/100"]);

    call(
        &mut d,
        "reconcile.autoClear",
        &json!({ "account": CHECKING, "clearedBalance": "4777.66" }),
    );
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let after = gnucash(&book, CHECKING, &["477766/100"]);
    assert_eq!(after[0]["code"], 1, "GnuCash sees the account at the cleared balance");
}

/// One split per entry: its amount in cents and whether it starts cleared.
type Splits = [(i64, bool)];

/// A fresh book with one bank account holding `cents` as splits, cleared where flagged; returns the account GUID.
fn bank_with(d: &mut red_ale_rpc::Dispatcher, data: &[(i64, bool)]) -> String {
    let bank = call(d, "account.create", &json!({ "name": "Bank", "type": "BANK", "commodity": "USD" }))["guid"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        d,
        "account.create",
        &json!({ "name": "Other", "type": "EXPENSE", "commodity": "USD" }),
    );
    data.iter().zip(1..).for_each(|(&(cents, cleared), n)| {
        let posted = format!("2024-{:02}-{:02}", 1 + (n - 1) / 28, 1 + (n - 1) % 28);
        let state = if cleared { "c" } else { "n" };
        let splits = json!([
            { "account": bank, "value": format!("{cents}/100"), "reconcileState": state },
            { "account": "Other", "value": format!("{}/100", -cents) },
        ]);
        call(d, "transaction.post", &json!({ "datePosted": posted, "splits": splits }));
    });
    bank
}

fn targets(cents: &[i64]) -> Vec<String> {
    cents.iter().map(|c| format!("{c}/100")).collect()
}

/// GnuCash's own test cases (`5.17:libgnucash/app-utils/test/test-autoclear.cpp:44-219`), written by red-ale and
/// searched by both.
#[test]
fn gnucash_test_cases_clear_alike() {
    let easy: Vec<(i64, bool)> = [
        -8234, -156_326, -4500, -694_056, -7358, -11700, -20497, -11900, -8275, -58700, 100_000, -13881, -5000, 200_000, -16800, -152_000,
    ]
    .iter()
    .map(|&a| (a, true))
    .chain(
        [
            160_000, -63610, -2702, -15400, -3900, -22042, -2900, -10900, -44400, -9200, -7900, -1990, -7901, -61200,
        ]
        .iter()
        .map(|&a| (a, false)),
    )
    .collect();
    let cases: [(&Splits, &[i64]); 4] = [
        (&easy, &[0, -869_227, -869_300, -869_230, -963_272]),
        (&[(-10, false), (-10, false), (-10, false)], &[-10, -20, -30]),
        (&[(2, false), (3, false), (5, false), (5, false)], &[2, 3, 5, 7, 10, 12, 13, 15]),
        (
            &[
                (30000, false),
                (-20000, false),
                (10000, false),
                (500, false),
                (-400, false),
                (300, false),
                (0, false),
            ],
            &[30300, 10000, 0],
        ),
    ];
    cases.iter().for_each(|(data, cents)| {
        let (book, mut d) = create();
        let bank = bank_with(&mut d, data);
        let t = targets(cents);
        compare(&mut d, &book, &bank, &t.iter().map(String::as_str).collect::<Vec<_>>());
    });
}
