//! Lots, the lot viewer, capital gains and stock splits through the dispatcher.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::ops::commit::{NewSplit, NewTransaction, add_transaction};
use red_ale_core::{CommodityRef, Date, Guid, Numeric};
use red_ale_rpc::{Dispatcher, codes};
use red_ale_testkit::{call, call_err, open};
use serde_json::{Value, json};

const ACME: &str = "Assets:Brokerage:ACME";
const CASH: &str = "Assets:Cash";

/// Record a trade of ACME against cash through the core, as other packages' methods are not used here.
fn trade(d: &mut Dispatcher, date: (u32, u32), shares: &str, value: &str) -> Guid {
    d.context()
        .mutate(|book, env| {
            let acme = book.account_by_full_name(ACME).unwrap().guid;
            let cash = book.account_by_full_name(CASH).unwrap().guid;
            let mut t = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, date.0, date.1).unwrap());
            let (shares, value) = (Numeric::parse_any(shares).unwrap(), Numeric::parse_any(value).unwrap());
            t.splits = vec![NewSplit::new(acme, value).with_amount(shares), NewSplit::new(cash, value.neg())];
            add_transaction(book, env, t)
        })
        .unwrap()
}

fn split_in(d: &mut Dispatcher, txn: Guid, account: &str) -> String {
    let book = d.context().book().unwrap();
    let acc = book.account_by_full_name(account).unwrap().guid;
    book.transaction(txn).unwrap().splits.iter().find(|s| s.account == Some(acc)).unwrap().guid.to_string()
}

fn amount(v: &Value) -> Numeric {
    Numeric::new(v["num"].as_str().unwrap().parse().unwrap(), v["denom"].as_str().unwrap().parse().unwrap())
}

#[test]
fn the_viewer_lists_the_fixture_lot() {
    let (_book, mut d) = open("synthetic/rich.gnucash");
    let lots = call(&mut d, "lot.list", &json!({ "account": ACME }));
    assert_eq!(lots.as_array().unwrap().len(), 1);
    let lot = &lots[0];
    assert_eq!(lot["notes"], "lot notes");
    assert_eq!(lot["closed"], false);
    assert_eq!(lot["openDate"], "2024-06-04");
    assert_eq!(lot["closeDate"], Value::Null);
    assert_eq!(amount(&lot["balance"]), Numeric::from_int(5));
    assert!(call(&mut d, "lot.list", &json!({ "account": ACME, "state": "closed" })).as_array().unwrap().is_empty());
    let detail = call(&mut d, "lot.get", &json!({ "lot": lot["guid"] }));
    let splits = detail["splits"].as_array().unwrap();
    assert_eq!(splits.iter().map(|s| s["action"].as_str().unwrap()).collect::<Vec<_>>(), ["Buy", "Sell"]);
    assert_eq!(splits[1]["kind"], "normal");
    assert_eq!(detail["splitCount"], 2);
}

#[test]
fn scrubbing_records_the_fixture_gain() {
    let (_book, mut d) = open("synthetic/rich.gnucash");
    let out = call(&mut d, "lot.scrubAccount", &json!({ "account": "Assets", "subaccounts": true }));
    assert_eq!(out["transactionsCreated"].as_array().unwrap().len(), 1);
    assert_eq!(out["accountsCreated"].as_array().unwrap().len(), 1);
    let lot = &call(&mut d, "lot.list", &json!({ "account": ACME }))[0];
    assert_eq!(amount(&lot["realizedGains"]), Numeric::new(10000, 100), "sold 5 for 600.00 that cost 500.00");
    assert_eq!(lot["gainsCurrency"], "CURRENCY::USD");
    let again = call(&mut d, "lot.scrubAccount", &json!({ "account": ACME }));
    assert!(again["transactionsChanged"].as_array().unwrap().is_empty() && again["transactionsCreated"].as_array().unwrap().is_empty());
    let detail = call(&mut d, "lot.get", &json!({ "lot": lot["guid"] }));
    let gains = detail["splits"].as_array().unwrap().iter().find(|s| s["kind"] == "gains").unwrap();
    assert_eq!(gains["memo"], "Realized Gain/Loss");
}

