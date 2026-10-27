//! CSV account import, hand-worked from `csv_account_import` (`5.17:gnucash/import-export/csv-imp/csv-account-import.c:201-374`):
//! GnuCash has no tests of it.

use proptest::prelude::*;
use red_ale_core::ops::accounts::{AccountEdit, create_account};
use red_ale_core::{AccountType, Book, CommodityRef, FixedEnv, Guid, Time64};

use super::*;
use crate::matcher::ImportSettings;
use crate::pipeline::Format;

const HEADER: &str =
    "Type,Full Account Name,Account Name,Account Code,Description,Account Color,Notes,Symbol,Namespace,Hidden,Tax Info,Placeholder\n";

struct Fx {
    book: Book,
    env: FixedEnv,
    bank: Guid,
}

fn fx() -> Fx {
    let mut env = FixedEnv::new(Time64(1_711_000_000), 1);
    let mut book = Book::new_empty(&mut env);
    let root = book.root;
    let add = |book: &mut Book, env: &mut FixedEnv, parent: Guid, name: &str, ty: AccountType| {
        let edit = AccountEdit {
            name: Some(name.to_owned()),
            account_type: Some(ty),
            commodity: Some(CommodityRef::currency("USD")),
            ..AccountEdit::default()
        };
        create_account(book, env, parent, &edit).unwrap()
    };
    let assets = add(&mut book, &mut env, root, "Assets", AccountType::Asset);
    let bank = add(&mut book, &mut env, assets, "Bank", AccountType::Bank);
    Fx { book, env, bank }
}

fn proposal(fx: &Fx, text: &str) -> Proposal {
    let file = read(text, Separator::Comma, None, 0);
    let (accounts, mut errors) = propose(&fx.book, &file.rows, 0);
    errors.extend(file.errors);
    errors.sort_by_key(|e| e.line);
    Proposal {
        accounts,
        errors,
        ignored_lines: file.ignored,
        ..Proposal::empty(Format::CsvAccounts, ImportSettings::default(), None)
    }
}

fn actions(p: &Proposal) -> Vec<(&str, &str)> {
    p.accounts.iter().map(|a| (a.row.full_name.as_str(), a.action.as_str())).collect()
}

fn messages(p: &Proposal) -> Vec<(usize, &str)> {
    p.errors.iter().map(|e| (e.line, e.message.as_str())).collect()
}

#[test]
fn new_accounts_are_created_parents_first_with_every_field() {
    let mut f = fx();
    let text = format!(
        "{HEADER}EXPENSE,Expenses,Expenses,5000,All spending,\"rgb(1,2,3)\",\"Notes, with \"\"quotes\"\"\",USD,CURRENCY,T,T,T\n\
         EXPENSE,Expenses:Food,Food,,,,,USD,CURRENCY,F,F,F\nSTOCK,Assets:ACME,ACME,,,,,ACME,NASDAQ,F,F,F\n"
    );
    f.book.commodities.insert(
        CommodityRef::new("NASDAQ", "ACME"),
        std::sync::Arc::new(red_ale_core::Commodity::new(CommodityRef::new("NASDAQ", "ACME"), None, 1)),
    );
    let p = proposal(&f, &text);
    assert_eq!(actions(&p), [("Expenses", "add"), ("Expenses:Food", "add"), ("Assets:ACME", "add")]);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    let applied = apply(&mut f.book, &mut f.env, &p, &[]).unwrap();
    assert_eq!(applied.accounts_created.len(), 3);
    let expenses = f.book.account_by_full_name("Expenses").unwrap();
    assert_eq!(
        (expenses.account_type, expenses.code.as_deref(), expenses.description.as_deref()),
        (AccountType::Expense, Some("5000"), Some("All spending"))
    );
    assert_eq!(expenses.notes(), Some("Notes, with \"quotes\""));
    assert_eq!(expenses.stored_color(), Some("rgb(1,2,3)"), "kept as written, as GnuCash keeps it");
    assert!(
        expenses.is_hidden() && expenses.is_placeholder() && !expenses.is_tax_related(),
        "Tax Info is never used"
    );
    let food = f.book.account_by_full_name("Expenses:Food").unwrap();
    assert!(!food.is_hidden() && food.code.is_none() && food.notes().is_none() && food.stored_color().is_none());
    let acme = f.book.account_by_full_name("Assets:ACME").unwrap();
    assert_eq!(
        (acme.commodity.clone(), acme.commodity_scu),
        (Some(CommodityRef::new("NASDAQ", "ACME")), Some(1))
    );
}

