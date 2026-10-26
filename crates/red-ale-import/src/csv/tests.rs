//! Tests ported from GnuCash's `5.17:gnucash/import-export/csv-imp/test/test-tokenizer.cpp` (Copyright (C) 2015
//! Geert Janssens; GPL-2.0-or-later), hand-worked cases of `gnc-import-tx.cpp`, and property tests.

use proptest::prelude::*;
use red_ale_core::{Date, Numeric, ReconcileState};

use super::*;

const SAMPLE1: &str = include_str!("../../../../fixtures/csv/sample1.csv");

fn csv_layout(separators: &str, escape: bool) -> CsvLayout {
    CsvLayout { separators: separators.to_owned(), enable_escape: escape, ..CsvLayout::default() }
}

fn one_line(line: &str, separators: &str, escape: bool) -> Row {
    tokenize(line, &csv_layout(separators, escape)).unwrap().remove(0)
}

/// `test-tokenizer.cpp:102-117` (`load_file_existing`): the fixture's text, line endings normalised.
#[test]
fn gnucash_load_file_existing() {
    let expected = "Date,Num,Description,Notes,Account,Deposit,Withdrawal,Balance\n05/01/15,45,Acme Inc.,,Miscellaneous,,\"1,100.00\",\n";
    assert_eq!(tokenizer::normalize_line_endings(SAMPLE1), expected);
}

/// `test-tokenizer.cpp:119-135` (`tokenize_from_csv_file`).
#[test]
fn gnucash_tokenize_from_csv_file() {
    let rows = tokenize(SAMPLE1, &CsvLayout::default()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].len(), 8);
    assert_eq!(rows[1].len(), 8);
    assert_eq!(rows[0][0], "Date");
    assert_eq!(rows[1][6], "1,100.00");
}

/// `test-tokenizer.cpp:181-206` (`tokenize_comma_sep`): every case.
#[test]
fn gnucash_tokenize_comma_sep() {
    let cases: &[(bool, &str, &[&str])] = &[
        (true, "Date,Num,Description,Notes,Account,Deposit,Withdrawal,Balance", &["Date", "Num", "Description", "Notes", "Account", "Deposit", "Withdrawal", "Balance"]),
        (false, "Date,Num,Description,Notes,Account,Deposit,Withdrawal,Balance", &["Date", "Num", "Description", "Notes", "Account", "Deposit", "Withdrawal", "Balance"]),
        (
            true,
            "05/01/15,45,Typical csv import line - including quoted empty field,,Miscellaneous,\"\",\"1,100.00\",",
            &["05/01/15", "45", "Typical csv import line - including quoted empty field", "", "Miscellaneous", "", "1,100.00", ""],
        ),
        (
            false,
            "05/01/15,45,Typical csv import line - including quoted empty field,,Miscellaneous,\"\",\"1,100.00\",",
            &["05/01/15", "45", "Typical csv import line - including quoted empty field", "", "Miscellaneous", "", "1,100.00", ""],
        ),
        (true, "05/01/15,45,Acme Inc.,,Miscellaneous,", &["05/01/15", "45", "Acme Inc.", "", "Miscellaneous", ""]),
        (false, "05/01/15,45,Acme Inc.,,Miscellaneous,", &["05/01/15", "45", "Acme Inc.", "", "Miscellaneous", ""]),
        (true, "Test\\ with backslash,nextfield", &["Test\\ with backslash", "nextfield"]),
        (false, "Test\\ with backslash,nextfield", &["Test\\ with backslash", "nextfield"]),
        (true, "Test with \\\" escaped quote,nextfield", &["Test with \" escaped quote", "nextfield"]),
        (true, "Test with \"\" escaped quote,nextfield", &["Test with \" escaped quote", "nextfield"]),
        (false, "Test with \"\" escaped quote,nextfield", &["Test with \" escaped quote", "nextfield"]),
        (true, "\"Test (quoted) with \"\" escaped quote\",\"nextfield\"", &["Test (quoted) with \" escaped quote", "nextfield"]),
        (true, "\"Unescaped quote test\",nextfield", &["Unescaped quote test", "nextfield"]),
        (false, "\"Unescaped quote test\",nextfield", &["Unescaped quote test", "nextfield"]),
        (
            false,
            "\"Unescaped quote test with trailing backslash and escape off\\\",nextfield",
            &["Unescaped quote test with trailing backslash and escape off\\", "nextfield"],
        ),
    ];
    cases.iter().for_each(|&(escape, line, fields)| assert_eq!(one_line(line, ",", escape), fields, "{line} (escape {escape})"));
}

