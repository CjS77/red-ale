//! The CSV exports on a hand-built Book, byte for byte. Each expectation cites the GnuCash 5.17 lines it follows;
//! `csv-exp/` is `5.17:gnucash/import-export/csv-exp/`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use red_ale_core::transaction::slot_paths;
use red_ale_core::{
    Account, AccountType, Book, Commodity, CommodityRef, Date, DomainError, Guid, Numeric, ReconcileState, SlotValue, Split, Time64,
    TimeZone, Transaction,
};
use red_ale_export::{CsvOptions, Formats, Layout, TransactionsRequest, export_transactions, export_tree};
use red_ale_locale::Locale;
use red_ale_report::settings::DateFormat;

fn guid(n: u8) -> Guid {
    let mut bytes = [0_u8; 16];
    bytes[15] = n;
    bytes[0] = 0xa0;
    Guid::from_bytes(bytes)
}

const ROOT: u8 = 1;
const ASSETS: u8 = 2;
const CHECKING: u8 = 3;
const BROKER: u8 = 4;
const EXPENSES: u8 = 5;
const FOOD: u8 = 6;
const RENT: u8 = 7;
const INCOME: u8 = 8;
const SALARY: u8 = 9;
const TRADING: u8 = 10;
const TRADING_ACME: u8 = 11;

fn usd() -> CommodityRef {
    CommodityRef::currency("USD")
}

fn acme() -> CommodityRef {
    CommodityRef::new("NASDAQ", "ACME")
}

fn add_account(book: &mut Book, n: u8, name: &str, t: AccountType, commodity: CommodityRef, parent: u8, edit: impl FnOnce(&mut Account)) {
    let mut a = Account::new(guid(n), name, t, Some(commodity), Some(guid(parent)));
    edit(&mut a);
    book.accounts.insert(a.guid, Arc::new(a));
}

fn n(num: i64, denom: i64) -> Numeric {
    Numeric::new(num, denom)
}

/// Local noon of a date in UTC, the time GnuCash posts at.
fn at(y: i32, m: u32, d: u32) -> Time64 {
    TimeZone::utc().noon(Date::from_ymd(y, m, d).unwrap())
}

fn split(id: u8, account: u8, value: Numeric, amount: Numeric) -> Split {
    Split::new(guid(id), guid(account), value, amount)
}

fn transaction(id: u8, posted: Time64, description: &str, splits: Vec<Split>) -> Transaction {
    Transaction {
        guid: guid(id),
        currency: usd(),
        num: None,
        date_posted: posted,
        date_entered: posted,
        description: Some(description.to_owned()),
        slots: red_ale_core::Frame::new(),
        splits,
    }
}

fn add(book: &mut Book, t: Transaction) {
    book.transactions.insert(t.guid, Arc::new(t));
}

