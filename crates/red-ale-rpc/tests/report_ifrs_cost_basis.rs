//! The IFRS weighted-average cost basis report (`standard/ifrs-cost-basis.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/ifrs-cost-basis.scm`, cited as IF) through `report.run` and `report.export`:
//! the checks of GnuCash's own test (`standard/test/test-ifrs-cost-basis.scm`, cited as TI) on
//! `fixtures/reports/ifrs-cost-basis.gnucash` and its SQLite copy, and hand-worked cases for the branches that test
//! does not reach, on the same book and on books built through the API.
//!
//! `ifrs-cost-basis` is `create-ifrs-cost-basis-test-data` (TI:54-243): fifteen SPY transactions in CAD from
//! 2019-07-01 to 2020-06-21, buying, selling, a return of capital, a 2:1 split, closing the long position, shorting and
//! covering. GnuCash's test reads each row's texts in order with empty cells giving none, so its lists do not say
//! which of the 28 columns a string sits in; the hand-worked checks below name their columns.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use serde_json::{Value, json};
use statements::{Setup, blocks, expected, expected_run, options_param, tables, test_settings};

const IFRS: &str = "IFRS weighted-average cost basis report";
const FIXTURE: &str = "ifrs-cost-basis.gnucash";

/// The fixture and its SQLite copy, each opened read-only, with the ISO dates TI:246 sets.
fn both_formats() -> [(&'static str, Setup); 2] {
    [
        ("xml", Setup::fixture(FIXTURE, &test_settings("iso"))),
        ("sqlite", Setup::sqlite_fixture(FIXTURE, &test_settings("iso"))),
    ]
}

/// The options of a run of `ifrs-cost-basis.expected.json`, its accounts (full names with `.`) given by GUID.
fn run_options(e: &Value, run: &Value) -> Value {
    let mut options = options_param(run);
    options["General"].as_object_mut().unwrap().values_mut().for_each(|v| {
        if let Some(guid) = v.as_str().and_then(|name| e["accounts"][name].as_str()) {
            *v = json!(guid);
        }
    });
    options
}

/// The options of TI:251-259.
fn test_options() -> Value {
    let e = expected("ifrs-cost-basis.expected.json");
    run_options(&e, expected_run(&e, "ifrs-cad"))
}

/// The test's options with `more` merged in, section by section.
fn with(mut options: Value, more: Value) -> Value {
    more.as_object().unwrap().iter().for_each(|(section, values)| {
        values
            .as_object()
            .unwrap()
            .iter()
            .for_each(|(k, v)| options[section][k] = v.clone());
    });
    options
}

/// A cell's text as GnuCash's test reads it: a text cell's text, else the display form; `""` for an empty cell.
fn cell_text(cell: &Value) -> String {
    if cell.is_null() {
        return String::new();
    }
    cell["text"].as_str().or_else(|| cell["display"].as_str()).unwrap().to_owned()
}

/// The data rows of the report's table.
fn data_rows(result: &Value) -> Vec<Value> {
    tables(result)[0]["rows"].as_array().unwrap().clone()
}

/// The texts of a row with empty cells left out, as `sxml->table-row-col` reads a row.
fn texts(row: &Value) -> Vec<String> {
    row["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(cell_text)
        .filter(|t| !t.is_empty())
        .collect()
}

/// The column labels of the table.
fn labels(result: &Value) -> Vec<String> {
    tables(result)[0]["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap().to_owned())
        .collect()
}

/// The text of row `row` (from 1) in the column labelled `label`, `""` when empty.
fn at(result: &Value, row: usize, label: &str) -> String {
    let column = labels(result)
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("no column {label}"));
    cell_text(&data_rows(result)[row - 1]["cells"][column])
}

/// Each `(label, text)` of row `row`, so a failure names the column.
fn assert_row(result: &Value, row: usize, want: &[(&str, &str)]) {
    let got: Vec<(&str, String)> = want.iter().map(|(l, _)| (*l, at(result, row, l))).collect();
    let want: Vec<(&str, String)> = want.iter().map(|(l, t)| (*l, (*t).to_owned())).collect();
    assert_eq!(got, want, "row {row}");
}

/// Every check of TI:264-357 on the XML fixture and its SQLite copy, through `report.run` and `report.export`.
#[test]
fn expected_figures_of_gnucash_test() {
    let e = expected("ifrs-cost-basis.expected.json");
    let run = expected_run(&e, "ifrs-cad");
    let checks = run["checks"].as_array().unwrap();
    for (format, mut s) in both_formats() {
        let options = run_options(&e, run);
        let result = s.run(IFRS, &options);
        let rows = data_rows(&result);
        assert_eq!(rows.len(), checks.len(), "{format}");
        let html = s.export(IFRS, &options);
        checks.iter().for_each(|c| {
            let row = usize::try_from(c["select"]["row"].as_u64().unwrap()).unwrap();
            let want: Vec<String> = c["expected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w.as_str().unwrap().to_owned())
                .collect();
            let (name, line) = (c["name"].as_str().unwrap(), &c["line"]);
            assert_eq!(texts(&rows[row - 1]), want, "{format}: {name} (test line {line})");
            want.iter()
                .for_each(|w| assert!(html.contains(w.as_str()), "{format}: {name} (test line {line}): {w} is in the HTML"));
        });
    }
}

/// The ported checks are every check of `ifrs-cost-basis.expected.json`, which has one run; its null test is
/// `the_null_test_renders`.
#[test]
fn every_check_of_the_expected_figures_is_ported() {
    let e = expected("ifrs-cost-basis.expected.json");
    let runs = e["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(
        (runs[0]["template"].as_str(), runs[0]["checks"].as_array().unwrap().len()),
        (Some(IFRS), 15)
    );
    let null_tests = e["renderOnly"].as_array().unwrap();
    assert_eq!((null_tests.len(), &null_tests[0]["line"]), (1, &json!(38)));
}

/// TI:38 renders the report with its defaults on an empty book: no STOCK or MUTUAL account, so no Stock Account and
/// the report is GnuCash's options warning under its first title, then the disclaimer (IF:325-331, :521).
#[test]
fn the_null_test_renders() {
    let mut s = Setup::new_book(&test_settings("iso"));
    let result = s.run(IFRS, &json!({}));
    assert_eq!(result["title"], "IFRS weighted average cost basis Report");
    let b = blocks(&result);
    let kinds: Vec<&str> = b.iter().map(|x| x["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["warning", "text", "text", "text"]);
    assert_eq!(b[0]["title"], "IFRS weighted-average cost basis report:");
    assert_eq!(b[0]["text"], "This report requires you to specify certain report options.");
    let html = s.export(IFRS, &json!({}));
    [
        "This report requires you to specify certain report options.",
        "This report is not designed with options reporting",
    ]
    .iter()
    .for_each(|w| assert!(html.contains(w), "{w} is in the HTML"));
}

/// The title with both dates and the report currency (IF:362-367), the 28 columns as GnuCash labels them
/// (IF:369-379), the units linked to the transaction's Stock Account split (IF:485), the rate as a price in the report
/// currency (IF:489), and the disclaimer's three paragraphs after the table (IF:33-50, :521). The Stock Account was
/// never set: it defaults to SPY, the book's first STOCK account.
#[test]
fn title_columns_links_and_disclaimer() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("iso"));
    let result = s.run(IFRS, &test_options());
    assert_eq!(
        result["title"],
        "Average-Cost (Basis) Report: From 2019-01-01 to 2021-01-01. Report-currency CAD"
    );
    assert_eq!(
        labels(&result),
        [
            "date",
            "description",
            "trans-units",
            "cumul-units",
            "note",
            "curr",
            "fx",
            "purchase-val",
            "purchase-cost",
            "cash-dividends",
            "proceeds-val",
            "proceeds-cost",
            "conv-purchase-val",
            "conv-purchase-cost",
            "conv-dividends",
            "conv-proceeds-val",
            "conv-proceeds-cost",
            "cumulative-average-cost-basis",
            "average-cost-basis/unit-for-sale",
            "average-cost-basis-of-sale",
            "net-proceeds",
            "gain-post-commission",
            "gain-pre-commission (calc)",
            "gain-pre-commission (register)",
            "gain-pre-commission variance",
            "cumul-gross-profit",
            "cumul-net-profit",
            "cumul-tot-return",
        ]
    );
    let row = &data_rows(&result)[0];
    let split = row["cells"][2]["link"]["split"].as_str().unwrap();
    let t = s.call(
        "transaction.list",
        json!({ "account": "Root:Asset:Broker:SPY", "from": "2019-07-01", "to": "2019-07-01" }),
    );
    let spy_splits: Vec<&str> = t["transactions"][0]["splits"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["accountName"] == "Root:Asset:Broker:SPY")
        .map(|x| x["guid"].as_str().unwrap())
        .collect();
    assert!(spy_splits.contains(&split), "{split} is one of {spy_splits:?}");
    let fx = &row["cells"][6];
    assert_eq!(
        (&fx["price"], &fx["currency"]),
        (&json!({ "num": "1", "denom": "1" }), &json!("CURRENCY::CAD"))
    );
    let b = blocks(&result);
    let kinds: Vec<&str> = b.iter().map(|x| x["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["table", "text", "text", "text"]);
    assert!(
        b[1]["text"]
            .as_str()
            .unwrap()
            .starts_with("This report is designed for cost basis accumulation")
    );
}

/// Two rows of the test run read by column, worked from IF:393-462: the sale of 75 of 150 SPY (TI:276) and the cover
/// of 50 of 150 short (TI:339). The basis per unit is the cumulative basis over the units before the transaction,
/// unsigned; the basis of the sale is that times the units sold, shown negated, and leaves the cumulative basis; the
/// gains add it to the proceeds before (pre) and after (post) the fees. USD CapGain is not the Cap Gains Account
/// (that defaults to Income), so the register column is empty and the variance is the gain.
#[test]
fn columns_of_a_sale_and_a_cover() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("iso"));
    let result = s.run(IFRS, &test_options());
    assert_row(
        &result,
        3,
        &[
            ("trans-units", "-75. SPY"),
            ("cumul-units", "75. SPY"),
            ("note", "Sell"),
            ("purchase-val", ""),
            ("purchase-cost", ""),
            ("cash-dividends", ""),
            ("proceeds-val", "C$12,000.00"),
            ("proceeds-cost", "C$9.95"),
            ("conv-proceeds-val", "C$12,000.00"),
            ("conv-proceeds-cost", "C$9.95"),
            ("cumulative-average-cost-basis", "C$18,009.95"),
            ("average-cost-basis/unit-for-sale", "C$240.13"),
            ("average-cost-basis-of-sale", "C$18,009.95"),
            ("net-proceeds", "C$11,990.05"),
            ("gain-post-commission", "-C$6,019.90"),
            ("gain-pre-commission (calc)", "-C$6,009.95"),
            ("gain-pre-commission (register)", ""),
            ("gain-pre-commission variance", "-C$6,009.95"),
            ("cumul-gross-profit", "-C$6,009.95"),
            ("cumul-net-profit", "-C$6,019.90"),
            ("cumul-tot-return", "-C$6,019.90"),
        ],
    );
    assert_row(
        &result,
        13,
        &[
            ("trans-units", "50. SPY"),
            ("cumul-units", "-100. SPY"),
            ("note", "Cover Buy"),
            ("proceeds-val", "-C$5,000.00"),
            ("proceeds-cost", "C$9.95"),
            ("cumulative-average-cost-basis", "-C$15,286.73"),
            ("average-cost-basis/unit-for-sale", "C$152.87"),
            ("average-cost-basis-of-sale", "-C$7,643.37"),
            ("net-proceeds", "-C$5,009.95"),
            ("gain-post-commission", "C$2,633.42"),
            ("gain-pre-commission (calc)", "C$2,643.37"),
            ("gain-pre-commission (register)", ""),
            ("gain-pre-commission variance", "C$2,643.37"),
        ],
    );
    // The basis per unit is kept exact: 22,930.10 / 150 × 50 is 7,643.366…, so the basis left is −15,286.733….
    let basis = &data_rows(&result)[12]["cells"][17]["monetary"]["amount"];
    assert_eq!(basis, &json!({ "num": "-229301", "denom": "15" }));
}

/// The shade of each row.
fn shades(result: &Value) -> Vec<bool> {
    data_rows(result).iter().map(|r| r["alternate"].as_bool().unwrap()).collect()
}

/// "Alternative row-style for shorts" (IF:480-482): on, a row is `alternate-row` while short, that is with fewer than
/// no units after it, or none after buying (IF:417-418): rows 11 to 14. Off, rows alternate from `normal-row`.
#[test]
fn shading_of_shorts_and_by_row() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("iso"));
    let shorts = shades(&s.run(IFRS, &test_options()));
    assert_eq!(shorts, (1..=15).map(|r| (11..=14).contains(&r)).collect::<Vec<_>>());
    let off = with(test_options(), json!({ "General": { "Alternative row-style for shorts": false } }));
    assert_eq!(shades(&s.run(IFRS, &off)), (1..=15).map(|r| r % 2 == 0).collect::<Vec<_>>());
}

/// "Capitalise purchase commissions" off (IF:441-450): the commission stays out of the basis and goes to the gain
/// after commission instead, with the sign it has, so each purchase adds C$9.95 to the net profit and total return.
#[test]
fn commissions_not_capitalised() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("iso"));
    let options = with(test_options(), json!({ "General": { "Capitalise purchase commissions": false } }));
    let result = s.run(IFRS, &options);
    assert_row(
        &result,
        1,
        &[
            ("purchase-val", "C$20,000.00"),
            ("conv-purchase-cost", "C$9.95"),
            ("cumulative-average-cost-basis", "C$20,000.00"),
            ("gain-post-commission", "C$9.95"),
            ("gain-pre-commission (calc)", ""),
            ("cumul-gross-profit", "C$0.00"),
            ("cumul-net-profit", "C$9.95"),
            ("cumul-tot-return", "C$9.95"),
        ],
    );
    assert_row(
        &result,
        2,
        &[
            ("average-cost-basis/unit-for-sale", "C$200.00"),
            ("cumulative-average-cost-basis", "C$36,000.00"),
            ("gain-post-commission", "C$9.95"),
            ("cumul-net-profit", "C$19.90"),
            ("cumul-tot-return", "C$19.90"),
        ],
    );
}

