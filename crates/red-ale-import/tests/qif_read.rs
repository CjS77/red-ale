//! Reading QIF files and parsing their fields: hand-worked cases, each citing the lines of
//! `5.17:gnucash/import-export/qif-imp/qif-file.scm` (or the file named) it follows, and the example files of
//! `fixtures/qif/` (GnuCash's `doc/examples/`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::{AccountType, Numeric};
use red_ale_import::qif::{
    Action, Cleared, DateFormat, Field, FieldKind, FieldProblem, Format, NumberFormat, ParseFieldsReport, QifDate, QifFile, QifSplit,
    QifXtn, ReadError, ReadOptions, ReadResult, ReadWarningKind, RecordKind, check_from_acct, parse_fields, read_file, reparse_dates,
    reparse_price_dates,
};

fn read(text: &str) -> ReadResult {
    read_file(text, &ReadOptions::default()).unwrap()
}

fn parsed(text: &str) -> (QifFile, ParseFieldsReport) {
    let mut file = read(text).file;
    let report = parse_fields(&mut file).unwrap();
    (file, report)
}

fn n(num: i64, denom: i64) -> Numeric {
    Numeric::new(num, denom)
}

fn value<T: Clone>(field: Option<&Field<T>>) -> Option<T> {
    field.and_then(Field::value).cloned()
}

fn date(day: u32, month: u32, year: i64) -> QifDate {
    QifDate { day, month, year }
}

fn amount(split: &QifSplit) -> Option<Numeric> {
    value(split.amount.as_ref())
}

/// The bank transaction fields. `qif-file.scm:261-331`, `:360-392`.
#[test]
fn bank_transaction_fields() {
    let text = "!Type:Bank\nD12/31/1999\nT-1,234.50\nPGrocer\nA1 Main St\nASpringfield\nN1001\nC*\nMweekly\nLFood:Groceries/Home\n^\n";
    let (file, report) = parsed(text);
    assert!(report.errors.is_empty());
    let [xtn] = file.xtns.as_slice() else { panic!("one transaction") };
    assert_eq!(xtn.line, 2);
    assert_eq!(value(xtn.date.as_ref()), Some(date(31, 12, 1999)));
    assert_eq!(xtn.payee.as_deref(), Some("Grocer"));
    // Each A line is appended after a newline, the first too (:286-292).
    assert_eq!(xtn.address.as_deref(), Some("\n1 Main St\nSpringfield"));
    assert_eq!(xtn.number.as_deref(), Some("1001"));
    assert_eq!(value(xtn.cleared.as_ref()), Some(Cleared::Cleared));
    assert_eq!(xtn.default_split, None);
    let [split] = xtn.splits.as_slice() else { panic!("one split") };
    assert_eq!(
        (split.category.as_str(), split.class.as_str(), split.category_is_account),
        ("Food:Groceries", "Home", false)
    );
    assert_eq!(split.memo.as_deref(), Some("weekly"));
    assert_eq!(amount(split), Some(n(-246_900, 200)));
    // No account record and no opening balance: the account is unknown and GnuCash asks for one (:631-640).
    assert_eq!(xtn.from_acct, None);
    assert!(!check_from_acct(&file));
}

/// `U` overrides `T`, `T` does not override `U`, and an amount starting `...` is skipped, so the split has none and
/// `parse-numbers/format` gives it zero. `qif-file.scm:139-143`, `:268-276`, `:55-59`; `qif-parse.scm:469-471`.
#[test]
fn total_lines() {
    let (file, _) = parsed("!Type:Bank\nD1/2/2003\nU1,000.00\nT1.00\n^\nD1/2/2003\nT5\n^\nD1/2/2003\nT...\n^\n");
    let amounts: Vec<_> = file.xtns.iter().map(|x| amount(&x.splits[0])).collect();
    assert_eq!(amounts, vec![Some(n(1000, 1)), Some(n(5, 1)), Some(Numeric::ZERO)]);
}

