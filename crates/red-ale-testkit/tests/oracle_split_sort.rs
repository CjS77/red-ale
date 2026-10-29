//! The split query's sorts, reconcile-state and closing matches and unique transactions against GnuCash 5.17's
//! QofQuery on the same book (`qof_query_set_sort_order`, `qof_query_set_sort_increasing`, `xaccQueryAddClearedMatch`,
//! `xaccQueryAddClosingTransMatch`, `xaccQueryGetSplitsUniqueTrans`), with the num field source off and on. Every key
//! is run ascending and descending with the default sort third, as the Transaction Report engine runs them
//! (`5.17:gnucash/report/trep-engine.scm:2541-2549`), so GnuCash's order is total and never hash order.
//!
//! The book is built for ties and for what tells the keys' rules apart: accounts whose path-wise order differs from
//! their full names' (`Assets:Bank` against `Assets 2`) and an ancestor with splits; codes `"9"` and `"110"`; nums
//! that `strtol` reads (`"10"`, `"9"`, `" 5"`, `"+5"`, `"-3"`, `"5a"`, `"abc"`, `""`); a one-split transaction, a
//! three-split one and a pair with a trading account, all of which give `-- Split Transaction --`; descriptions
//! differing in case; equal values and dates; cleared, reconciled, frozen and voided splits; a closing transaction.
//!
//! With the num field source on, `xaccSplitOrder` is not a total order: on one day, a transaction whose splits'
//! memos sort one way and actions the other forms a cycle with a transaction entered between them, and GnuCash's
//! order then follows its hash order. Each transaction here orders its splits' memos as their actions.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use red_ale_core::ops::accounts::{AccountEdit, create_account};
use red_ale_core::ops::commit::{NewSplit, NewTransaction, add_transaction};
use red_ale_core::ops::transactions::find::{PeriodQuery, period_splits};
use red_ale_core::ops::transactions::sort::{SortKey, SortLevel};
use red_ale_core::transaction::slot_paths;
use red_ale_core::{AccountType, Book, CommodityRef, Date, DomainResult, Env, Guid, Numeric, ReconcileState, SlotValue, Time64};
use red_ale_testkit::{TempBook, call, create, oracle};
use serde_json::{Value, json};

type Accounts = BTreeMap<&'static str, Guid>;

const ACCOUNTS: [(&str, AccountType, Option<&str>); 10] = [
    ("Assets", AccountType::Asset, Some("100")),
    ("Assets:Bank", AccountType::Bank, Some("110")),
    ("Assets:Cash", AccountType::Cash, Some("9")),
    ("Assets 2", AccountType::Asset, None),
    ("Expenses", AccountType::Expense, Some("500")),
    ("Expenses:Food", AccountType::Expense, Some("510")),
    ("Income", AccountType::Income, Some("400")),
    ("Trading", AccountType::Trading, Some("T")),
    ("Equity", AccountType::Equity, Some("300")),
    ("Assets:Bank:Sub", AccountType::Bank, Some("110")),
];

fn build_accounts(book: &mut Book, env: &mut dyn Env) -> DomainResult<Accounts> {
    let mut a = Accounts::new();
    ACCOUNTS.iter().try_for_each(|(name, t, code)| {
        let (parent, leaf) = name.rsplit_once(':').map_or((book.root, *name), |(p, l)| (a[p], l));
        let edit = AccountEdit {
            name: Some(leaf.into()),
            account_type: Some(*t),
            commodity: Some(CommodityRef::currency("USD")),
            code: code.map(Into::into),
            ..AccountEdit::default()
        };
        a.insert(name, create_account(book, env, parent, &edit)?);
        Ok::<_, red_ale_core::DomainError>(())
    })?;
    Ok(a)
}