/// "Format monetary cells" off (IF:345-358): every figure is a bare number, printed by GnuCash's number renderer to
/// two places (`report/html-style-info.scm:233-236`), the units too; the rate stays a price (IF:489).
#[test]
fn format_cells_off() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("iso"));
    let options = with(test_options(), json!({ "General": { "Format monetary cells": false } }));
    let result = s.run(IFRS, &options);
    assert_eq!(
        texts(&data_rows(&result)[2]),
        [
            "2020-03-18",
            "Sell SPY",
            "-75.00",
            "75.00",
            "Sell",
            "CAD",
            "C$1.00",
            "12,000.00",
            "9.95",
            "12,000.00",
            "9.95",
            "18,009.95",
            "240.13",
            "18,009.95",
            "11,990.05",
            "-6,019.90",
            "-6,009.95",
            "-6,009.95",
            "-6,009.95",
            "-6,019.90",
            "-6,019.90",
        ]
    );
    assert!(
        data_rows(&result)[2]["cells"][2]["link"]["split"].is_string(),
        "the bare units keep their link"
    );
}

/// The window (IF:299-304, :467-469): both dates at the start of their day and a row only when posted strictly
/// between them, so the Start Date's transactions are in and the End Date's out. Transactions before the window still
/// move the running figures: from 2020-03-18 to 2020-06-10 the rows are the test's rows 3 to 9, with the same
/// figures, and the shading starts again at `normal-row`.
#[test]
fn start_date_in_end_date_out() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("iso"));
    let all = s.run(IFRS, &test_options());
    let options = with(
        test_options(),
        json!({ "General": {
        "Start Date": { "absolute": "2020-03-18" }, "End Date": { "absolute": "2020-06-10" },
        "Alternative row-style for shorts": false,
    } }),
    );
    let window = s.run(IFRS, &options);
    assert_eq!(
        window["title"],
        "Average-Cost (Basis) Report: From 2020-03-18 to 2020-06-10. Report-currency CAD"
    );
    let got: Vec<Vec<String>> = data_rows(&window).iter().map(texts).collect();
    let want: Vec<Vec<String>> = data_rows(&all)[2..9].iter().map(texts).collect();
    assert_eq!(got, want);
    assert_eq!(shades(&window), [false, true, false, true, false, true, false]);
}

