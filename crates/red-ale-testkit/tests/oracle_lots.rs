//! Lots, capital gains and stock splits against GnuCash 5.17: books red-ale writes read the same in GnuCash, and
//! GnuCash's own lot scrub (`xaccAccountTreeScrubLots`) makes the same lots, cut splits and gains transactions as
//! red-ale's.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::Path;

use red_ale_core::ops::commit::{NewSplit, NewTransaction, add_transaction};
use red_ale_core::{CommodityRef, Date, FixedEnv, Guid, Numeric};
use red_ale_rpc::Dispatcher;
use red_ale_testkit::{TEST_NOW, TempBook, call, diff, open, oracle, oracle_check};
use serde_json::{Value, json};

const ACME: &str = "Assets:Brokerage:ACME";
const ZETA: &str = "Assets:Brokerage:Zeta";
const CASH: &str = "Assets:Cash";

fn trade(d: &mut Dispatcher, account: &str, date: (u32, u32), shares: &str, value: &str) -> Guid {
    d.context()
        .mutate(|book, env| {
            let acc = book.account_by_full_name(account).unwrap().guid;
            let cash = book.account_by_full_name(CASH).unwrap().guid;
            let mut t = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, date.0, date.1).unwrap());
            t.description = Some(format!("{account} {shares}"));
            let (shares, value) = (Numeric::parse_any(shares).unwrap(), Numeric::parse_any(value).unwrap());
            t.splits = vec![NewSplit::new(acc, value).with_amount(shares), NewSplit::new(cash, value.neg())];
            add_transaction(book, env, t)
        })
        .unwrap()
}

/// Trades that exercise FIFO across lots, cutting, value rounding and a stock split; no lots assigned yet.
fn trading_book() -> (TempBook, Dispatcher) {
    let (book, mut d) = open("synthetic/rich.gnucash");
    trade(&mut d, ACME, (6, 10), "10", "1100.00");
    trade(&mut d, ACME, (6, 12), "-12", "-1500.00");
    call(
        &mut d,
        "stock.split",
        &json!({ "account": ACME, "date": "2024-06-15", "shares": "3" }),
    );
    trade(&mut d, ACME, (6, 20), "-4", "-500.00");
    (1..=3).for_each(|day| {
        trade(&mut d, ZETA, (7, day), "10", "100.00");
    });
    trade(&mut d, ZETA, (7, 4), "-25", "-375.01");
    (book, d)
}

const GNUCASH_SCRUB: &str = r#"
import sys
from gnucash import Session, SessionOpenMode, _sw_core_utils
assert _sw_core_utils.gnc_version() == "5.17"
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_NORMAL_OPEN)
try:
    s.book.get_root_account().TreeScrubLots()
    s.save()
finally:
    s.end()
"#;

/// Full names of the dump's accounts by GUID.
fn account_names(dump: &Value) -> BTreeMap<String, String> {
    let accounts = dump["accounts"].as_array().unwrap();
    let by_guid: BTreeMap<&str, &Value> = accounts.iter().map(|a| (a["guid"].as_str().unwrap(), a)).collect();
    let full = |g: &str| {
        let mut parts = Vec::new();
        let mut cur = by_guid.get(g).copied();
        while let Some(a) = cur {
            parts.push(a["name"].as_str().unwrap().to_owned());
            cur = a["parent"].as_str().and_then(|p| by_guid.get(p).copied());
        }
        parts.reverse();
        parts.join(":")
    };
    by_guid.keys().map(|g| ((*g).to_owned(), full(g))).collect()
}