/// `test-tokenizer.cpp:208-217` (`tokenize_semicolon_sep`).
#[test]
fn gnucash_tokenize_semicolon_sep() {
    let cases: &[(&str, &[&str])] = &[
        ("Date;Num;Description;Notes;Account;Deposit;Withdrawal;Balance", &["Date", "Num", "Description", "Notes", "Account", "Deposit", "Withdrawal", "Balance"]),
        (
            "05/01/15;45;Typical csv import line - including quoted empty field;;Miscellaneous;\"\";\"1,100.00\";",
            &["05/01/15", "45", "Typical csv import line - including quoted empty field", "", "Miscellaneous", "", "1,100.00", ""],
        ),
        ("05/01/15;45;Acme Inc.;;Miscellaneous;", &["05/01/15", "45", "Acme Inc.", "", "Miscellaneous", ""]),
    ];
    cases.iter().for_each(|&(line, fields)| assert_eq!(one_line(line, ";", true), fields, "{line}"));
}

/// `test-tokenizer.cpp:250-268` (`tokenize_fw`).
#[test]
fn gnucash_tokenize_fw() {
    let cases: &[(&str, &[u32], &[&str])] = &[
        ("Date    NumDescriptionAccountDeposit", &[8, 3, 11, 7, 7], &["Date", "Num", "Description", "Account", "Deposit"]),
        ("05/01/1545Acme Inc.Miscellaneous1,100.00", &[8, 2, 9, 13, 8], &["05/01/15", "45", "Acme Inc.", "Miscellaneous", "1,100.00"]),
        (
            "05/01/15  45  Acme Inc.Miscellaneous        1,100.00",
            &[10, 4, 9, 13, 8, 8],
            &["05/01/15", "45", "Acme Inc.", "Miscellaneous", "", "1,100.00"],
        ),
    ];
    cases.iter().for_each(|&(line, widths, fields)| {
        let layout = CsvLayout { file_format: FileFormat::FixedWidth, column_widths: widths.to_vec(), ..CsvLayout::default() };
        assert_eq!(tokenize(line, &layout).unwrap().remove(0), fields, "{line}");
    });
}

fn ctx() -> ParseContext<'static> {
    ParseContext::new(Date::from_ymd(2026, 10, 26).unwrap())
}

fn import(text: &str, layout: &TxLayout) -> TxImport {
    TxImport::parse(tokenize(text, &layout.csv).unwrap(), layout, &ctx())
}

fn d(y: i32, m: u32, day: u32) -> Date {
    Date::from_ymd(y, m, day).unwrap()
}

/// sample1.csv with the bank-statement columns GnuCash's assistant would be given for it: dates `m-d-y`, Deposit and
/// Withdrawal as Amount and Amount (Negated), the header skipped.
#[test]
fn sample1_drafts() {
    use ColumnType::{Account, Amount, AmountNeg, Date as D, Description, None as N, Notes, Num};
    let mut layout = TxLayout { column_types: vec![D, Num, Description, Notes, Account, Amount, AmountNeg, N], ..TxLayout::default() };
    layout.csv.skip_start_lines = 1;
    layout.csv.date_format = DateFormat::Mdy;
    layout.csv.currency_format = CurrencyFormat::Period;
    let imp = import(SAMPLE1, &layout);
    assert!(imp.verify(true, false).is_empty());
    assert_eq!(imp.account_names(false).into_iter().collect::<Vec<_>>(), ["Miscellaneous"]);
    let drafts = imp.create_drafts(false).unwrap();
    assert_eq!(drafts.len(), 1);
    let t = &drafts[0];
    assert_eq!((t.date, t.num.as_deref(), t.description.as_deref(), t.notes.as_deref()), (d(2015, 5, 1), Some("45"), Some("Acme Inc."), None));
    assert_eq!(t.splits.len(), 1);
    let s = &t.splits[0];
    assert_eq!(s.account, SplitAccount::Named("Miscellaneous".to_owned()));
    assert!(s.amount.same_repr(Numeric::new(-110_000, 100)), "{}", s.amount);
    assert_eq!(s.value, None);
}