/// A book in `currency` for the hand-worked cases: Broker (ASSET) with Cash (BANK) and XYZ (STOCK, `NYSE::XYZ`, SCU
/// 1), Income with Dividends and Gains, Expenses with Fees.
fn book(currency: &str) -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    let c = format!("CURRENCY::{currency}");
    s.call(
        "commodity.create",
        json!({ "namespace": "NYSE", "mnemonic": "XYZ", "fullname": "XYZ", "fraction": 1 }),
    );
    s.account("Broker", "ASSET", None, &c);
    s.account("Cash", "BANK", Some("Broker"), &c);
    s.account("XYZ", "STOCK", Some("Broker"), "NYSE::XYZ");
    s.account("Income", "INCOME", None, &c);
    s.account("Dividends", "INCOME", Some("Income"), &c);
    s.account("Gains", "INCOME", Some("Income"), &c);
    s.account("Expenses", "EXPENSE", None, &c);
    s.account("Fees", "EXPENSE", Some("Expenses"), &c);
    s
}

/// A split of `post`: `(account, value, amount, action)`, the amount given only for XYZ.
type SplitSpec<'a> = (&'a str, &'a str, &'a str, &'a str);

/// A transaction in `currency` on `date` with `splits`. Returns its GUID.
fn post(s: &mut Setup, currency: &str, date: &str, description: &str, splits: &[SplitSpec<'_>]) -> String {
    let splits: Vec<Value> = splits
        .iter()
        .map(|(account, value, amount, action)| {
            let mut split = json!({ "account": account, "value": value, "action": action });
            if !amount.is_empty() {
                split["amount"] = json!(amount);
            }
            split
        })
        .collect();
    let t = json!({ "datePosted": date, "currency": format!("CURRENCY::{currency}"), "description": description, "splits": splits });
    s.call("transaction.post", t)["guid"].as_str().unwrap().to_owned()
}

/// The options naming `book`'s accounts, over January 2024, with `more`.
fn book_options(currency: &str, more: Value) -> Value {
    let options = json!({ "General": {
        "Report's currency": format!("CURRENCY::{currency}"),
        "Stock Account": "Broker:XYZ", "Proceeds Account": "Broker:Cash", "Dividend Account": "Income:Dividends",
        "Cap Gains Account": "Income:Gains", "Fees Account": "Expenses:Fees",
        "Start Date": { "absolute": "2024-01-01" }, "End Date": { "absolute": "2024-02-01" },
    } });
    with(options, more)
}

/// Eleven transactions in USD through the types of IF:174-211 that GnuCash's test does not reach.
fn types_book() -> Setup {
    let mut s = book("USD");
    let rows: [(&str, &str, Vec<SplitSpec<'_>>); 11] = [
        (
            "2024-01-02",
            "Buy",
            vec![
                ("Broker:XYZ", "100", "10", "Buy"),
                ("Broker:XYZ", "1", "0", "Fee"),
                ("Broker:Cash", "-101", "", ""),
            ],
        ),
        (
            "2024-01-03",
            "Dividend",
            vec![
                ("Broker:XYZ", "0", "0", ""),
                ("Broker:Cash", "5", "", ""),
                ("Income:Dividends", "-5", "", ""),
            ],
        ),
        (
            "2024-01-04",
            "Reclassified",
            vec![("Broker:XYZ", "-3", "0", ""), ("Income:Dividends", "3", "", "")],
        ),
        (
            "2024-01-05",
            "Notional",
            vec![("Broker:XYZ", "4", "0", ""), ("Income:Gains", "-4", "", "")],
        ),
        (
            "2024-01-08",
            "Reverse split with cash",
            vec![("Broker:XYZ", "-25", "-2", "Sell"), ("Broker:Cash", "25", "", "")],
        ),
        ("2024-01-09", "Reverse split", vec![("Broker:XYZ", "0", "-6", "")]),
        (
            "2024-01-10",
            "Buy, fee expensed",
            vec![
                ("Broker:XYZ", "10", "1", "Buy"),
                ("Expenses:Fees", "1", "", ""),
                ("Broker:Cash", "-11", "", ""),
            ],
        ),
        (
            "2024-01-11",
            "Oversell",
            vec![("Broker:XYZ", "-150", "-5", "Sell"), ("Broker:Cash", "150", "", "")],
        ),
        (
            "2024-01-12",
            "Compensation",
            vec![
                ("Broker:XYZ", "0", "0", ""),
                ("Broker:Cash", "-2", "", ""),
                ("Income:Dividends", "2", "", ""),
            ],
        ),
        (
            "2024-01-15",
            "Overbuy",
            vec![("Broker:XYZ", "100", "5", "Buy"), ("Broker:Cash", "-100", "", "")],
        ),
        (
            "2024-01-16",
            "Voided",
            vec![("Broker:XYZ", "10", "1", "Buy"), ("Broker:Cash", "-10", "", "")],
        ),
    ];
    let guids: Vec<String> = rows
        .iter()
        .map(|(date, description, splits)| post(&mut s, "USD", date, description, splits))
        .collect();
    s.call("transaction.void", json!({ "guid": guids[10], "reason": "test" }));
    s
}

/// The types GnuCash's test does not reach, worked from IF:174-234 and :393-462 on `types_book`. The first type whose
/// seven sign tests pass names the row; a sign test of `....` passes on none or zero, so an expensed fee on a purchase
/// fails "Buy" and the row is "Unknown" (7), and "Reverse split w/ cash in lieu for fractionals" can never match,
/// "Sell" passing first on every transaction it would (5). Crossing zero is one of the ERROR rows, which count nothing
/// (8, 10). A dividend paid while short is compensatory (9). The voided transaction is found, its splits now zero
/// (11). The basis per unit is the basis over the units before, so the return of capital keeps $10.10 a share and
/// the next row shows $98 over 10; a sale takes its share of the basis, $10.20 × 2.
#[test]
fn transaction_types_the_test_does_not_reach() {
    let mut s = types_book();
    let result = s.run(IFRS, &book_options("USD", json!({})));
    let notes: Vec<String> = (1..=11).map(|r| at(&result, r, "note")).collect();
    assert_eq!(
        notes,
        [
            "Open Long",
            "Dividend",
            "Return of capital (reclassification)",
            "Notional distribution (capital gain)",
            "Sell",
            "Reverse split",
            "Unknown",
            "ERROR: long→short",
            "Compensatory dividend",
            "ERROR: short→long",
            "Unknown",
        ]
    );
    let units: Vec<String> = (1..=11)
        .map(|r| format!("{} {}", at(&result, r, "trans-units"), at(&result, r, "cumul-units")))
        .collect();
    assert_eq!(
        units,
        [
            "10. XYZ 10. XYZ",
            "0. XYZ 10. XYZ",
            "0. XYZ 10. XYZ",
            "0. XYZ 10. XYZ",
            "-2. XYZ 8. XYZ",
            "-6. XYZ 2. XYZ",
            "1. XYZ 3. XYZ",
            "-5. XYZ -2. XYZ",
            "0. XYZ -2. XYZ",
            "5. XYZ 3. XYZ",
            "0. XYZ 3. XYZ",
        ]
    );
    assert_row(
        &result,
        1,
        &[
            ("purchase-val", "$100.00"),
            ("purchase-cost", "$1.00"),
            ("cumulative-average-cost-basis", "$101.00"),
            ("average-cost-basis/unit-for-sale", ""),
            ("cumul-tot-return", "$0.00"),
        ],
    );
    assert_row(
        &result,
        2,
        &[
            ("cash-dividends", "$5.00"),
            ("conv-dividends", "$5.00"),
            ("average-cost-basis/unit-for-sale", "$10.10"),
            ("cumulative-average-cost-basis", "$101.00"),
            ("cumul-net-profit", "$0.00"),
            ("cumul-tot-return", "$5.00"),
        ],
    );
    assert_row(
        &result,
        3,
        &[
            ("purchase-val", "-$3.00"),
            ("purchase-cost", ""),
            ("cash-dividends", "-$3.00"),
            ("cumulative-average-cost-basis", "$98.00"),
            ("average-cost-basis/unit-for-sale", "$10.10"),
            ("cumul-tot-return", "$2.00"),
        ],
    );
    assert_row(
        &result,
        4,
        &[
            ("purchase-val", "$4.00"),
            ("cumulative-average-cost-basis", "$102.00"),
            ("average-cost-basis/unit-for-sale", "$9.80"),
            ("gain-pre-commission (register)", "-$4.00"),
            ("gain-pre-commission variance", "-$4.00"),
            ("cumul-gross-profit", "$0.00"),
        ],
    );
    assert_row(
        &result,
        5,
        &[
            ("proceeds-val", "$25.00"),
            ("proceeds-cost", ""),
            ("cumulative-average-cost-basis", "$81.60"),
            ("average-cost-basis/unit-for-sale", "$10.20"),
            ("average-cost-basis-of-sale", "$20.40"),
            ("net-proceeds", "$25.00"),
            ("gain-post-commission", "$4.60"),
            ("gain-pre-commission (calc)", "$4.60"),
            ("gain-pre-commission variance", "$4.60"),
            ("cumul-gross-profit", "$4.60"),
            ("cumul-net-profit", "$4.60"),
            ("cumul-tot-return", "$6.60"),
        ],
    );
    assert_row(
        &result,
        6,
        &[
            ("cumulative-average-cost-basis", "$81.60"),
            ("average-cost-basis/unit-for-sale", "$10.20"),
        ],
    );
    assert_row(
        &result,
        7,
        &[
            ("purchase-val", ""),
            ("purchase-cost", ""),
            ("cumulative-average-cost-basis", "$81.60"),
            ("average-cost-basis/unit-for-sale", "$40.80"),
        ],
    );
    assert_row(
        &result,
        8,
        &[
            ("proceeds-val", ""),
            ("average-cost-basis/unit-for-sale", "$27.20"),
            ("cumul-tot-return", "$6.60"),
        ],
    );
    assert_row(
        &result,
        9,
        &[
            ("cash-dividends", "-$2.00"),
            ("average-cost-basis/unit-for-sale", "$40.80"),
            ("cumul-tot-return", "$4.60"),
        ],
    );
    assert_row(&result, 10, &[("purchase-val", ""), ("average-cost-basis/unit-for-sale", "$40.80")]);
    assert_row(
        &result,
        11,
        &[("average-cost-basis/unit-for-sale", "$27.20"), ("cumul-tot-return", "$4.60")],
    );
    assert_eq!(shades(&result), (1..=11).map(|r| r == 8 || r == 9).collect::<Vec<_>>());
    let html = s.export(IFRS, &book_options("USD", json!({})));
    ["ERROR: long→short", "Compensatory dividend", "$40.80"]
        .iter()
        .for_each(|w| assert!(html.contains(w), "{w} is in the HTML"));
}

/// The Dividend Account and the Cap Gains Account the same (as GnuCash's test leaves both at their default, Income):
/// capital gains are matched first (IF:261-273), so the dividend is a capital gain, and with no dividend the row
/// matches no type.
#[test]
fn one_account_for_dividends_and_gains() {
    let mut s = types_book();
    let options = book_options("USD", json!({ "General": { "Cap Gains Account": "Income:Dividends" } }));
    let result = s.run(IFRS, &options);
    assert_row(
        &result,
        2,
        &[
            ("note", "Unknown"),
            ("cash-dividends", ""),
            ("gain-pre-commission (register)", "-$5.00"),
            ("gain-pre-commission variance", "-$5.00"),
            ("cumul-tot-return", "$0.00"),
        ],
    );
}

/// The rate (IF:317-320, :427-428): 10,000,000 units of the Stock Account's currency, its parent's EUR
/// (`gnc-account-get-currency-or-parent`, IF:335), converted at the price nearest local noon of the posted date and
/// rounded to the report currency's cents, over 10,000,000. A price of $1.23456789012 gives $1.23456789. The five
/// figures stay in EUR; their conversions and everything after are in USD.
#[test]
fn rate_from_the_price_database() {
    let mut s = book("EUR");
    let price = json!({ "commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "value": "1.23456789012", "date": "2024-01-01" });
    s.call("price.add", price);
    let buy = [
        ("Broker:XYZ", "100", "10", "Buy"),
        ("Broker:XYZ", "1", "0", "Fee"),
        ("Broker:Cash", "-101", "", ""),
    ];
    post(&mut s, "EUR", "2024-01-02", "Buy", &buy);
    let result = s.run(IFRS, &book_options("USD", json!({})));
    assert_eq!(
        result["title"],
        "Average-Cost (Basis) Report: From 2024-01-01 to 2024-02-01. Report-currency USD"
    );
    assert_row(
        &result,
        1,
        &[
            ("curr", "EUR"),
            ("fx", "$1.23456789"),
            ("purchase-val", "€100.00"),
            ("purchase-cost", "€1.00"),
            ("conv-purchase-val", "$123.46"),
            ("conv-purchase-cost", "$1.23"),
            ("cumulative-average-cost-basis", "$124.69"),
        ],
    );
    let basis = &data_rows(&result)[0]["cells"][17]["monetary"]["amount"];
    assert_eq!(
        basis,
        &json!({ "num": "12469135689", "denom": "100000000" }),
        "101 × 1.23456789, exactly"
    );
}

/// The fee filter (IF:241-243, :277-280): a Stock Account split whose action is "Action field filter for fees" is a
/// capitalised fee, its value counted apart from the stock's. With the default "Fee" the "Commission" split is stock,
/// so the purchase is $101 with no cost; with the filter "Commission" it is $100 and $1. With the book's num field
/// source on, the filter reads the transaction's num (`gnc-get-action-num`), so a split action of "Fee" counts as
/// stock too. A transaction whose Stock Account splits are all fees has no units; held units keep it from failing, its
/// units cell is empty and it matches no type (row 2).
#[test]
fn fee_filter_and_the_num_field_source() {
    let mut s = book("USD");
    let buy = [
        ("Broker:XYZ", "100", "10", "Buy"),
        ("Broker:XYZ", "1", "0", "Commission"),
        ("Broker:Cash", "-101", "", ""),
    ];
    post(&mut s, "USD", "2024-01-02", "Buy", &buy);
    post(
        &mut s,
        "USD",
        "2024-01-03",
        "Fee only",
        &[("Broker:XYZ", "2", "0", "Fee"), ("Broker:Cash", "-2", "", "")],
    );
    let default = s.run(IFRS, &book_options("USD", json!({})));
    assert_row(
        &default,
        1,
        &[
            ("purchase-val", "$101.00"),
            ("purchase-cost", ""),
            ("cumulative-average-cost-basis", "$101.00"),
        ],
    );
    assert_row(&default, 2, &[("trans-units", ""), ("cumul-units", "10. XYZ"), ("note", "Unknown")]);
    assert!(data_rows(&default)[1]["cells"][2].is_null());
    let commission = s.run(
        IFRS,
        &book_options("USD", json!({ "General": { "Action field filter for fees": "Commission" } })),
    );
    assert_row(
        &commission,
        1,
        &[
            ("purchase-val", "$100.00"),
            ("purchase-cost", "$1.00"),
            ("cumulative-average-cost-basis", "$101.00"),
        ],
    );
    assert_row(&commission, 2, &[("trans-units", "0. XYZ"), ("note", "Unknown")]);

    let mut s = book("USD");
    s.call("book.setOptions", json!({ "splitActionForNum": true }));
    let buy = [
        ("Broker:XYZ", "100", "10", "Buy"),
        ("Broker:XYZ", "1", "0", "Fee"),
        ("Broker:Cash", "-101", "", ""),
    ];
    post(&mut s, "USD", "2024-01-02", "Buy", &buy);
    let by_num = s.run(IFRS, &book_options("USD", json!({})));
    assert_row(&by_num, 1, &[("purchase-val", "$101.00"), ("purchase-cost", "")]);
}

/// With no units held and no stock amount in the transaction, GnuCash's `(< 0 #f)` (IF:417-418) is a Guile error that
/// stops the report; red-ale stops it with a `report-script` error naming the transaction (the note's Q11).
#[test]
fn a_fee_with_nothing_held_fails() {
    let mut s = book("USD");
    post(
        &mut s,
        "USD",
        "2024-01-02",
        "Fee first",
        &[("Broker:XYZ", "2", "0", "Fee"), ("Broker:Cash", "-2", "", "")],
    );
    let e = s.err("report.run", json!({ "report": IFRS, "options": book_options("USD", json!({})) }));
    assert_eq!(e.code, 3000);
    let message = e.data.unwrap()["message"].as_str().unwrap().to_owned();
    assert!(message.contains(r#""Fee first" of 2024-01-02"#), "{message}");
}

/// The options of IF:79-120 in GnuCash's order: sections by name, then keys, so the date interval's `" a"` and `" b"`
/// come before the General options every template gets. The account options have no default of their own; each
/// resolves, when the report runs, to the first account of its types.
#[test]
fn options_are_gnucash_s() {
    let mut s = Setup::with_config(&test_settings("iso"));
    let described = s.call("report.options", json!({ "report": IFRS }));
    let got: Vec<String> = described["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}/{} {} {} {}",
                o["section"].as_str().unwrap(),
                o["name"].as_str().unwrap(),
                o["key"],
                o["type"],
                o["default"]
            )
        })
        .collect();
    let want = [
        r#"General/Start Date " a" "date" {"relative":"start-accounting-period"}"#,
        r#"General/End Date " b" "date" {"relative":"end-accounting-period"}"#,
        r#"General/Report name "0a" "string" "IFRS weighted-average cost basis report""#,
        r#"General/Stylesheet "0b" "multichoice" "Default""#,
        r#"General/Add options summary "0c" "multichoice" "never""#,
        r#"General/Report's currency "a" "currency" "CURRENCY::USD""#,
        r#"General/Stock Account "b" "accountSel" null"#,
        r#"General/Proceeds Account "c" "accountSel" null"#,
        r#"General/Dividend Account "c" "accountSel" null"#,
        r#"General/Fees Account "c5" "accountSel" null"#,
        r#"General/Cap Gains Account "d" "accountSel" null"#,
        r#"General/Action field filter for fees "d5" "string" "Fee""#,
        r#"General/Format monetary cells "e" "boolean" true"#,
        r#"General/Alternative row-style for shorts "f" "boolean" true"#,
        r#"General/Capitalise purchase commissions "g" "boolean" true"#,
    ];
    assert_eq!(got, want);
}

/// Running the report leaves the Book, its revision and its file as they were and writes nothing beside it, on XML
/// and SQLite; two exports are the same bytes, with no script.
#[test]
fn a_run_changes_nothing() {
    for (format, mut s) in both_formats() {
        let bytes = std::fs::read(s.book()).unwrap();
        let before = s.d.context().book().unwrap().clone();
        let revision = s.d.context().session().unwrap().revision();
        let files = s.files();
        [
            json!({}),
            json!({ "General": { "Format monetary cells": false, "Capitalise purchase commissions": false } }),
        ]
        .iter()
        .for_each(|more| {
            let options = with(test_options(), more.clone());
            s.run(IFRS, &options);
            let (a, b) = (s.export(IFRS, &options), s.export(IFRS, &options));
            assert_eq!(a, b, "{format}");
            assert!(!a.to_ascii_lowercase().contains("<script"), "{format}");
        });
        assert_eq!(s.d.context().book().unwrap(), &before, "{format}");
        assert_eq!(s.d.context().session().unwrap().revision(), revision, "{format}");
        assert_eq!(s.call("book.info", json!({}))["dirty"], false, "{format}");
        assert_eq!(s.files(), files, "{format}: no journal or other file was written");
        assert_eq!(std::fs::read(s.book()).unwrap(), bytes, "{format}: the file is unchanged");
    }
}