/// The dump with GUIDs replaced by account names and entities sorted by content, so two books made independently
/// can be compared. Dates entered are dropped: GnuCash stamps gains transactions with the time of the scrub.
fn normalize(dump: &Value) -> Value {
    let names = account_names(dump);
    let name = |v: &Value| v.as_str().and_then(|g| names.get(g)).cloned().map_or(Value::Null, Value::String);
    let mut txns: Vec<Value> = dump["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            let mut splits: Vec<Value> = t["splits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| {
                    json!({ "account": name(&s["account"]), "memo": s["memo"], "action": s["action"], "value": s["value"],
                            "amount": s["amount"], "reconcile": s["reconcile"], "lot": s["lotTitle"] })
                })
                .collect();
            splits.sort_by_key(ToString::to_string);
            json!({ "currency": t["currency"], "datePosted": t["datePosted"], "description": t["description"], "num": t["num"],
                    "notes": t["notes"], "splits": splits })
        })
        .collect();
    txns.sort_by_key(ToString::to_string);
    let mut lots: Vec<Value> = dump["lots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let mut l = l.clone();
            l["account"] = name(&l["account"]);
            l["splits"] = json!(l["splits"].as_array().unwrap().len());
            l
        })
        .collect();
    lots.sort_by_key(ToString::to_string);
    let mut accounts: Vec<Value> = dump["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            json!({ "name": name(&a["guid"]), "type": a["type"], "commodity": a["commodity"], "description": a["description"],
                         "notes": a["notes"], "balance": a["balance"] })
        })
        .collect();
    accounts.sort_by_key(ToString::to_string);
    json!({ "accounts": accounts, "transactions": txns, "lots": lots, "prices": dump["prices"] })
}

/// For every split of the security accounts: its transaction, amount, value and slot keys (the slots' GUIDs differ
/// between books made independently), sorted.
fn split_slot_keys(path: &Path) -> Vec<String> {
    let (book, _) = red_ale_xml::read_book(&std::fs::read(path).unwrap()).unwrap();
    let accounts = [ACME, ZETA].map(|a| book.account_by_full_name(a).unwrap().guid);
    let mut out: Vec<String> = accounts
        .iter()
        .flat_map(|a| book.splits_of(*a))
        .map(|(t, s)| {
            let keys: Vec<&String> = s.slots.iter().map(|(k, _)| k).collect();
            format!(
                "{} {} {} {keys:?}",
                t.description.clone().unwrap_or_default(),
                s.amount.reduce().to_decimal_string(),
                s.value.reduce().to_decimal_string()
            )
        })
        .collect();
    out.sort();
    out
}

fn save_and_close(d: &mut Dispatcher) {
    call(d, "book.close", &json!({ "save": true }));
}

fn copy_to(src: &Path) -> TempBook {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(src, &path).unwrap();
    TempBook { dir, path }
}

#[test]
fn red_ale_scrubs_lots_as_gnucash_does() {
    let (book, mut d) = trading_book();
    save_and_close(&mut d);
    compare_scrubs(&book, 0x00ee_0000_0000_0000_0000_0000_0000_0000);
    let ours = normalize(&oracle::dump(&book.path).unwrap());
    let lots = ours["lots"].as_array().unwrap();
    assert!(lots.iter().any(|l| l["gains"] != "0/1"), "the scenario realises gains");
    assert!(
        lots.iter().any(|l| l["balance"] == "-10/1"),
        "GnuCash's stale adjusted amounts overfill a lot, and red-ale matches"
    );
    assert!(
        ours["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == "Orphaned Gains-USD")
    );
}

#[test]
fn lot_methods_write_books_gnucash_reads() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    let lot = call(
        &mut d,
        "lot.create",
        &json!({ "account": ACME, "title": "Manual", "notes": "made by red-ale" }),
    );
    call(&mut d, "lot.create", &json!({ "account": ZETA }));
    let buy = trade(&mut d, ACME, (7, 1), "6", "720.00");
    let sell = trade(&mut d, ACME, (7, 2), "-8", "-1000.00");
    let split_of = |d: &mut Dispatcher, txn: Guid| {
        let b = d.context().book().unwrap();
        let acme = b.account_by_full_name(ACME).unwrap().guid;
        b.transaction(txn)
            .unwrap()
            .splits
            .iter()
            .find(|s| s.account == Some(acme))
            .unwrap()
            .guid
            .to_string()
    };
    let (buy_split, sell_split) = (split_of(&mut d, buy), split_of(&mut d, sell));
    call(&mut d, "lot.assignSplit", &json!({ "split": buy_split, "lot": lot["guid"] }));
    let out = call(&mut d, "lot.assignSplit", &json!({ "split": sell_split, "lot": lot["guid"] }));
    call(&mut d, "lot.assignSplit", &json!({ "split": out["rest"] }));
    call(
        &mut d,
        "lot.computeGains",
        &json!({ "lot": lot["guid"], "gainsAccount": "Income:Capital Gains" }),
    );
    let fixture_lot = call(&mut d, "lot.list", &json!({ "account": ACME, "state": "open" }))[0]["guid"].clone();
    call(&mut d, "lot.update", &json!({ "lot": fixture_lot, "title": "Renamed" }));
    let problems = oracle_check(&mut d, &book, &[]);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    let dump = oracle::dump(&book.path).unwrap();
    let gains = dump["lots"].as_array().unwrap().iter().find(|l| l["title"] == "Manual").unwrap();
    assert_eq!(
        gains["gains"], "30/1",
        "6 sold for 750.00 that cost 720.00, against the given gains account"
    );
    assert_eq!(gains["closed"], true);
}