/// The Book: a tree with codes, flags, colours, notes and awkward names, and four transactions: a reconciled two-split
/// one, a three-split one, a share purchase through a trading account, and a voided one.
#[allow(clippy::too_many_lines)]
fn book() -> Book {
    let mut book = Book::new(guid(0xf0), Account::new(guid(ROOT), "Root Account", AccountType::Root, None, None));
    book.commodities.insert(acme(), Arc::new(Commodity::new(acme(), Some("Acme Corp".into()), 10_000)));
    add_account(&mut book, ASSETS, "Assets", AccountType::Asset, usd(), ROOT, |a| {
        a.code = Some("1000".into());
        a.set_placeholder(true);
    });
    add_account(&mut book, CHECKING, "Checking", AccountType::Bank, usd(), ASSETS, |a| {
        a.description = Some("Main, everyday".into());
        a.set_color(Some("#ff0000"));
        a.slots.set("notes", SlotValue::String("line one\nline \"two\"".into()));
    });
    add_account(&mut book, BROKER, "Broker", AccountType::Stock, acme(), ASSETS, |_| {});
    add_account(&mut book, EXPENSES, "Expenses", AccountType::Expense, usd(), ROOT, |_| {});
    add_account(&mut book, RENT, "Rent", AccountType::Expense, usd(), EXPENSES, |_| {});
    add_account(&mut book, FOOD, "Food \"Fine\"", AccountType::Expense, usd(), EXPENSES, |_| {});
    add_account(&mut book, INCOME, "Income", AccountType::Income, usd(), ROOT, |_| {});
    add_account(&mut book, SALARY, "Salary", AccountType::Income, usd(), INCOME, |a| {
        a.set_hidden(true);
        a.set_tax_related(true);
    });
    add_account(&mut book, TRADING, "Trading", AccountType::Trading, usd(), ROOT, |_| {});
    add_account(&mut book, TRADING_ACME, "ACME", AccountType::Trading, acme(), TRADING, |_| {});

    let mut rent = transaction(20, at(2024, 1, 15), "Rent, January", vec![split(21, CHECKING, n(-123_456, 100), n(-123_456, 100)), split(22, RENT, n(123_456, 100), n(123_456, 100))]);
    rent.num = Some("101".into());
    rent.slots.set(slot_paths::NOTES, SlotValue::String("paid \"late\"".into()));
    rent.splits[0].reconcile_state = ReconcileState::Reconciled;
    rent.splits[0].reconcile_date = Some(at(2024, 1, 20));
    rent.splits[0].memo = Some("cheque\n101".into());
    rent.splits[1].reconcile_state = ReconcileState::Cleared;
    add(&mut book, rent);

    let mut pay = transaction(30, at(2024, 2, 1), "Pay", vec![
        split(31, SALARY, n(-350_000, 100), n(-350_000, 100)),
        split(32, CHECKING, n(300_000, 100), n(300_000, 100)),
        split(33, FOOD, n(50_000, 100), n(50_000, 100)),
    ]);
    pay.splits[1].action = Some("Deposit".into());
    add(&mut book, pay);

    let buy = transaction(40, at(2024, 3, 10), "Buy ACME", vec![
        split(41, BROKER, n(100_000, 100), n(30_000, 10_000)),
        split(42, CHECKING, n(-100_000, 100), n(-100_000, 100)),
        split(43, TRADING_ACME, n(-100_000, 100), n(-30_000, 10_000)),
        split(44, TRADING, n(100_000, 100), n(100_000, 100)),
    ]);
    add(&mut book, buy);

    let mut void = transaction(50, at(2024, 4, 1), "Lunch", vec![split(51, CHECKING, n(0, 100), n(0, 100)), split(52, FOOD, n(0, 100), n(0, 100))]);
    void.slots.set(slot_paths::VOID_REASON, SlotValue::String("typo".into()));
    void.slots.set(slot_paths::VOID_TIME, SlotValue::String("2024-04-02 10:00:00 +0000".into()));
    [(0, -1250), (1, 1250)].into_iter().for_each(|(i, cents)| {
        let s = &mut void.splits[i];
        s.reconcile_state = ReconcileState::Voided;
        s.slots.set(slot_paths::VOID_FORMER_AMOUNT, SlotValue::Numeric(n(cents, 100)));
        s.slots.set(slot_paths::VOID_FORMER_VALUE, SlotValue::Numeric(n(cents, 100)));
    });
    add(&mut book, void);
    book
}

fn request(accounts: &[u8], layout: Layout) -> TransactionsRequest {
    TransactionsRequest { accounts: accounts.iter().map(|a| guid(*a)).collect(), from: None, to: None, layout }
}

fn export(book: &Book, r: &TransactionsRequest, options: &CsvOptions) -> red_ale_export::TransactionsCsv {
    export_transactions(book, &TimeZone::utc(), &Formats::default(), options, r).unwrap()
}

