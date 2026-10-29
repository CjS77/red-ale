//! The Cash Flow Barchart (`standard/cashflow-barchart.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/cashflow-barchart.scm`, cited as CB): the figures of GnuCash's own test
//! (`standard/test/test-cashflow-barchart.scm`) on `fixtures/reports/cashflow-barchart`, as XML and as its SQLite
//! copy, and hand-worked cases for the options and branches that test does not reach.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod chart_reports;

use chart_reports::{Charts, Expected, blocks_of, chart, check_read_only, check_run, table, values};
use serde_json::{Value, json};

const CHART: &str = "5426e4d987f6444387fe70880e5b28a0";
const CASH_FLOW: &str = "f8748b813fab4220ba26e743aedf38da";

/// The in-txn group's Bank and Wallet (`fixtures/reports/cashflow-barchart.expected.json`).
const IN_BANK: &str = "a0000000000000000000000000000003";
const IN_WALLET: &str = "a0000000000000000000000000000004";

/// Every run of `cashflow-barchart.expected.json` on `charts`; the number of checks.
fn expected_runs(charts: &mut Charts) -> usize {
    let expected = Expected::load("cashflow-barchart");
    let ids = ["test-in-txn", "test-out-txn", "test-null-txn"];
    assert_eq!(expected.0["runs"].as_array().unwrap().len(), ids.len());
    ids.iter().map(|id| check_run(charts, &expected, id)).sum()
}

/// test-cashflow-barchart.scm:60-196: 12 checks, as XML and as SQLite.
#[test]
fn expected_figures_of_gnucash_test() {
    assert_eq!(expected_runs(&mut Charts::open("cashflow-barchart.gnucash", "locale")), 12);
    assert_eq!(expected_runs(&mut Charts::open_sqlite("cashflow-barchart.gnucash", "locale")), 12);
}

/// The options of CB:63-119 in GnuCash's order.
#[test]
fn options_are_gnucash_s() {
    let mut charts = Charts::new("iso");
    let described = charts.call("report.options", json!({ "report": CHART }));
    let got: Vec<String> = described["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}/{} {} {}",
                o["section"].as_str().unwrap(),
                o["name"].as_str().unwrap(),
                o["key"],
                o["default"]
            )
        })
        .collect();
    let want = [
        r#"Accounts/Accounts "a" []"#,
        r#"Accounts/Include Trading Accounts in report "b" false"#,
        r#"Display/Show Money In "a" true"#,
        r#"Display/Show Money Out "b" true"#,
        r#"Display/Show Net Flow "c" true"#,
        r#"Display/Show Table "d" false"#,
        r#"Display/Plot Width "ea" {"percent":100}"#,
        r#"Display/Plot Height "eb" {"percent":100}"#,
        r#"General/Report name "0a" "Cash Flow Barchart""#,
        r#"General/Stylesheet "0b" "Default""#,
        r#"General/Add options summary "0c" "never""#,
        r#"General/Start Date "aa" {"relative":"start-accounting-period"}"#,
        r#"General/End Date "ab" {"relative":"end-accounting-period"}"#,
        r#"General/Step Size "b" "MonthDelta""#,
        r#"General/Report's currency "c" "CURRENCY::USD""#,
        r#"General/Price Source "d" "pricedb-nearest""#,
    ];
    assert_eq!(got, want);
}

/// The chart on the in-txn group from `from` to `to` by `step`, with `more` Display options.
fn in_txn(from: &str, to: &str, step: &str, more: Value) -> Value {
    let mut charts = Charts::open("cashflow-barchart.gnucash", "locale");
    let mut options = json!({
        "General": { "Start Date": { "absolute": from }, "End Date": { "absolute": to }, "Step Size": step },
        "Accounts": { "Accounts": [IN_WALLET, IN_BANK] },
        "Display": more,
    });
    if options["Display"].is_null() {
        options["Display"] = json!({});
    }
    charts.run(CHART, &options)
}

fn series_labels(result: &Value) -> Vec<String> {
    chart(result)["series"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["label"].as_str().unwrap().to_owned())
        .collect()
}

