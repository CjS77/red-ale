//! The Budget Report (`standard/budget.rhai`) through `report.run` and `report.export`: every Budget Report
//! assertion of 5.17's `test-budget.scm` on `fixtures/reports/budget.gnucash` (`budget.expected.json`), then
//! hand-worked cases for the options and branches the test does not reach. `BR` stands for
//! `5.17:gnucash/report/reports/standard/budget.scm`, `TB` for `5.17:gnucash/report/reports/standard/test/test-budget.scm`.
//!
//! The fixture's budget, "test budget", has six monthly periods from 2024-03-01, so on the pinned now (2024-03-15)
//! the current period is 0. Bank's actuals are $35, -$20, $67 and $77 in periods 0-3; Income's -$55, $0 (period 1's
//! transaction is a closing one, which actuals leave out), -$67 and -$77; Expenses' $20 and $20.
//!
//! GnuCash's test counts and reads text nodes of the rendered table (`test-report-extras.scm:96-109`). Here a row's
//! texts are its cells' display strings in order, empty cells left out, and a cell with a footnote reads as GnuCash's
//! does: its figure and a space, then the footnote's number (BR:253-259).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod report_runs;

use report_runs::{Fixture, html_table, only};
use serde_json::{Value, json};

const TEMPLATE: &str = "Budget Report";

const BANK: &str = "a0000000000000000000000000000003";
const GBP_BANK: &str = "a0000000000000000000000000000004";
const ASSET: &str = "a0000000000000000000000000000002";

/// The texts GnuCash's test reads from one cell: none for an empty cell.
fn cell_texts(cell: &Value) -> Vec<String> {
    if cell.is_null() {
        return vec![];
    }
    let shown = cell["display"].as_str().or_else(|| cell["text"].as_str()).unwrap().to_owned();
    match cell["footnote"].as_u64() {
        Some(n) => vec![format!("{shown} "), n.to_string()],
        None => vec![shown],
    }
}

/// The texts of a row, cell by cell.
fn row_texts(row: &Value) -> Vec<String> {
    row["cells"].as_array().unwrap().iter().flat_map(cell_texts).collect()
}

/// The number of text nodes in the table, as `(length (sxml->table-row-col sxml 1 #f #f))` counts them.
fn count(table: &Value) -> usize {
    table["rows"].as_array().unwrap().iter().map(|r| row_texts(r).len()).sum()
}

/// The account row labelled `label`.
fn row<'a>(table: &'a Value, label: &str) -> &'a Value {
    table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["role"] == "account" && r["cells"][0]["text"] == label)
        .unwrap_or_else(|| panic!("no row {label}"))
}

/// The labels of the account rows, in order.
fn labels(table: &Value) -> Vec<String> {
    let rows = table["rows"].as_array().unwrap();
    rows.iter()
        .filter(|r| r["role"] == "account")
        .map(|r| r["cells"][0]["text"].as_str().unwrap().to_owned())
        .collect()
}

/// Each cell of the result's rows as the exported page shows it: its text, a footnote's number after a space.
fn shown_cells(table: &Value) -> Vec<Vec<String>> {
    let rows = table["rows"].as_array().unwrap();
    rows.iter()
        .map(|r| r["cells"].as_array().unwrap().iter().map(|c| cell_texts(c).concat()).collect())
        .collect()
}

/// The run's only table, checked against the exported page, which must show the same cells and footnotes.
fn table(f: &mut Fixture, options: &Value) -> Value {
    let result = f.run(TEMPLATE, options);
    let table = only(&result, "table").clone();
    let html = f.export(TEMPLATE, options);
    assert_eq!(html_table(&html, 0), shown_cells(&table), "the exported table shows the run's cells");
    let notes: Vec<String> = table["footnotes"]
        .as_array()
        .map(|n| n.iter().map(|n| format!("<li>{}</li>", n.as_str().unwrap())).collect())
        .unwrap_or_default();
    assert!(html.contains(&notes.concat()), "the footnotes follow the table");
    table
}

fn fixture() -> Fixture {
    Fixture::open("budget", "locale", true)
}

/// TB:74-79: every account level, and every column off.
fn all_display_off() -> Value {
    json!({
        "Accounts": { "Account Display Depth": "all" },
        "Display": { "Show Difference": false, "Show Budget": false, "Show Budget Notes": false, "Show Actual": false },
    })
}

