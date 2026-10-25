//! Transactions through the API, checked against GnuCash 5.17: GnuCash reads what red-ale wrote and sees the same
//! data, and where GnuCash can do the same operation itself (void, unvoid, reverse, currency change, price recording,
//! find), it does it on another copy of the book and the results are compared.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use red_ale_core::transaction::slot_paths;
use red_ale_core::{Book, Guid, SlotValue, Transaction};
use red_ale_testkit::{TempBook, call, open, oracle, oracle_check};
use serde_json::{Value, json};

const RICH: &str = "synthetic/rich.gnucash";
const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";
const GROCERIES: &str = "439480360f6342f9abfd6df6f40cc5e9";
const MISTAKE: &str = "7e04ac3fbb8d4eff83068196f09164f4";
const TO_EURO: &str = "bf5e53a96f4749dcb43fd23fe9479cab";

/// Python that opens the book in `sys.argv[1]` for writing, defines `book`, `root` and `txn(guid)`, runs `body`
/// (indented four spaces), saves and ends the session.
fn gnucash_edit(book: &TempBook, body: &str) -> String {
    oracle::check_version().unwrap();
    let code = format!(
        r#"import datetime, json, sys
from gnucash import Session, SessionOpenMode, Transaction, Split, GncNumeric
from gnucash import gnucash_core_c as c
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_NORMAL_OPEN)
try:
    book = s.book
    root = book.get_root_account()
    def txn(g):
        for a in root.get_descendants():
            for sp in a.GetSplitList():
                t = sp.GetParent()
                if t.GetGUID().to_string() == g:
                    return t
        raise KeyError(g)
{body}
    s.save()
finally:
    s.end()
"#
    );
    oracle::run_code(&code, &[&book.path_str()]).unwrap_or_else(|e| panic!("{e}"))
}

fn read(path: &Path) -> Book {
    red_ale_xml::read_book(&std::fs::read(path).unwrap()).unwrap().0
}

fn guid(s: &str) -> Guid {
    Guid::parse(s).unwrap()
}

fn no_diffs(diffs: &[String]) {
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}

/// The transaction with its void time replaced, so voids made at different times compare equal.
fn without_void_time(mut t: Transaction) -> Transaction {
    if t.slots.get(slot_paths::VOID_TIME).is_some() {
        t.slots.set(slot_paths::VOID_TIME, SlotValue::String("<time>".into()));
    }
    t
}

#[test]
fn posted_transactions_read_the_same_in_gnucash() {
    let (book, mut d) = open(RICH);
    call(
        &mut d,
        "transaction.post",
        &json!({
            "datePosted": "2024-06-20", "num": "102", "description": "Rent", "notes": "June",
            "dateEntered": "2024-06-21T08:30:00Z",
            "splits": [
                { "account": CHECKING, "value": "-750.00", "memo": "cheque", "action": "Pay" },
                { "account": "Expenses:Food", "value": "750.00", "reconcileState": "c" }
            ]
        }),
    );
    call(
        &mut d,
        "transaction.post",
        &json!({
            "currency": "USD", "datePosted": "2024-06-21", "description": "Euros",
            "splits": [
                { "account": CHECKING, "value": "-55.00" },
                { "account": "Assets:Euro Bank", "value": "55.00", "amount": "50.00", "memo": "fx" }
            ]
        }),
    );
    call(
        &mut d,
        "transaction.post",
        &json!({
            "currency": "EUR", "datePosted": "2024-06-22", "description": "Lunch in Paris",
            "splits": [
                { "account": "Assets:Euro Bank", "value": "-12.345" },
                { "account": "Expenses:Food", "value": "12.345", "amount": "13.58" }
            ]
        }),
    );
    no_diffs(&oracle_check(&mut d, &book, &[]));
}

#[test]
fn updates_and_deletes_read_the_same_in_gnucash() {
    let (book, mut d) = open(RICH);
    let g = call(&mut d, "transaction.get", &json!({ "guid": GROCERIES }));
    let food = g["splits"][0]["guid"].clone();
    call(
        &mut d,
        "transaction.update",
        &json!({
            "guid": GROCERIES, "description": "Groceries and fees", "notes": "", "num": "7", "datePosted": "2024-06-12",
            "changeSplits": [{ "guid": food, "value": "10.00", "memo": "food" }],
            "addSplits": [{ "account": "Expenses:Fees", "value": "2.34", "action": "Fee" }]
        }),
    );
    let sell = "dfd7e387825e4495aa56c67aea81b014";
    call(&mut d, "transaction.delete", &json!({ "guid": sell }));
    no_diffs(&oracle_check(&mut d, &book, &[]));
}