/// charts-daily's 2024 by month for Bank and Liability (scripts/report-fixtures/charts.py:56-66): in $3 (01-01, from
/// Equity), $29 (02-11), $11 (07-10), $8 (09-10); out $8 (01-11), $27 (02-22, from Liability), $15 (03-01); the two
/// voided transfers left out. Totals $51, $50 and $1, the sums of the rounded months (CB:244-252).
#[test]
fn a_year_of_charts_daily_by_month() {
    let mut charts = Charts::open("charts-daily.gnucash", "iso");
    let options = json!({
        "General": { "Start Date": { "absolute": "2024-01-01" }, "End Date": { "absolute": "2024-12-31" } },
        "Accounts": { "Accounts": ["a0000000000000000000000000000006", "a0000000000000000000000000000008"] },
        "Display": { "Show Table": true },
    });
    let result = charts.run(CHART, &options);
    let c = chart(&result);
    assert_eq!(c["type"], "bar");
    assert_eq!(c["title"], json!(["Cash Flow Barchart", "2024-01-01 to 2024-12-31"]));
    assert_eq!(c["labels"][0], "2024-01-01");
    assert_eq!(c["labels"][11], "2024-12-01");
    assert_eq!(c["yAxis"]["label"], "USD");
    let series = c["series"].as_array().unwrap();
    let colors: Vec<&str> = series.iter().map(|s| s["color"].as_str().unwrap()).collect();
    assert_eq!(
        colors,
        ["#0074d9", "#ff4136", "#2ecc40"],
        "GnuCash's colours, lower-cased by the result"
    );
    assert_eq!(values(&series[0]), ["3", "29", "0", "0", "0", "0", "11", "0", "8", "0", "0", "0"]);
    assert_eq!(values(&series[1]), ["8", "27", "15", "0", "0", "0", "0", "0", "0", "0", "0", "0"]);
    assert_eq!(values(&series[2]), ["-5", "2", "-15", "0", "0", "0", "11", "0", "8", "0", "0", "0"]);
    let t = table(&result);
    assert_eq!(t.header, ["Date", "Money In", "Money Out", "Net Flow"]);
    assert_eq!(t.rows.len(), 13);
    assert_eq!(t.rows[12], ["Total", "$51.00", "$50.00", "$1.00"]);
    assert_eq!(blocks_of(&result, "text")[0]["text"], "Overview:");
}

/// No accounts: the warning alone (CB:176, :329-332). An empty Accounts option reads as its default, so the book has
/// no account of the default types.
#[test]
fn no_accounts_give_the_warning() {
    let mut charts = Charts::new("iso");
    let book = charts.path().join("book.gnucash");
    charts.call("book.create", json!({ "path": book.display().to_string() }));
    charts.call(
        "account.create",
        json!({ "name": "Income", "type": "INCOME", "commodity": "CURRENCY::USD" }),
    );
    let result = charts.run(CHART, &json!({}));
    let warnings = blocks_of(&result, "warning");
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["title"], "No accounts selected");
    assert!(blocks_of(&result, "chart").is_empty());
}

/// All zero over in, out and net: the empty-data warning and no table, even with "Show Table" (CB:289-301).
#[test]
fn all_zero_gives_the_warning() {
    let result = in_txn("2024-02-01", "2024-02-29", "DayDelta", json!({ "Show Table": true }));
    assert_eq!(blocks_of(&result, "warning")[0]["title"], "No data");
    assert!(blocks_of(&result, "chart").is_empty());
    assert!(blocks_of(&result, "table").is_empty());
}

/// The zero test reads every series, shown or not: with only Money Out shown, all zero, the chart is still drawn
/// (CB:289-295), and the table shows the date and Money Out alone (CB:304-318).
#[test]
fn a_hidden_series_still_draws_the_chart() {
    let more = json!({ "Show Money In": false, "Show Net Flow": false, "Show Table": true });
    let result = in_txn("2024-03-01", "2024-03-03", "DayDelta", more);
    assert_eq!(series_labels(&result), ["Money Out"]);
    assert_eq!(values(&chart(&result)["series"][0]), ["0", "0", "0"]);
    let t = table(&result);
    assert_eq!(t.header, ["Date", "Money Out"]);
    assert_eq!(t.rows[3], ["Total", "$0.00"]);
}