/// A transaction without a date is dropped with a warning, and line numbers count a CR LF once.
/// `qif-file.scm:383-387`.
#[test]
fn transaction_without_date() {
    let result = read("!Type:Bank\r\nT1.00\r\n^\r\nD1/1/2001\r\n^\r\n");
    assert_eq!(result.file.xtns.len(), 1);
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(
        (result.warnings[0].line, &result.warnings[0].kind),
        (3, &ReadWarningKind::DateRequired)
    );
    assert_eq!(
        result.warnings[0].to_string(),
        "Line 3: Date required. Discarding this transaction."
    );
}

/// A credit card file whose account record comes first: a first transaction to that same account is its opening
/// balance, from the equity account, whatever the payee. `qif-file.scm:422-444`, `:602-629`.
#[test]
fn credit_card_opening_balance_to_known_account() {
    let text =
        "!Account\nNVisa\nTCCard\nL5,000.00\n^\n!Type:CCard\nD1/5/2004\nT-250.00\nPStart\nL[Visa]\n^\nD1/9/2004\nT-12.00\nLDining\n^\n";
    let (file, _) = parsed(text);
    let acct = &file.accounts[0];
    assert_eq!(acct.name, "Visa");
    assert_eq!(acct.acct_type, Field::Value(vec![AccountType::Credit]));
    assert_eq!(value(acct.limit.as_ref()), Some(n(5000, 1)));
    let opening = &file.xtns[0].splits[0];
    assert_eq!(
        (opening.category.as_str(), opening.category_is_account),
        ("Equity:Retained Earnings", true)
    );
    assert!(file.xtns.iter().all(|x| x.from_acct.as_deref() == Some("Visa")));
    assert_eq!(file.xtns[1].splits[0].category, "Dining");
}

/// Microsoft Money's form: no account record, a first transaction with payee "Opening Balance" to `[account]`
/// names the account. The equity account name comes from the options. `qif-file.scm:612-617`.
#[test]
fn opening_balance_names_the_account() {
    let text = "!Type:Bank\nD12/03/95\nT4,706.57\nPOpening Balance \nL[New Bank]\n^\nD12/04/95\nT-5\nLFees\n^\n";
    let options = ReadOptions {
        default_equity_account: "Capitaux propres:Bénéfices".to_owned(),
        ..ReadOptions::default()
    };
    let file = read_file(text, &options).unwrap().file;
    assert_eq!(file.xtns[0].splits[0].category, "Capitaux propres:Bénéfices");
    assert!(file.xtns.iter().all(|x| x.from_acct.as_deref() == Some("New Bank")));
    assert!(check_from_acct(&file));
}

/// Splits: `S`, `E` and `$` lines; the own split is kept aside and later `T`, `M` and `L` lines are ignored; splits
/// are newest first; split amounts are negated when they cancel the total. `qif-file.scm:328-358`,
/// `qif-objects.scm:449-475`.
#[test]
fn splits() {
    let text =
        "!Type:Bank\nD3/13/2002\nT-100.00\nPShop\nLHousehold\nSFood\nEbread\n$40.00\nS[Savings]/Kids\n$60.00\nT-1.00\nMlate memo\n^\n";
    let (file, report) = parsed(text);
    assert!(report.errors.is_empty());
    let xtn = &file.xtns[0];
    let own = xtn.default_split.as_ref().unwrap();
    assert_eq!(own.category, "Household");
    // The own split's amount is never parsed (qif-objects.scm:472-474), and the late M line did not reach it.
    assert_eq!(own.amount, Some(Field::text_of("-100.00")));
    assert_eq!(own.memo, None);
    let categories: Vec<_> = xtn
        .splits
        .iter()
        .map(|s| (s.category.as_str(), s.category_is_account, s.class.as_str()))
        .collect();
    assert_eq!(categories, vec![("Savings", true, "Kids"), ("Food", false, "")]);
    assert_eq!(xtn.splits[1].memo.as_deref(), Some("bread"));
    // -100 + 40 + 60 = 0, so the splits are negated.
    assert_eq!(
        xtn.splits.iter().map(amount).collect::<Vec<_>>(),
        vec![Some(n(-60, 1)), Some(n(-40, 1))]
    );
}