#[test]
fn void_matches_gnucash() {
    let (ours, mut d) = open(RICH);
    call(&mut d, "transaction.void", &json!({ "guid": GROCERIES, "reason": "duplicate" }));
    call(&mut d, "transaction.void", &json!({ "guid": TO_EURO, "reason": "" }));
    no_diffs(&oracle_check(&mut d, &ours, &[]));
    let theirs = TempBook::copy(RICH);
    gnucash_edit(&theirs, &format!("    txn('{GROCERIES}').Void('duplicate')\n    txn('{TO_EURO}').Void('')"));
    let (a, b) = (read(&ours.path), read(&theirs.path));
    [GROCERIES, TO_EURO].iter().for_each(|g| {
        let (ours, theirs) = (a.transaction(guid(g)).unwrap().clone(), b.transaction(guid(g)).unwrap().clone());
        assert!(ours.is_void() && theirs.is_void(), "GnuCash 5.17 counts an empty-reason void as void");
        assert_eq!(without_void_time(ours), without_void_time(theirs), "{g}");
    });
}

#[test]
fn unvoid_matches_gnucash() {
    let (ours, mut d) = open(RICH);
    call(&mut d, "transaction.unvoid", &json!({ "guid": MISTAKE }));
    no_diffs(&oracle_check(&mut d, &ours, &[]));
    let theirs = TempBook::copy(RICH);
    gnucash_edit(&theirs, &format!("    txn('{MISTAKE}').Unvoid()"));
    let (a, b) = (read(&ours.path), read(&theirs.path));
    assert_eq!(a.transaction(guid(MISTAKE)), b.transaction(guid(MISTAKE)));
}

/// A transaction with its GUIDs and entered date blanked, for comparing two reversals.
fn anonymous(mut t: Transaction) -> Transaction {
    t.guid = Guid::NULL;
    t.date_entered = red_ale_core::Time64(0);
    t.splits.iter_mut().for_each(|s| s.guid = Guid::NULL);
    t
}

#[test]
fn reverse_matches_gnucash() {
    let (ours, mut d) = open(RICH);
    call(&mut d, "transaction.reverse", &json!({ "guid": GROCERIES, "date": "2024-06-30" }));
    let buy = "95689dde3f844412825189d84f6e7902";
    call(&mut d, "transaction.reverse", &json!({ "guid": buy, "date": "2024-06-30" }));
    no_diffs(&oracle_check(&mut d, &ours, &[]));
    let theirs = TempBook::copy(RICH);
    let body = [GROCERIES, buy]
        .iter()
        .map(|g| {
            format!(
                "    n = txn('{g}').Reverse()\n    n.BeginEdit()\n    n.SetDatePostedSecsNormalized(datetime.datetime(2024, 6, 30, 12, 0))\n    \
                 n.SetDateEnteredSecs(datetime.datetime(2024, 7, 1, 12, 0))\n    n.CommitEdit()\n"
            )
        })
        .collect::<Vec<_>>()
        .concat();
    gnucash_edit(&theirs, &body);
    let (a, b) = (read(&ours.path), read(&theirs.path));
    [GROCERIES, buy].iter().for_each(|g| {
        let (oa, ob) = (a.transaction(guid(g)).unwrap(), b.transaction(guid(g)).unwrap());
        let (ra, rb) = (oa.reversed_by().unwrap(), ob.reversed_by().unwrap());
        assert_eq!(anonymous(a.transaction(ra).unwrap().clone()), anonymous(b.transaction(rb).unwrap().clone()), "{g}");
        let mut oa = oa.clone();
        oa.slots.set(slot_paths::REVERSED_BY, SlotValue::Guid(rb));
        assert_eq!(&oa, ob, "the original only gains reversed-by");
    });
}

#[test]
fn currency_change_matches_gnucash_when_gnucash_balances() {
    let (ours, mut d) = open(RICH);
    call(&mut d, "transaction.update", &json!({ "guid": TO_EURO, "currency": "EUR" }));
    no_diffs(&oracle_check(&mut d, &ours, &[]));
    let theirs = TempBook::copy(RICH);
    gnucash_edit(&theirs, &format!("    eur = book.get_table().lookup('CURRENCY', 'EUR')\n    txn('{TO_EURO}').SetCurrency(eur)"));
    let (a, b) = (read(&ours.path), read(&theirs.path));
    assert_eq!(a.transaction(guid(TO_EURO)), b.transaction(guid(TO_EURO)));
}