#[test]
fn lots_are_made_edited_filled_and_deleted() {
    let (_book, mut d) = open("synthetic/rich.gnucash");
    let lot = call(&mut d, "lot.create", &json!({ "account": ACME }));
    assert_eq!(lot["title"], "Lot 0");
    let lot_id = lot["guid"].clone();
    let lot = call(&mut d, "lot.update", &json!({ "lot": lot_id, "title": "Spring", "notes": "n" }));
    assert_eq!((lot["title"].as_str(), lot["notes"].as_str()), (Some("Spring"), Some("n")));
    let buy = trade(&mut d, (7, 1), "8", "960.00");
    let sell = trade(&mut d, (7, 2), "-10", "-1300.00");
    let buy_split = split_in(&mut d, buy, ACME);
    let sell_split = split_in(&mut d, sell, ACME);
    let free = call(&mut d, "lot.freeSplits", &json!({ "account": ACME, "lot": lot_id }));
    assert_eq!(free.as_array().unwrap().len(), 2);
    call(&mut d, "lot.assignSplit", &json!({ "split": buy_split, "lot": lot_id }));
    let out = call(&mut d, "lot.assignSplit", &json!({ "split": sell_split, "lot": lot_id }));
    assert_eq!(out["pieces"][0]["lot"], lot_id);
    let rest = out["rest"].as_str().unwrap().to_owned();
    let lot = call(&mut d, "lot.get", &json!({ "lot": lot_id }));
    assert_eq!(lot["closed"], true);
    assert_eq!(lot["closeDate"], "2024-07-02");
    assert_eq!(call_err(&mut d, "lot.assignSplit", &json!({ "split": rest, "lot": lot_id })).code, codes::CONFLICT);
    assert_eq!(call_err(&mut d, "lot.delete", &json!({ "lot": lot_id })).code, codes::CONFLICT);
    let deleted = call(&mut d, "lot.delete", &json!({ "lot": lot_id, "releaseSplits": true }));
    assert_eq!(deleted["releasedSplits"].as_array().unwrap().len(), 2);
    let fifo = call(&mut d, "lot.assignSplit", &json!({ "split": rest }));
    assert_eq!(fifo["pieces"].as_array().unwrap().len(), 1, "FIFO puts the rest into the fixture's open lot");
    assert_eq!(call(&mut d, "lot.removeSplit", &json!({ "split": rest }))["lotDeleted"], false);
}

#[test]
fn refusals_have_stable_codes() {
    let (_book, mut d) = open("synthetic/rich.gnucash");
    let missing = "0123456789abcdef0123456789abcdef";
    assert_eq!(call_err(&mut d, "lot.get", &json!({ "lot": missing })).code, codes::NOT_FOUND);
    assert_eq!(call_err(&mut d, "lot.list", &json!({ "account": "Nope" })).code, codes::NOT_FOUND);
    let other = call(&mut d, "lot.create", &json!({ "account": CASH }));
    let buy = trade(&mut d, (7, 1), "1", "100.00");
    let split = split_in(&mut d, buy, ACME);
    assert_eq!(call_err(&mut d, "lot.assignSplit", &json!({ "split": split, "lot": other["guid"] })).code, codes::INVALID);
    let bank_split = split_in(&mut d, buy, CASH);
    assert_eq!(call_err(&mut d, "lot.assignSplit", &json!({ "split": bank_split })).code, codes::INVALID, "a cash account has no trades");
    assert_eq!(call_err(&mut d, "lot.removeSplit", &json!({ "split": split })).code, codes::INVALID);
    let bad = json!({ "account": ACME, "date": "2024-07-01", "shares": 10 });
    assert_eq!(call_err(&mut d, "stock.split", &bad).code, codes::INVALID_PARAMS);
    let no_cash_accounts = json!({ "account": ACME, "date": "2024-07-01", "shares": "10", "cash": "1.00" });
    assert_eq!(call_err(&mut d, "stock.split", &no_cash_accounts).code, codes::INVALID);
}

#[test]
fn stock_split_records_the_assistants_transaction() {
    let (_book, mut d) = open("synthetic/rich.gnucash");
    let out = call(
        &mut d,
        "stock.split",
        &json!({
            "account": ACME, "date": "2024-07-01", "shares": "5", "price": "55.5",
            "cash": "12.00", "incomeAccount": "Income:Capital Gains", "assetAccount": CASH
        }),
    );
    assert_eq!(out["priceStatus"], "added");
    let txn = Guid::parse(out["transaction"].as_str().unwrap()).unwrap();
    let book = d.context().book().unwrap();
    let t = book.transaction(txn).unwrap();
    assert_eq!(t.currency, CommodityRef::currency("USD"));
    assert_eq!(t.description.as_deref(), Some("Stock Split"));
    assert!(t.splits[0].value.same_repr(Numeric::new(0, 1)));
    assert!(t.splits[0].amount.same_repr(Numeric::new(50000, 10000)));
    assert_eq!(t.splits.iter().map(|s| s.memo.as_deref()).collect::<Vec<_>>(), [None, Some("Cash in lieu"), Some("Cash in lieu")]);
    let lots = call(&mut d, "lot.scrubAccount", &json!({ "account": ACME }));
    assert!(lots["lotsCreated"].as_array().unwrap().is_empty(), "the stock split stays out of lots");
    let lot = &call(&mut d, "lot.list", &json!({ "account": ACME }))[0];
    assert_eq!(amount(&lot["balance"]), Numeric::from_int(10), "5 held, doubled by the split");
}

#[test]
fn a_read_only_book_still_answers_queries() {
    let book = red_ale_testkit::TempBook::copy("piecash/investment.gnucash");
    let mut d = red_ale_testkit::dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str(), "readOnly": true }));
    let lots = call(&mut d, "lot.list", &json!({ "account": "Assets:Investments:Brokerage Account:Stock:VEUR" }));
    assert!(lots.as_array().unwrap().is_empty());
    let err = call_err(&mut d, "lot.create", &json!({ "account": "Assets:Investments:Brokerage Account:Stock:VEUR" }));
    assert_eq!(err.code, codes::READ_ONLY);
}