/// Splits whose amounts do not cancel the total keep their sign (divx.qif's paycheque). `qif-objects.scm:464-475`.
#[test]
fn splits_keep_their_sign() {
    let (file, _) = parsed("!Type:Bank\nD3/1/2002\nT100.00\nSA\n$40.00\nSB\n$60.00\n^\n");
    assert_eq!(
        file.xtns[0].splits.iter().map(amount).collect::<Vec<_>>(),
        vec![Some(n(60, 1)), Some(n(40, 1))]
    );
}

/// Investments: the action in `N`, with its aliases; `Y`, `I`, `Q`, `O`; a missing security becomes `""`; the
/// security list goes to the ticker map. `qif-file.scm:294-331`, `:375-378`, `:490-518`; `qif-parse.scm:194-239`.
#[test]
fn investments() {
    let text = "!Type:Security\nNAcme Corp\nSACM\nTStock\nGGrowth\n^\n!Type:Invst\nD1/15/2003\nNKauf\nYAcme Corp\nI12.50\nQ100\nO9.95\nT1,259.95\nCR\n^\nD1/16/2003\nNIntInc\nT3.20\n^\nD1/17/2003\nNGrant\nT1\n^\n";
    let result = read(text);
    assert_eq!(result.securities.len(), 1);
    assert_eq!(
        (result.securities[0].name.as_str(), result.securities[0].symbol.as_str()),
        ("Acme Corp", "ACM")
    );
    let mut file = result.file;
    let report = parse_fields(&mut file).unwrap();
    let buy = &file.xtns[0];
    assert_eq!(value(buy.action.as_ref()), Some(Action::Buy));
    assert_eq!(buy.number, None);
    assert_eq!(buy.security_name.as_deref(), Some("Acme Corp"));
    assert_eq!(value(buy.num_shares.as_ref()), Some(n(100, 1)));
    assert_eq!(value(buy.commission.as_ref()), Some(n(995, 100)));
    assert_eq!(value(buy.cleared.as_ref()), Some(Cleared::Reconciled));
    assert_eq!(value(buy.share_price.as_ref()), Some(n(25, 2)));
    assert_eq!(file.xtns[1].security_name.as_deref(), Some(""));
    assert_eq!(value(file.xtns[1].action.as_ref()), Some(Action::IntInc));
    assert_eq!(value(file.xtns[2].action.as_ref()), Some(Action::Unrecognized));
    let messages: Vec<_> = report.errors.iter().map(|e| (e.field, e.line, e.problem.clone())).collect();
    assert!(messages.contains(&(
        FieldKind::Action,
        Some(21),
        FieldProblem::Message("Unrecognized action 'Grant'.".to_owned())
    )));
}

/// `I`, `Q` and `$` values padded with spaces read as zero in GnuCash (Scheme's `string->number` refuses the space),
/// and red-ale reports them. swipe.qif and quicktest.qif have such lines. `qif-parse.scm:440-450`.
#[test]
fn padded_numbers_read_as_zero_and_are_reported() {
    let (file, report) = parsed("!Type:Invst\nD9/13/97\nNBuy     \nYHALCorp.\nI35        \nQ100\nT3,500.00\n^\n");
    assert_eq!(value(file.xtns[0].action.as_ref()), Some(Action::Buy));
    assert_eq!(value(file.xtns[0].share_price.as_ref()), Some(Numeric::ZERO));
    let unreadable: Vec<_> = report.errors.iter().map(|e| (e.field, e.line, e.problem.clone())).collect();
    assert_eq!(
        unreadable,
        vec![(FieldKind::SharePrice, Some(2), FieldProblem::Unreadable("35        ".to_owned()))]
    );
}