#[test]
fn existing_accounts_get_only_non_empty_code_description_notes_and_colour() {
    let mut f = fx();
    let p = proposal(
        &f,
        &format!(
            "{HEADER}CASH,Assets:Bank,Bank,1010,Main account,#a0b0c0,,EUR,CURRENCY,T,F,T\nASSET,Assets,Assets,,,,,USD,CURRENCY,F,F,F\n"
        ),
    );
    assert_eq!(
        actions(&p),
        [("Assets:Bank", "update"), ("Assets", "skip")],
        "a row that changes nothing is skipped"
    );
    let bank = &p.accounts[0];
    assert_eq!(bank.account, Some(f.bank));
    let fields: Vec<(&str, Option<&str>, &str)> = bank.changes.iter().map(|c| (c.field, c.from.as_deref(), c.to.as_str())).collect();
    assert_eq!(
        fields,
        [
            ("code", None, "1010"),
            ("description", None, "Main account"),
            ("color", None, "#a0b0c0")
        ]
    );
    assert_eq!(bank.warnings[0].code, "not-updated");
    assert!(
        bank.warnings[0].message.contains("type, commodity, hidden, placeholder differ"),
        "{}",
        bank.warnings[0].message
    );
    let applied = apply(&mut f.book, &mut f.env, &p, &[]).unwrap();
    assert_eq!((applied.accounts_updated.clone(), applied.skipped.clone()), (vec![f.bank], vec![1]));
    let account = f.book.account(f.bank).unwrap();
    assert_eq!(
        (account.account_type, account.code.as_deref(), account.stored_color()),
        (AccountType::Bank, Some("1010"), Some("#a0b0c0"))
    );
    assert!(!account.is_hidden() && !account.is_placeholder());
    let again = proposal(
        &f,
        &format!("{HEADER}BANK,Assets:Bank,Bank,1010,Main account,#a0b0c0,,USD,CURRENCY,F,F,F\n"),
    );
    assert_eq!(actions(&again), [("Assets:Bank", "skip")], "the same file again changes nothing");
    assert!(again.accounts[0].warnings.is_empty());
}

#[test]
fn a_colour_gnucash_cant_parse_leaves_the_colour_as_it_was() {
    let mut f = fx();
    let p = proposal(&f, &format!("{HEADER}BANK,Assets:Bank,Bank,,,#a0b0c0,,USD,CURRENCY,F,F,F\n"));
    apply(&mut f.book, &mut f.env, &p, &[]).unwrap();
    let p = proposal(
        &f,
        &format!("{HEADER}BANK,Assets:Bank,Bank,,,no colour,,USD,CURRENCY,F,F,F\nBANK,Assets:New,New,,,red,,USD,CURRENCY,F,F,F\n"),
    );
    assert_eq!(actions(&p), [("Assets:Bank", "skip"), ("Assets:New", "add")]);
    assert_eq!((p.accounts[0].warnings[0].code, p.accounts[1].warnings[0].code), ("color", "color"));
    apply(&mut f.book, &mut f.env, &p, &[]).unwrap();
    assert_eq!(
        f.book.account(f.bank).unwrap().stored_color(),
        Some("#a0b0c0"),
        "GnuCash would clear it"
    );
    assert_eq!(f.book.account_by_full_name("Assets:New").unwrap().stored_color(), None);
    [
        "#abc",
        "#aabbcc",
        "#aaabbbccc",
        "#aaaabbbbcccc",
        "rgb(1, 2, 3)",
        "rgba(1,2,3,0.5)",
        "rgb(10%,20%,30%)",
    ]
    .iter()
    .for_each(|c| assert!(is_gdk_color(c), "{c}"));
    ["#ab", "#gggggg", "rgb(1,2)", "rgb(a,b,c)", "rgba(1,2,3)", "red", ""]
        .iter()
        .for_each(|c| assert!(!is_gdk_color(c), "{c}"));
}

