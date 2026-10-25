//! Closing the books against GnuCash 5.17: GnuCash closes a copy of a book with the steps of its Close Book dialog,
//! red-ale closes the book with `book.closeBooks`, and both books must hold the same accounts and transactions.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::Path;

use red_ale_rpc::Dispatcher;
use red_ale_testkit::{TempBook, call, diff, open, oracle, oracle_check};
use serde_json::{Value, json};

/// `gnc_book_close_response_cb` and its callbacks (5.17:gnucash/gnome-utils/dialog-book-close.c:89-316) replayed
/// through the bindings: the closing date as the date edit gives it (local midnight), balances as of the end of that
/// day, one transaction per currency, the equity account chosen or made as `finish_txn_cb` does.
/// Arguments: book, closing date, income total account, expense total account (full names with ':'), description.
const GNUCASH_CLOSE_BOOK: &str = r#"
import ctypes, sys
from datetime import datetime
from gnucash import (Session, SessionOpenMode, Transaction, Split, Account, GncCommodity, GncNumeric, _sw_core_utils,
                     ACCT_TYPE_INCOME, ACCT_TYPE_EXPENSE, ACCT_TYPE_EQUITY, GNC_DENOM_AUTO, GNC_HOW_DENOM_FIXED, GNC_HOW_RND_NEVER)
from gnucash import gnucash_core_c as c
import gnc_engine as g
assert _sw_core_utils.gnc_version() == "5.17"
g.engine.gnc_time64_get_day_end.restype = ctypes.c_int64
g.engine.gnc_time64_get_day_end.argtypes = [ctypes.c_int64]

path, date, income_name, expense_name, desc = sys.argv[1:6]
s = Session("xml://" + path, SessionOpenMode.SESSION_NORMAL_OPEN)
try:
    book = s.book
    root = book.get_root_account()
    by_name = {a.get_full_name(): a for a in root.get_descendants()}
    close_date = datetime(*map(int, date.split("-")))
    day_end = datetime.fromtimestamp(g.engine.gnc_time64_get_day_end(int(close_date.timestamp())))

    def close_accounts_of_type(base, acct_type):
        txns = {}
        for a in root.get_descendants():
            if a.GetType() != acct_type:
                continue
            bal = a.GetBalanceAsOfDate(day_end)
            if bal.zero_p():
                continue
            cmdty = GncCommodity(instance=c.gnc_account_get_currency_or_parent(a.instance))
            key = cmdty.get_unique_name()
            if key not in txns:
                t = Transaction(book)
                t.BeginEdit()
                t.SetDateEnteredSecs(datetime.now())
                t.SetDatePostedSecsNormalized(close_date)
                t.SetDescription(desc)
                t.SetCurrency(cmdty)
                t.SetIsClosingTxn(True)
                txns[key] = [cmdty, t, GncNumeric(0, 1)]
            entry = txns[key]
            split = Split(book)
            split.SetParent(entry[1])
            split.SetAccount(a)
            split.SetBaseValue(bal.neg(), cmdty)
            entry[2] = entry[2].add(bal, GNC_DENOM_AUTO, GNC_HOW_DENOM_FIXED | GNC_HOW_RND_NEVER)
        for cmdty, t, total in txns.values():
            if len(txns) == 1 and cmdty.equal(base.GetCommodity()):
                acc = base
            else:
                acc = base.lookup_by_name(cmdty.get_mnemonic())
                if acc is None:
                    acc = Account(book)
                    acc.BeginEdit()
                    acc.SetType(ACCT_TYPE_EQUITY)
                    acc.SetName(cmdty.get_mnemonic())
                    acc.SetDescription(cmdty.get_mnemonic())
                    acc.SetCommodity(cmdty)
                    base.append_child(acc)
                    acc.CommitEdit()
            assert cmdty.equal(acc.GetCommodity())
            split = Split(book)
            split.SetParent(t)
            split.SetAccount(acc)
            split.SetBaseValue(total, cmdty)
            t.CommitEdit()

    close_accounts_of_type(by_name[income_name.replace(":", ".")], ACCT_TYPE_INCOME)
    close_accounts_of_type(by_name[expense_name.replace(":", ".")], ACCT_TYPE_EXPENSE)
    s.save()
finally:
    s.end()
"#;

/// Every transaction GnuCash reads as a closing transaction, as `description|datePosted` (with the `date-posted`
/// slot GnuCash reads back as the posted date).
const CLOSING_TRANSACTIONS: &str = r#"
import sys
from gnucash import Session, SessionOpenMode, _sw_core_utils
assert _sw_core_utils.gnc_version() == "5.17"
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    seen, out = set(), []
    for a in s.book.get_root_account().get_descendants():
        for sp in a.GetSplitList():
            t = sp.GetParent()
            g = t.GetGUID().to_string()
            if g not in seen and t.GetIsClosingTxn():
                seen.add(g)
                out.append("%s|%s|%s" % (t.GetDescription(), t.GetDate().date(), t.GetCurrency().get_mnemonic()))
    print("\n".join(sorted(out)))
finally:
    s.end()
"#;

const DATE: &str = "2024-06-30";
const DESCRIPTION: &str = "Closing Entries 2024";

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