/// A price list: every line but `^` is a price; values are read by `gnc-numeric-from-string` and kept as Scheme
/// prints them; a bad line is a warning. `qif-file.scm:520-533`, `:1139-1165`.
#[test]
fn prices() {
    let text = "!Type:Prices\n\"ABC\",1 15/16,\"01/03/19\"\n^\n!Type:Prices\n\"DEF\",42.50,\" 1/20'00\"\n\"XYZ\",,\"1/1/01\"\n^\n";
    let result = read(text);
    assert_eq!(result.warnings.iter().map(|w| w.line).collect::<Vec<_>>(), vec![6]);
    let mut file = result.file;
    // Newest first.
    assert_eq!(
        file.prices.iter().map(|p| p.share_price.text().unwrap()).collect::<Vec<_>>(),
        vec!["85/2", "31/16"]
    );
    let report = parse_fields(&mut file).unwrap();
    assert!(report.errors.is_empty());
    let prices: Vec<_> = file
        .prices
        .iter()
        .map(|p| (p.symbol.clone(), value(Some(&p.share_price)), value(Some(&p.date))))
        .collect();
    assert_eq!(
        prices,
        vec![
            ("DEF".to_owned(), Some(n(85, 2)), Some(date(20, 1, 2000))),
            ("ABC".to_owned(), Some(n(31, 16)), Some(date(3, 1, 2019)))
        ]
    );
}

/// Memorised transactions are skipped, and the section after them is read. `qif-file.scm:227-230`, `:535-541`.
#[test]
fn memorised_transactions_are_skipped() {
    let text = "!Type:Memorized\nKC\nD1/1/2001\nT-10.00\nPRent\n^\n!Type:Bank\nD2/1/2001\nT-11.00\nPRent\n^\n";
    let result = read(text);
    assert!(result.warnings.is_empty());
    assert_eq!(result.file.xtns.len(), 1);
    assert_eq!(result.file.xtns[0].line, 8);
}

/// Classes and categories, with their flags, tax lines and budgets; an unknown line is a warning.
/// `qif-file.scm:395-487`.
#[test]
fn classes_and_categories() {
    let text =
        "!Type:Class\nNHome\nDHousehold\nRtax copy\nXnot a field\n^\n!Type:Cat\nNSalary\nDPay\nT\nI\nR4592\nB1.500,00\n^\nNRent\nE\n^\n";
    let result = read(text);
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(
        result.warnings[0].kind,
        ReadWarningKind::IgnoringClassLine("Xnot a field".to_owned())
    );
    let mut file = result.file;
    let report = parse_fields(&mut file).unwrap();
    assert!(report.errors.is_empty());
    assert_eq!(
        (file.classes[0].name.as_deref(), file.classes[0].description.as_deref()),
        (Some("Home"), Some("Household"))
    );
    // Newest first.
    let rent = &file.cats[0];
    assert_eq!(
        (rent.name.as_deref(), rent.expense_cat, rent.income_cat),
        (Some("Rent"), true, false)
    );
    let salary = &file.cats[1];
    assert_eq!(
        (salary.taxable, salary.income_cat, salary.description.as_deref()),
        (true, true, Some("Pay"))
    );
    assert_eq!(value(salary.tax_class.as_ref()), Some(n(4592, 1)));
    assert_eq!(value(salary.budget_amt.as_ref()), Some(n(1500, 1)));
}

/// `!Option:AutoSwitch` lists accounts without switching to them; the next transaction header takes the last one
/// named; after `!Clear:AutoSwitch` an account record switches again. `qif-file.scm:203-209`, `:236-240`,
/// `:426-444`.
#[test]
fn auto_switch_account_lists() {
    let text = "!Option:AutoSwitch\n!Account\nNChecking\nTBank\n^\nNSavings\nTBank\n^\n!Clear:AutoSwitch\n!Account\nNVisa\nTCCard\n^\n!Type:CCard\nD1/1/2001\nT-5\nLFees\n^\n";
    let file = read(text).file;
    assert_eq!(
        file.accounts.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        vec!["Visa", "Savings", "Checking"]
    );
    assert_eq!(file.xtns[0].from_acct.as_deref(), Some("Visa"));

    let text = "!Option:AutoSwitch\n!Account\nNChecking\nTBank\n^\nNSavings\nTBank\n^\n!Type:Bank\nD1/1/2001\nT-5\nLFees\n^\n";
    assert_eq!(read(text).file.xtns[0].from_acct.as_deref(), Some("Savings"));
}