#[test]
fn recorded_prices_match_xacc_trans_record_price() {
    let buy = json!({
        "datePosted": "2024-06-21",
        "splits": [
            { "account": CHECKING, "value": "-1300.00" },
            { "account": "Assets:Brokerage:ACME", "value": "1300.00", "amount": "1.5", "action": "Buy" },
            { "account": "Assets:Brokerage:Zeta", "value": "0.00", "amount": "0" }
        ]
    });
    let (ours, mut d) = open(RICH);
    let mut with_price = buy.clone();
    with_price["recordPrice"] = json!(true);
    call(&mut d, "transaction.post", &with_price);
    // A same-day price with a better source (Finance::Quote on 2024-06-05) is kept.
    let on_fq_day = json!({
        "datePosted": "2024-06-05", "recordPrice": true,
        "splits": [{ "account": CHECKING, "value": "-1250.00" }, { "account": "Assets:Brokerage:ACME", "value": "1250.00", "amount": "1" }]
    });
    call(&mut d, "transaction.post", &on_fq_day);
    no_diffs(&oracle_check(&mut d, &ours, &[]));

    let (posted, mut d2) = open(RICH);
    let first = call(&mut d2, "transaction.post", &buy)["guid"].as_str().unwrap().to_owned();
    let mut plain = on_fq_day.clone();
    plain["recordPrice"] = json!(false);
    let second = call(&mut d2, "transaction.post", &plain)["guid"].as_str().unwrap().to_owned();
    call(&mut d2, "book.close", &json!({ "save": true }));
    // A directory of its own, so GnuCash's backup cannot clash with the one red-ale made this second.
    let theirs = TempBook::new_path();
    std::fs::copy(&posted.path, &theirs.path).unwrap();
    gnucash_edit(&theirs, &format!("    txn('{first}').RecordPrice(c.PRICE_SOURCE_SPLIT_REG)\n    txn('{second}').RecordPrice(c.PRICE_SOURCE_SPLIT_REG)"));
    let (a, b) = (oracle::dump(&ours.path).unwrap(), oracle::dump(&theirs.path).unwrap());
    assert_eq!(a["prices"], b["prices"]);
}

#[test]
fn transfers_read_the_same_and_use_gnucash_arithmetic() {
    let (book, mut d) = open(RICH);
    let by_rate = call(
        &mut d,
        "transaction.transfer",
        &json!({ "from": CHECKING, "to": "Assets:Euro Bank", "amount": "123.45", "rate": "0.9137", "date": "2024-06-25", "memo": "fx" }),
    );
    let by_amount = call(
        &mut d,
        "transaction.transfer",
        &json!({ "from": CHECKING, "to": "Assets:Brokerage:ACME", "amount": "1000.00", "toAmount": "0.75", "date": "2024-06-26" }),
    );
    call(&mut d, "transaction.transfer", &json!({ "from": CHECKING, "to": "Assets:Euro Bank", "amount": "50", "date": "2024-06-03" }));
    no_diffs(&oracle_check(&mut d, &book, &[]));
    // GnuCash's own arithmetic: to amount = gnc_numeric_mul(amount, rate, SCU, ROUND_HALF_UP), and the transaction
    // create_transaction builds with xaccSplitSetBaseValue.
    let out = oracle::run_code(
        r"from gnucash import GncNumeric
from gnucash import gnucash_core_c as c
n = GncNumeric(12345, 100).mul(GncNumeric(9137, 10000), 100, c.GNC_HOW_RND_ROUND_HALF_UP)
r = GncNumeric(75, 100).div(GncNumeric(100000, 100), c.GNC_DENOM_AUTO, c.GNC_HOW_DENOM_REDUCE)
print(n.num(), n.denom(), r.num(), r.denom())
",
        &[],
    )
    .unwrap();
    let to = &by_rate["toAmount"];
    let rate = &by_amount["rate"];
    let ours = format!("{} {} {} {}", to["num"].as_str().unwrap(), to["denom"].as_str().unwrap(), rate["num"].as_str().unwrap(), rate["denom"].as_str().unwrap());
    assert_eq!(out.trim(), ours);
    let price: Value = by_amount["price"].clone();
    assert_eq!(price["action"], "created");
}

