//! `report.exportCsv`: the CSV export of the net charts (`5.17:gnucash/report/reports/standard/net-charts.scm:433-445`)
//! and the category bar charts (`5.17:gnucash/report/reports/standard/category-barchart.scm:674-700`), written as
//! `gnc:lists->csv` writes it (`5.17:gnucash/report/trep-engine.scm:463-512`): quoted text, numbers as Guile prints
//! them once inexact (`23500.0`), rows joined by `\n` with none after the last. The figures are the ones the chart
//! tests work out by hand (`report_net_charts.rs` on `charts-daily`, `report_category_barchart.rs` on
//! `category-multi`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod chart_reports;

use chart_reports::Charts;
use serde_json::{Value, json};

fn daily() -> Charts {
    Charts::open("charts-daily.gnucash", "iso")
}

fn multi() -> Charts {
    Charts::open("category-multi.gnucash", "locale")
}

fn csv(c: &mut Charts, report: &str, options: &Value) -> String {
    let result = c.call("report.exportCsv", json!({ "report": report, "options": options }));
    assert_eq!(result["title"], "", "GnuCash gives the charts no title");
    result["csv"].as_str().unwrap().to_owned()
}

/// Rows of `"date",a,b,c` for the dates and figures given.
fn rows(header: &str, lines: &[(&str, &str)]) -> String {
    let body = lines.iter().map(|(date, figures)| format!("\"{date}\",{figures}"));
    std::iter::once(header.to_owned()).chain(body).collect::<Vec<_>>().join("\n")
}

/// The thirteen dates of the net worth defaults: the end of 2024-01-01, of the first of each month, and of
/// 2024-12-31 (:200-206), printed `%Y-%m-%d` (:434, :442). Assets, Liabilities as computed (a credit balance is
/// negative; the chart negates it, :360, the CSV does not), and Net Worth (:306).
#[test]
fn net_worth_defaults() {
    let mut c = daily();
    let lines = [
        ("2024-01-01", "5053.0,0.0,5053.0"),
        ("2024-02-01", "5045.0,0.0,5045.0"),
        ("2024-03-01", "5059.0,-27.0,5032.0"),
        ("2024-04-01", "5059.0,-27.0,5032.0"),
        ("2024-05-01", "5059.0,-27.0,5032.0"),
        ("2024-06-01", "5059.0,-27.0,5032.0"),
        ("2024-07-01", "5059.0,-27.0,5032.0"),
        ("2024-08-01", "5070.0,-27.0,5043.0"),
        ("2024-09-01", "5070.0,-27.0,5043.0"),
        ("2024-10-01", "5078.0,-27.0,5051.0"),
        ("2024-11-01", "5078.0,-27.0,5051.0"),
        ("2024-12-01", "5078.0,-27.0,5051.0"),
        ("2024-12-31", "5078.0,-27.0,5051.0"),
    ];
    let want = rows("\"Date\",\"Assets\",\"Liabilities\",\"Net Worth\"", &lines);
    assert_eq!(csv(&mut c, "Net Worth Barchart", &json!({})), want);
    // The CSV has all four columns whatever the Display options show (:436-441), and the line chart's is the same.
    let hidden = json!({ "Display": { "Show Asset & Liability": false, "Show Net Worth": false } });
    assert_eq!(csv(&mut c, "Net Worth Barchart", &hidden), want);
    assert_eq!(csv(&mut c, "Net Worth Linechart", &json!({})), want);
}

/// Income and expense are changes between consecutive dates, so the last date starts no interval and is left out
/// (:308-310); the dates are the start of each day (:201-203).
#[test]
fn income_expense_defaults() {
    let mut c = daily();
    let lines = [
        ("2024-01-01", "0.0,-8.0,-8.0"),
        ("2024-02-01", "29.0,-27.0,2.0"),
        ("2024-03-01", "0.0,-15.0,-15.0"),
        ("2024-04-01", "0.0,0.0,0.0"),
        ("2024-05-01", "0.0,0.0,0.0"),
        ("2024-06-01", "0.0,0.0,0.0"),
        ("2024-07-01", "0.0,11.0,11.0"),
        ("2024-08-01", "0.0,0.0,0.0"),
        ("2024-09-01", "8.0,0.0,8.0"),
        ("2024-10-01", "0.0,0.0,0.0"),
        ("2024-11-01", "0.0,0.0,0.0"),
        ("2024-12-01", "0.0,0.0,0.0"),
    ];
    let want = rows("\"Date\",\"Income\",\"Expense\",\"Net Profit\"", &lines);
    assert_eq!(csv(&mut c, "Income/Expense Chart", &json!({})), want);
    assert_eq!(csv(&mut c, "Income & Expense Linechart", &json!({})), want);
}