fn export_layout() -> TxLayout {
    TxLayout { csv: CsvLayout { currency_format: CurrencyFormat::Period, ..TxLayout::gnucash_export().csv }, ..TxLayout::gnucash_export() }
}

const EXPORT: &str = "\
Date,Transaction ID,Number,Description,Notes,Commodity/Currency,Void Reason,Action,Memo,Full Account Name,Account Name,Amount With Sym,Amount Num.,Value With Sym,Value Num.,Reconcile,Reconcile Date,Rate/Price
2024-01-05,aa,1,Groceries,,CURRENCY::USD,,,,Expenses:Food,Food,$12.50,12.50,$12.50,12.50,n,,1.00
,,,,,,,,,Assets:Cash,Cash,-$12.50,-12.50,-$12.50,-12.50,c,,1.00
2024-01-03,bb,,Pay,,CURRENCY::USD,,,note,Assets:Cash,Cash,$100.00,100.00,$100.00,100.00,y,2024-01-31,1.00
2024-01-03,bb,,Pay,,CURRENCY::USD,,,,Income:Salary,Salary,-$100.00,-100.00,-$100.00,-100.00,n,,1.00
";

/// Multi-split lines group by their transaction properties, drafts sort by date, and reconcile states follow
/// `trans_add_split`.
#[test]
fn multi_split_groups_lines() {
    let imp = import(EXPORT, &export_layout());
    assert_eq!(imp.verify(true, false), Vec::<String>::new());
    let drafts = imp.create_drafts(false).unwrap();
    assert_eq!(drafts.len(), 2);
    assert_eq!((drafts[0].unique_id.as_deref(), drafts[0].date, drafts[0].splits.len()), (Some("bb"), d(2024, 1, 3), 2));
    assert_eq!((drafts[1].unique_id.as_deref(), drafts[1].date, drafts[1].splits.len()), (Some("aa"), d(2024, 1, 5), 2));
    let pay = &drafts[0].splits[0];
    assert_eq!((pay.reconcile, pay.reconcile_date, pay.memo.as_deref()), (Some(ReconcileState::Reconciled), Some(d(2024, 1, 31)), Some("note")));
    assert!(pay.value.unwrap().same_repr(Numeric::new(10_000, 100)));
    assert_eq!(drafts[1].splits[1].reconcile, Some(ReconcileState::Cleared));
    assert_eq!(drafts[1].splits[0].reconcile, None);
    assert_eq!(drafts[1].commodity.as_deref(), Some("CURRENCY::USD"));
    let states = imp.line_states(false);
    assert_eq!(states.iter().map(|s| s.trans_line).collect::<Vec<_>>(), [0, 1, 1, 3, 3]);
    assert!(states[0].skip && states[1..].iter().all(|s| !s.skip));
}

/// A line whose own date can't be read still joins the transaction above it, with no error: it reports the
/// starting line's transaction errors (`gnc-import-tx.cpp:848-884`).
#[test]
fn continuation_line_hides_its_own_date_error() {
    let text = EXPORT.replacen(",,,,,,,,,Assets:Cash", "notadate,,,,,,,,,Assets:Cash", 1);
    let imp = import(&text, &export_layout());
    assert!(imp.lines()[2].pre_trans.errors().contains_key(&ColumnType::Date));
    assert!(imp.line_states(false)[2].errors.is_empty());
    assert_eq!(imp.create_drafts(false).unwrap()[1].splits.len(), 2);
}

/// GnuCash voids every draft with a void reason when it starts the next one, so the last is never voided
/// (`gnc-import-tx.cpp:646-660`).
#[test]
fn last_draft_is_never_voided() {
    use ColumnType::{Account, Amount, Date as D, Description, VoidReason};
    let layout = TxLayout { multi_split: true, column_types: vec![D, Description, VoidReason, Account, Amount], ..TxLayout::default() };
    let imp = import("2024-01-01,a,oops,X,1\n2024-01-02,b,,X,2\n2024-01-03,c,again,X,3\n", &layout);
    let drafts = imp.create_drafts(false).unwrap();
    assert_eq!(drafts.iter().map(|t| t.voided).collect::<Vec<_>>(), [true, false, false]);
    assert_eq!(drafts[2].void_reason.as_deref(), Some("again"));
}