/// `csv-exp/csv-tree-export.cpp:59-95`: the header, then every account but the root in Display order (code, then type
/// rank, then name: `5.17:libgnucash/engine/Account.cpp:2363-2413`), with `T`/`F` flags and quoting where a field holds
/// the separator, a quote or a newline (`csv-exp/csv-export-helpers.cpp:48-84`).
#[test]
fn account_tree() {
    let tree = export_tree(&book(), &CsvOptions::default());
    let expected = concat!(
        "Type,Full Account Name,Account Name,Account Code,Description,Account Color,Notes,Symbol,Namespace,Hidden,Tax Info,Placeholder\r\n",
        "INCOME,Income,Income,,,,,USD,CURRENCY,F,F,F\r\n",
        "INCOME,Income:Salary,Salary,,,,,USD,CURRENCY,T,T,F\r\n",
        "EXPENSE,Expenses,Expenses,,,,,USD,CURRENCY,F,F,F\r\n",
        "EXPENSE,\"Expenses:Food \"\"Fine\"\"\",\"Food \"\"Fine\"\"\",,,,,USD,CURRENCY,F,F,F\r\n",
        "EXPENSE,Expenses:Rent,Rent,,,,,USD,CURRENCY,F,F,F\r\n",
        "TRADING,Trading,Trading,,,,,USD,CURRENCY,F,F,F\r\n",
        "TRADING,Trading:ACME,ACME,,,,,ACME,NASDAQ,F,F,F\r\n",
        "ASSET,Assets,Assets,1000,,,,USD,CURRENCY,F,F,T\r\n",
        "STOCK,Assets:Broker,Broker,,,,,ACME,NASDAQ,F,F,F\r\n",
        "BANK,Assets:Checking,Checking,,\"Main, everyday\",#ff0000,\"line one\nline \"\"two\"\"\",USD,CURRENCY,F,F,F\r\n",
    );
    assert_eq!(tree.csv, expected);
    assert_eq!(tree.rows, 10);
}

/// "Use quotes" quotes every field, and a custom separator of several characters joins them
/// (`csv-exp/assistant-csv-export.c:203-315`).
#[test]
fn account_tree_with_quotes_and_a_custom_separator() {
    let options = CsvOptions::new(" | ", true).unwrap();
    let tree = export_tree(&book(), &options);
    let second = tree.csv.split("\r\n").nth(1).unwrap();
    assert_eq!(second, "\"INCOME\" | \"Income\" | \"Income\" | \"\" | \"\" | \"\" | \"\" | \"USD\" | \"CURRENCY\" | \"F\" | \"F\" | \"F\"");
}

/// The simple layout of Checking (`csv-exp/csv-transactions-export.cpp:210-227`): one line per transaction; Account Name
/// is the full name; Full Category Path is the other split's account, or the placeholder when there is not exactly one
/// other split, trading splits not counted (`5.17:libgnucash/engine/Split.cpp:2160-2185`). Amounts print with the
/// account's print info, Num. without separators; the rate is the share price with the price print info of the
/// account's commodity, 0 for a void transaction, whose former amounts are shown (`:161-192`).
#[test]
fn simple_layout() {
    let out = export(&book(), &request(&[CHECKING], Layout::Simple), &CsvOptions::default());
    let expected = concat!(
        "Date,Account Name,Number,Description,Full Category Path,Reconcile,Amount With Sym,Amount Num.,Value With Sym,Value Num.,Rate/Price\r\n",
        "01/15/24,Assets:Checking,101,\"Rent, January\",Expenses:Rent,y,\"-$1,234.56\",-1234.56,\"-$1,234.56\",-1234.56,1.00\r\n",
        "02/01/24,Assets:Checking,,Pay,-- Split Transaction --,n,\"$3,000.00\",3000.00,\"$3,000.00\",3000.00,1.00\r\n",
        "03/10/24,Assets:Checking,,Buy ACME,Assets:Broker,n,\"-$1,000.00\",-1000.00,\"-$1,000.00\",-1000.00,1.00\r\n",
        "04/01/24,Assets:Checking,,Lunch,\"Expenses:Food \"\"Fine\"\"\",v,-$12.50,-12.50,-$12.50,-12.50,0.00\r\n",
    );
    assert_eq!(out.csv, expected);
    assert_eq!((out.transactions, out.lines), (4, 4));
}

