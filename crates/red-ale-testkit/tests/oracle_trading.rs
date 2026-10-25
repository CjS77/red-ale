//! Trading accounts, checked against GnuCash 5.17: GnuCash posts multi-commodity transactions in a book with trading
//! accounts on one copy (adding the trading splits itself), red-ale posts the same through its API on another, and the
//! two must match split for split; books with trading accounts edited by red-ale read the same in GnuCash, every
//! transaction balances by `xaccTransIsBalanced`, and GnuCash's own imbalance scrub changes nothing.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, create, dispatcher, oracle, oracle_check};
use serde_json::{Value, json};

fn trading(command: &str, book: &TempBook, extra: &[&str]) -> String {
    oracle::check_version().unwrap();
    let path = book.path_str();
    let args: Vec<&str> = [command, path.as_str()].into_iter().chain(extra.iter().copied()).collect();
    oracle::run_script(&oracle::scripts_dir().join("trading.py"), &args).unwrap_or_else(|e| panic!("{e}"))
}

/// GnuCash's view of the trading tree and of every transaction's splits, in stored order.
fn show(book: &TempBook) -> Value {
    serde_json::from_str(&trading("show", book, &[])).unwrap()
}

/// What GnuCash finds wrong with a book: unbalanced transactions, transactions its imbalance scrub changes, accounts
/// it adds, and anything its engine logs.
fn check(book: &TempBook) -> Value {
    serde_json::from_str(&trading("check", book, &[])).unwrap()
}

fn assert_nothing_to_fix(book: &TempBook) {
    let found = check(book);
    assert_eq!(found, json!({ "unbalanced": [], "changed": [], "accountsAdded": 0, "logged": [] }), "{found}");
}

/// The transactions both sides post, as `trading.py post` takes them.
fn transactions() -> Value {
    let s = |account: &str, value: [i64; 2], amount: [i64; 2]| json!({ "account": account, "value": value, "amount": amount });
    json!([
        { "description": "Exchange", "currency": "CURRENCY::USD", "date": "2024-03-01", "splits": [
            s("Assets:Euro", [11000, 100], [10000, 100]), s("Assets:Checking", [-11000, 100], [-11000, 100]) ] },
        { "description": "Buy", "currency": "CURRENCY::USD", "date": "2024-03-02", "splits": [
            s("Assets:ACME", [50000, 100], [10000, 1000]), s("Assets:Checking", [-50000, 100], [-50000, 100]) ] },
        { "description": "Gain", "currency": "CURRENCY::USD", "date": "2024-03-03", "splits": [
            s("Assets:ACME", [2500, 100], [0, 1000]), s("Income:Gains", [-2500, 100], [-2500, 100]) ] },
        { "description": "Euros for shares", "currency": "CURRENCY::EUR", "date": "2024-03-04", "splits": [
            s("Assets:ACME", [9500, 100], [2000, 1000]), s("Assets:Checking", [-9500, 100], [-10500, 100]) ] },
        { "description": "Thirds", "currency": "CURRENCY::USD", "date": "2024-03-05", "splits": [
            s("Assets:Euro", [3333, 100], [3000, 100]), s("Assets:Checking", [-1111, 100], [-1111, 100]),
            s("Assets:Checking", [-2222, 100], [-2222, 100]) ] },
    ])
}

/// A new red-ale book with USD, EUR and stock accounts and trading accounts on, saved and closed, with the dispatcher
/// that made it: reopening books in it keeps new GUIDs from repeating the ones it already gave.
fn trading_book() -> (TempBook, red_ale_rpc::Dispatcher) {
    let (book, mut d) = create();
    call(&mut d, "commodity.create", &json!({ "namespace": "NASDAQ", "mnemonic": "ACME", "fullname": "Acme", "fraction": 1000 }));
    [
        ("Assets", "ASSET", None, "CURRENCY::USD"),
        ("Checking", "BANK", Some("Assets"), "CURRENCY::USD"),
        ("Euro", "BANK", Some("Assets"), "CURRENCY::EUR"),
        ("ACME", "STOCK", Some("Assets"), "NASDAQ::ACME"),
        ("Income", "INCOME", None, "CURRENCY::USD"),
        ("Gains", "INCOME", Some("Income"), "CURRENCY::USD"),
    ]
    .iter()
    .for_each(|(name, ty, parent, commodity)| {
        call(&mut d, "account.create", &json!({ "name": name, "type": ty, "parent": parent, "commodity": commodity }));
    });
    call(&mut d, "book.setOptions", &json!({ "useTradingAccounts": true }));
    call(&mut d, "book.close", &json!({ "save": true }));
    (book, d)
}

fn copy_of(book: &TempBook) -> TempBook {
    let copy = TempBook::new_path();
    std::fs::copy(&book.path, &copy.path).unwrap();
    copy
}

