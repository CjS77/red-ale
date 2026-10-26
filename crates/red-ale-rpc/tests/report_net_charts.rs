//! The four net charts of `standard/net-charts.rhai` (`5.17:gnucash/report/reports/standard/net-charts.scm`, cited
//! below by line): the figures of GnuCash 5.17's own tests, ported in `fixtures/reports/*.expected.json`, and
//! hand-worked cases.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod chart_reports;

use chart_reports::{Charts, Expected, blocks_of, chart, check_read_only, check_run, table, values};
use serde_json::{Value, json};

const NET_WORTH_BARCHART: &str = "cbba1696c8c24744848062c7f1cf4a72";
const INCOME_EXPENSE_CHART: &str = "80769921e87943adade887b9835a7685";
const NET_WORTH_LINECHART: &str = "d8b63264186b11e19038001558291366";
const INCOME_EXPENSE_LINECHART: &str = "e533c998186b11e1b2e2001558291366";
const TEMPLATES: [&str; 4] = [NET_WORTH_BARCHART, INCOME_EXPENSE_CHART, NET_WORTH_LINECHART, INCOME_EXPENSE_LINECHART];

/// Every run of `<fixture>.expected.json`, each assertion a named check.
fn expected_runs(fixture: &str, date_format: &str, ids: &[&str]) {
    let expected = Expected::load(fixture);
    let mut charts = Charts::open(&format!("{fixture}.gnucash"), date_format);
    let checked: usize = ids.iter().map(|id| check_run(&mut charts, &expected, id)).sum();
    let all = expected.0["runs"].as_array().unwrap();
    assert_eq!(ids.len(), all.len(), "every run of {fixture} is checked");
    let assertions: usize = all.iter().map(|r| r["checks"].as_array().unwrap().len()).sum();
    assert_eq!(checked, assertions);
}

// test-charts.scm:116-146, book B: monthly, the closing transaction of 1970-01-03 excluded.
#[test]
fn test_charts_monthly_net_charts() {
    let ids = ["net-worth-barchart 3 months", "net-worth-linechart 3 months", "income-expense-barchart 3 months",
               "income-expense-linechart 3 months"];
    expected_runs("charts-net", "iso", &ids);
}

// test-charts.scm:201-238, book A with its 101 daily transactions.
#[test]
fn test_charts_daily_net_charts() {
    expected_runs("charts-daily", "iso", &["net-worth-barchart 2 months", "income-expense-barchart 2 years"]);
}

// test-standard-net-barchart.scm:58-311 and test-standard-net-linechart.scm:55-198, day steps.
#[test]
fn test_standard_net_barchart_and_linechart() {
    let ids = ["barchart single-txn-test", "barchart two-txn-test", "barchart two-txn-test-2", "barchart two-txn-test-income",
               "barchart closing-test", "linechart single-txn-test", "linechart two-txn-test", "linechart two-txn-test-2"];
    expected_runs("net-days", "locale", &ids);
}

// Hand-worked cases. Book A of charts-daily (scripts/report-fixtures/charts.py:56-80) dates nine transactions in
// 2024, each posted at 10:59 UTC: 01-01 Equity to Bank $3; 01-11 Bank to Expenses $8; 02-11 Income to Bank $29;
// 02-21 Income to Bank $10, voided; 02-22 Liability to Expenses $27; 03-01 Bank to Expenses $15; 05-10 Bank to
// Expenses $10, voided; 07-10 Expenses to Bank $11; 09-10 Income to Bank $8. Before 2024 Bank holds the $5,050 of
// the 1970 daily transfers from Income.

const BANK: &str = "a0000000000000000000000000000006";
const INCOME: &str = "a0000000000000000000000000000007";
const LIABILITY: &str = "a0000000000000000000000000000008";
const EXPENSES: &str = "a0000000000000000000000000000009";

fn daily() -> Charts {
    Charts::open("charts-daily.gnucash", "iso")
}

/// January to June 2024 by month, with the table.
fn half_year(accounts: &[&str]) -> Value {
    json!({ "General": { "Start Date": { "absolute": "2024-01-01" }, "End Date": { "absolute": "2024-06-30" } },
            "Accounts": { "Accounts": accounts }, "Display": { "Show table": true } })
}