/// The complex layout of Broker (`csv-exp/csv-transactions-export.cpp:229-253`, `:299-315`): one line per split in
/// stored order, trading splits left out because Broker is not a trading account. The share price prints with the
/// price print info of each split's own account commodity: ACME's fraction 10000 pads to four places; a value that is
/// not a decimal prints as `w + n/d` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1227-1247`).
#[test]
fn complex_layout_leaves_out_trading_splits() {
    let out = export(&book(), &request(&[BROKER], Layout::Complex), &CsvOptions::default());
    let expected = concat!(
        "Date,Transaction ID,Number,Description,Notes,Commodity/Currency,Void Reason,Action,Memo,Full Account Name,Account Name,",
        "Amount With Sym,Amount Num.,Value With Sym,Value Num.,Reconcile,Reconcile Date,Rate/Price\r\n",
        "03/10/24,a0000000000000000000000000000028,,Buy ACME,,CURRENCY::USD,,,,Assets:Broker,Broker,3. ACME,3.,\"$1,000.00\",1000.00,n,,333 + 1/3\r\n",
        "03/10/24,a0000000000000000000000000000028,,Buy ACME,,CURRENCY::USD,,,,Assets:Checking,Checking,\"-$1,000.00\",-1000.00,\"-$1,000.00\",-1000.00,n,,1.00\r\n",
    );
    assert_eq!(out.csv, expected);
    assert_eq!((out.transactions, out.lines), (1, 2));
}

/// Exporting a trading account keeps the trading splits (`csv-exp/csv-transactions-export.cpp:305-309`, `:332`).
#[test]
fn a_trading_account_keeps_trading_splits() {
    let out = export(&book(), &request(&[TRADING], Layout::Complex), &CsvOptions::default());
    assert_eq!(out.lines, 4, "{}", out.csv);
    assert!(out.csv.contains(",Trading:ACME,ACME,-3. ACME,-3.,\"-$1,000.00\",-1000.00,n,,333 + 1/3\r\n"), "{}", out.csv);
}

/// Complex lines carry notes, action, memo (newlines raw inside quotes), the reconcile date of a reconciled split only,
/// and a void transaction's reason, former amounts and former value over former amount to six significant figures
/// (`csv-exp/csv-transactions-export.cpp:70-80`, `:194-206`).
#[test]
fn complex_layout_of_checking() {
    let out = export(&book(), &request(&[CHECKING], Layout::Complex), &CsvOptions::default());
    let lines: Vec<&str> = out.csv.split("\r\n").collect();
    assert_eq!(
        lines[1],
        "01/15/24,a0000000000000000000000000000014,101,\"Rent, January\",\"paid \"\"late\"\"\",CURRENCY::USD,,,\"cheque\n101\",Assets:Checking,Checking,\"-$1,234.56\",-1234.56,\"-$1,234.56\",-1234.56,y,01/20/24,1.00"
    );
    assert_eq!(
        lines[2],
        "01/15/24,a0000000000000000000000000000014,101,\"Rent, January\",\"paid \"\"late\"\"\",CURRENCY::USD,,,,Expenses:Rent,Rent,\"$1,234.56\",1234.56,\"$1,234.56\",1234.56,c,,1.00"
    );
    assert_eq!(lines[4], "02/01/24,a000000000000000000000000000001e,,Pay,,CURRENCY::USD,,Deposit,,Assets:Checking,Checking,\"$3,000.00\",3000.00,\"$3,000.00\",3000.00,n,,1.00");
    assert_eq!(lines[8], "04/01/24,a0000000000000000000000000000032,,Lunch,,CURRENCY::USD,typo,,,Assets:Checking,Checking,-$12.50,-12.50,-$12.50,-12.50,v,,1.00");
    assert_eq!((out.transactions, out.lines), (4, 9));
}