fn post_through_api(d: &mut red_ale_rpc::Dispatcher, spec: &Value) {
    let amount = |n: &Value| json!({ "num": n[0].to_string(), "denom": n[1].to_string() });
    spec.as_array().unwrap().iter().for_each(|t| {
        let splits: Vec<Value> = t["splits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| json!({ "account": s["account"], "value": amount(&s["value"]), "amount": amount(&s["amount"]) }))
            .collect();
        let params = json!({ "description": t["description"], "currency": t["currency"], "datePosted": t["date"], "splits": splits });
        call(d, "transaction.post", &params);
    });
}

#[test]
fn red_ale_posts_the_trading_splits_gnucash_posts() {
    let (base, mut d) = trading_book();
    let theirs = copy_of(&base);
    let spec_file = theirs.dir.path().join("spec.json");
    std::fs::write(&spec_file, transactions().to_string()).unwrap();
    trading("post", &theirs, &[&spec_file.display().to_string()]);

    let ours = copy_of(&base);
    call(&mut d, "book.open", &json!({ "path": ours.path_str() }));
    post_through_api(&mut d, &transactions());
    let diffs = oracle_check(&mut d, &ours, &[]);
    assert!(diffs.is_empty(), "GnuCash reads red-ale's book differently:\n{}", diffs.join("\n"));

    let (gnucash, red_ale) = (show(&theirs), show(&ours));
    assert_eq!(gnucash, red_ale, "\nGnuCash posted:\n{gnucash:#}\nred-ale posted:\n{red_ale:#}");
    assert_eq!(gnucash["transactions"]["Exchange"]["splits"].as_array().unwrap().len(), 4, "{gnucash:#}");
    assert_nothing_to_fix(&ours);
}

#[test]
fn gnucash_rebalances_a_transaction_posted_before_the_option_was_on() {
    let (book, mut d) = trading_book();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    call(&mut d, "book.setOptions", &json!({ "useTradingAccounts": false }));
    post_through_api(&mut d, &json!([transactions()[0]]));
    call(&mut d, "book.setOptions", &json!({ "useTradingAccounts": true }));
    call(&mut d, "book.close", &json!({ "save": true }));
    let found = check(&book);
    assert_eq!((&found["unbalanced"], &found["changed"]), (&json!(["Exchange"]), &json!(["Exchange"])), "{found}");
}

#[test]
fn red_ale_scrubs_a_trading_book_as_gnucash_does() {
    let (base, mut d) = trading_book();
    call(&mut d, "book.open", &json!({ "path": base.path_str() }));
    call(&mut d, "book.setOptions", &json!({ "useTradingAccounts": false }));
    post_through_api(&mut d, &transactions());
    call(&mut d, "book.setOptions", &json!({ "useTradingAccounts": true }));
    call(&mut d, "book.close", &json!({ "save": true }));

    let theirs = copy_of(&base);
    oracle::run_script(&oracle::scripts_dir().join("scrub.py"), &[&theirs.path_str()]).unwrap_or_else(|e| panic!("{e}"));
    let ours = copy_of(&base);
    call(&mut d, "book.open", &json!({ "path": ours.path_str() }));
    let result = call(&mut d, "scrub.checkAndRepair", &json!({}));
    assert!(result["repairs"].as_array().unwrap().iter().any(|r| r["kind"] == "tradingSplitAdded"), "{result}");
    let diffs = oracle_check(&mut d, &ours, &[]);
    assert!(diffs.is_empty(), "GnuCash reads red-ale's scrubbed book differently:\n{}", diffs.join("\n"));
    let (gnucash, red_ale) = (show(&theirs), show(&ours));
    assert_eq!(gnucash, red_ale, "\nGnuCash scrubbed:\n{gnucash:#}\nred-ale scrubbed:\n{red_ale:#}");
    assert_nothing_to_fix(&ours);
}

#[test]
fn trading_fixtures_edited_by_red_ale_need_nothing_from_gnucash() {
    ["piecash/simple_sample.gnucash", "piecash/investment.gnucash"].iter().for_each(|name| {
        let book = TempBook::copy(name);
        assert_nothing_to_fix(&book);
        let mut d = dispatcher();
        let info = call(&mut d, "book.open", &json!({ "path": book.path_str() }));
        assert_eq!(info["readOnly"], false, "{name}: {info}");
        assert_eq!(call(&mut d, "book.options", &json!({}))["useTradingAccounts"], true, "{name}");
        let diffs = oracle_check(&mut d, &book, &[]);
        assert!(diffs.is_empty(), "{name} saved unedited:\n{}", diffs.join("\n"));
    });
}

#[test]
fn an_investment_book_edited_by_red_ale_reads_the_same_and_balances_in_gnucash() {
    let book = TempBook::copy("piecash/investment.gnucash");
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    let checking = "Assets:Current Assets:Checking Account";
    let veur = "Assets:Investments:Brokerage Account:Stock:VEUR";
    let buy = call(
        &mut d,
        "transaction.post",
        &json!({ "description": "More VEUR", "currency": "CURRENCY::EUR", "datePosted": "2024-05-01", "splits": [
            { "account": veur, "value": "301.50", "amount": "7" },
            { "account": checking, "value": "-301.50" },
        ]}),
    );
    assert_eq!(buy["splits"].as_array().unwrap().len(), 4, "{buy}");
    let veur_split = buy["splits"].as_array().unwrap().iter().find(|s| s["accountName"] == veur).unwrap()["guid"].clone();
    call(&mut d, "transaction.update", &json!({ "guid": buy["guid"], "changeSplits": [{ "guid": veur_split, "amount": "6" }] }));
    let reversed = call(&mut d, "transaction.reverse", &json!({ "guid": buy["guid"], "date": "2024-05-02" }));
    // `show` keys transactions by description, and a reversal copies the original's.
    call(&mut d, "transaction.update", &json!({ "guid": reversed["guid"], "description": "Reversal" }));
    call(&mut d, "transaction.void", &json!({ "guid": reversed["guid"], "reason": "test" }));
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "GnuCash reads red-ale's book differently:\n{}", diffs.join("\n"));
    let seen = show(&book);
    assert!(
        seen["transactions"]["More VEUR"]["splits"].as_array().unwrap().iter().any(|s| s[0] == "Trading:AEX:VEUR" && s[2] == "-6/1"),
        "{seen:#}"
    );
    assert_nothing_to_fix(&book);
}