/// Two-split mode: a base account replaces the Account column, every line is a transaction, and transfer columns
/// carry through.
#[test]
fn two_split_with_base_account_and_transfer() {
    use ColumnType::{Account, AmountNeg, Date as D, Description, TransferAccount, TransferAmount};
    let layout = TxLayout {
        base_account: Some(BaseAccount { guid: None, full_name: Some("Assets:Bank".to_owned()) }),
        column_types: vec![D, Description, Account, AmountNeg, TransferAccount, TransferAmount],
        ..TxLayout::default()
    };
    let imp = import("2024-02-01,Coffee,ignored,3.50,Expenses:Coffee,3.5\n", &layout);
    assert!(imp.uses_base_account());
    assert_eq!(imp.layout().column_types[2], ColumnType::None);
    let drafts = imp.create_drafts(false).unwrap();
    let split = &drafts[0].splits[0];
    assert_eq!(split.account, SplitAccount::Base);
    assert!(split.amount.same_repr(Numeric::new(-350, 100)));
    assert_eq!(split.transfer.account.as_deref(), Some("Expenses:Coffee"));
    assert!(split.transfer.amount.unwrap().same_repr(Numeric::new(35, 10)));
    let value = split.split_value(false, || None).unwrap();
    assert!(value.same_repr(Numeric::new(-35, 10)));
    assert!(split.transfer_amount(value, true, || None).unwrap().unwrap().same_repr(Numeric::new(35, 10)));
}

#[test]
fn values_from_price_and_price_db() {
    use ColumnType::{Account, Amount, Date as D, Description, Price};
    let layout = TxLayout { column_types: vec![D, Description, Account, Amount, Price], ..TxLayout::default() };
    let imp = import("2024-02-01,Buy,Assets:Shares,10,12.5\n2024-02-02,Buy,Assets:Shares,4,\n", &layout);
    let drafts = imp.create_drafts(false).unwrap();
    let priced = &drafts[0].splits[0];
    assert!(priced.split_value(false, || None).unwrap().same_repr(Numeric::new(1250, 10)));
    // An empty price cell is zero, so GnuCash multiplies by it.
    let zero_price = &drafts[1].splits[0];
    assert!(zero_price.split_value(false, || None).unwrap().is_zero());
    let unpriced = DraftSplit { price: None, ..zero_price.clone() };
    let quote = NearestPrice { value: Numeric::new(2, 1), in_transaction_currency: false };
    // Quoted in the account's commodity, so inverted: 4 × 1/2, unreduced as GncNumeric's product is.
    assert!(unpriced.split_value(false, || Some(quote)).unwrap().same_repr(Numeric::new(4, 2)));
    assert!(unpriced.split_value(false, || None).unwrap().is_zero());
    assert_eq!(unpriced.transfer_amount(Numeric::new(8, 1), false, || None), Ok(None));
}

#[test]
fn verify_messages() {
    let imp = import("a,b\n", &TxLayout::default());
    assert_eq!(imp.verify(true, false), [
        "Please select a date column.",
        "Please select an account column or set a base account in the Account field.",
        "Please select a description column.",
        "Please select a (negated) amount column.",
    ]);
    let empty = import("\n\n", &TxLayout::default());
    assert_eq!(empty.verify(true, false).len(), 1);
    let mut layout = TxLayout::default();
    layout.csv.skip_start_lines = 1;
    assert_eq!(import("a\n", &layout).verify(true, false), ["No lines are selected for importing. Please reduce the number of lines to skip."]);
    assert!(matches!(import("a,b\n", &TxLayout::default()).create_drafts(false), Err(CreateError::Verify(_))));
}

#[test]
fn line_errors_block_unless_skipped() {
    use ColumnType::{Account, Amount, Date as D, Description};
    let layout = TxLayout { column_types: vec![D, Description, Account, Amount], ..TxLayout::default() };
    let mut imp = import("2024-01-01,a,X,1\nbad,b,X,2\n2024-01-03,c,,3\n", &layout);
    let not_parsed = "Not all fields could be parsed. Please correct the issues reported for each line or adjust the lines to skip.";
    assert_eq!(imp.verify(true, false), [not_parsed]);
    assert_eq!(imp.line_states(false)[2].errors[&ColumnType::Account], "Account: Account value can't be empty.");
    // Account errors are allowed before accounts are matched.
    imp.line_mut(1).unwrap().pre_trans = imp.lines()[0].pre_trans.clone();
    assert!(imp.verify(false, false).is_empty());
    assert_eq!(imp.verify(true, false), [not_parsed]);
    imp.line_mut(0).unwrap().pre_split.add_error(ColumnType::Account, "Account value can't be mapped back to an account.");
    let drafts = imp.create_drafts(true).unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].description.as_deref(), Some("a"));
}