#[test]
fn every_error_branch() {
    let f = fx();
    let text = format!(
        "{HEADER}BANK,Assets:Savings,Bank,,,,,USD,CURRENCY,F,F,F\n\
         BANK,Nowhere:Savings,Savings,,,,,USD,CURRENCY,F,F,F\n\
         BANK,Assets:Euro,Euro,,,,,XXZ,CURRENCY,F,F,F\n\
         BANKING,Assets:Odd,Odd,,,,,USD,CURRENCY,F,F,F\n\
         INCOME,Assets:Pay,Pay,,,,,USD,CURRENCY,F,F,F\n\
         BANK,Assets:Short\n\
         \"BANK,Assets:Open\n"
    );
    let p = proposal(&f, &text);
    assert!(p.accounts.is_empty(), "{:?}", actions(&p));
    assert_eq!(
        messages(&p),
        [
            (2, "account Bank not in Assets:Savings"),
            (3, "path to account Savings not found"),
            (4, "commodity XXZ / CURRENCY not found"),
            (5, "account type \"BANKING\" is not one GnuCash knows"),
            (6, "a INCOME account cannot be placed under a ASSET account"),
            (7, "2 field(s), not the 12 of GnuCash's account export"),
            (8, "a quote that is never closed"),
        ]
    );
}

#[test]
fn rows_run_in_order_like_gnucash() {
    let mut f = fx();
    let text = format!(
        "{HEADER}EXPENSE,Food:Fruit,Fruit,,,,,USD,CURRENCY,F,F,F\nEXPENSE,Food,Food,,,,,USD,CURRENCY,F,F,F\n\
         EXPENSE,Food:Veg,Veg,,,,,USD,CURRENCY,F,F,F\nEXPENSE,Food,Food,6000,,,,USD,CURRENCY,F,F,F\n"
    );
    let p = proposal(&f, &text);
    assert_eq!(
        messages(&p),
        [(2, "path to account Fruit not found")],
        "the parent comes later in the file"
    );
    assert_eq!(
        actions(&p),
        [("Food", "add"), ("Food:Veg", "add"), ("Food", "update")],
        "a second row for Food updates it"
    );
    assert_eq!(p.accounts[2].account, None, "the account is not in the book yet");
    let applied = apply(&mut f.book.clone(), &mut f.env, &p, &[]).unwrap();
    assert_eq!(applied.accounts_created.len(), 2);
    assert!(
        applied.accounts_updated.is_empty(),
        "an account created by the import is reported once, as created"
    );
    let skip_food = [Change {
        index: 0,
        action: Some(ImportAction::Skip),
        ..Change::default()
    }];
    let e = apply(&mut f.book, &mut f.env, &p, &skip_food).unwrap_err();
    assert_eq!(e.to_string(), "line 4: path to account Veg not found");
    let e = apply(
        &mut f.book,
        &mut f.env,
        &p,
        &[Change {
            index: 2,
            action: Some(ImportAction::Add),
            ..Change::default()
        }],
    )
    .unwrap_err();
    assert_eq!(e.to_string(), "account row 2: update can only become skip, not add");
    assert!(
        apply(
            &mut f.book,
            &mut f.env,
            &p,
            &[Change {
                index: 9,
                ..Change::default()
            }]
        )
        .is_err()
    );
    assert!(
        apply(
            &mut f.book,
            &mut f.env,
            &p,
            &[Change {
                index: 0,
                account: Some(f.bank),
                ..Change::default()
            }]
        )
        .is_err()
    );
}

#[test]
fn parent_names() {
    assert_eq!(parent_full_name("A", "A"), Some(""));
    assert_eq!(parent_full_name("A:B:C", "C"), Some("A:B"));
    assert_eq!(parent_full_name("Assets:Bank", "Ass"), None);
    assert_eq!(parent_full_name(":C", "C"), None);
    assert_eq!(parent_full_name("A:B", "A:B"), Some(""));
}

proptest! {
    #[test]
    fn no_file_panics(text in "[A-Za-z:,;\"\n\r ]{0,200}") {
        let f = fx();
        [Separator::Comma, Separator::Semicolon, Separator::Colon].iter().for_each(|s| {
            let file = read(&text, *s, None, 0);
            let _ = propose(&f.book, &file.rows, 0);
        });
    }
}
