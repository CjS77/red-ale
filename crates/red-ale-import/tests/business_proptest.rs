//! No input makes the business importers panic, and what a preview proposes always applies: arbitrary text, valid
//! files mutated, and arbitrary dates and numbers (`design/milestone3.md`, "Testing").

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use proptest::prelude::*;
use red_ale_core::business::{Address, AmountType, Customer, OwnerKind, TaxIncluded, TaxTableEntry, Vendor};
use red_ale_core::ops::business::contacts::{add_customer, add_vendor};
use red_ale_core::ops::business::documents::DocumentKind;
use red_ale_core::ops::business::terms::add_tax_table;
use red_ale_core::{Account, AccountType, Book, CommodityRef, Date, Env, FixedEnv, Frame, Numeric, Time64, TimeZone};
use red_ale_import::business::date::{DateContext, DateFormat, scan_date};
use red_ale_import::business::expr::{NumberFormat, parse_expression};
use red_ale_import::business::{AccountNames, Context, DocumentOptions, OwnerOptions, Records, Separator, apply, documents, owners};
use red_ale_locale::Locale;

const NOW: i64 = 1_719_835_200;

const SEPARATORS: [Separator; 4] = [
    Separator::Semicolon,
    Separator::Comma,
    Separator::SemicolonQuoted,
    Separator::CommaQuoted,
];

/// Bills, an invoice and Owners in every layout, to mutate.
const DOCUMENTS: &str = "B1;2024-03-01;V1;REF;notes;2024-03-02;Toner;Mat;Expenses:Office;2;4.5;;;;yes;no;VAT;2024-03-05;2024-04-01;Liabilities:Payable;memo;x\n\
;;;;;2024-03-03;Paper;;Expenses:Office;;1+2;%;=;5;t;1;;;;;;\n\
B2;2024-03-10;V1;;;;Taxi;;Expenses:Office;1;30;;;;;;;2024-03-11;bad;Liabilities:Payable;;\n\
I1;2024-03-10;C1;;\"q\"\"\";;Work;;Income:Sales;1;(3*4);$;>;1;;;;;;;;\n";

const OWNERS: &str = "id;company;name;addr1;addr2;addr3;addr4;phone;fax;email;notes;shipname;shipaddr1;shipaddr2;shipaddr3;shipaddr4;shipphone;shipfax;shipemail\n\
C1;Acme;Wile;1 Road;;;;555;;a@b;;Dock;;;;;;;\n\
;;Solo;;;;;;;;;;;;;;;;\n\
C1;Acme 2;;;;;;;;;;;;;;;;;\n";

fn book() -> Book {
    let mut env = FixedEnv::new(Time64(NOW), 1 << 90);
    let mut book = Book::new_empty(&mut env);
    let root = book.root;
    let mut add = |name: &str, ty, parent| {
        let a = Account::new(env.new_guid(), name, ty, Some(CommodityRef::currency("USD")), parent);
        let g = a.guid;
        book.accounts.insert(g, Arc::new(a));
        g
    };
    let expenses = add("Expenses", AccountType::Expense, Some(root));
    add("Office", AccountType::Expense, Some(expenses));
    let income = add("Income", AccountType::Income, Some(root));
    add("Sales", AccountType::Income, Some(income));
    let liabilities = add("Liabilities", AccountType::Liability, Some(root));
    add("Payable", AccountType::Payable, Some(liabilities));
    let tax = add("Tax", AccountType::Liability, Some(liabilities));
    let assets = add("Assets", AccountType::Asset, Some(root));
    add("Receivable", AccountType::Receivable, Some(assets));
    let usd = Some(CommodityRef::currency("USD"));
    let vendor = Vendor {
        guid: env.new_guid(),
        name: "Ink".into(),
        id: "V1".into(),
        addr: Address::default(),
        notes: String::new(),
        terms: None,
        tax_included: TaxIncluded::UseGlobal,
        active: true,
        currency: usd.clone(),
        use_tax_table: false,
        tax_table: None,
        slots: Frame::new(),
    };
    add_vendor(&mut book, vendor).unwrap();
    let customer = Customer {
        guid: env.new_guid(),
        name: "Acme".into(),
        id: "C1".into(),
        addr: Address::default(),
        ship_addr: Address::default(),
        notes: String::new(),
        terms: None,
        tax_included: TaxIncluded::UseGlobal,
        active: true,
        discount: Numeric::ZERO,
        credit: Numeric::ZERO,
        currency: usd,
        use_tax_table: false,
        tax_table: None,
        slots: Frame::new(),
    };
    add_customer(&mut book, customer).unwrap();
    let line = TaxTableEntry {
        account: Some(tax),
        amount: Numeric::from_int(20),
        kind: AmountType::Percent,
    };
    add_tax_table(&mut book, &mut env, "VAT", vec![line]).unwrap();
    book
}