/// TB:74, :85-89: every column on, with the totals.
fn basic() -> Value {
    json!({
        "Accounts": { "Account Display Depth": "all" },
        "Display": { "Show Difference": true, "Show Budget": true, "Show Budget Notes": true, "Show Actual": true,
                     "Show Column with Totals": true },
    })
}

/// `basic()` with the General options of a later step of the test.
fn with_general(general: &Value) -> Value {
    let mut options = basic();
    options["General"] = general.clone();
    options
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

// Expected figures from 5.17 (TB:66-161).

#[test]
fn test_budget_all_display_off() {
    let mut f = fixture();
    let t = table(&mut f, &all_display_off());
    // TB:81 "all display OFF, table has 15 cells": the six period titles and nine labels.
    assert_eq!(count(&t), 15);
}

#[test]
fn test_budget_basic() {
    let mut f = fixture();
    let t = table(&mut f, &basic());
    // TB:91 "all display ON, table has 228 cells".
    assert_eq!(count(&t), 228);
    // TB:94 "bank".
    assert_eq!(
        row_texts(row(&t, "Bank")),
        strings(&[
            "Bank", "$20.00", "$35.00", "-$15.00", "$40.00", "-$20.00", "$60.00", ".", "$67.00", "-$67.00", "$60.00", "$77.00",
            "-$17.00", ".", "$0.00", ".", ".", "$0.00", ".", "$120.00", "$159.00", "-$39.00",
        ])
    );
    // TB:99 "income".
    assert_eq!(
        row_texts(row(&t, "Income")),
        strings(&[
            "Income", "-$55.00 ", "1", "-$55.00", "$0.00", ".", "$0.00", ".", "-$65.00", "-$67.00", "$2.00", "-$75.00",
            "-$77.00", "$2.00", ".", "$0.00", ".", ".", "$0.00", ".", "-$195.00", "-$199.00", "$4.00",
        ])
    );
    // TB:104 "expense".
    assert_eq!(
        row_texts(row(&t, "Expenses")),
        strings(&[
            "Expenses", ".", "$20.00", "-$20.00", "$30.00 ", "2", "$20.00", "$10.00", "$20.00", "$0.00", "$20.00", "$40.00",
            "$0.00", "$40.00", ".", "$0.00", ".", ".", "$0.00", ".", "$90.00", "$40.00", "$50.00",
        ])
    );
    // TB:109 "budget notes".
    assert_eq!(t["footnotes"], json!(["income-0 -$60", "expense-1 $25"]));
}

#[test]
fn test_budget_only_next_period() {
    let mut f = fixture();
    let options = with_general(&json!({ "Report for range of budget periods": true, "Range start": "current", "Range end": "next" }));
    let t = table(&mut f, &options);
    // TB:118 "only next period - 135 cells".
    assert_eq!(count(&t), 135);
    // TB:122 "only next period- bank".
    assert_eq!(
        row_texts(row(&t, "Bank")),
        strings(&[
            "Bank", "$20.00", "$35.00", "-$15.00", "$40.00", "-$20.00", "$60.00", "$60.00", "$144.00", "-$84.00", "$120.00",
            "$159.00", "-$39.00",
        ])
    );
}

#[test]
fn test_budget_only_last_period() {
    let mut f = fixture();
    let options = with_general(&json!({ "Report for range of budget periods": true, "Range start": "last", "Range end": "last" }));
    let t = table(&mut f, &options);
    // TB:130 "only last period - 102 cells".
    assert_eq!(count(&t), 102);
    // TB:134 "only last period - bank".
    assert_eq!(
        row_texts(row(&t, "Bank")),
        strings(&["Bank", "$120.00", "$159.00", "-$39.00", ".", "$0.00", ".", "$120.00", "$159.00", "-$39.00"])
    );
}

/// TB:139-144: periods 2 to 4 as the user counts them, nothing collapsed.
fn exact_periods() -> Value {
    json!({
        "Report for range of budget periods": true, "Range start": "manual", "Range end": "manual",
        "Exact start period": 2, "Exact end period": 4,
        "Include collapsed periods before selected.": false, "Include collapsed periods after selected.": false,
    })
}

#[test]
fn test_budget_exact_periods() {
    let mut f = fixture();
    let t = table(&mut f, &with_general(&exact_periods()));
    // TB:146 "exact periods - 134 cells".
    assert_eq!(count(&t), 134);
    // TB:150 "exact periods - bank".
    assert_eq!(
        row_texts(row(&t, "Bank")),
        strings(&[
            "Bank", "$40.00", "-$20.00", "$60.00", ".", "$67.00", "-$67.00", "$60.00", "$77.00", "-$17.00", "$100.00", "$124.00",
            "-$24.00",
        ])
    );
}

#[test]
fn test_budget_use_accumulated_amounts() {
    let mut f = fixture();
    let mut general = exact_periods();
    general["Use accumulated amounts"] = json!(true);
    let t = table(&mut f, &with_general(&general));
    // TB:157 "use accumulated amounts".
    assert_eq!(
        row_texts(row(&t, "Bank")),
        strings(&[
            "Bank", "$60.00", "$15.00", "$45.00", "$60.00", "$82.00", "-$22.00", "$120.00", "$159.00", "-$39.00", "$120.00",
            "$159.00", "-$39.00",
        ])
    );
}

// Hand-worked cases.

/// The options of `budget-report-options-generator` (BR:108-238), in GnuCash's order, with the General options every
/// template gets. With no book open there is no budget to default to, and no account.
#[test]
fn options_are_gnucash_s() {
    let want = [
        r#"Accounts/Account Display Depth "aa" "multichoice" 2"#,
        r#"Accounts/Always show sub-accounts "ab" "boolean" false"#,
        r#"Accounts/Account "ac" "accountList" []"#,
        r#"Accounts/Flatten list to depth limit "c" "boolean" false"#,
        r#"Display/Show Budget "s1" "boolean" true"#,
        r#"Display/Show Budget Notes "s15" "boolean" true"#,
        r#"Display/Show Actual "s2" "boolean" true"#,
        r#"Display/Link to actual transactions "s25" "boolean" false"#,
        r#"Display/Show Difference "s3" "boolean" false"#,
        r#"Display/Show Column with Totals "s4" "boolean" false"#,
        r#"Display/Include accounts with zero total balances and budget values "s5" "boolean" true"#,
        r#"General/Report name "0a" "string" "Budget Report""#,
        r#"General/Stylesheet "0b" "multichoice" "Default""#,
        r#"General/Add options summary "0c" "multichoice" "never""#,
        r#"General/Budget "a" "budget" null"#,
        r#"General/Use accumulated amounts "b" "boolean" false"#,
        r#"General/Report for range of budget periods "f" "boolean" false"#,
        r#"General/Range start "g1.1" "multichoice" "current""#,
        r#"General/Exact start period "g1.2" "numberRange" 1"#,
        r#"General/Range end "g2.1" "multichoice" "next""#,
        r#"General/Exact end period "g2.2" "numberRange" 1"#,
        r#"General/Include collapsed periods before selected. "g3" "boolean" true"#,
        r#"General/Include collapsed periods after selected. "g4" "boolean" true"#,
    ];
    assert_eq!(report_runs::described_options(TEMPLATE), want);
}

/// With the book open, the budget defaults to the book's only one (`gnc-budget-get-default`, BR:121-124) and the
/// accounts to those of exactly the types ASSET, LIABILITY, INCOME and EXPENSE (BR:193-201): not Equity.
#[test]
fn defaults_come_from_the_book() {
    let mut f = fixture();
    let described = f.call("report.options", &json!({ "report": TEMPLATE }));
    let option = |name: &str| {
        described["options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["name"] == name)
            .unwrap()["default"]
            .clone()
    };
    assert_eq!(option("Budget"), json!("f0000000000000000000000000000000"));
    // In display order: Root, Asset, Bank, GBP Bank, Wallet, Liabilities, Income, Income-GBP, Expenses.
    let accounts: Vec<String> = [1, 2, 3, 4, 5, 9, 6, 7, 8].iter().map(|n| format!("a000000000000000000000000000000{n}")).collect();
    assert_eq!(option("Account"), json!(accounts));
}

/// The default options (BR:108-238): two levels of accounts, so Bank, GBP Bank and Wallet sum into Asset
/// (`summarize`, HAT:1001-1004 through the account table), budget and actual for each of the six periods, no
/// difference and no total; the title names the budget, with a space where "using accumulated amounts" would go
/// (BR:727-735). Headers are the periods' start dates (BR:451-453).
#[test]
fn default_options() {
    let mut f = fixture();
    let result = f.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], "Budget Report: test budget ");
    let t = only(&result, "table");
    assert_eq!(labels(t), ["Root", "Asset", "Liabilities", "Income", "Income-GBP", "Expenses"]);
    let rows = t["rows"].as_array().unwrap();
    assert_eq!(
        row_texts(&rows[0]),
        ["03/01/24", "04/01/24", "05/01/24", "06/01/24", "07/01/24", "08/01/24"]
    );
    assert_eq!(rows[0]["cells"][1]["span"], 2, "a title spans its period's figures");
    assert_eq!(row_texts(&rows[1]), ["Bgt", "Act"].repeat(6));
    assert_eq!(
        row_texts(row(t, "Asset")),
        ["Asset", "$20.00", "$35.00", "$40.00", "-$20.00", ".", "$67.00", "$60.00", "$77.00", ".", "$0.00", ".", "$0.00"]
    );
    assert_eq!(t["columns"].as_array().unwrap().len(), 13);
}