/// divx.qif: auto-switch, then a first transaction to `[Checking]` in Checking is its opening balance; later account
/// records switch the account. `qif-file.scm:203-209`, `:602-629`.
#[test]
fn divx_fixture() {
    let (file, report) = parsed(&fixture("divx.qif"));
    assert!(report.errors.is_empty());
    let accounts: Vec<_> = file.xtns.iter().map(|x| x.from_acct.as_deref().unwrap()).collect();
    assert_eq!(
        accounts,
        vec!["Checking", "Checking", "Checking", "G Stock", "Schwab", "Schwab", "Schwab"]
    );
    assert_eq!(file.xtns[0].splits[0].category, "Equity:Retained Earnings");
    assert_eq!(value(file.xtns[0].date.as_ref()), Some(date(29, 3, 2000)));
    let paycheque = &file.xtns[2];
    assert_eq!(paycheque.splits.len(), 8);
    assert_eq!(amount(&paycheque.splits[7]), Some(n(183_369, 100)));
    let last = &file.xtns[6];
    assert_eq!(value(last.action.as_ref()), Some(Action::MiscIncX));
    let split = &last.splits[0];
    assert_eq!(
        (split.category.as_str(), split.miscx_category.as_deref(), split.miscx_is_account),
        ("C Inc:fedtax", Some("Schwab"), true)
    );
}

/// Crédit Lyonnais writes `!type bank`; unknown options are warned about and the section before them continues.
/// `qif-parse.scm:186-191`, `qif-file.scm:242-250`.
#[test]
fn header_variants() {
    let result = read("\u{feff}!type bank\nD1/1/2001\n!Option:MDY\nT-5\n^\n");
    assert_eq!(result.warnings[0].kind, ReadWarningKind::UnknownOption("option:mdy".to_owned()));
    assert_eq!(result.file.xtns[0].splits[0].amount, Some(Field::text_of("-5")));
}

/// A line before any header is not QIF; blank lines before it are fine. `qif-file.scm:535-541`.
#[test]
fn not_qif() {
    let error = read_file("\n   \nhello\n", &ReadOptions::default()).unwrap_err();
    assert_eq!(
        error,
        ReadError::NotQif {
            line: 3,
            text: "hello".to_owned()
        }
    );
    assert_eq!(error.to_string(), "Line 3: File does not appear to be in QIF format: hello");
}

/// When several date formats fit and read some date differently, the dates stay unparsed, the formats come back,
/// and `reparse-dates` parses them in the one chosen. `qif-file.scm:642-651`, `:1029-1046`, `:1120-1136`.
#[test]
fn ambiguous_dates() {
    let mut file = read("!Type:Bank\nD01/02/03\nT1\n^\nD04/05/06\nT2\n^\n").file;
    let report = parse_fields(&mut file).unwrap();
    assert_eq!(
        report.date_formats(),
        Some(vec![DateFormat::Mdy, DateFormat::Dmy, DateFormat::Ymd, DateFormat::Ydm])
    );
    assert_eq!(file.xtns[0].date, Some(Field::text_of("01/02/03")));
    // The amounts were still parsed.
    assert_eq!(amount(&file.xtns[1].splits[0]), Some(n(2, 1)));
    reparse_dates(&mut file, DateFormat::Dmy).unwrap();
    assert_eq!(value(file.xtns[0].date.as_ref()), Some(date(1, 2, 2003)));
}

/// Dates every format reads the same are not ambiguous. `qif-file.scm:1078-1117`.
#[test]
fn equivalent_date_formats() {
    let (file, report) = parsed("!Type:Bank\nD1/1/2001\nT1\n^\nD2/2/2001\nT1\n^\n");
    assert_eq!(report.date_formats(), None);
    assert_eq!(value(file.xtns[1].date.as_ref()), Some(date(2, 2, 2001)));
}