/// Preview `text` in every layout and kind, then apply what each proposes to a copy: it must apply.
fn exercise(base: &Book, text: &str) {
    let locale = Locale::c();
    let zone = TimeZone::utc();
    let ctx = Context {
        book: base,
        locale: &locale,
        today: Date::from_ymd(2024, 7, 1).unwrap(),
        now: Time64(NOW),
        zone: &zone,
        file: 0,
        threshold: None,
    };
    SEPARATORS.iter().for_each(|separator| {
        [DocumentKind::Bill, DocumentKind::Invoice].iter().for_each(|kind| {
            let opts = DocumentOptions {
                kind: *kind,
                separator: *separator,
                date_format: DateFormat::Iso,
                account_names: AccountNames::Full,
                update_existing: true,
                auto_pay: true,
            };
            let found = documents::propose(text, &ctx, &opts).unwrap();
            let mut book = base.clone();
            let mut env = FixedEnv::new(Time64(NOW), 1 << 100);
            apply(&mut book, &mut env, &Records::Documents(found.documents), &[], None)
                .unwrap_or_else(|e| panic!("documents {text:?}: {e}"));
        });
        [OwnerKind::Customer, OwnerKind::Vendor].iter().for_each(|kind| {
            let opts = OwnerOptions {
                kind: *kind,
                separator: *separator,
                currency: CommodityRef::currency("USD"),
            };
            let found = owners::propose(text, &ctx, &opts).unwrap();
            let mut book = base.clone();
            let mut env = FixedEnv::new(Time64(NOW), 1 << 100);
            apply(&mut book, &mut env, &Records::Owners(found.owners), &[], None).unwrap_or_else(|e| panic!("owners {text:?}: {e}"));
        });
    });
}

/// One edit: replace, insert or delete at a position.
fn mutate(seed: &str, at: usize, edit: u8, text: &str) -> String {
    let chars: Vec<char> = seed.chars().collect();
    let at = at % (chars.len() + 1);
    let (head, tail) = chars.split_at(at);
    let head: String = head.iter().collect();
    let tail: String = tail.iter().collect();
    match edit % 3 {
        0 => format!("{head}{text}{}", tail.chars().skip(text.chars().count()).collect::<String>()),
        1 => format!("{head}{text}{tail}"),
        _ => format!("{head}{}", tail.chars().skip(1 + text.len() % 4).collect::<String>()),
    }
}

#[test]
fn the_seeds_import() {
    let base = book();
    exercise(&base, DOCUMENTS);
    exercise(&base, OWNERS);
    let locale = Locale::c();
    let zone = TimeZone::utc();
    let ctx = Context {
        book: &base,
        locale: &locale,
        today: Date::from_ymd(2024, 7, 1).unwrap(),
        now: Time64(NOW),
        zone: &zone,
        file: 0,
        threshold: None,
    };
    let bills = documents::propose(
        DOCUMENTS,
        &ctx,
        &DocumentOptions {
            date_format: DateFormat::Iso,
            ..DocumentOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        bills.documents.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        ["B1", "B2"],
        "{:?}",
        bills.errors
    );
    assert!(bills.documents[0].will_post() && bills.documents[1].will_post());
    let opts = DocumentOptions {
        kind: DocumentKind::Invoice,
        date_format: DateFormat::Iso,
        ..DocumentOptions::default()
    };
    assert_eq!(documents::propose(DOCUMENTS, &ctx, &opts).unwrap().documents.len(), 1);
    let opts = OwnerOptions {
        kind: OwnerKind::Customer,
        separator: Separator::Semicolon,
        currency: CommodityRef::currency("USD"),
    };
    assert_eq!(owners::propose(OWNERS, &ctx, &opts).unwrap().owners.len(), 4);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn arbitrary_text(text in "(?s).{0,300}") {
        exercise(&book(), &text);
    }

    #[test]
    fn mutated_files(seed in 0..2_usize, at in 0..1000_usize, edit in any::<u8>(), text in "[;,\"\\n0-9a-zA-Z:.%=>()*/+ -]{0,12}") {
        let seed = [DOCUMENTS, OWNERS][seed];
        exercise(&book(), &mutate(seed, at, edit, &text));
    }

    #[test]
    fn dates_and_numbers(text in "(?s).{0,40}", format in 0..5_usize) {
        let locale = Locale::by_name("fr_FR").unwrap();
        let formats = [DateFormat::Us, DateFormat::Uk, DateFormat::Ce, DateFormat::Iso, DateFormat::Locale];
        let _ = scan_date(&text, &DateContext::new(formats[format], &locale, Date::from_ymd(2024, 7, 1).unwrap()));
        let _ = parse_expression(&text, NumberFormat::default());
        let _ = parse_expression(&text, NumberFormat { decimal_point: ',', group_separator: ' ' });
    }
}