/// "Show Budget" off leaves no budget cells, and so no notes, whatever "Show Budget Notes" says (BR:704-706); notes
/// off keep the budget cells but drop the footnotes (BR:370).
#[test]
fn notes_need_the_budget_column() {
    let mut f = fixture();
    let mut options = basic();
    options["Display"]["Show Budget"] = json!(false);
    let t = table(&mut f, &options);
    assert!(t.get("footnotes").is_none());
    assert_eq!(row_texts(row(&t, "Income"))[1..4], ["-$55.00", "$0.00", "$0.00"]);
    let mut options = basic();
    options["Display"]["Show Budget Notes"] = json!(false);
    let t = table(&mut f, &options);
    assert!(t.get("footnotes").is_none());
    assert_eq!(row_texts(row(&t, "Income"))[1..4], ["-$55.00", "-$55.00", "$0.00"]);
}

/// A collapsed column is titled from its first period's start to its last period's start (BR:467-470), and a column
/// of a single period by its start (BR:471-472).
#[test]
fn collapsed_columns_are_titled_by_their_periods_starts() {
    let mut f = fixture();
    let options = with_general(&json!({ "Report for range of budget periods": true, "Range start": "current", "Range end": "next" }));
    let t = table(&mut f, &options);
    assert_eq!(
        row_texts(&t["rows"][0]),
        ["03/01/24", "04/01/24", "05/01/24 to 08/01/24", "Total"]
    );
}