#[test]
fn skip_lines() {
    let mut layout = TxLayout::default();
    layout.csv.skip_start_lines = 1;
    layout.csv.skip_end_lines = 1;
    layout.csv.skip_alt_lines = true;
    let imp = import("0\n1\n2\n3\n4\n5\n", &layout);
    let skips: Vec<bool> = imp.line_states(false).iter().map(|s| s.skip).collect();
    assert_eq!(skips, [true, false, true, false, true, true]);
    // More end lines than lines wraps in GnuCash's unsigned arithmetic, and none are skipped at the end.
    layout.csv.skip_end_lines = 10;
    layout.csv.skip_alt_lines = false;
    assert!(import("0\n1\n2\n", &layout).line_states(false)[1..].iter().all(|s| !s.skip));
}

#[test]
fn column_types_dedupe_and_fit() {
    use ColumnType::{Amount, Date as D, Description};
    let layout = TxLayout { column_types: vec![D, D, Amount, Amount, Description, Description, Description], ..TxLayout::default() };
    let imp = import("2024-01-01,x,1,2,d,e\n", &layout);
    assert_eq!(imp.layout().column_types, [D, ColumnType::None, Amount, Amount, Description, ColumnType::None]);
    assert!(imp.lines()[0].pre_split.amount.unwrap().same_repr(Numeric::new(3, 1)));
}

proptest! {
    #[test]
    fn tokenizers_never_panic(text in "\\PC{0,200}", seps in "[,;\\t \"\\\\a-]{0,3}", escape: bool, widths in prop::collection::vec(0u32..6, 0..5)) {
        let _ = tokenize(&text, &CsvLayout { separators: seps, enable_escape: escape, ..CsvLayout::default() });
        let _ = tokenize(&text, &CsvLayout { file_format: FileFormat::FixedWidth, column_widths: widths, ..CsvLayout::default() });
    }

    #[test]
    fn csv_text_with_quotes_and_escapes_never_panics(text in "[a,\"\\\\\n\r n]{0,60}", escape: bool) {
        let _ = tokenize(&text, &csv_layout(",", escape));
    }

    #[test]
    fn cell_parsers_never_panic(text in "\\PC{0,40}", format in 0usize..3, date_format in 0usize..5) {
        let _ = parse_monetary(&text, CurrencyFormat::from_index(format).unwrap_or_default(), &MonetaryLocale::default());
        let dates = DateContext { today: Date::from_ymd(2026, 1, 1).unwrap(), locale_date: None };
        let _ = parse_date(&text, DateFormat::from_index(date_format).unwrap_or_default(), &dates);
    }

    #[test]
    fn numeric_like_cells_never_panic(text in "[-+()$., 0-9٣]{0,30}", format in 0usize..3) {
        let _ = parse_monetary(&text, CurrencyFormat::from_index(format).unwrap_or_default(), &MonetaryLocale::default());
    }

    #[test]
    fn decimal_amounts_parse_exactly(whole in 0i64..1_000_000_000, frac in 0u32..100) {
        let text = format!("{whole}.{frac:02}");
        let n = parse_monetary(&text, CurrencyFormat::Period, &MonetaryLocale::default()).unwrap();
        prop_assert!(n.same_repr(Numeric::new(whole * 100 + i64::from(frac), 100)));
    }

    #[test]
    fn imports_never_panic(text in "[0-9a-z,\\-/.\"\n]{0,200}", types in prop::collection::vec(0usize..25, 0..8), multi: bool, skip: bool) {
        let layout = TxLayout {
            multi_split: multi,
            column_types: types.into_iter().map(|i| ColumnType::ALL[i]).collect(),
            ..TxLayout::default()
        };
        if let Ok(rows) = tokenize(&text, &layout.csv) {
            let imp = TxImport::parse(rows, &layout, &ctx());
            let _ = imp.verify(true, skip);
            let _ = imp.account_names(skip);
            let _ = imp.create_drafts(skip);
        }
    }
}