/// GnuCash's query engine over the same book: the splits each query finds, in its default sort.
const QUERIES: &str = r#"import json, sys, datetime
import gnucash
from gnucash import Session, SessionOpenMode, Query, Split, GncNumeric, QueryStringPredicate, QueryNumericPredicate, QueryDatePredicate
from gnucash import gnucash_core_c as c
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    book = s.book
    root = book.get_root_account().GetGUID().to_string()
    def run(terms):
        q = Query()
        q.search_for('Split')
        q.set_book(book)
        for path, pred, op in terms:
            q.add_term(path, pred, op)
        out = []
        for x in q.run():
            sp = Split(instance=x)
            if sp.GetAccount().get_root().GetGUID().to_string() == root:
                out.append(sp.GetGUID().to_string())
        return out
    AND, OR = gnucash.QOF_QUERY_AND, gnucash.QOF_QUERY_OR
    ci = gnucash.QOF_STRING_MATCH_CASEINSENSITIVE
    print(json.dumps([
        run([(['trans', 'desc'], QueryStringPredicate(gnucash.QOF_COMPARE_CONTAINS, 'groc', ci, False), AND)]),
        run([(['value'], QueryNumericPredicate(gnucash.QOF_COMPARE_GTE, gnucash.QOF_NUMERIC_MATCH_DEBIT, GncNumeric(750, 1)), AND),
             (['trans', 'date-posted'], QueryDatePredicate(gnucash.QOF_COMPARE_GTE, gnucash.QOF_DATE_MATCH_NORMAL, datetime.datetime(2024, 6, 2, 10, 59)), AND)]),
        run([(['reconcile-flag'], c.qof_query_char_predicate(gnucash.QOF_CHAR_MATCH_ANY, 'v'), AND)]),
        run([(['memo'], QueryStringPredicate(gnucash.QOF_COMPARE_CONTAINS, 'CHEQUE', ci, False), AND)]),
        run([]),
        run([(['trans', 'desc'], QueryStringPredicate(gnucash.QOF_COMPARE_EQUAL, '^(rent|opening)$', ci, True), AND)]),
        run([(['amount'], QueryNumericPredicate(gnucash.QOF_COMPARE_GT, gnucash.QOF_NUMERIC_MATCH_ANY, GncNumeric(4, 1)), AND)]),
        run([(['trans', 'desc'], QueryStringPredicate(gnucash.QOF_COMPARE_CONTAINS, 'fund', ci, False), AND),
             (['memo'], QueryStringPredicate(gnucash.QOF_COMPARE_CONTAINS, 'cheque', ci, False), OR)]),
        run([(['trans', 'date-posted'], QueryDatePredicate(gnucash.QOF_COMPARE_LTE, gnucash.QOF_DATE_MATCH_NORMAL, datetime.datetime(2024, 6, 4, 23, 59, 59)), AND),
             (['value'], QueryNumericPredicate(gnucash.QOF_COMPARE_LT, gnucash.QOF_NUMERIC_MATCH_CREDIT, GncNumeric(100, 1)), AND)]),
    ]))
finally:
    s.end()
"#;

#[test]
fn find_matches_gnucash_queries() {
    oracle::check_version().unwrap();
    let (book, mut d) = open(RICH);
    call(
        &mut d,
        "transaction.post",
        &json!({
            "datePosted": "2024-06-20", "description": "Rent",
            "splits": [{ "account": CHECKING, "value": "-750.00", "memo": "Cheque 12" }, { "account": "Expenses:Food", "value": "750.00" }]
        }),
    );
    let queries = [
        json!({ "description": { "text": "groc" } }),
        json!({ "all": [{ "value": { "op": "ge", "amount": "750", "sign": "debit" } }, { "datePosted": { "op": "onOrAfter", "date": "2024-06-02" } }] }),
        json!({ "reconcile": { "states": ["v"] } }),
        json!({ "memo": { "text": "CHEQUE" } }),
        json!({ "all": [] }),
        json!({ "description": { "op": "matchesRegex", "text": "^(rent|opening)$" } }),
        json!({ "amount": { "op": "gt", "amount": "4" } }),
        json!({ "any": [{ "description": { "text": "fund" } }, { "memo": { "text": "cheque" } }] }),
        json!({ "all": [{ "datePosted": { "op": "onOrBefore", "date": "2024-06-04" } }, { "value": { "op": "lt", "amount": "100", "sign": "credit" } }] }),
    ];
    let ours: Vec<Vec<String>> = queries
        .iter()
        .map(|q| {
            let found = call(&mut d, "transaction.find", &json!({ "query": q }));
            found["matches"].as_array().unwrap().iter().map(|m| m["split"].as_str().unwrap().to_owned()).collect()
        })
        .collect();
    call(&mut d, "book.close", &json!({ "save": true }));
    let theirs: Vec<Vec<String>> = serde_json::from_str(&oracle::run_code(QUERIES, &[&book.path_str()]).unwrap()).unwrap();
    ours.iter().zip(&theirs).enumerate().for_each(|(i, (o, t))| assert_eq!(o, t, "query {i}: {}", queries[i]));
    assert!(theirs.iter().all(|t| !t.is_empty()), "every query finds something: {theirs:?}");
}