/// "previous" on the first period stays on it (`(max 0 …)`, BR:534), so previous to current is period 0 alone, with
/// periods 1-5 collapsed after it. Notes mark only columns of a single period (BR:422-424).
#[test]
fn previous_period_is_kept_within_the_budget() {
    let mut f = fixture();
    let options = with_general(&json!({ "Report for range of budget periods": true, "Range start": "previous", "Range end": "current" }));
    let t = table(&mut f, &options);
    assert_eq!(row_texts(&t["rows"][0]), ["03/01/24", "04/01/24 to 08/01/24", "Total"]);
    assert_eq!(t["footnotes"], json!(["income-0 -$60"]));
    assert_eq!(
        row_texts(row(&t, "Bank")),
        ["Bank", "$20.00", "$35.00", "-$15.00", "$100.00", "$124.00", "-$24.00", "$120.00", "$159.00", "-$39.00"]
    );
}

/// Relative periods when now is outside the budget (BR:525-526): before it, every relative choice is period 1, not
/// adjusted; after it, the number of periods, past the last. With "current" to "next" after the budget, the range
/// is period 6, the periods before collapse into one column and the periods after, `(range 7 6)`, are period 6 again
/// (BR:562-565). Period 6 has no budget amounts (unset past the count reads 0) and its actuals are the recurrence's
/// seventh month, September 2024, which has no splits.
#[test]
fn relative_periods_outside_the_budget() {
    let mut f = fixture();
    let options = with_general(&json!({ "Report for range of budget periods": true, "Range start": "current", "Range end": "next" }));
    let run = |f: &mut Fixture, now: &str| {
        let result = f.call("report.run", &json!({ "report": TEMPLATE, "options": options, "now": now }));
        only(&result, "table").clone()
    };
    let before = run(&mut f, "2023-06-01T00:00:00Z");
    assert_eq!(row_texts(&before["rows"][0]), ["03/01/24", "04/01/24", "05/01/24 to 08/01/24", "Total"]);
    let after = run(&mut f, "2025-01-01T00:00:00Z");
    assert_eq!(
        row_texts(&after["rows"][0]),
        ["03/01/24 to 08/01/24", "09/01/24", "09/01/24", "Total"]
    );
    assert_eq!(
        row_texts(row(&after, "Bank")),
        ["Bank", "$120.00", "$159.00", "-$39.00", ".", "$0.00", ".", ".", "$0.00", ".", "$120.00", "$159.00", "-$39.00"]
    );
}