fn with(mut options: Value, section: &str, name: &str, value: Value) -> Value {
    options[section][name] = value;
    options
}

/// Each series' label, colour and values.
fn series_values(result: &Value) -> Vec<(String, String, Vec<String>)> {
    let c = chart(result);
    let series = c["series"].as_array().unwrap();
    series.iter().map(|s| (s["label"].as_str().unwrap().to_owned(), s["color"].as_str().unwrap().to_owned(), values(s))).collect()
}

fn labels(result: &Value) -> Vec<String> {
    series_values(result).into_iter().map(|s| s.0).collect()
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

fn series(label: &str, color: &str, v: &[&str]) -> (String, String, Vec<String>) {
    (label.to_owned(), color.to_owned(), strings(v))
}

// Defaults (:65-158): this calendar year by month, weighted-average prices, every account neither income nor
// expense, both series and the net, no table. The dates are the end of 2024-01-01, the end of the first of each
// month, then the end of 2024-12-31 (:200-206); Bank's balance at each (:227-235), the Liability's negated (:360),
// and their sum (:306).
#[test]
fn net_worth_barchart_defaults() {
    let mut c = daily();
    let result = c.run("Net Worth Barchart", &json!({}));
    assert_eq!(result["title"], "", "GnuCash gives the document no title (:210)");
    let ch = chart(&result);
    assert_eq!(ch["type"], "bar");
    assert_eq!(ch["title"], json!(["Net Worth Barchart", "2024-01-01 to 2024-12-31"]), ":319-320");
    assert_eq!(ch["labels"].as_array().unwrap().len(), 13);
    assert_eq!((ch["labels"][0].as_str(), ch["labels"][12].as_str()), (Some("2024-01-01"), Some("2024-12-31")));
    assert_eq!((ch["grid"].as_bool(), ch["stacked"].as_bool()), (Some(true), Some(false)), "bars always have the grid (:196)");
    assert_eq!(ch["yAxis"], json!({ "label": "USD", "format": "currency", "commodity": "CURRENCY::USD" }), ":326-332");
    assert_eq!(ch["tooltip"], json!({ "grouped": true, "nonZeroOnly": true }), ":321-322");
    let assets = ["5053", "5045", "5059", "5059", "5059", "5059", "5059", "5070", "5070", "5078", "5078", "5078", "5078"];
    let liabilities = ["0", "0", "27", "27", "27", "27", "27", "27", "27", "27", "27", "27", "27"];
    let net = ["5053", "5045", "5032", "5032", "5032", "5032", "5032", "5043", "5043", "5051", "5051", "5051", "5051"];
    let want = vec![series("Assets", "#0074d9", &assets), series("Liabilities", "#ff4136", &liabilities),
                    series("Net Worth", "#2ecc40", &net)];
    assert_eq!(series_values(&result), want, ":336-384");
    assert_eq!(ch["series"][0]["fill"], true, "bars are filled (:342)");
    assert!(ch["series"][0].get("lineWidth").is_none(), "bars have no line width (:194)");
    assert_eq!(ch["series"][0]["displays"][0], "$5,053.00");
    assert!(blocks_of(&result, "table").is_empty(), "no table by default (:117-121)");
}

// Defaults of the Income/Expense Chart: the income and expense accounts, from the start of 2024-01-01; each month's
// change between consecutive dates (:270-281), so twelve labels. Income is the earlier balance less the later;
// Expense the same negated (:360); Net Profit their difference. The voided transactions count nothing.
#[test]
fn income_expense_chart_defaults() {
    let mut c = daily();
    let result = c.run("Income/Expense Chart", &json!({}));
    let ch = chart(&result);
    assert_eq!(ch["labels"].as_array().unwrap().len(), 12, "the last date starts no interval (:308-310)");
    assert_eq!(ch["labels"][11], "2024-12-01");
    let income = ["0", "29", "0", "0", "0", "0", "0", "0", "8", "0", "0", "0"];
    let expense = ["8", "27", "15", "0", "0", "0", "-11", "0", "0", "0", "0", "0"];
    let profit = ["-8", "2", "-15", "0", "0", "0", "11", "0", "8", "0", "0", "0"];
    let want = vec![series("Income", "#0074d9", &income), series("Expense", "#ff4136", &expense),
                    series("Net Profit", "#2ecc40", &profit)];
    assert_eq!(series_values(&result), want);
}

// The table shows the second series as computed (:423): liabilities and expenses with their sign, unlike the chart.
#[test]
fn show_table_keeps_the_signs_of_liabilities_and_expenses() {
    let mut c = daily();
    let net = table(&c.run("Net Worth Barchart", &half_year(&[BANK, LIABILITY])));
    assert_eq!(net.header, ["Date", "Assets", "Liabilities", "Net Worth"]);
    assert_eq!(net.rows.len(), 7, "the end of 01-01, of each first of the month to 06-01, and of 06-30");
    assert_eq!(net.rows[2], ["2024-03-01", "$5,059.00", "-$27.00", "$5,032.00"]);
    assert_eq!(net.rows[6], ["2024-06-30", "$5,059.00", "-$27.00", "$5,032.00"]);
    let ie = table(&c.run("Income/Expense Chart", &half_year(&[INCOME, EXPENSES])));
    assert_eq!(ie.header, ["Date", "Income", "Expense", "Net Profit"]);
    assert_eq!(ie.rows.len(), 6);
    assert_eq!(ie.rows[0], ["2024-01-01", "$0.00", "-$8.00", "-$8.00"]);
    assert_eq!(ie.rows[1], ["2024-02-01", "$29.00", "-$27.00", "$2.00"]);
    assert_eq!(ie.rows[2], ["2024-03-01", "$0.00", "-$15.00", "-$15.00"]);
}

// Step Size (:73-74, :200-206): by week from the end of 2024-01-01 the dates are 01-01, 01-08, … 01-29, then the
// end of 01-31; the $8 of 01-11 shows from 01-15.
#[test]
fn step_size_changes_the_dates() {
    let mut c = daily();
    let general = json!({ "Start Date": { "absolute": "2024-01-01" }, "End Date": { "absolute": "2024-01-31" }, "Step Size": "WeekDelta" });
    let options = json!({ "General": general, "Accounts": { "Accounts": [BANK] }, "Display": { "Show table": true } });
    let t = table(&c.run("Net Worth Barchart", &options));
    let dates: Vec<&str> = t.rows.iter().map(|r| r[0].as_str()).collect();
    assert_eq!(dates, ["2024-01-01", "2024-01-08", "2024-01-15", "2024-01-22", "2024-01-29", "2024-01-31"]);
    let assets: Vec<&str> = t.rows.iter().map(|r| r[1].as_str()).collect();
    assert_eq!(assets, ["$5,053.00", "$5,053.00", "$5,045.00", "$5,045.00", "$5,045.00", "$5,045.00"]);
}

// Show Asset & Liability and Show Net Worth (:99-115) choose the series and the table's columns (:336-384,
// :399-424).
#[test]
fn display_options_hide_series_and_columns() {
    let mut c = daily();
    let base = half_year(&[BANK, LIABILITY]);
    let no_sep = c.run("Net Worth Barchart", &with(base.clone(), "Display", "Show Asset & Liability", json!(false)));
    assert_eq!(labels(&no_sep), ["Net Worth"]);
    assert_eq!(table(&no_sep).header, ["Date", "Net Worth"]);
    assert_eq!(table(&no_sep).rows[2], ["2024-03-01", "$5,032.00"]);
    let no_net = c.run("Net Worth Barchart", &with(base, "Display", "Show Net Worth", json!(false)));
    assert_eq!(labels(&no_net), ["Assets", "Liabilities"]);
    assert_eq!(table(&no_net).rows[2], ["2024-03-01", "$5,059.00", "-$27.00"]);
    let ie = with(half_year(&[INCOME, EXPENSES]), "Display", "Show Income/Expense", json!(false));
    let ie = c.run("Income/Expense Chart", &with(ie, "Display", "Show Net Profit", json!(false)));
    assert!(chart(&ie)["series"].as_array().unwrap().is_empty());
    assert_eq!(table(&ie).header, ["Date"]);
}

// The line charts (:127-138, :194-196): unfilled lines of Line Width, 1.5 by default, and the Grid option.
#[test]
fn line_charts_take_line_width_and_grid() {
    let mut c = daily();
    let ch = chart(&c.run("Net Worth Linechart", &half_year(&[BANK, LIABILITY])));
    assert_eq!((ch["type"].as_str(), ch["grid"].as_bool()), (Some("line"), Some(true)));
    ch["series"].as_array().unwrap().iter().for_each(|s| {
        assert_eq!(s["lineWidth"], json!({ "num": "3", "denom": "2" }));
        assert!(s.get("fill").is_none(), "lines are not filled (:342)");
    });
    assert_eq!(values(&ch["series"][2]), strings(&["5053", "5045", "5032", "5032", "5032", "5032", "5032"]));
    let options = with(half_year(&[INCOME, EXPENSES]), "Display", "Line Width", json!("2.5"));
    let ch = chart(&c.run("Income & Expense Linechart", &with(options, "Display", "Grid", json!(false))));
    assert_eq!(ch["grid"], false);
    assert_eq!(ch["series"][1]["lineWidth"], json!({ "num": "5", "denom": "2" }));
    assert_eq!(values(&ch["series"][1]), strings(&["8", "27", "15", "0", "0", "0"]));
}

// The tooltip and plot size options reach the chart (:123-125, :146-154, :317-322).
#[test]
fn tooltip_and_plot_size_options_reach_the_chart() {
    let mut c = daily();
    let display = json!({ "Plot Width": { "pixels": 640 }, "Plot Height": { "percent": 50 },
                          "Show a grouped (indexed) tooltip": false, "Show only non-zero values in a tooltip": false });
    let ch = chart(&c.run("Net Worth Barchart", &json!({ "Accounts": { "Accounts": [BANK] }, "Display": display })));
    assert_eq!((ch["width"].clone(), ch["height"].clone()), (json!({ "pixels": 640 }), json!({ "percent": 50 })));
    assert_eq!(ch["tooltip"], json!({ "grouped": false, "nonZeroOnly": false }));
}

// The series link to the "Over Time" chart of the same kind (:344-375): this report's options copied where that
// chart has them, stacked, and named by its menu name. The accounts are copied only when that chart allows all of
// them (5.17:libgnucash/engine/gnc-option-impl.cpp:385-407): Bank is an asset, the Liability is not.
#[test]
fn series_link_to_the_category_charts() {
    let mut c = daily();
    let ch = chart(&c.run("Net Worth Barchart", &half_year(&[BANK, LIABILITY])));
    let assets = &ch["series"][0]["link"]["report"];
    assert_eq!(assets["template"], "e9cf815f79db44bcb637d0295093ae3d");
    let options = &assets["options"];
    assert_eq!(options["Display"]["Use Stacked Charts"], true);
    assert_eq!(options["General"]["Report name"], "Asset Chart");
    assert_eq!(options["Display"]["Show table"], true, "copied");
    assert_eq!(options["General"]["Price Source"], "weighted-average", "copied");
    assert!(options["Accounts"].get("Accounts").is_none(), "not every account is an asset");
    assert_eq!(ch["series"][1]["link"]["report"]["template"], "faf410e8f8da481fbc09e4763da40bcc");
    assert_eq!(ch["series"][1]["link"]["report"]["options"]["General"]["Report name"], "Liability Chart");
    assert!(ch["series"][2].get("link").is_none(), "the net has no link (:377-384)");
    let ch = chart(&c.run("Income/Expense Chart", &half_year(&[INCOME])));
    let income = &ch["series"][0]["link"]["report"];
    assert_eq!(income["template"], "44f81bee049b4b3ea908f8dac9a9474e");
    assert_eq!(income["options"]["General"]["Report name"], "Income Chart");
    assert_eq!(income["options"]["Accounts"]["Accounts"], json!([INCOME]));
    assert_eq!(ch["series"][1]["link"]["report"]["options"]["General"]["Report name"], "Expense Chart");
}

// All-zero data gives the empty-data warning (:386-389, :445-448); no accounts at all the no-account warning
// (:450-454). An account list left empty reads as its default, which is empty only in a book with no accounts.
#[test]
fn warnings_for_no_data_and_no_accounts() {
    let mut c = daily();
    let general = json!({ "Start Date": { "absolute": "2024-01-01" }, "End Date": { "absolute": "2024-01-31" } });
    let result = c.run("Net Worth Barchart", &json!({ "General": general, "Accounts": { "Accounts": [LIABILITY] } }));
    let warnings = blocks_of(&result, "warning");
    assert_eq!((warnings.len(), warnings[0]["title"].as_str()), (1, Some("No data")));
    let mut empty = Charts::new("iso");
    let book = empty.path().join("empty.gnucash").display().to_string();
    empty.call("book.create", json!({ "path": book }));
    TEMPLATES.iter().for_each(|t| {
        let result = empty.run(t, &json!({}));
        assert_eq!(blocks_of(&result, "warning")[0]["title"], "No accounts selected", "{t}");
        assert!(blocks_of(&result, "chart").is_empty());
    });
}

// Report's currency and Price Source (:76-81, :216-223, :285-292) on test-data (account_summary.py:35-70): GBP
// Bank holds £612 from 1978; on 2000-01-15 £10 go to Bank as $14, adding a price of USD in GBP 10/14; on
// 2000-02-15 $9 come back as £6, adding a price of GBP in USD 9/6. Bank holds $2,181 before 2000, $2,195 after the
// first exchange and $2,186 after the second. By month from the end of 2000-01-01, GBP Bank holds £612, £602, £608,
// £608.
#[test]
fn report_currency_and_price_source_exchange_foreign_accounts() {
    let mut c = Charts::open("test-data.gnucash", "iso");
    let (bank, gbp_bank) = ("a0000000000000000000000000000003", "a0000000000000000000000000000004");
    let mut assets = |source: &str, currency: &str| -> Vec<String> {
        let general = json!({ "Start Date": { "absolute": "2000-01-01" }, "End Date": { "absolute": "2000-03-31" },
                              "Price Source": source, "Report's currency": currency });
        let options = json!({ "General": general, "Accounts": { "Accounts": [bank, gbp_bank] }, "Display": { "Show table": true } });
        table(&c.run("Net Worth Barchart", &options)).rows.iter().map(|r| r[1].clone()).collect()
    };
    // Nearest: the 01-15 price for 01-01 ($1.40 a pound), the 02-15 one after ($1.50).
    assert_eq!(assets("pricedb-nearest", "CURRENCY::USD"), ["$3,037.80", "$3,098.00", "$3,098.00", "$3,098.00"]);
    // Latest: $1.50 throughout.
    assert_eq!(assets("pricedb-latest", "CURRENCY::USD"), ["$3,099.00", "$3,098.00", "$3,098.00", "$3,098.00"]);
    // Before: no price before 01-15, so the pounds count nothing; then $1.40 until 02-15.
    assert_eq!(assets("pricedb-before", "CURRENCY::USD"), ["$2,181.00", "$3,037.80", "$3,098.00", "$3,098.00"]);
    // Weighted average of the exchanges up to each day (5.17:gnucash/report/commodity-utilities.scm:829-876): none
    // by 01-01, $14 for £10 by 02-01, $23 for £16 after 02-15.
    assert_eq!(assets("weighted-average", "CURRENCY::USD"), ["$2,181.00", "$3,037.80", "$3,060.00", "$3,060.00"]);
    // In pounds, nearest: Bank's dollars at 10/14 for 01-01 ($2,181 is £1,557.857…, rounded to £1,557.86 before it
    // is added, :219-222), then at 6/9.
    assert_eq!(assets("pricedb-nearest", "CURRENCY::GBP"), ["#2,169.86", "#2,065.33", "#2,065.33", "#2,065.33"]);
}

// Running every template leaves the Book as it was, writes nothing, and exports the same bytes twice with no script.
#[test]
fn reports_only_read_and_export_deterministically() {
    let mut c = daily();
    check_read_only(&mut c, &TEMPLATES, &half_year(&[BANK, LIABILITY, INCOME, EXPENSES]));
    check_read_only(&mut c, &TEMPLATES, &json!({}));
}