/// Each "Show" option takes its series out of the chart and the table; none shown leaves an empty chart.
#[test]
fn each_show_flag() {
    [
        (json!({ "Show Money In": false }), vec!["Money Out", "Net Flow"]),
        (json!({ "Show Money Out": false }), vec!["Money In", "Net Flow"]),
        (json!({ "Show Net Flow": false }), vec!["Money In", "Money Out"]),
        (
            json!({ "Show Money In": false, "Show Money Out": false, "Show Net Flow": false }),
            vec![],
        ),
    ]
    .into_iter()
    .for_each(|(mut flags, want)| {
        flags["Show Table"] = json!(true);
        let result = in_txn("2024-03-01", "2024-03-03", "DayDelta", flags.clone());
        assert_eq!(series_labels(&result), want, "{flags}");
        let header: Vec<String> = std::iter::once("Date".to_owned())
            .chain(want.iter().map(|s| (*s).to_owned()))
            .collect();
        assert_eq!(table(&result).header, header, "{flags}");
    });
    let without = in_txn("2024-03-01", "2024-03-03", "DayDelta", json!({}));
    assert!(blocks_of(&without, "table").is_empty(), "no table by default");
}

/// Every value links to the Cash Flow over its interval on the chart's accounts, with the options both reports have
/// copied (CB:207-217; `5.17:gnucash/report/html-utilities.scm:168-185`), so the linked Cash Flow keeps this
/// report's name. Running the link gives that day's figures.
#[test]
fn values_link_to_the_cash_flow() {
    let mut charts = Charts::open("cashflow-barchart.gnucash", "locale");
    let options = json!({
        "General": { "Start Date": { "absolute": "2024-03-01" }, "End Date": { "absolute": "2024-03-03" }, "Step Size": "DayDelta",
                     "Price Source": "pricedb-latest" },
        "Accounts": { "Accounts": [IN_WALLET, IN_BANK] },
    });
    let result = charts.run(CHART, &options);
    let series = chart(&result)["series"].as_array().unwrap().clone();
    series.iter().for_each(|s| assert_eq!(s["links"], series[0]["links"]));
    let link = &series[0]["links"][1]["report"];
    assert_eq!(link["template"], CASH_FLOW);
    let want = json!({
        "General": {
            "Report name": "Cash Flow Barchart", "Stylesheet": "Default", "Add options summary": "never",
            "Start Date": { "absolute": "2024-03-02T00:00:00Z" }, "End Date": { "absolute": "2024-03-02T23:59:59Z" },
            "Report's currency": "CURRENCY::USD", "Price Source": "pricedb-latest",
        },
        "Accounts": { "Include Trading Accounts in report": false, "Account": [IN_WALLET, IN_BANK] },
    });
    assert_eq!(link["options"], want);
    let linked = charts.run(CASH_FLOW, &link["options"]);
    assert!(
        linked["title"].as_str().unwrap().starts_with("Cash Flow Barchart - "),
        "{}",
        linked["title"]
    );
    let html = charts.export(CASH_FLOW, &link["options"]);
    assert!(html.contains("$1.00"));
}

/// Weekly from 2024-03-01 to 03-10: the last interval is three days (DU:323-349). Swapped dates are swapped back:
/// from the end of 03-01 to the start of 03-03, so the 03-03 transfer falls outside.
#[test]
fn a_short_last_interval_and_swapped_dates() {
    let weekly = in_txn("2024-03-01", "2024-03-10", "WeekDelta", json!({}));
    assert_eq!(chart(&weekly)["labels"], json!(["03/01/24", "03/08/24"]));
    assert_eq!(values(&chart(&weekly)["series"][0]), ["6", "0"]);
    let swapped = in_txn("2024-03-03", "2024-03-01", "DayDelta", json!({}));
    assert_eq!(chart(&swapped)["labels"], json!(["03/01/24", "03/02/24"]));
    assert_eq!(values(&chart(&swapped)["series"][0]), ["1", "0"]);
}

/// The chart and its table read the Book and write nothing, on XML and SQLite.
#[test]
fn a_run_changes_nothing() {
    let options = json!({
        "General": { "Start Date": { "absolute": "2024-03-01" }, "End Date": { "absolute": "2024-03-03" }, "Step Size": "DayDelta" },
        "Accounts": { "Accounts": [IN_WALLET, IN_BANK] },
        "Display": { "Show Table": true },
    });
    check_read_only(&mut Charts::open("cashflow-barchart.gnucash", "locale"), &[CHART], &options);
    check_read_only(&mut Charts::open_sqlite("cashflow-barchart.gnucash", "locale"), &[CHART], &options);
}