/// An exact period is truncated to a whole number (BR:677-680): 2.9 is period 2 as the user counts it, period 1 here.
#[test]
fn exact_periods_are_truncated() {
    let mut f = fixture();
    let mut general = exact_periods();
    general["Exact start period"] = json!("2.9");
    general["Exact end period"] = json!("2.9");
    let t = table(&mut f, &with_general(&general));
    assert_eq!(row_texts(&t["rows"][0]), ["04/01/24", "Total"]);
}

/// A range that ends the period before it starts, collapsing nothing, leaves the total column without periods; GnuCash
/// fails taking the first of them (`(car total-periods)`, BR:413); so does red-ale.
#[test]
fn a_total_of_no_periods_fails_as_in_gnucash() {
    let mut f = fixture();
    let mut general = exact_periods();
    general["Exact start period"] = json!(3);
    general["Exact end period"] = json!(2);
    let params = json!({ "report": TEMPLATE, "options": with_general(&general) });
    let err = f.d.call("report.run", params).unwrap_err();
    assert_eq!(err.code, red_ale_rpc::codes::REPORT_SCRIPT);
}

/// "Use accumulated amounts" adds every period from the first: a period column is periods 0 to it, a collapsed
/// column only its own periods, and the total periods 0 to the last shown (BR:343-345, :418-421); the title says so.
#[test]
fn accumulated_amounts_in_every_kind_of_column() {
    let mut f = fixture();
    let general = json!({ "Use accumulated amounts": true, "Report for range of budget periods": true, "Range start": "current",
                          "Range end": "next" });
    let result = f.run(TEMPLATE, &with_general(&general));
    assert_eq!(result["title"], "Budget Report: test budget using accumulated amounts");
    let t = only(&result, "table");
    assert_eq!(
        row_texts(row(t, "Bank")),
        ["Bank", "$20.00", "$35.00", "-$15.00", "$60.00", "$15.00", "$45.00", "$60.00", "$144.00", "-$84.00", "$120.00", "$159.00",
         "-$39.00"]
    );
    // Period 0's column is still one period, so its note is marked; Expenses' period 1 is now periods 0-1 (BR:422-424).
    assert_eq!(t["footnotes"], json!(["income-0 -$60"]));
}

/// Reversed accounts (`gnc-reverse-balance`, BR:340-341): under `credit`, Income's budget, actual and difference
/// are negated; Expenses' are not.
#[test]
fn reversed_accounts_are_negated() {
    let mut f = Fixture::open_reversing("budget", "locale", true, "credit");
    let t = table(&mut f, &basic());
    assert_eq!(row_texts(row(&t, "Income"))[..5], ["Income", "$55.00 ", "1", "$55.00", "$0.00"]);
    assert_eq!(row_texts(row(&t, "Income"))[20..], ["$195.00", "$199.00", "-$4.00"]);
    assert_eq!(row_texts(row(&t, "Expenses"))[20..], ["$90.00", "$40.00", "$50.00"]);
}

