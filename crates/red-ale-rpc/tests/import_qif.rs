//! QIF import through `import.preview` and `import.apply` (`design/api/import.md`, "QIF", "Tests"): hand-worked
//! files, each case working out what GnuCash's `qif-to-gnc.scm` makes of it, and GnuCash's own example files.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

/// 2024-07-01 12:00 UTC.
const NOW: i64 = 1_719_835_200;

struct Book {
    _dir: tempfile::TempDir,
    d: Dispatcher,
}

impl Book {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut d = Dispatcher::new(Settings::default());
        d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(NOW), 1 << 100))));
        d.call("book.create", json!({ "path": dir.path().join("book.gnucash").display().to_string() })).unwrap();
        Self { _dir: dir, d }
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        self.d.call(method, params).unwrap_err()
    }

    /// Preview the files, `(name, text)`, with more params.
    fn preview(&mut self, files: &[(&str, &str)], more: Value) -> Value {
        let mut params = json!({ "format": "qif", "files": files.iter().map(|(n, t)| json!({ "name": n, "text": t })).collect::<Vec<_>>() });
        more.as_object().into_iter().flatten().for_each(|(k, v)| params[k] = v.clone());
        self.ok("import.preview", params)
    }

    fn apply(&mut self, p: &Value, more: Value) -> Value {
        let mut params = json!({ "proposal": p["id"] });
        more.as_object().into_iter().flatten().for_each(|(k, v)| params[k] = v.clone());
        self.ok("import.apply", params)
    }

    fn transactions(&mut self) -> Vec<Value> {
        self.ok("transaction.list", json!({}))["transactions"].as_array().unwrap().clone()
    }

    fn balance(&mut self, account: &str) -> Value {
        self.ok("account.balance", json!({ "account": account }))["balances"]["balance"].clone()
    }

    fn account_type(&mut self, account: &str) -> String {
        self.ok("account.get", json!({ "account": account }))["type"].as_str().unwrap().to_owned()
    }
}

/// True when the exact amount `v` equals the decimal `d`.
fn is(v: &Value, d: &str) -> bool {
    let num: i128 = v["num"].as_str().unwrap().parse().unwrap();
    let denom: i128 = v["denom"].as_str().unwrap().parse().unwrap();
    let (int, frac) = d.split_once('.').unwrap_or((d, ""));
    let scale = 10_i128.pow(u32::try_from(frac.len()).unwrap());
    let magnitude = int.trim_start_matches('-').parse::<i128>().unwrap() * scale + frac.parse::<i128>().unwrap_or(0);
    num * scale == (if int.starts_with('-') { -magnitude } else { magnitude }) * denom
}

#[track_caller]
fn assert_amount(v: &Value, d: &str) {
    assert!(is(v, d), "{v} is not {d}");
}

fn names(list: &Value) -> Vec<String> {
    list.as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap().to_owned()).collect()
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../../fixtures/qif/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// A checking account: a purchase, a cleared salary, and a split dinner. Every day is past the 12th, so the dates
/// are `m-d-y` only.
const CHECKING: &str = "!Account\nNChecking\nTBank\n^\n!Type:Bank\nD03/14/2024\nT-45.10\nPCorner Grocer\nLGroceries\n^\n\
D03/15/2024\nT2,000.00\nPACME\nLSalary\nC*\n^\nD03/20/2024\nT-100.00\nPSplit dinner\nMwith friends\nLDining\nSDining\n$-60.00\nEmeal\n\
SGroceries\n$-40.00\nEwine\n^\n";