/// A transaction is written once, under the first account it is met in, in the order the accounts are given
/// (`csv-exp/csv-transactions-export.cpp:276-278`, `:400-401`); sub-accounts are not included.
#[test]
fn each_transaction_once_under_the_first_account() {
    let out = export(&book(), &request(&[FOOD, CHECKING, EXPENSES], Layout::Simple), &CsvOptions::default());
    let accounts: Vec<&str> = out.csv.lines().skip(1).map(|l| l.split(',').nth(1).unwrap()).collect();
    assert_eq!(accounts, ["\"Expenses:Food \"\"Fine\"\"\"", "\"Expenses:Food \"\"Fine\"\"\"", "Assets:Checking", "Assets:Checking"]);
    assert_eq!(out.transactions, 4);
}

/// From local 00:00:00 of `from` to local 23:59:59 of `to`, inclusive (`csv-exp/assistant-csv-export.c:483-515`).
#[test]
fn date_range() {
    let mut r = request(&[CHECKING], Layout::Simple);
    r.from = Date::from_ymd(2024, 2, 1);
    r.to = Date::from_ymd(2024, 3, 10);
    let out = export(&book(), &r, &CsvOptions::default());
    let dates: Vec<&str> = out.csv.lines().skip(1).map(|l| &l[..8]).collect();
    assert_eq!(dates, ["02/01/24", "03/10/24"]);
    // At UTC+14 the posting at 12:00 UTC on 2024-03-10 is on the 11th locally, after the range.
    let zone = TimeZone::fixed(14 * 3600).unwrap();
    let out = export_transactions(&book(), &zone, &Formats::default(), &CsvOptions::default(), &r).unwrap();
    assert_eq!(out.csv.lines().skip(1).map(|l| &l[..8]).collect::<Vec<_>>(), ["02/02/24"]);
}

/// The num field source renames two headers and nothing else (`csv-exp/csv-transactions-export.cpp:347-388`).
#[test]
fn num_field_source_renames_headers() {
    let mut b = book();
    red_ale_core::book_options::set_split_action_for_num(&mut b, true);
    let simple = export(&b, &request(&[CHECKING], Layout::Simple), &CsvOptions::default());
    assert!(simple.csv.starts_with("Date,Account Name,Transaction Number,Description,"));
    assert!(simple.csv.contains(",101,"));
    let complex = export(&b, &request(&[CHECKING], Layout::Complex), &CsvOptions::default());
    assert!(complex.csv.starts_with("Date,Transaction ID,Transaction Number,Description,Notes,Commodity/Currency,Void Reason,Number/Action,Memo,"));
}

/// Dates follow the date format and the Locale's `D_FMT`; numbers the Locale's separators.
#[test]
fn formats_follow_the_locale() {
    let formats = Formats::new(DateFormat::Locale, Locale::by_name("de_DE").unwrap());
    let out = export_transactions(&book(), &TimeZone::utc(), &formats, &CsvOptions::new(";", false).unwrap(), &request(&[RENT], Layout::Simple)).unwrap();
    let line = out.csv.lines().nth(1).unwrap();
    assert!(line.starts_with("15.01.2024;Expenses:Rent;101;Rent, January;Assets:Checking;c;"), "{line}");
    assert!(line.contains(";1234,56;"), "{line}");
    let iso = Formats::new(DateFormat::Iso, Locale::c());
    let out = export_transactions(&book(), &TimeZone::utc(), &iso, &CsvOptions::default(), &request(&[RENT], Layout::Simple)).unwrap();
    assert!(out.csv.lines().nth(1).unwrap().starts_with("2024-01-15,"));
}

#[test]
fn requests_are_checked() {
    let b = book();
    let refused = |r: &TransactionsRequest| export_transactions(&b, &TimeZone::utc(), &Formats::default(), &CsvOptions::default(), r).unwrap_err();
    assert!(matches!(refused(&request(&[], Layout::Simple)), DomainError::Invalid(_)));
    assert!(matches!(refused(&request(&[99], Layout::Simple)), DomainError::NotFound { .. }));
    let mut r = request(&[CHECKING], Layout::Simple);
    r.from = Date::from_ymd(2024, 2, 2);
    r.to = Date::from_ymd(2024, 2, 1);
    assert!(matches!(refused(&r), DomainError::Invalid(_)));
}