/// Without "Include accounts with zero total balances and budget values" the account table omits an account whose
/// balance over the budget is zero, unless its rolled-up budget amounts are not (BR:687-689, HAT:749-756): GBP Bank,
/// Wallet, Liabilities and Income-GBP go.
#[test]
fn zero_accounts_are_omitted_on_request() {
    let mut f = fixture();
    let mut options = basic();
    options["Display"]["Include accounts with zero total balances and budget values"] = json!(false);
    let t = table(&mut f, &options);
    assert_eq!(labels(&t), ["Root", "Asset", "Bank", "Income", "Expenses"]);
}

/// "Always show sub-accounts" adds the descendants of the selected accounts (BR:656-658), and "Flatten list to depth
/// limit" shows accounts below the depth at it (BR:685-686).
#[test]
fn sub_accounts_and_flattening() {
    let mut f = fixture();
    let accounts = json!({ "Account Display Depth": "all", "Account": [ASSET] });
    let only_asset = table(&mut f, &json!({ "Accounts": accounts }));
    assert_eq!(labels(&only_asset), ["Asset"]);
    let mut with_subs = accounts.clone();
    with_subs["Always show sub-accounts"] = json!(true);
    let t = table(&mut f, &json!({ "Accounts": with_subs }));
    assert_eq!(labels(&t), ["Asset", "Bank", "GBP Bank", "Wallet"]);
    let flat = json!({ "Accounts": { "Account Display Depth": 1, "Flatten list to depth limit": true } });
    let t = table(&mut f, &flat);
    let depths: Vec<&Value> = t["rows"].as_array().unwrap().iter().filter(|r| r["role"] == "account").map(|r| &r["depth"]).collect();
    assert_eq!(labels(&t).len(), 9);
    assert!(depths.iter().all(|d| **d == json!(0)), "every account at the top level: {depths:?}");
}

/// "Link to actual transactions" links each actual to a Transaction Report on the account and its sub-accounts, from
/// the column's first period's start to its first period's end, or the total's last (BR:375-386, :411-415), in the
/// account's currency; the options both reports have are copied first (`gnc:make-report-anchor`,
/// `html-utilities.scm:167-185`).
#[test]
fn actuals_link_to_the_transaction_report() {
    let mut f = fixture();
    let mut options = basic();
    options["Display"]["Link to actual transactions"] = json!(true);
    let t = table(&mut f, &options);
    let bank = row(&t, "Bank");
    let link = &bank["cells"][2]["link"]["report"];
    assert_eq!(link["template"], "2fe3b9833af044abb929a88d5a59620f");
    assert_eq!(
        link["options"],
        json!({
            "General": { "Report name": "Budget Report", "Stylesheet": "Default", "Add options summary": "never",
                         "Start Date": { "absolute": "2024-03-01T00:00:00Z" }, "End Date": { "absolute": "2024-03-31T23:59:59Z" } },
            "Accounts": { "Accounts": [BANK] },
            "Currency": { "Common Currency": true, "Report's currency": "CURRENCY::USD" },
        })
    );
    let total = &bank["cells"][20]["link"]["report"]["options"]["General"];
    assert_eq!(total["End Date"], json!({ "absolute": "2024-08-31T23:59:59Z" }));
    let gbp = &row(&t, "GBP Bank")["cells"][2]["link"]["report"]["options"];
    assert_eq!(gbp["Currency"]["Report's currency"], "CURRENCY::GBP");
    assert_eq!(gbp["Accounts"]["Accounts"], json!([GBP_BANK]));
    let asset = &row(&t, "Asset")["cells"][2]["link"]["report"]["options"]["Accounts"]["Accounts"];
    assert_eq!(asset.as_array().unwrap().len(), 4, "Asset and its three sub-accounts");
}

/// With no budget in the book the report warns (BR:668-671); with no account selected it says so first (BR:663-666):
/// a new book has neither, and an empty account list reads as the default.
#[test]
fn warnings() {
    let mut f = Fixture::open("test-data", "locale", true);
    let result = f.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], TEMPLATE);
    assert_eq!(only(&result, "warning")["text"], "No budgets exist. You must create at least one budget.");
    let dir = tempfile::tempdir().unwrap();
    f.call("book.close", &json!({}));
    f.call("book.create", &json!({ "path": dir.path().join("new.gnucash").display().to_string() }));
    let result = f.run(TEMPLATE, &json!({}));
    assert_eq!(only(&result, "warning")["title"], "No accounts selected");
}