#[test]
fn a_bank_file_makes_its_accounts_and_transactions() {
    let mut b = Book::new();
    let p = b.preview(&[("checking.qif", CHECKING)], json!({}));
    assert_eq!(p["format"], "qif");
    assert_eq!(p["summary"]["add"], 3);
    assert_eq!(p["errors"], json!([]));
    assert_eq!(names(&p["newAccounts"]), ["Dining", "Groceries", "Salary", "Checking"], "payees, then categories, then accounts");
    let types: Vec<&str> = p["newAccounts"].as_array().unwrap().iter().map(|a| a["type"].as_str().unwrap()).collect();
    assert_eq!(types, ["EXPENSE", "EXPENSE", "INCOME", "BANK"]);
    let rows = &p["qif"]["accounts"];
    assert_eq!(rows[0]["qifName"], "Checking");
    assert_eq!(rows[0]["new"], true);
    assert_eq!(rows[0]["guessed"], true);
    assert_eq!(p["qif"]["currency"], "CURRENCY::USD");

    let salary = &p["transactions"][1];
    assert_eq!(salary["datePosted"], "2024-03-15");
    assert_eq!(salary["description"], "ACME");
    assert_eq!(salary["source"], json!({ "file": 0, "lines": [11, 16] }));
    let near = salary["splits"].as_array().unwrap().iter().find(|s| s["accountName"] == "Checking").unwrap();
    assert_eq!(near["reconcileState"], "c", "C* is cleared");
    assert_eq!(near["account"], Value::Null, "Checking is still to be created");

    let dinner = &p["transactions"][2];
    assert_eq!(dinner["notes"], "with friends");
    let mut memos: Vec<&str> = dinner["splits"].as_array().unwrap().iter().filter_map(|s| s["memo"].as_str()).collect();
    memos.sort_unstable();
    assert_eq!(memos, ["meal", "wine"], "each S line's E memo is its split's");

    let applied = b.apply(&p, json!({}));
    assert_eq!(applied["added"].as_array().unwrap().len(), 3);
    assert_eq!(applied["accounts"]["created"].as_array().unwrap().len(), 4);
    assert_eq!(b.transactions().len(), 3);
    assert_amount(&b.balance("Checking"), "1854.90");
    assert_amount(&b.balance("Groceries"), "85.10");
    assert_amount(&b.balance("Dining"), "60");
    assert_amount(&b.balance("Salary"), "-2000");
}