/// A split: account, value in cents, memo, action, reconcile state and day of reconciliation in January.
type Leg = (&'static str, i64, &'static str, &'static str, char, Option<u32>);

/// A transaction: posted day in January 2024, entered (seconds after 2024-03-01), num, description, splits.
type Txn = (u32, i64, &'static str, &'static str, &'static [Leg]);

const TXNS: [Txn; 13] = [
    (
        5,
        30,
        "10",
        "Salary",
        &[
            ("Assets:Bank", 300_000, "pay", "b", 'n', None),
            ("Income", -300_000, "", "", 'y', Some(31)),
        ],
    ),
    (
        5,
        10,
        "9",
        "Groceries",
        &[
            ("Expenses:Food", 4550, "", "", 'n', None),
            ("Assets:Bank", -4550, "veg", "a", 'c', Some(20)),
        ],
    ),
    (
        6,
        20,
        "abc",
        "groceries",
        &[
            ("Expenses:Food", 1225, "fruit", "c", 'n', None),
            ("Assets:Cash", -1225, "", "", 'n', None),
        ],
    ),
    (
        6,
        20,
        "",
        "Split three",
        &[
            ("Assets:Bank", -10_000, "", "", 'f', None),
            ("Expenses:Food", 8000, "x", "", 'n', None),
            ("Assets:Cash", 2000, "", "", 'n', None),
        ],
    ),
    (
        7,
        5,
        " 5",
        "Trading pair",
        &[("Assets:Bank", -700, "", "9", 'n', None), ("Trading", 700, "", "", 'n', None)],
    ),
    (7, 40, "-3", "Lone", &[("Assets:Bank", 0, "lone", "", 'n', None)]),
    (
        8,
        50,
        "5a",
        "Move",
        &[("Assets 2", 5000, "", "", 'n', None), ("Assets", -5000, "", "", 'n', None)],
    ),
    (
        8,
        50,
        "+5",
        "Move",
        &[
            ("Assets:Bank:Sub", 5000, "", "", 'c', Some(20)),
            ("Assets", -5000, "", "", 'n', None),
        ],
    ),
    (
        5,
        60,
        "10",
        "Salary",
        &[
            ("Assets:Bank", 300_000, "pay", "b", 'n', None),
            ("Income", -300_000, "", "", 'y', Some(31)),
        ],
    ),
    (
        9,
        70,
        "11",
        "Mistake",
        &[("Expenses:Food", 990, "", "", 'n', None), ("Assets:Bank", -990, "", "", 'n', None)],
    ),
    (
        31,
        80,
        "",
        "Closing Entries",
        &[("Income", 600_000, "", "", 'n', None), ("Equity", -600_000, "", "", 'n', None)],
    ),
    (
        10,
        15,
        "2",
        "Snack",
        &[("Expenses", 300, "", "", 'n', None), ("Assets:Cash", -300, "", "", 'c', Some(20))],
    ),
    (
        10,
        15,
        "2",
        "snack",
        &[("Expenses", 300, "", "", 'n', None), ("Assets:Cash", -300, "", "", 'n', None)],
    ),
];

fn day(d: u32) -> Date {
    Date::from_ymd(2024, 1, d).unwrap()
}

/// Posts [`TXNS`]; returns the GUIDs of "Mistake" (to void) and "Closing Entries".
fn build_transactions(book: &mut Book, env: &mut dyn Env, a: &Accounts) -> DomainResult<(Guid, Guid)> {
    let entered = Date::from_ymd(2024, 3, 1).unwrap().neutral_time().0;
    let guids = TXNS
        .iter()
        .map(|(posted, at, num, description, legs)| {
            let mut t = NewTransaction::new(CommodityRef::currency("USD"), day(*posted));
            t.date_entered = Some(Time64(entered + at));
            t.num = (!num.is_empty()).then(|| (*num).into());
            t.description = Some((*description).into());
            t.splits = legs.iter().map(|leg| new_split(a, leg)).collect();
            add_transaction(book, env, t)
        })
        .collect::<DomainResult<Vec<Guid>>>()?;
    let closing = guids[10];
    Arc::make_mut(book.transactions.get_mut(&closing).unwrap())
        .slots
        .set(slot_paths::BOOK_CLOSING, SlotValue::Integer(1));
    Ok((guids[9], closing))
}

fn new_split(a: &Accounts, (account, cents, memo, action, state, reconciled): &Leg) -> NewSplit {
    let mut s = NewSplit::new(a[account], Numeric::new(*cents, 100));
    s.memo = (!memo.is_empty()).then(|| (*memo).into());
    s.action = (!action.is_empty()).then(|| (*action).into());
    s.reconcile_state = ReconcileState::from_char(*state).unwrap();
    s.reconcile_date = reconciled.map(|d| day(d).neutral_time());
    s
}

/// The test book, saved, with the num field source on or off and "Mistake" voided.
fn sort_book(num_source: bool) -> (TempBook, Accounts) {
    let (book, mut d) = create();
    let (accounts, (mistake, _)) = d
        .context()
        .mutate(|book, env| {
            let a = build_accounts(book, env)?;
            let made = build_transactions(book, env, &a)?;
            red_ale_core::book_options::set_split_action_for_num(book, num_source);
            Ok((a, made))
        })
        .unwrap();
    call(
        &mut d,
        "transaction.void",
        &json!({ "guid": mistake.to_string(), "reason": "typo" }),
    );
    call(&mut d, "book.close", &json!({ "save": true }));
    (book, accounts)
}

/// Each sort key with its QOF parameter path.
const KEYS: [(SortKey, &[&str]); 14] = [
    (SortKey::AccountFullName, &["acct-fullname"]),
    (SortKey::AccountCode, &["account", "code"]),
    (SortKey::CorrAccountFullName, &["corr-acct-fullname"]),
    (SortKey::CorrAccountCode, &["corr-acct-code"]),
    (SortKey::DatePosted, &["trans", "date-posted"]),
    (SortKey::DateEntered, &["trans", "date-entered"]),
    (SortKey::DateReconciled, &["date-reconciled"]),
    (SortKey::Value, &["value"]),
    (SortKey::Amount, &["amount"]),
    (SortKey::Num, &["trans", "num"]),
    (SortKey::Action, &["action"]),
    (SortKey::Memo, &["memo"]),
    (SortKey::Description, &["trans", "desc"]),
    (SortKey::Default, &["QofQueryDefaultSort"]),
];

fn params(key: Option<SortKey>) -> Vec<&'static str> {
    key.map_or_else(Vec::new, |k| KEYS.iter().find(|(x, _)| *x == k).unwrap().1.to_vec())
}

/// One query of both: reconcile states, closing match, three sort levels, unique.
struct Case {
    states: Option<&'static str>,
    closing: Option<bool>,
    sort: [(Option<SortKey>, bool); 3],
    unique: bool,
}

fn sorted(primary: SortKey, ascending: bool) -> Case {
    Case {
        states: None,
        closing: None,
        sort: [(Some(primary), ascending), (None, true), (Some(SortKey::Default), true)],
        unique: false,
    }
}

fn cases() -> Vec<Case> {
    let mut cases: Vec<Case> = KEYS.iter().flat_map(|(k, _)| [sorted(*k, true), sorted(*k, false)]).collect();
    let combos = [
        [
            (Some(SortKey::DatePosted), true),
            (Some(SortKey::AccountFullName), false),
            (Some(SortKey::Default), true),
        ],
        [
            (Some(SortKey::CorrAccountCode), false),
            (Some(SortKey::Num), true),
            (Some(SortKey::Default), false),
        ],
        [
            (Some(SortKey::Memo), true),
            (Some(SortKey::Description), false),
            (Some(SortKey::Default), true),
        ],
        [
            (Some(SortKey::AccountCode), true),
            (Some(SortKey::DateReconciled), false),
            (Some(SortKey::Default), false),
        ],
    ];
    cases.extend(combos.into_iter().map(|sort| Case {
        sort,
        ..sorted(SortKey::Default, true)
    }));
    cases.extend([
        Case {
            states: Some("ncyfv"),
            ..sorted(SortKey::Value, true)
        },
        Case {
            states: Some("v"),
            ..sorted(SortKey::Default, true)
        },
        Case {
            states: Some("cy"),
            ..sorted(SortKey::AccountFullName, false)
        },
        Case {
            states: Some("nf"),
            closing: Some(false),
            ..sorted(SortKey::Num, true)
        },
        Case {
            closing: Some(true),
            ..sorted(SortKey::Default, true)
        },
        Case {
            unique: true,
            ..sorted(SortKey::AccountFullName, true)
        },
        Case {
            unique: true,
            ..sorted(SortKey::Value, false)
        },
    ]);
    cases
}

/// `xaccQueryAddClearedMatch`'s mask of states (`5.17:libgnucash/engine/Query.h:171-180`).
fn cleared_mask(states: &str) -> u32 {
    states
        .chars()
        .map(|c| match c {
            'n' => 1,
            'c' => 2,
            'y' => 4,
            'f' => 8,
            'v' => 16,
            _ => panic!("state {c}"),
        })
        .sum()
}

fn case_json(c: &Case) -> Value {
    json!({
        // GnuCash's split query has no account term here; red-ale's lists every account, and leaves voided
        // splits out by default, as `CLEARED-ALL` less `CLEARED-VOIDED` does.
        "cleared": cleared_mask(c.states.unwrap_or("ncyf")),
        "closing": c.closing,
        "sort": c.sort.iter().map(|(k, _)| params(*k)).collect::<Vec<_>>(),
        "increasing": c.sort.iter().map(|(_, up)| up).collect::<Vec<_>>(),
        "unique": c.unique,
    })
}

fn red_ale(book: &TempBook) -> Vec<Vec<String>> {
    let (ours, _) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    let accounts: Vec<Guid> = ours.accounts.keys().copied().filter(|g| *g != ours.root).collect();
    cases()
        .iter()
        .map(|c| {
            let query = PeriodQuery {
                accounts: accounts.clone(),
                reconcile: c.states.map(|s| s.chars().map(|x| ReconcileState::from_char(x).unwrap()).collect()),
                is_closing: c.closing,
                sort: c
                    .sort
                    .iter()
                    .map(|(key, ascending)| SortLevel {
                        key: *key,
                        ascending: *ascending,
                    })
                    .collect(),
                unique: c.unique,
                ..PeriodQuery::default()
            };
            period_splits(&ours, &query)
                .unwrap()
                .iter()
                .map(|(_, s)| s.guid.to_string())
                .collect()
        })
        .collect()
}

fn gnucash(book: &TempBook) -> Vec<Vec<String>> {
    oracle::check_version().unwrap();
    let spec: Vec<Value> = cases().iter().map(case_json).collect();
    let out = oracle::run_script(
        &oracle::scripts_dir().join("split_sort.py"),
        &[&book.path_str(), &Value::Array(spec).to_string()],
    )
    .unwrap_or_else(|e| panic!("oracle: {e}"));
    serde_json::from_str(&out).unwrap()
}

fn compare(num_source: bool) {
    let (book, _) = sort_book(num_source);
    let theirs = gnucash(&book);
    let ours = red_ale(&book);
    assert_eq!(theirs.len(), ours.len());
    assert!(theirs[0].len() > 20, "the queries find the book's splits");
    let wrong: Vec<String> = cases()
        .iter()
        .zip(theirs.iter().zip(&ours))
        .filter(|(_, (t, o))| t != o)
        .map(|(c, (t, o))| format!("{}\n  GnuCash  {t:?}\n  red-ale  {o:?}", case_json(c)))
        .collect();
    assert!(
        wrong.is_empty(),
        "num source {num_source}: {} of {} differ:\n{}",
        wrong.len(),
        ours.len(),
        wrong.join("\n")
    );
}

#[test]
fn split_query_sorts_match_gnucash() {
    compare(false);
}

#[test]
fn split_query_sorts_match_gnucash_with_the_num_field_source() {
    compare(true);
}