/// The dump with GUIDs replaced by account names and lists sorted by content. Dates entered are dropped: GnuCash
/// stamps its closing transactions with the time of the run. Split order is kept: both commit the same sort.
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
                .map(|s| json!({ "account": name(&s["account"]), "memo": s["memo"], "action": s["action"], "value": s["value"],
                                 "amount": s["amount"], "reconcile": s["reconcile"] }))
                .collect();
            splits.sort_by_key(ToString::to_string);
            json!({ "currency": t["currency"], "datePosted": t["datePosted"], "description": t["description"], "splits": splits })
        })
        .collect();
    txns.sort_by_key(ToString::to_string);
    let mut accounts: Vec<Value> = dump["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| json!({ "name": name(&a["guid"]), "type": a["type"], "commodity": a["commodity"], "scu": a["scu"],
                         "description": a["description"], "placeholder": a["placeholder"], "balance": a["balance"] }))
        .collect();
    accounts.sort_by_key(ToString::to_string);
    json!({ "accounts": accounts, "transactions": txns })
}

fn copy_to(src: &Path) -> TempBook {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(src, &path).unwrap();
    TempBook { dir, path }
}

fn post(d: &mut Dispatcher, date: &str, currency: &str, splits: &Value) {
    call(d, "transaction.post", &json!({ "currency": currency, "datePosted": date, "description": "trade", "splits": splits }));
}

/// The rich fixture plus a euro income account, a placeholder income account with a balance, and income after the
/// closing date.
fn book_to_close() -> (TempBook, Dispatcher) {
    let (book, mut d) = open("synthetic/rich.gnucash");
    call(&mut d, "account.create", &json!({ "name": "Euro Sales", "type": "INCOME", "parent": "Income", "commodity": "EUR" }));
    call(&mut d, "account.create", &json!({ "name": "Grants", "type": "INCOME", "parent": "Income" }));
    call(&mut d, "account.create", &json!({ "name": "Retained", "type": "EQUITY", "parent": "Equity" }));
    post(&mut d, "2024-03-01", "EUR", &json!([{ "account": "Assets:Euro Bank", "value": "80.00" }, { "account": "Income:Euro Sales", "value": "-80.00" }]));
    post(&mut d, "2024-04-01", "USD", &json!([{ "account": "Assets:Cash", "value": "12.34" }, { "account": "Income:Grants", "value": "-12.34" }]));
    post(&mut d, "2024-06-30", "USD", &json!([{ "account": "Expenses:Fees", "value": "1.05" }, { "account": "Assets:Cash", "value": "-1.05" }]));
    post(&mut d, "2024-07-01", "USD", &json!([{ "account": "Assets:Cash", "value": "500.00" }, { "account": "Income:Salary", "value": "-500.00" }]));
    call(&mut d, "account.update", &json!({ "account": "Income:Grants", "placeholder": true }));
    call(&mut d, "book.save", &json!({}));
    (book, d)
}

fn close_both(income: &str, expense: &str) -> (TempBook, TempBook, Value) {
    let (book, mut d) = book_to_close();
    let theirs = copy_to(&book.path);
    let scripts = oracle::scripts_dir().display().to_string();
    let code = GNUCASH_CLOSE_BOOK.replace("import gnc_engine as g", &format!("sys.path.insert(0, {scripts:?})\nimport gnc_engine as g"));
    oracle::run_code(&code, &[&theirs.path.display().to_string(), DATE, income, expense, DESCRIPTION]).unwrap();
    let result = call(
        &mut d,
        "book.closeBooks",
        &json!({ "date": DATE, "incomeTotalAccount": income, "expenseTotalAccount": expense, "description": DESCRIPTION }),
    );
    let problems = oracle_check(&mut d, &book, &[]);
    assert!(problems.is_empty(), "GnuCash reads red-ale's closed book differently:\n{}", problems.join("\n"));
    (book, theirs, result)
}

fn compare(book: &TempBook, theirs: &TempBook) {
    let differences = diff(&normalize(&oracle::dump(&theirs.path).unwrap()), &normalize(&oracle::dump(&book.path).unwrap()), &[]);
    assert!(differences.is_empty(), "red-ale's closing differs from GnuCash's:\n{}", differences.join("\n"));
    let closing = |p: &Path| oracle::run_code(CLOSING_TRANSACTIONS, &[&p.display().to_string()]).unwrap();
    let ours = closing(&book.path);
    assert_eq!(ours, closing(&theirs.path));
    assert!(ours.contains("Closing Entries 2024|2024-06-30|EUR"), "{ours}");
}

#[test]
fn closing_into_one_equity_account_matches_the_dialog() {
    let (book, theirs, result) = close_both("Equity", "Equity");
    compare(&book, &theirs);
    // Two currencies of income: Equity:USD and Equity:EUR are made; the expense pass, in USD only, closes into
    // Equity itself, the account's own currency.
    assert_eq!(result["createdAccounts"].as_array().unwrap().len(), 2);
    assert_eq!(result["transactions"].as_array().unwrap().len(), 3);
}

#[test]
fn closing_into_separate_accounts_matches_the_dialog() {
    let (book, theirs, result) = close_both("Equity:Retained", "Equity:Opening Balances");
    compare(&book, &theirs);
    let kinds: Vec<&str> = result["transactions"].as_array().unwrap().iter().map(|t| t["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["income", "income", "expense"]);
}