#[test]
fn deleting_and_emptying_lots_writes_books_gnucash_reads() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    let lot = call(&mut d, "lot.list", &json!({ "account": ACME }))[0]["guid"].clone();
    let detail = call(&mut d, "lot.get", &json!({ "lot": lot }));
    let first = detail["splits"][0]["split"].clone();
    call(&mut d, "lot.removeSplit", &json!({ "split": first }));
    call(&mut d, "lot.delete", &json!({ "lot": lot, "releaseSplits": true }));
    let problems = oracle_check(&mut d, &book, &[]);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(oracle::dump(&book.path).unwrap()["lots"].as_array().unwrap().is_empty());
}

const STOCK_SPLIT_CHECK: &str = r#"
import sys, json
from gnucash import Session, SessionOpenMode, Split, _sw_core_utils
assert _sw_core_utils.gnc_version() == "5.17"
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    acme = [a for a in s.book.get_root_account().get_descendants() if a.GetName() == "ACME"][0]
    out = []
    for sp in acme.GetSplitList():
        out.append({"stockSplit": bool(sp.IsStockSplit()), "type": sp.GetType(), "action": sp.GetAction(),
                    "adjusted": "%d/%d" % (sp.GetAdjustedAmount().num, sp.GetAdjustedAmount().denom)})
    print(json.dumps(out))
finally:
    s.end()
"#;

#[test]
fn stock_splits_read_as_stock_splits_in_gnucash() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    let out = call(
        &mut d,
        "stock.split",
        &json!({ "account": ACME, "date": "2024-06-10", "shares": "5", "price": "60", "cash": "2.50",
                 "incomeAccount": "Income:Capital Gains", "assetAccount": CASH }),
    );
    assert_eq!(out["priceStatus"], "added");
    let problems = oracle_check(&mut d, &book, &[]);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    let report: Value = serde_json::from_str(&oracle::run_code(STOCK_SPLIT_CHECK, &[&book.path_str()]).unwrap()).unwrap();
    let rows = report.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows[2],
        json!({ "stockSplit": true, "type": "stock-split", "action": "Split", "adjusted": "50000/10000" })
    );
    // GnuCash restates the earlier buy and sell in post-split units: 10 -> 20, -5 -> -10.
    assert_eq!(rows[0]["adjusted"], "200000/10000");
    assert_eq!(rows[1]["adjusted"], "-100000/10000");
    let lot = &oracle::dump(&book.path).unwrap()["lots"][0];
    assert_eq!(lot["balance"], "10/1");
}

/// Scrub a copy with GnuCash and the book with red-ale, then compare what GnuCash reads from each. Red-ale's session
/// draws GUIDs from `seed`, away from those the session that wrote the book gave out.
fn compare_scrubs(book: &TempBook, seed: u128) {
    let theirs = copy_to(&book.path);
    oracle::run_code(GNUCASH_SCRUB, &[&theirs.path.display().to_string()]).unwrap();
    let mut d = Dispatcher::new(red_ale_service::Settings::default());
    d.set_env_factory(Box::new(move || Box::new(FixedEnv::new(TEST_NOW, seed))));
    let info = call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    call(
        &mut d,
        "lot.scrubAccount",
        &json!({ "account": info["rootAccount"], "subaccounts": true }),
    );
    let problems = oracle_check(&mut d, book, &[]);
    assert!(
        problems.is_empty(),
        "GnuCash reads red-ale's scrubbed book differently:\n{}",
        problems.join("\n")
    );
    let differences = diff(
        &normalize(&oracle::dump(&theirs.path).unwrap()),
        &normalize(&oracle::dump(&book.path).unwrap()),
        &[],
    );
    assert!(
        differences.is_empty(),
        "red-ale's scrub differs from GnuCash's:\n{}",
        differences.join("\n")
    );
    assert_eq!(split_slot_keys(&theirs.path), split_slot_keys(&book.path));
}

