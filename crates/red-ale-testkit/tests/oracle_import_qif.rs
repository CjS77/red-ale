//! QIF imports red-ale applied, checked against GnuCash 5.17: GnuCash opens the book with no warnings and sees the
//! same data (`oracle_check`): accounts, securities, transactions with their values and amounts, and prices
//! (`design/api/import.md`, "Tests").

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{call, create, oracle, oracle_check};
use serde_json::{Value, json};

fn gnucash(script: &str, args: &[&str]) -> Value {
    oracle::check_version().unwrap();
    let out = oracle::run_script(&oracle::scripts_dir().join(script), args).unwrap_or_else(|e| panic!("{e}"));
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"))
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../../fixtures/qif/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// A checking account with splits, a void and a transfer to a brokerage account, whose file buys, splits, pays a
/// dividend on and gives away a security, with a price list.
const CHECKING: &str = "!Account\nNChecking\nTBank\n^\n!Type:Bank\nD03/14/2024\nT-500.00\nPTo broker\nL[Broker]\n^\n\
D03/15/2024\nT-100.00\nPDinner\nMwith friends\nLDining\nSDining\n$-60.00\nEmeal\nSGroceries\n$-40.00\n^\n\
D03/16/2024\nT1,000.00\nPOpening Balance\nL[Checking]\n^\nD03/17/2024\nT-1.00\nP**VOID**Cheque\nLFees\nC*\n^\n";
const BROKER: &str = "!Account\nNBroker\nTInvst\n^\n!Type:Security\nNAcme\nSACM\nTStock\n^\n!Type:Invst\nD03/14/2024\nNXIn\nT500.00\nL[Checking]\n^\n\
D03/15/2024\nNBuy\nYAcme\nI10.50\nQ40\nT429.95\nO9.95\n^\nD03/16/2024\nNStkSplit\nYAcme\nQ20\n^\nD03/17/2024\nNDivX\nYAcme\nT5.00\nL[Checking]\n^\n\
D03/18/2024\nNShrsOut\nYAcme\nQ10\n^\n!Type:Prices\n\"ACM\",11 1/2,\"03/20/2024\"\n^\n!Type:Prices\n\"ACM\",12,\"03/21/2024\"\n^\n";

fn preview(d: &mut red_ale_rpc::Dispatcher, files: &[(&str, &str)]) -> Value {
    let files: Vec<Value> = files.iter().map(|(name, text)| json!({ "name": name, "text": text })).collect();
    call(d, "import.preview", &json!({ "format": "qif", "files": files }))
}

#[test]
fn gnucash_reads_books_red_ale_imported_qif_into() {
    let (book, mut d) = create();
    let p = preview(&mut d, &[("checking.qif", CHECKING), ("broker.qif", BROKER)]);
    assert_eq!(p["errors"], json!([]));
    let applied = call(&mut d, "import.apply", &json!({ "proposal": p["id"] }));
    assert_eq!(applied["commodities"], json!(["NONCURRENCY::ACM"]));
    ["abc-all.qif", "divx.qif", "price.qif", "swipe.qif", "web.qif"].iter().for_each(|name| {
        let p = preview(&mut d, &[(name, &fixture(name))]);
        call(&mut d, "import.apply", &json!({ "proposal": p["id"], "skipErrors": true }));
    });
    // The same files again: every transaction is a duplicate, and skipping them all adds nothing.
    let again = preview(&mut d, &[("checking.qif", CHECKING), ("broker.qif", BROKER)]);
    let skip: Vec<Value> = again["transactions"].as_array().unwrap().iter().map(|t| json!({ "index": t["index"], "action": "skip" })).collect();
    let applied = call(&mut d, "import.apply", &json!({ "proposal": again["id"], "changes": skip }));
    assert_eq!(applied["added"], json!([]));

    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    assert_eq!(gnucash("opened.py", &[&book.path_str()]), json!({ "unknownFeatures": null, "logged": [] }));
}

#[test]
fn gnucash_reads_a_large_import() {
    let (book, mut d) = create();
    let p = preview(&mut d, &[("ms-money.qif", &fixture("ms-money.qif")), ("cbb-export.qif", &fixture("cbb-export.qif"))]);
    call(&mut d, "import.apply", &json!({ "proposal": p["id"] }));
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    assert_eq!(gnucash("opened.py", &[&book.path_str()]), json!({ "unknownFeatures": null, "logged": [] }));
}