/// March 2024 by week, at most three bars: Auto, Groceries, then "Other" for the rest (:554-574), and the Grand Total
/// (:690-698). Each account gets $N on day N, so a week holds the sum of its days (1-7: 28, 8-14: 77, 15-21: 126,
/// 22-28: 175, 29-31: 90); Auto with its three children counts four accounts, Other (Rent and Expenses) two. A
/// percentage chart exports amounts, not the shares it draws (:686-689 take the rows, not the ratios of :611).
#[test]
fn category_chart_with_other_and_percentages() {
    let mut c = multi();
    let general = json!({ "Start Date": { "absolute": "2024-03-01" }, "End Date": { "absolute": "2024-03-31" }, "Step Size": "WeekDelta" });
    let options = json!({ "General": general, "Display": { "Maximum Bars": 3, "Percentage chart": true } });
    let lines = [
        ("2024-03-01", "112.0,28.0,56.0,196.0"),
        ("2024-03-08", "308.0,77.0,154.0,539.0"),
        ("2024-03-15", "504.0,126.0,252.0,882.0"),
        ("2024-03-22", "700.0,175.0,350.0,1225.0"),
        ("2024-03-29", "360.0,90.0,180.0,630.0"),
    ];
    let want = rows("\"Date\",\"Auto\",\"Groceries\",\"Other\",\"Grand Total\"", &lines);
    assert_eq!(csv(&mut c, "Expense Over Time", &options), want);
}

/// One series has no Grand Total (:695-697). Averaged by week over a year by month (:299-320), each amount is
/// multiplied by 12 / 52.285714 (366 days in weeks, rounded to 6 decimals): March's $3,472 (seven accounts at $496)
/// is 20832000000/26142857, whose nearest double prints as 796.8524633707785.
#[test]
fn averaged_single_series() {
    let mut c = multi();
    let options = json!({ "General": { "Show Average": "WeekDelta" }, "Accounts": { "Levels of Subaccounts": 1 } });
    let dates: Vec<String> = (1..=12).map(|m| format!("2024-{m:02}-01")).collect();
    let lines: Vec<(&str, &str)> = dates
        .iter()
        .enumerate()
        .map(|(i, d)| (d.as_str(), if i == 2 { "796.8524633707785" } else { "0.0" }))
        .collect();
    assert_eq!(csv(&mut c, "Expense Over Time", &options), rows("\"Date\",\"Expenses\"", &lines));
}

#[test]
fn only_the_charts_export_csv() {
    let mut c = daily();
    let list = c.call("report.list", json!({}));
    let reports = list["reports"].as_array().unwrap();
    let with_csv: Vec<&str> = reports
        .iter()
        .filter(|r| r["exportTypes"] == json!(["csv"]))
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(with_csv.len(), 8, "the four net and four category charts: {with_csv:?}");
    let e = c.err("report.exportCsv", json!({ "report": "Balance Sheet" }));
    assert_eq!((e.code, e.data.unwrap()["reason"].clone()), (2001, json!("no-export")));
    // GnuCash's "No exportable data" (:456-457): nothing in the period, or no accounts selected. An account list
    // left empty reads as its default, so "no accounts" takes a book with none.
    let quiet = json!({ "General": { "Start Date": { "absolute": "1960-01-01" }, "End Date": { "absolute": "1960-12-31" } } });
    ["Net Worth Barchart", "Expense Over Time"].iter().for_each(|report| {
        let e = c.err("report.exportCsv", json!({ "report": report, "options": quiet }));
        assert_eq!(e.data.unwrap()["reason"], "no-exportable-data", "{report}");
    });
    let mut empty = Charts::new("iso");
    let book = empty.path().join("empty.gnucash").display().to_string();
    empty.call("book.create", json!({ "path": book }));
    let e = empty.err("report.exportCsv", json!({ "report": "Net Worth Barchart" }));
    assert_eq!(e.data.unwrap()["reason"], "no-exportable-data");
}

/// A saved configuration of a chart exports with its options, and `report.run` carries no export rows.
#[test]
fn saved_configurations_export_and_runs_carry_no_rows() {
    let mut c = daily();
    let options = json!({ "General": { "Step Size": "QuarterDelta" } });
    let saved = c.call(
        "report.saveConfiguration",
        json!({ "name": "Quarterly", "report": "Net Worth Barchart", "options": options }),
    );
    assert_eq!(saved["exportTypes"], json!(["csv"]));
    let text = csv(&mut c, "Quarterly", &json!({}));
    let dates: Vec<&str> = text.lines().skip(1).map(|l| l.split(',').next().unwrap()).collect();
    assert_eq!(
        dates,
        [
            "\"2024-01-01\"",
            "\"2024-04-01\"",
            "\"2024-07-01\"",
            "\"2024-10-01\"",
            "\"2024-12-31\""
        ]
    );
    let run = c.call("report.run", json!({ "report": "Quarterly" }));
    assert!(run.get("export").is_none());
    let before = c.files();
    assert_eq!(csv(&mut c, "Quarterly", &json!({})), text, "the same bytes every time");
    assert_eq!(c.files(), before, "nothing is written");
}
