//! A property of the account pie charts (`standard/account-piecharts.rhai`, translating
//! `5.17:gnucash/report/reports/standard/account-piecharts.scm`, cited as AP), on generated books: a pie's unrounded
//! slices sum to its unrounded total (`design/reports.md` §13). The hand-worked figures are in
//! `crates/red-ale-rpc/tests/report_account_piecharts.rs`.
//!
//! The report shows slices rounded to the report currency's SCU (AP:528-532) and the total rounded once, from the
//! unrounded sum (AP:518-524). With no averaging, a book in whole cents gives slices that need no rounding, so
//! there the shown slices must add up to the shown total exactly. With averaging, each unrounded slice is that
//! figure times the averaging multiplier (AP:397-405), so each shown slice must be its rounding, and the shown total
//! the rounding of their exact sum. "Maximum Slices" folds the smallest slices into "Other" (AP:451-457), which must
//! keep the sum. At one level and at all levels the total is also worked out from the book directly.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::Host;
use proptest::prelude::*;
use red_ale_core::ops::accounts::{AccountEdit, create_account};
use red_ale_core::ops::commit::{NewSplit, NewTransaction, add_transaction};
use red_ale_core::{AccountType, Book, CommodityRef, Date, FixedEnv, Guid, Numeric, Time64};
use red_ale_report::engine::Registry;
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;
use red_ale_report::number::{Rational, Rounding, SizeCap};
use serde_json::{Value, json};

/// A generated expense tree: top-level "E" with children "C0"… and, for some, one grandchild "G0"…; postings of
/// whole cents on days of the first quarter of 2024, each from a top-level bank account.
#[derive(Debug, Clone)]
struct Tree {
    grandchildren: Vec<bool>,
    postings: Vec<(usize, u32, u32, i64)>,
}

fn tree() -> impl Strategy<Value = Tree> {
    prop::collection::vec(any::<bool>(), 1..=5).prop_flat_map(|grandchildren| {
        let accounts = 1 + grandchildren.len() + grandchildren.iter().filter(|g| **g).count();
        let posting = (0..accounts, 1_u32..=3, 1_u32..=28, -20_000_i64..=20_000);
        prop::collection::vec(posting, 0..16).prop_map(move |postings| Tree {
            grandchildren: grandchildren.clone(),
            postings,
        })
    })
}

fn account(book: &mut Book, env: &mut FixedEnv, parent: Guid, name: &str, t: AccountType) -> Guid {
    let usd = Some(CommodityRef::currency("USD"));
    let edit = AccountEdit {
        name: Some(name.into()),
        account_type: Some(t),
        commodity: usd,
        ..AccountEdit::default()
    };
    create_account(book, env, parent, &edit).unwrap()
}

/// The book, and each expense account's name and own balance in cents, "E" first.
fn build(t: &Tree) -> (Book, Vec<(String, i64)>) {
    let mut env = FixedEnv::new(Time64(1_700_000_000), 7);
    let mut book = Book::new_empty(&mut env);
    let root = book.root;
    let bank = account(&mut book, &mut env, root, "Bank", AccountType::Bank);
    let e = account(&mut book, &mut env, root, "E", AccountType::Expense);
    let mut accounts = vec![(e, "E".to_owned())];
    t.grandchildren.iter().enumerate().for_each(|(i, has)| {
        let c = account(&mut book, &mut env, e, &format!("C{i}"), AccountType::Expense);
        accounts.push((c, format!("C{i}")));
        if *has {
            let g = account(&mut book, &mut env, c, &format!("G{i}"), AccountType::Expense);
            accounts.push((g, format!("G{i}")));
        }
    });
    let mut own = vec![0_i64; accounts.len()];
    t.postings.iter().for_each(|&(i, month, day, cents)| {
        let mut txn = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, month, day).unwrap());
        txn.splits = vec![
            NewSplit::new(accounts[i].0, Numeric::new(cents, 100)),
            NewSplit::new(bank, Numeric::new(-cents, 100)),
        ];
        add_transaction(&mut book, &mut env, txn).unwrap();
        own[i] += cents;
    });
    (book, accounts.into_iter().map(|(_, n)| n).zip(own).collect())
}

fn cents(c: i64) -> Rational {
    Rational::new(c, 100).unwrap()
}

fn add(a: &Rational, b: &Rational) -> Rational {
    a.checked_add(b, SizeCap::default()).unwrap()
}