#[test]
fn rescrubbing_an_edited_sale_matches_gnucash() {
    let (book, mut d) = trading_book();
    let root = call(&mut d, "book.info", &json!({}))["rootAccount"].clone();
    call(&mut d, "lot.scrubAccount", &json!({ "account": root, "subaccounts": true }));
    // Sell one Zeta piece dearer, as a user editing the register might: its gains transaction must follow.
    d.context()
        .mutate(|b, _| {
            let zeta = b.account_by_full_name(ZETA).unwrap().guid;
            let cash = b.account_by_full_name(CASH).unwrap().guid;
            let (txn, piece) = b
                .splits_of(zeta)
                .find(|(_, s)| s.value == Numeric::new(-15000, 100))
                .map(|(t, s)| (t.guid, s.guid))
                .unwrap();
            let t = std::sync::Arc::make_mut(b.transactions.get_mut(&txn).unwrap());
            t.splits.iter_mut().for_each(|s| {
                if s.guid == piece {
                    s.value = Numeric::new(-16000, 100);
                } else if s.account == Some(cash) {
                    s.value = s.value.add_fixed(Numeric::new(1000, 100)).unwrap();
                    s.amount = s.value;
                }
            });
            Ok(())
        })
        .unwrap();
    save_and_close(&mut d);
    compare_scrubs(&book, 0x00ef_0000_0000_0000_0000_0000_0000_0000);
}

/// `gnc_stock_split_assistant_finish` (5.17:gnucash/gnome/assistant-stock-split.c:339-473) replayed through the
/// bindings, with the user's default currency USD: 5 shares, price 60, cash in lieu 2.50.
const GNUCASH_STOCK_SPLIT: &str = r#"
import sys
from datetime import datetime
from gnucash import Session, SessionOpenMode, Transaction, Split, GncPrice, GncNumeric, _sw_core_utils
from gnucash import gnucash_core_c as c
assert _sw_core_utils.gnc_version() == "5.17"
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_NORMAL_OPEN)
try:
    book = s.book
    accounts = {a.get_full_name(): a for a in book.get_root_account().get_descendants()}
    acme, cash, income = accounts["Assets.Brokerage.ACME"], accounts["Assets.Cash"], accounts["Income.Capital Gains"]
    usd = book.get_table().lookup("CURRENCY", "USD")
    date = datetime(2024, 6, 10, 10, 59)
    t = Transaction(book)
    t.BeginEdit()
    t.SetCurrency(usd)
    t.SetDatePostedSecsNormalized(date)
    t.SetDescription("Stock Split")
    sp = Split(book)
    sp.SetParent(t)
    sp.SetAccount(acme)
    sp.SetAmount(GncNumeric(5, 1))
    sp.MakeStockSplit()
    sp.SetAction("Split")
    p = GncPrice(book)
    p.begin_edit()
    p.set_commodity(acme.GetCommodity())
    p.set_currency(usd)
    p.set_time64(date)
    p.set_source(c.PRICE_SOURCE_STOCK_SPLIT)
    p.set_typestr("unknown")
    p.set_value(GncNumeric(60, 1))
    p.commit_edit()
    book.get_price_db().add_price(p)
    for account, amount in ((cash, GncNumeric(250, 100)), (income, GncNumeric(-250, 100))):
        x = Split(book)
        x.SetAccount(account)
        x.SetParent(t)
        x.SetAmount(amount)
        x.SetValue(amount)
        x.SetMemo("Cash in lieu")
    t.CommitEdit()
    s.save()
finally:
    s.end()
"#;

#[test]
fn stock_split_matches_the_assistant() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    let theirs = copy_to(&book.path);
    oracle::run_code(GNUCASH_STOCK_SPLIT, &[&theirs.path.display().to_string()]).unwrap();
    call(
        &mut d,
        "stock.split",
        &json!({ "account": ACME, "date": "2024-06-10", "shares": "5", "price": "60", "cash": "2.50",
                 "incomeAccount": "Income:Capital Gains", "assetAccount": CASH }),
    );
    save_and_close(&mut d);
    let differences = diff(
        &normalize(&oracle::dump(&theirs.path).unwrap()),
        &normalize(&oracle::dump(&book.path).unwrap()),
        &[],
    );
    assert!(
        differences.is_empty(),
        "red-ale's stock split differs from the assistant's:\n{}",
        differences.join("\n")
    );
    assert_eq!(split_slot_keys(&theirs.path), split_slot_keys(&book.path));
}