/// An ambiguous price date: GnuCash never reparses it; red-ale's `reparse_price_dates` does.
#[test]
fn ambiguous_price_dates() {
    let mut file = read("!Type:Prices\n\"A\",1,\"01/02/03\"\n^\n").file;
    let report = parse_fields(&mut file).unwrap();
    assert_eq!(report.errors[0].record, RecordKind::Price);
    assert_eq!(report.date_formats().map(|f| f.len()), Some(4));
    reparse_dates(&mut file, DateFormat::Mdy).unwrap();
    assert_eq!(file.prices[0].date, Field::text_of("01/02/03"));
    reparse_price_dates(&mut file, DateFormat::Mdy).unwrap();
    assert_eq!(file.prices[0].date, Field::Value(date(2, 1, 2003)));
}

/// Amounts both decimal and comma read differently are guessed as decimal; red-ale reports the guess.
/// `qif-file.scm:916-927`, `:1035-1038`.
#[test]
fn guessed_number_format() {
    let (file, report) = parsed("!Type:Bank\nD1/1/2001\nT1,000\n^\nD1/2/2001\nT2,500\n^\n");
    assert_eq!(amount(&file.xtns[0].splits[0]), Some(n(1000, 1)));
    assert_eq!(report.guesses.len(), 1);
    let guess = &report.guesses[0];
    assert_eq!(
        (guess.field, guess.chosen),
        (FieldKind::SplitAmounts, Format::Number(NumberFormat::Decimal))
    );
    assert_eq!(
        guess.candidates,
        vec![Format::Number(NumberFormat::Decimal), Format::Number(NumberFormat::Comma)]
    );
    // Comma amounts settle it.
    let (file, report) = parsed("!Type:Bank\nD1/1/2001\nT1,000\n^\nD1/2/2001\nT2,50\n^\n");
    assert!(report.guesses.is_empty());
    assert_eq!(amount(&file.xtns[0].splits[0]), Some(n(1, 1)));
}

/// No format fits every amount: parsing stops there with an error. `qif-file.scm:1021-1027`, `:736-941`.
#[test]
fn inconsistent_number_formats() {
    let mut file = read("!Type:Bank\nD1/13/2001\nT1,000.50\n^\nD1/14/2001\nT1.000,50\n^\n").file;
    let error = parse_fields(&mut file).unwrap_err();
    let first = &error.report.errors[0];
    assert_eq!(
        (first.field, first.line, &first.problem),
        (FieldKind::SplitAmounts, Some(5), &FieldProblem::UnrecognizedFormat)
    );
    assert_eq!(
        error.to_string(),
        "Line 5: Transaction amount: Unrecognized or inconsistent format."
    );
    // The dates, parsed before the amounts, are parsed.
    assert!(file.xtns.iter().all(|x| x.date.as_ref().is_some_and(|d| d.value().is_some())));
}

/// A date no format reads stops parsing. GnuCash raises there; the file fails either way. `qif-file.scm:993-1027`.
#[test]
fn unreadable_date() {
    let mut file = read("!Type:Bank\nDyesterday\nT1\n^\nD1/1/2001\nT1\n^\n").file;
    let error = parse_fields(&mut file).unwrap_err();
    assert_eq!(error.report.errors[0].field, FieldKind::Date);
    assert_eq!(error.report.errors[0].line, Some(2));
}

/// A share price written as a mixed fraction fits no number format, so parsing stops. `qif-parse.scm:397-410`.
#[test]
fn mixed_fraction_share_price() {
    let mut file = read("!Type:Invst\nD1/15/2003\nNBuy\nI12 1/2\n^\n").file;
    let error = parse_fields(&mut file).unwrap_err();
    assert_eq!(error.report.errors[0].field, FieldKind::SharePrice);
}

/// Unknown account types, actions and reconciliation statuses are reported and replaced. `qif-parse.scm:159-260`.
#[test]
fn unknown_values_are_warnings() {
    let (file, report) = parsed("!Account\nNOdd\nTWeird\n^\n!Type:Bank\nD1/1/2001\nT1\nCq\n^\n");
    assert_eq!(file.accounts[0].acct_type, Field::Value(vec![AccountType::Bank]));
    assert_eq!(value(file.xtns[0].cleared.as_ref()), Some(Cleared::Unrecognized));
    assert_eq!(
        report.errors.iter().map(|e| e.field).collect::<Vec<_>>(),
        vec![FieldKind::AcctType, FieldKind::Cleared]
    );
}