#[test]
fn importing_the_same_file_again_finds_every_transaction_and_adds_nothing() {
    let mut b = Book::new();
    let p = b.preview(&[("checking.qif", CHECKING)], json!({}));
    b.apply(&p, json!({}));
    let again = b.preview(&[("checking.qif", CHECKING)], json!({}));
    assert_eq!(again["newAccounts"], json!([]), "the saved map sends every name to the accounts made the first time");
    assert!(again["qif"]["accounts"].as_array().unwrap().iter().all(|r| r["guessed"] == false && r["new"] == false));
    assert_eq!(again["summary"]["skip"], 3);
    assert_eq!(again["summary"]["add"], 0);
    let transactions = again["transactions"].as_array().unwrap();
    assert!(transactions.iter().all(|t| !t["matches"].as_array().unwrap().is_empty()), "QIF's finder finds every transaction");
    assert!(transactions.iter().all(|t| t["matches"][0]["score"].is_null()), "unscored");
    assert!(transactions.iter().all(|t| t["action"] == "skip" && t["selectedMatch"] == t["matches"][0]["split"]), "skipped, first match selected");
    assert!(transactions.iter().all(|t| t["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "possible-duplicate")));
    let applied = b.apply(&again, json!({}));
    assert_eq!(applied["added"], json!([]));
    assert_eq!(applied["skipped"], json!([0, 1, 2]));
    assert_eq!(applied["accounts"]["created"], json!([]));
    assert_eq!(b.transactions().len(), 3, "nothing added the second time");
    assert_amount(&b.balance("Checking"), "1854.90");

    // The caller may still add one.
    let third = b.preview(&[("checking.qif", CHECKING)], json!({}));
    let applied = b.apply(&third, json!({ "changes": [{ "index": 0, "action": "add" }] }));
    assert_eq!(applied["added"].as_array().unwrap().len(), 1);
    assert_eq!(b.transactions().len(), 4);
}

#[test]
fn the_account_map_is_learned_and_reused() {
    let mut b = Book::new();
    b.ok("account.create", json!({ "name": "Assets", "type": "ASSET", "commodity": "USD" }));
    b.ok("account.create", json!({ "name": "My Bank", "type": "BANK", "commodity": "USD", "parent": "Assets" }));
    b.ok("account.create", json!({ "name": "Expenses", "type": "EXPENSE", "commodity": "USD" }));
    b.ok("account.create", json!({ "name": "Food", "type": "EXPENSE", "commodity": "USD", "parent": "Expenses" }));
    let p = b.preview(
        &[("checking.qif", CHECKING)],
        json!({ "accounts": { "Checking": "Assets:My Bank" }, "categories": { "Groceries": "Expenses:Food" } }),
    );
    let checking = &p["qif"]["accounts"][0];
    assert_eq!(checking["account"], "Assets:My Bank");
    assert_eq!(checking["new"], false);
    assert_eq!(checking["guessed"], false);
    assert!(checking["id"].is_string());
    let applied = b.apply(&p, json!({}));
    assert_eq!(applied["warnings"].as_array().unwrap().iter().filter(|w| w["code"] == "qif-account-map").count(), 0);
    let got = b.ok("config.get", json!({ "path": "import.qifAccountMap" }));
    assert_eq!(got["problems"], json!([]));
    let map = &got["value"];
    assert_eq!(map["accounts"]["Checking"]["account"], "Assets:My Bank");
    assert_eq!(map["accounts"]["Checking"]["types"], json!(["BANK"]));
    assert_eq!(map["categories"]["Groceries"]["account"], "Expenses:Food");
    assert_eq!(map["categories"]["Salary"]["account"], "Salary", "guesses are saved too, as GnuCash saves its whole map");

    let april = "!Account\nNChecking\nTBank\n^\n!Type:Bank\nD04/14/2024\nT-12.00\nPMarket\nLGroceries\n^\n";
    let again = b.preview(&[("april.qif", april)], json!({}));
    assert_eq!(again["qif"]["accounts"][0]["account"], "Assets:My Bank");
    assert_eq!(again["qif"]["accounts"][0]["guessed"], false, "from the saved map");
    assert_eq!(again["qif"]["categories"][0]["account"], "Expenses:Food");
    assert_eq!(again["newAccounts"], json!([]));
    b.apply(&again, json!({}));
    assert_amount(&b.balance("Expenses:Food"), "97.10");
}

#[test]
fn a_transfer_between_two_qif_files_is_imported_once() {
    let mut b = Book::new();
    let checking = "!Account\nNChecking\nTBank\n^\n!Type:Bank\nD03/14/2024\nT-500.00\nPTo savings\nL[Savings]\n^\n\
                    D03/15/2024\nT-45.10\nPCorner Grocer\nLGroceries\n^\n";
    let savings = "!Account\nNSavings\nTBank\n^\n!Type:Bank\nD03/14/2024\nT500.00\nPFrom checking\nL[Checking]\nCX\n^\n";
    let p = b.preview(&[("checking.qif", checking), ("savings.qif", savings)], json!({}));
    assert_eq!(p["summary"]["add"], 2, "the transfer's two halves make one transaction");
    b.apply(&p, json!({}));
    assert_eq!(b.transactions().len(), 2);
    assert_amount(&b.balance("Savings"), "500");
    assert_amount(&b.balance("Checking"), "-545.10");
}

#[test]
fn a_credit_card_account_is_a_credit_account() {
    let mut b = Book::new();
    let card = "!Account\nNVisa\nTCCard\n^\n!Type:CCard\nD03/14/2024\nT-30.00\nPBook shop\nLBooks\n^\n";
    let p = b.preview(&[("visa.qif", card)], json!({}));
    assert_eq!(p["qif"]["accounts"][0]["type"], json!(["CREDIT"]), "qif-parse:parse-acct-type: CCard is CREDIT");
    b.apply(&p, json!({}));
    assert_eq!(b.account_type("Visa"), "CREDIT");
    assert_amount(&b.balance("Visa"), "-30");
    assert_amount(&b.balance("Books"), "30");
}

#[test]
fn a_file_without_an_account_takes_its_name_or_the_caller_s() {
    let mut b = Book::new();
    // Read under no `!Account`, the transactions keep no account type, so GnuCash offers the cash types, BANK first
    // (`qif-dialog-utils.scm:395-405`).
    let card = "!Type:CCard\nD03/14/2024\nT-30.00\nPBook shop\nLBooks\n^\n";
    let p = b.preview(&[("visa-card.qif", card)], json!({}));
    let account = &p["qif"]["accounts"][0];
    assert_eq!(account["qifName"], "visa card", "the file name, with - and _ as spaces (qif-file:path-to-accountname)");
    assert_eq!(account["type"][0], "BANK");
    let p = b.ok("import.preview", json!({ "format": "qif", "files": [{ "name": "visa-card.qif", "text": card, "defaultAccount": "Liabilities:Visa" }] }));
    assert_eq!(p["qif"]["accounts"][0]["qifName"], "Liabilities:Visa");
    b.apply(&p, json!({}));
    assert_amount(&b.balance("Liabilities:Visa"), "-30");
}

#[test]
fn an_opening_balance_comes_from_retained_earnings() {
    let mut b = Book::new();
    let qif = "!Type:Bank\nD01/14/2024\nT1,000.00\nPOpening Balance\nL[Checking]\n^\nD01/15/2024\nT-10.00\nPFee\nLBank Charges\n^\n";
    let p = b.preview(&[("statement.qif", qif)], json!({}));
    let rows: Vec<&str> = p["qif"]["accounts"].as_array().unwrap().iter().map(|r| r["qifName"].as_str().unwrap()).collect();
    assert_eq!(rows, ["Checking", "Equity:Retained Earnings"]);
    b.apply(&p, json!({}));
    assert_amount(&b.balance("Checking"), "990");
    assert_amount(&b.balance("Equity:Retained Earnings"), "-1000");
    assert_eq!(b.account_type("Equity:Retained Earnings"), "EQUITY");
}

/// A brokerage account buying, earning a dividend on and selling a security, with a price list.
const BROKER: &str = "!Account\nNBroker\nTInvst\n^\n!Type:Security\nNAcme Corp\nSACME\nTStock\n^\n!Type:Invst\n\
D01/15/2024\nNBuy\nYAcme Corp\nI10.50\nQ100\nT1,059.95\nO9.95\n^\n\
D02/15/2024\nNDiv\nYAcme Corp\nT25.00\n^\n\
D03/15/2024\nNSell\nYAcme Corp\nI12.00\nQ40\nT470.05\nO9.95\n^\n\
!Type:Prices\n\"ACME\",11.25,\"01/31/2024\"\n^\n!Type:Prices\n\"ACME\",11 3/4,\"02/29/2024\"\n^\n";

#[test]
fn investments_make_their_security_splits_and_prices() {
    let mut b = Book::new();
    let p = b.preview(&[("broker.qif", BROKER)], json!({}));
    assert_eq!(p["errors"], json!([]));
    assert_eq!(p["newCommodities"], json!([{ "id": "NONCURRENCY::ACME", "fullname": "Acme Corp", "fraction": 100_000 }]));
    assert_eq!(p["qif"]["securities"], json!([{ "name": "Acme Corp", "id": "NONCURRENCY::ACME", "fullname": "Acme Corp", "new": true }]));
    let securities = json!({ "Acme Corp": { "namespace": "NASDAQ", "mnemonic": "ACME", "fullname": "Acme Corporation" } });
    let p = b.preview(&[("broker.qif", BROKER)], json!({ "securities": securities }));
    assert_eq!(p["newCommodities"][0]["id"], "NASDAQ::ACME");
    let applied = b.apply(&p, json!({}));
    assert_eq!(applied["commodities"], json!(["NASDAQ::ACME"]));
    assert_eq!(b.ok("commodity.get", json!({ "commodity": "NASDAQ::ACME" }))["fullname"], "Acme Corporation");

    assert_eq!(b.account_type("Broker:Acme Corp"), "STOCK");
    assert_amount(&b.balance("Broker:Acme Corp"), "60");
    // Buy: 1059.95 out, of which 9.95 commission; sell: 470.05 in after 9.95 commission; a 25.00 dividend.
    assert_amount(&b.balance("Broker"), "-564.90");
    assert_amount(&b.balance("Expenses:Commissions:Broker"), "19.90");
    assert_amount(&b.balance("Income:Dividends:Broker:Acme Corp"), "-25");

    let prices = b.ok("price.list", json!({ "commodity": "NASDAQ::ACME" }));
    let mut found: Vec<(String, String)> =
        prices.as_array().unwrap().iter().map(|p| (p["date"].as_str().unwrap().to_owned(), p["source"].as_str().unwrap().to_owned())).collect();
    found.sort();
    let expected: Vec<(String, String)> =
        [("2024-01-15", "user:split-import"), ("2024-01-31", "user:price"), ("2024-02-29", "user:price"), ("2024-03-15", "user:split-import")]
            .iter()
            .map(|(d, s)| ((*d).to_owned(), (*s).to_owned()))
            .collect();
    assert_eq!(found, expected);
    let feb = prices.as_array().unwrap().iter().find(|p| p["date"] == "2024-02-29").unwrap();
    assert_amount(&feb["value"], "11.75");
    assert_eq!(feb["type"], "unknown");
}

#[test]
fn a_stock_split_and_shares_given_away_and_a_transfer_from_the_bank() {
    let mut b = Book::new();
    // Checking pays 500 into the brokerage (XIn there, a transfer in both files); the brokerage buys 40 shares,
    // splits them 2 for 1 (Q is new shares per 10 old), gives 10 away, and sends a 5.00 dividend to checking (DivX).
    let checking = "!Account\nNChecking\nTBank\n^\n!Type:Bank\nD03/14/2024\nT-500.00\nPTo broker\nL[Broker]\n^\n";
    let broker = "!Account\nNBroker\nTInvst\n^\n!Type:Invst\nD03/14/2024\nNXIn\nT500.00\nL[Checking]\n^\n\
                  D03/15/2024\nNBuy\nYAcme\nI10.00\nQ40\nT400.00\n^\nD03/16/2024\nNStkSplit\nYAcme\nQ20\n^\n\
                  D03/17/2024\nNShrsOut\nYAcme\nQ10\n^\nD03/18/2024\nNDivX\nYAcme\nT5.00\nL[Checking]\n^\n";
    let p = b.preview(&[("checking.qif", checking), ("broker.qif", broker)], json!({}));
    assert_eq!(p["errors"], json!([]));
    assert_eq!(p["summary"]["add"], 5, "the transfer once, then buy, split, shares out and the dividend");
    b.apply(&p, json!({}));
    // `qif-to-gnc.scm:868-876`: the split adds balance × Q/10 and takes the balance away, at no value.
    assert_amount(&b.balance("Broker:Acme"), "70");
    assert_amount(&b.balance("Broker"), "100");
    assert_amount(&b.balance("Checking"), "-495");
    assert_amount(&b.balance("Income:Dividends:Broker:Acme"), "-5");
    assert_eq!(b.account_type("Equity:Retained Earnings"), "EQUITY", "shares given away go to equity");
}

#[test]
fn ambiguous_dates_wait_for_a_date_format() {
    let mut b = Book::new();
    let qif = "!Type:Bank\nD03/01/2024\nT-1.00\nPOne\n^\nD03/02/2024\nT-2.00\nPTwo\n^\n";
    let p = b.preview(&[("dates.qif", qif)], json!({}));
    assert_eq!(p["qif"]["dateFormats"], json!(["m-d-y", "d-m-y"]));
    assert_eq!(p["transactions"], json!([]));
    assert_eq!(p["warnings"][0]["code"], "date-format");
    let e = b.err("import.apply", json!({ "proposal": p["id"] }));
    assert_eq!(e.code, codes::INVALID, "{e}");
    let e = b.err("import.preview", json!({ "format": "qif", "files": [{ "text": qif }], "dateFormat": "y-m-d" }));
    assert_eq!(e.data.as_ref().unwrap()["reason"], "date-format", "{e}");
    let p = b.preview(&[("dates.qif", qif)], json!({ "dateFormat": "d-m-y" }));
    assert_eq!(p["transactions"][0]["datePosted"], "2024-01-03");
    assert_eq!(p["transactions"][1]["datePosted"], "2024-02-03");
}

#[test]
fn records_gnucash_would_import_wrongly_are_error_rows() {
    let mut b = Book::new();
    // A padded amount (GnuCash's `string->number` gives up and takes 0, `qif-parse.scm:449`), a day that does not
    // exist, an action GnuCash does not know, and a good transaction.
    let qif = "!Type:Bank\nD03/14/2024\nT-45.10   \nPPadded\n^\nD02/30/2024\nT-1.00\nPNo such day\n^\nD03/15/2024\nT-2.00\nPGood\n^\n\
               !Type:Invst\nD03/16/2024\nNBuyIt\nYAcme\nT10.00\n^\n";
    let p = b.preview(&[("bad.qif", qif)], json!({}));
    let errors: Vec<(u64, &str)> = p["errors"].as_array().unwrap().iter().map(|e| (e["line"].as_u64().unwrap(), e["message"].as_str().unwrap())).collect();
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(errors.iter().any(|(line, m)| *line == 2 && m.contains("Not a number")), "{errors:?}");
    assert!(errors.iter().any(|(line, m)| *line == 6 && m.contains("no day 2024-02-30")), "{errors:?}");
    assert!(errors.iter().any(|(line, m)| *line == 15 && m.contains("BuyIt")), "{errors:?}");
    assert_eq!(p["summary"]["add"], 1);
    let e = b.err("import.apply", json!({ "proposal": p["id"] }));
    assert_eq!(e.code, codes::INVALID);
    let applied = b.apply(&p, json!({ "skipErrors": true }));
    assert_eq!(applied["added"].as_array().unwrap().len(), 1);
    assert_eq!(b.transactions().len(), 1);
}

#[test]
fn qif_transactions_are_only_added_or_skipped() {
    let mut b = Book::new();
    let p = b.preview(&[("checking.qif", CHECKING)], json!({}));
    let id = p["id"].clone();
    let mut refused = |change: Value| b.err("import.apply", json!({ "proposal": id, "changes": [change] })).code;
    assert_eq!(refused(json!({ "index": 0, "action": "clear" })), codes::INVALID);
    assert_eq!(refused(json!({ "index": 0, "action": "update" })), codes::INVALID);
    assert_eq!(refused(json!({ "index": 0, "amount": "1.00" })), codes::INVALID);
    assert_eq!(refused(json!({ "index": 9, "action": "skip" })), codes::INVALID);
    let applied = b.apply(&p, json!({ "changes": [{ "index": 1, "action": "skip" }] }));
    assert_eq!(applied["skipped"], json!([1]));
    assert_eq!(b.transactions().len(), 2);
    assert_amount(&b.balance("Checking"), "-145.10");
}

#[test]
fn files_that_are_not_qif_and_names_the_files_do_not_use_are_refused() {
    let mut b = Book::new();
    let e = b.err("import.preview", json!({ "format": "qif", "files": [{ "name": "x.csv", "text": "Date,Amount\n" }] }));
    assert_eq!(e.data.as_ref().unwrap()["reason"], "qif", "{e}");
    assert_eq!(e.data.as_ref().unwrap()["line"], 1);
    let e = b.err("import.preview", json!({ "format": "qif", "files": [{ "text": CHECKING }], "accounts": { "Savings": "Assets:Savings" } }));
    assert_eq!(e.code, codes::INVALID, "{e}");
    let e = b.err("import.preview", json!({ "format": "qif", "files": [] }));
    assert_eq!(e.code, codes::INVALID, "{e}");
    let e = b.err("import.preview", json!({ "format": "qif", "files": [{ "text": CHECKING }], "currency": "XXX::NOPE" }));
    assert_eq!(e.code, codes::NOT_FOUND, "{e}");
}

/// Every one of GnuCash's example files previews, applies with its error rows left out, and is found again.
#[test]
fn gnucash_s_example_files_import() {
    [
        ("abc.qif", 7, 0),
        ("abc-all.qif", 10, 0),
        ("bogus.qif", 8, 0),
        ("cbb-export.qif", 9, 0),
        ("divx.qif", 6, 0),
        ("ms-money.qif", 347, 0),
        ("price.qif", 2, 0),
        ("web.qif", 7, 0),
        ("swipe.qif", 7, 8),
    ]
    .iter()
    .for_each(|(name, added, errors)| {
        let mut b = Book::new();
        let text = fixture(name);
        let p = b.preview(&[(name, &text)], json!({}));
        assert_eq!(p["summary"]["add"], *added, "{name}");
        assert_eq!(p["errors"].as_array().unwrap().len(), *errors, "{name}");
        let applied = b.apply(&p, json!({ "skipErrors": true }));
        assert_eq!(applied["added"].as_array().unwrap().len(), *added, "{name}");
        let again = b.preview(&[(name, &text)], json!({}));
        assert_eq!(again["newAccounts"], json!([]), "{name}: the map is reused");
        let unmatched = again["transactions"].as_array().unwrap().iter().filter(|t| t["matches"].as_array().unwrap().is_empty()).count();
        assert_eq!(unmatched, 0, "{name}: every transaction is found again");
        assert_eq!(again["summary"]["add"], 0, "{name}: so nothing is added again");
    });
}