fn mul(a: &Rational, b: &Rational) -> Rational {
    a.checked_mul(b, SizeCap::default()).unwrap()
}

fn round(r: &Rational) -> Rational {
    r.round_to(100, Rounding::Bankers).unwrap()
}

/// A display like `$1,234.56` as an amount.
fn amount(display: &str) -> Rational {
    Rational::parse(&display.replace(['$', ','], "")).unwrap()
}

/// The averaging multiplier for 2024-01-01 to 2024-03-31 (AP:365-378): 1 over 3 months, 13 weeks or 91/366 years.
fn multiplier(average: &str) -> Rational {
    match average {
        "MonthDelta" => Rational::new(1, 3).unwrap(),
        "WeekDelta" => Rational::new(1, 13).unwrap(),
        "YearDelta" => Rational::new(366, 91).unwrap(),
        _ => Rational::one(),
    }
}

/// A pie's slices as `(name, value)` and its shown total, or `None` for the no-data warning.
fn pie(host: &Host, registry: &Registry, options: &Value) -> Option<(Vec<(String, Rational)>, Rational)> {
    let result = host
        .run(registry, "Expense Accounts", options)
        .unwrap_or_else(|e| panic!("{options}: {e:?}"));
    let block = serde_json::to_value(&result.sections[0].blocks[0]).unwrap();
    if block["kind"] == "warning" {
        assert_eq!(block["title"], "No data");
        return None;
    }
    let labels = block["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().split(" - ").next().unwrap().to_owned());
    let values = block["series"][0]["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| serde_json::from_value::<Rational>(v.clone()).unwrap());
    let title = block["title"][1].as_str().unwrap();
    Some((labels.zip(values).collect(), amount(title.rsplit(": ").next().unwrap())))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn unrounded_slices_sum_to_the_unrounded_total(
        t in tree(),
        levels in prop::sample::select(vec![json!(1), json!(2), json!(3), json!("all")]),
        max_slices in 2_i64..=8,
        sort in prop::sample::select(vec!["amount", "acct-code", "alphabetical"]),
        average in prop::sample::select(vec!["MonthDelta", "WeekDelta", "YearDelta"]),
    ) {
        let (book, own) = build(&t);
        let host = Host::new(book);
        let registry = Registry::load(&Library::builtin(), &[], Limits::default()).unwrap();
        let options = |average: &str| json!({
            "General": { "Start Date": { "absolute": "2024-01-01" }, "End Date": { "absolute": "2024-03-31" }, "Show Average": average },
            "Accounts": { "Levels of Subaccounts": levels },
            "Display": { "Maximum Slices": max_slices, "Sort Method": sort },
        });
        let plain = pie(&host, &registry, &options("None"));
        let expected_total = match levels.as_str() {
            Some(_) => Some(own.iter().map(|(_, c)| (*c).max(0)).sum::<i64>()),
            None if levels == json!(1) => Some(own.iter().map(|(_, c)| c).sum::<i64>().max(0)),
            None => None,
        };
        let Some((slices, total)) = plain else {
            prop_assert!(expected_total.is_none_or(|e| e == 0), "no data, but the book holds {expected_total:?}");
            return Ok(());
        };
        prop_assert!(slices.len() <= usize::try_from(max_slices).unwrap());
        let sum = slices.iter().fold(Rational::zero(), |acc, (_, v)| add(&acc, v));
        prop_assert_eq!(&sum, &total, "whole cents: the shown slices are unrounded and add up to the total");
        if let Some(e) = expected_total {
            prop_assert_eq!(&total, &cents(e), "the total worked out from the book");
        }

        let m = multiplier(average);
        let (averaged, averaged_total) = pie(&host, &registry, &options(average)).unwrap();
        let names = |s: &[(String, Rational)]| s.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
        prop_assert_eq!(names(&averaged), names(&slices), "averaging scales every slice alike");
        let unrounded: Vec<Rational> = slices.iter().map(|(_, v)| mul(v, &m)).collect();
        averaged.iter().zip(&unrounded).for_each(|((_, shown), exact)| assert_eq!(shown, &round(exact), "a slice is its rounding"));
        let exact_total = unrounded.iter().fold(Rational::zero(), |acc, v| add(&acc, v));
        prop_assert_eq!(averaged_total, round(&exact_total), "the total is the rounding of the unrounded slices' sum");
    }
}