fn fixture(name: &str) -> String {
    let bytes = std::fs::read(format!("{}/../../fixtures/qif/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    // The Money95 files are ISO-8859-1, whose bytes are the first 256 code points; the caller decodes, as a client
    // passes `encoding`.
    String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.iter().map(|b| char::from(*b)).collect())
}

/// Every example file of GnuCash's `doc/examples/` reads and parses. The counts were checked by hand against the
/// files.
#[test]
fn gnucash_example_files() {
    let expected = [
        ("abc-all.qif", 10, 7, 75, 2, 0),
        ("abc.qif", 7, 0, 0, 0, 0),
        ("bogus.qif", 8, 3, 88, 0, 0),
        ("cbb-export.qif", 9, 0, 0, 0, 0),
        ("divx.qif", 7, 3, 0, 0, 0),
        ("every.qif", 164, 17, 66, 1, 0),
        ("ms-money.qif", 347, 0, 0, 0, 0),
        ("Money95bank_fr.qif", 143, 0, 0, 0, 0),
        ("Money95invst_fr.qif", 10, 0, 0, 0, 0),
        ("Money95mfunds_fr.qif", 3, 0, 0, 0, 0),
        ("Money95stocks_fr.qif", 28, 0, 0, 0, 0),
        ("price.qif", 2, 3, 0, 0, 8),
        ("quicktest.qif", 47, 15, 66, 1, 0),
        ("swipe.qif", 11, 0, 0, 0, 0),
        ("web.qif", 7, 0, 0, 0, 0),
    ];
    expected.iter().for_each(|(name, xtns, accounts, cats, classes, prices)| {
        let mut file = read(&fixture(name)).file;
        let counts = (
            file.xtns.len(),
            file.accounts.len(),
            file.cats.len(),
            file.classes.len(),
            file.prices.len(),
        );
        assert_eq!(counts, (*xtns, *accounts, *cats, *classes, *prices), "{name}");
        let report = parse_fields(&mut file).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(report.date_formats(), None, "{name}");
        assert!(file.xtns.iter().all(|x| x.date.as_ref().and_then(Field::value).is_some()), "{name}");
    });
}

/// web.qif's dates run into the text after them; the year 099 is 1999. `qif-parse.scm:304-306`, `:143-144`.
#[test]
fn web_fixture_dates() {
    let (file, _) = parsed(&fixture("web.qif"));
    assert_eq!(value(file.xtns[0].date.as_ref()), Some(date(25, 3, 1999)));
}

/// The French Money 95 file, decoded from ISO-8859-1: its first transaction is an opening balance naming the
/// account, and accented categories come through.
#[test]
fn money95_fixture() {
    let (file, _) = parsed(&fixture("Money95bank_fr.qif"));
    assert!(file.xtns.iter().all(|x| x.from_acct.as_deref() == Some("Compte courant")));
    assert_eq!(file.xtns[1].splits[0].category, "Loisirs:Canal Plus-Câble");
    assert_eq!(amount(&file.xtns[0].splits[0]), Some(n(576_236, 100)));
}

/// price.qif: the third transaction has no `^` before the price list and is dropped; the bad price line is a
/// warning; prices parse.
#[test]
fn price_fixture() {
    let result = read(&fixture("price.qif"));
    assert_eq!(
        result.securities.iter().map(|s| s.symbol.as_str()).collect::<Vec<_>>(),
        vec!["ABC", "DEF", "GHI"]
    );
    assert_eq!(result.warnings.len(), 1);
    let mut file = result.file;
    parse_fields(&mut file).unwrap();
    let oldest = file.prices.back().unwrap();
    assert_eq!((oldest.symbol.as_str(), value(Some(&oldest.share_price))), ("DEF", Some(n(21, 20))));
    let sell: &QifXtn = &file.xtns[1];
    assert_eq!(value(sell.action.as_ref()), Some(Action::SellX));
    assert_eq!(value(sell.num_shares.as_ref()), Some(n(1_343_648_538, 1_000_000)));
}
