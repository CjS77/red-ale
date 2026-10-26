//! The Average Balance report (`example/average-balance.rhai`) through `report.run` and `report.export`: every
//! assertion of 5.17's `test-average-balance.scm` (`fixtures/reports/average-balance.expected.json`), then
//! hand-worked cases for the default options and each option that changes a figure, each worked out from the
//! Scheme. `AB` stands for `5.17:gnucash/report/reports/example/average-balance.scm`, `TAB` for
//! `5.17:gnucash/report/reports/standard/test/test-average-balance.scm`.
//!
//! How a figure comes about (AB:141-254, :345-373): the daily dates run from the start of the first day to the end of
//! the last, a day apart, and the first is skipped, so each daily balance is the one at the start of the next day
//! (the last at the report's end). Each step's row takes the balances whose dates fall within it, up to and including
//! the start of the next step, and the amounts of the splits posted up to then.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod report_runs;

use std::cmp::Ordering;

use proptest::prelude::{ProptestConfig, proptest};
use report_runs::{Fixture, blocks_of, html_table, only, rational, table_rows, values};
use serde_json::{Value, json};

const TEMPLATE: &str = "Average Balance";

/// Accounts of `average-balance.gnucash`.
const BANK: &str = "a0000000000000000000000000000002";
const ANOTHER_BANK: &str = "a0000000000000000000000000000003";

/// Accounts of `balsheet-pnl.gnucash`.
const BANK1: &str = "a0000000000000000000000000000003";
const FOREIGN_SAVINGS: &str = "a000000000000000000000000000000a";

/// `default-testing-options` (TAB:53-61): Bank and Another Bank, the table shown, January to June 1979.
fn testing_options() -> Value {
    json!({
        "Accounts": { "Accounts": [BANK, ANOTHER_BANK] },
        "Display": { "Show table": true },
        "General": { "Start Date": { "absolute": "1979-01-01" }, "End Date": { "absolute": "1979-06-30" } },
    })
}

/// `options` with `section`/`name` set to `value`.
fn with(mut options: Value, section: &str, name: &str, value: Value) -> Value {
    options[section][name] = value;
    options
}

/// The table's rows, as `report.run` displays them and as the exported page shows them, which must agree.
fn rows(f: &mut Fixture, options: &Value) -> Vec<Vec<String>> {
    let result = f.run(TEMPLATE, options);
    let shown = table_rows(only(&result, "table"));
    let html = f.export(TEMPLATE, options);
    let table_index = 0;
    assert_eq!(html_table(&html, table_index), shown, "the exported table shows the run's figures");
    shown
}

/// Column `col` (from 0) of `rows`.
fn column(rows: &[Vec<String>], col: usize) -> Vec<&str> {
    rows.iter().map(|r| r[col].as_str()).collect()
}

/// Rows as owned strings, for comparing with [`rows`].
fn expect(rows: &[[&str; 8]]) -> Vec<Vec<String>> {
    rows.iter().map(|r| r.iter().map(|c| (*c).to_owned()).collect()).collect()
}

// Expected figures from 5.17 (TAB:66-99). The fixture holds all three transfers; the first run's figures hold on it
// (fixtures/reports/average-balance.expected.json, "book").

#[test]
fn test_average_balance_default_run() {
    let mut f = Fixture::open("average-balance", "locale", true);
    let rows = rows(&mut f, &testing_options());
    // TAB:68 "averages".
    assert_eq!(column(&rows, 2), ["$0.00", "$50.00", "$100.00", "$150.00", "$200.00", "$200.00"]);
    // TAB:71 "maximums".
    assert_eq!(column(&rows, 3), ["$0.00", "$100.00", "$100.00", "$200.00", "$200.00", "$200.00"]);
    // TAB:74 "minimums".
    assert_eq!(column(&rows, 4), ["$0.00", "$0.00", "$100.00", "$100.00", "$200.00", "$200.00"]);
    // TAB:77 "net".
    assert_eq!(column(&rows, 7), ["$0.00", "$100.00", "$0.00", "$100.00", "$0.00", "$0.00"]);
}

#[test]
fn test_average_balance_include_internal() {
    let mut f = Fixture::open("average-balance", "locale", true);
    let rows = rows(&mut f, &testing_options());
    // TAB:84 "gains-include-internal".
    assert_eq!(column(&rows, 5), ["$0.00", "$100.00", "$25.00", "$100.00", "$0.00", "$0.00"]);
    // TAB:87 "loss-include-internal".
    assert_eq!(column(&rows, 6), ["$0.00", "$0.00", "$25.00", "$0.00", "$0.00", "$0.00"]);
}

#[test]
fn test_average_balance_exclude_internal() {
    let mut f = Fixture::open("average-balance", "locale", true);
    let options = with(testing_options(), "Accounts", "Exclude transactions between selected accounts", json!(true));
    let rows = rows(&mut f, &options);
    // TAB:94 "gain-exclude-internal".
    assert_eq!(column(&rows, 5), ["$0.00", "$100.00", "$0.00", "$100.00", "$0.00", "$0.00"]);
    // TAB:97 "loss-exclude-internal".
    assert_eq!(column(&rows, 6), ["$0.00", "$0.00", "$0.00", "$0.00", "$0.00", "$0.00"]);
}

// Hand-worked cases.

/// The default options: the accounting period, here today to today, has no splits, so there is no data (AB:377-380)
/// and the chart gives way to the empty-data warning (AB:435-451); the table is off by default (AB:104-106).
#[test]
fn default_options_show_the_empty_data_warning() {
    let mut f = Fixture::open("average-balance", "locale", true);
    let result = f.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], "Average Balance");
    let warning = only(&result, "warning");
    assert_eq!(warning["title"], "No data");
    assert!(blocks_of(&result, "table").is_empty() && blocks_of(&result, "chart").is_empty());
}

/// The default accounts are the asset and liability accounts (AB:84-101): Root, Bank and Another Bank, and with the
/// default "Include Sub-Accounts" their descendants (AB:283-286), which bring in Income. Every transfer then has both
/// sides selected, so the summed balance is always zero while each month's gains and losses are the transfers'
/// amounts. Every average is zero, so the Average series is left out and the chart gives way to the empty-data
/// warning (AB:391-402, :435-451).
#[test]
fn default_accounts_include_the_income_account_below_root() {
    let mut f = Fixture::open("average-balance", "locale", true);
    let options = json!({
        "Display": { "Show table": true },
        "General": { "Start Date": { "absolute": "1979-01-01" }, "End Date": { "absolute": "1979-06-30" } },
    });
    let rows = rows(&mut f, &options);
    [2, 3, 4, 7].into_iter().for_each(|col| assert_eq!(column(&rows, col), ["$0.00"; 6], "column {col}"));
    assert_eq!(column(&rows, 5), ["$0.00", "$100.00", "$25.00", "$100.00", "$0.00", "$0.00"]);
    assert_eq!(column(&rows, 6), ["$0.00", "$100.00", "$25.00", "$100.00", "$0.00", "$0.00"]);
    assert_eq!(only(&f.run(TEMPLATE, &options), "warning")["title"], "No data");
    // Every transfer has two splits, both selected, so excluding them leaves no gains or losses (AB:220-233).
    let excluded = with(options, "Accounts", "Exclude transactions between selected accounts", json!(true));
    let rows = self::rows(&mut f, &excluded);
    [5, 6, 7].into_iter().for_each(|col| assert_eq!(column(&rows, col), ["$0.00"; 6], "column {col}"));
}

/// Step Size "DayDelta", 14 to 16 February 1979: one row per day. The Bank's $100 arrives on the 15th at 10:59, so
/// the 14th's row has the balance at the start of the 15th, $0, and the 15th's row the split and the balance at the
/// start of the 16th; the last row's balance is the one at the report's end (AB:161-173, :177-198, :202-213).
#[test]
fn step_size_day() {
    let mut f = Fixture::open("average-balance", "locale", true);
    let options = json!({
        "Accounts": { "Accounts": [BANK, ANOTHER_BANK] },
        "Display": { "Show table": true },
        "General": { "Start Date": { "absolute": "1979-02-14" }, "End Date": { "absolute": "1979-02-16" }, "Step Size": "DayDelta" },
    });
    let want = [
        ["02/14/79", "02/14/79", "$0.00", "$0.00", "$0.00", "$0.00", "$0.00", "$0.00"],
        ["02/15/79", "02/15/79", "$100.00", "$100.00", "$100.00", "$100.00", "$0.00", "$100.00"],
        ["02/16/79", "02/16/79", "$100.00", "$100.00", "$100.00", "$0.00", "$0.00", "$0.00"],
    ];
    assert_eq!(rows(&mut f, &options), expect(&want));
}

/// Step Size "QuarterDelta", January to June 1979: steps start on 1 January and 1 April (lib/dates' make_date_list
/// of 5.17:bindings/guile/date-utilities.scm:356-381). The first quarter has 90 daily balances (2 January to
/// 1 April), 45 of them $100 (from 16 February): $50. The second has 91 (2 April to 30 June and the end), 15 at $100
/// and 76 at $200: 16,700 / 91 = $183.5164…, shown $183.52. The first quarter's splits are Bank's $100 and the
/// internal $25 out of Bank into Another Bank.
#[test]
fn step_size_quarter() {
    let mut f = Fixture::open("average-balance", "locale", true);
    let options = with(testing_options(), "General", "Step Size", json!("QuarterDelta"));
    let want = [
        ["01/01/79", "03/31/79", "$50.00", "$100.00", "$0.00", "$125.00", "$25.00", "$100.00"],
        ["04/01/79", "06/30/79", "$183.52", "$200.00", "$100.00", "$100.00", "$0.00", "$100.00"],
    ];
    assert_eq!(rows(&mut f, &options), expect(&want));
}

/// Step Size "YearDelta" over 1979: one row of 365 daily balances (2 January to 31 December and the end), 45 at $0,
/// 60 at $100 (16 February to 16 April) and 260 at $200: 58,000 / 365 = $158.9041…, shown $158.90.
#[test]
fn step_size_year() {
    let mut f = Fixture::open("average-balance", "locale", true);
    let options = json!({
        "Accounts": { "Accounts": [BANK, ANOTHER_BANK] },
        "Display": { "Show table": true },
        "General": { "Start Date": { "absolute": "1979-01-01" }, "End Date": { "absolute": "1979-12-31" }, "Step Size": "YearDelta" },
    });
    let want = [["01/01/79", "12/31/79", "$158.90", "$200.00", "$0.00", "$225.00", "$25.00", "$200.00"]];
    assert_eq!(rows(&mut f, &options), expect(&want));
}

/// No splits in the report period: no data (AB:377-380), so the table has no rows and the chart gives way to the
/// empty-data warning, though the accounts hold $200 throughout.
#[test]
fn no_splits_in_range() {
    let mut f = Fixture::open("average-balance", "locale", true);
    let options = json!({
        "Accounts": { "Accounts": [BANK, ANOTHER_BANK] },
        "Display": { "Show table": true },
        "General": { "Start Date": { "absolute": "1979-07-01" }, "End Date": { "absolute": "1979-12-31" } },
    });
    assert!(rows(&mut f, &options).is_empty());
    let result = f.run(TEMPLATE, &options);
    assert_eq!(only(&result, "warning")["title"], "No data");
    assert_eq!(only(&result, "table")["columns"].as_array().unwrap().len(), 8, "the table keeps its headings (AB:458-459)");
}

/// Plot Type: each choice adds its series (AB:391-433), in the order Average, Profit, then Gain and Loss together,
/// with GnuCash's colours, over the periods' starts. The values are the exact figures.
#[test]
fn plot_types_choose_the_series() {
    let mut f = Fixture::open("average-balance", "locale", true);
    let options = with(testing_options(), "Display", "Plot Type", json!(["AvgBalPlot", "GainPlot", "GLPlot"]));
    let result = f.run(TEMPLATE, &options);
    let chart = only(&result, "chart");
    assert_eq!(chart["type"], "bar");
    assert_eq!(chart["labels"], json!(["01/01/79", "02/01/79", "03/01/79", "04/01/79", "05/01/79", "06/01/79"]));
    assert_eq!(chart["yAxis"]["format"], "currency");
    assert_eq!(chart["yAxis"]["commodity"], "CURRENCY::USD");
    let series = chart["series"].as_array().unwrap();
    let seen: Vec<(&str, &str, Vec<String>)> =
        series.iter().map(|s| (s["label"].as_str().unwrap(), s["color"].as_str().unwrap(), values(s))).collect();
    let strings = |v: [&str; 6]| v.map(str::to_owned).to_vec();
    assert_eq!(seen, [
        ("Average", "#0074d9", strings(["0", "50", "100", "150", "200", "200"])),
        ("Profit", "#2ecc40", strings(["0", "100", "0", "100", "0", "0"])),
        ("Gain", "#111111", strings(["0", "100", "25", "100", "0", "0"])),
        ("Loss", "#ff4136", strings(["0", "0", "25", "0", "0", "0"])),
    ]);
    let html = f.export(TEMPLATE, &options);
    assert!(html.contains("<svg"), "the chart is drawn");
    assert_eq!(series[0]["displays"][1], "$50.00", "hover labels in the report's currency");
}

/// With "Exclude transactions between selected accounts" the losses are all zero, but GnuCash adds both Gain/Loss
/// series unless both are (AB:421-433). Show plot off leaves only the table (AB:386).
#[test]
fn gain_loss_series_and_show_plot() {
    let mut f = Fixture::open("average-balance", "locale", true);
    let options = with(testing_options(), "Display", "Plot Type", json!(["GLPlot"]));
    let options = with(options, "Accounts", "Exclude transactions between selected accounts", json!(true));
    let result = f.run(TEMPLATE, &options);
    let series = only(&result, "chart")["series"].as_array().unwrap().clone();
    assert_eq!(series.len(), 2);
    assert_eq!(values(&series[0]), ["0", "100", "0", "100", "0", "0"]);
    assert_eq!(values(&series[1]), ["0"; 6]);
    let options = with(options, "Display", "Show plot", json!(false));
    let result = f.run(TEMPLATE, &options);
    assert!(blocks_of(&result, "chart").is_empty() && blocks_of(&result, "warning").is_empty());
    assert_eq!(blocks_of(&result, "table").len(), 1);
}

/// Include Sub-Accounts, on `balsheet-pnl` in January 1970. On (the default), Bank1 brings in Savings ($100),
/// Bonds ($2,000), Empty and Current ($9,000 in, $130 and $2,000 out), all posted on 1 January: a constant $8,970,
/// gains $11,100, losses $2,130. Off, Bank1 alone has no splits and there is no data (AB:283-286, :377-380).
#[test]
fn include_sub_accounts() {
    let mut f = Fixture::open("balsheet-pnl", "iso", true);
    let options = json!({
        "Accounts": { "Accounts": [BANK1] },
        "Display": { "Show table": true },
        "General": { "Start Date": { "absolute": "1970-01-01" }, "End Date": { "absolute": "1970-01-31" } },
    });
    let want = [["1970-01-01", "1970-01-31", "$8,970.00", "$8,970.00", "$8,970.00", "$11,100.00", "$2,130.00", "$8,970.00"]];
    assert_eq!(rows(&mut f, &options), expect(&want));
    let alone = with(options, "Accounts", "Include Sub-Accounts", json!(false));
    assert!(rows(&mut f, &alone).is_empty());
}

/// A GBP account in a USD report under each price source, on `balsheet-pnl`: ForeignSavings holds £200 from
/// 1 February 1970 and sells £100 on 15 February at $1.42 (10:59). Weekly steps from 10 February to 20 February
/// give two rows: the first with daily balances at the start of 11 to 17 February (five of £200, two of £100) and
/// the sale, exchanged at the step's end, the start of 17 February (AB:240-247); the second with four of £100.
/// Each daily balance is exchanged at its own date and rounded to the cent (AB:362-373; CU is
/// `5.17:gnucash/report/commodity-utilities.scm`):
///
/// - pricedb-nearest (CU:708-721): $1.42, the 15 February price, is nearest throughout: (5 × 284 + 2 × 142) / 7 =
///   $243.4285…;
/// - pricedb-before (CU:723-734): $1.40 of 1 February until the 15 February price at midnight: (4 × 280 + 284 +
///   2 × 142) / 7 = $241.1428…;
/// - pricedb-latest (CU:686-699): the newest price of the pair is USD in GBP, 10/17 on 1 February 1980, inverted:
///   $1.70, (5 × 340 + 2 × 170) / 7 = $291.4285…;
/// - average-cost (CU:520-618, :852-860): the price table at the end of each day, value over amount of every GBP
///   exchange so far: $270 / £200 until the sale, then $128 / £100: (4 × 270 + 256 + 2 × 128) / 7 = $227.4285…;
/// - weighted-average (CU:130-223, :861-870): the running average of the GBP purchases and sales at the end of each
///   day, nearest to the date (later on a tie): 27/20 until 15 February, then 412/300 = 103/75, so £200 is $274.67
///   and £100 $137.33: (4 × 270 + 274.67 + 2 × 137.33) / 7 = $232.7614…, and the sale is −$137.333….
#[test]
fn foreign_currency_account_under_each_price_source() {
    let mut f = Fixture::open("balsheet-pnl", "iso", true);
    let options = json!({
        "Accounts": { "Accounts": [FOREIGN_SAVINGS] },
        "Display": { "Show table": true },
        "General": { "Start Date": { "absolute": "1970-02-10" }, "End Date": { "absolute": "1970-02-20" }, "Step Size": "WeekDelta" },
    });
    let cases = [
        ("pricedb-nearest", ["$243.43", "$284.00", "$142.00", "$142.00"], "$142.00"),
        ("pricedb-before", ["$241.14", "$284.00", "$142.00", "$142.00"], "$142.00"),
        ("pricedb-latest", ["$291.43", "$340.00", "$170.00", "$170.00"], "$170.00"),
        ("average-cost", ["$227.43", "$270.00", "$128.00", "$128.00"], "$128.00"),
        ("weighted-average", ["$232.76", "$274.67", "$137.33", "$137.33"], "$137.33"),
    ];
    cases.into_iter().for_each(|(source, [average, maximum, minimum, later], sold)| {
        let options = with(options.clone(), "General", "Price Source", json!(source));
        let loss = format!("-{sold}");
        let want = [
            ["1970-02-10", "1970-02-16", average, maximum, minimum, "$0.00", sold, loss.as_str()],
            ["1970-02-17", "1970-02-20", later, later, later, "$0.00", "$0.00", "$0.00"],
        ];
        assert_eq!(rows(&mut f, &options), expect(&want), "{source}");
    });
    // The exact figure behind the weighted average's loss: £100 at 103/75.
    let options = with(options, "General", "Price Source", json!("weighted-average"));
    let result = f.run(TEMPLATE, &options);
    assert_eq!(rational(&only(&result, "table")["rows"][0]["cells"][6]["monetary"]["amount"]), "412/3");
}

/// Running the template leaves the Book as it was, not dirty, with no journal written; two exports are the same
/// bytes and hold no script (`design/reports.md` §13, "Properties and safety").
#[test]
fn runs_only_read_and_exports_are_deterministic() {
    let mut f = Fixture::open("average-balance", "locale", false);
    let before = f.d.context().book().unwrap().clone();
    let files = f.files();
    let options = with(testing_options(), "Display", "Plot Type", json!(["AvgBalPlot", "GainPlot", "GLPlot"]));
    f.run(TEMPLATE, &options);
    let (a, b) = (f.export(TEMPLATE, &options), f.export(TEMPLATE, &options));
    assert_eq!(a, b);
    assert!(!a.to_ascii_lowercase().contains("<script"));
    assert_eq!(f.d.context().book().unwrap(), &before, "the Book is unchanged");
    assert_eq!(f.call("book.info", &json!({}))["dirty"], false);
    assert_eq!(f.files(), files, "no journal or other file was written");
}

/// The report runs on every report fixture (`design/reports.md` §13: reports never fail on a book red-ale can open),
/// with the default options, over the first quarter of each year the fixtures hold data in, and with each price
/// source in a GBP report.
#[test]
fn runs_on_every_fixture_book() {
    let books = ["average-balance", "balsheet-pnl", "category-average", "category-daily", "category-liability", "category-multi"];
    let books = books.into_iter().chain(["charts-daily", "charts-net", "net-days", "test-data"]);
    books.for_each(|book| {
        let mut f = Fixture::open(book, "iso", true);
        f.run(TEMPLATE, &json!({}));
        ["1970", "1979", "2024"].into_iter().for_each(|year| {
            let (start, end) = (format!("{year}-01-01"), format!("{year}-03-31"));
            let dates = json!({ "Start Date": { "absolute": start }, "End Date": { "absolute": end } });
            let options = json!({ "General": dates, "Display": { "Show table": true, "Plot Type": ["AvgBalPlot", "GainPlot", "GLPlot"] } });
            f.run(TEMPLATE, &options);
        });
        ["average-cost", "weighted-average", "pricedb-before", "pricedb-nearest", "pricedb-latest"].into_iter().for_each(|source| {
            let general = json!({ "Start Date": { "absolute": "1970-01-01" }, "End Date": { "absolute": "1970-03-31" },
                                  "Report's currency": "CURRENCY::GBP", "Price Source": source });
            f.run(TEMPLATE, &json!({ "General": general, "Display": { "Show table": true } }));
        });
    });
}

/// An exact amount of the wire as `(num, denom)`.
fn exact(v: &Value) -> (i128, i128) {
    (v["num"].as_str().unwrap().parse().unwrap(), v["denom"].as_str().unwrap().parse().unwrap())
}

/// The exact figures of each table row: average, maximum, minimum, gain, loss, profit.
fn figures(result: &Value) -> Vec<[(i128, i128); 6]> {
    let rows = only(result, "table")["rows"].as_array().unwrap();
    rows.iter().map(|r| std::array::from_fn(|i| exact(&r["cells"][i + 2]["monetary"]["amount"]))).collect()
}

/// `a` compared with `b`, exactly.
fn cmp((an, ad): (i128, i128), (bn, bd): (i128, i128)) -> Ordering {
    (an * bd).cmp(&(bn * ad))
}

/// `a + b`, not reduced.
fn add((an, ad): (i128, i128), (bn, bd): (i128, i128)) -> (i128, i128) {
    (an * bd + bn * ad, ad * bd)
}

/// A generated transfer on the `average-balance` book: a day of January to June 1979, which way, and an amount in
/// cents. Kind 0 is Income to Bank, 1 Bank to Another Bank (both selected), 2 Another Bank to Income.
type Transfer = (u32, u8, u32);

/// Post `transfers`; the gains, losses and internal amounts they add to the selected accounts, in cents.
fn post(f: &mut Fixture, transfers: &[Transfer]) -> (i128, i128, i128) {
    const INCOME: &str = "a0000000000000000000000000000004";
    transfers.iter().fold((0, 0, 0), |(gain, loss, internal), &(day, kind, cents)| {
        let (from, to) = [(INCOME, BANK), (BANK, ANOTHER_BANK), (ANOTHER_BANK, INCOME)][usize::from(kind)];
        let value = format!("{}.{:02}", cents / 100, cents % 100);
        let date = chrono::NaiveDate::from_ymd_opt(1979, 1, 1).unwrap() + chrono::Days::new(u64::from(day));
        let splits = json!([{ "account": from, "value": format!("-{value}") }, { "account": to, "value": value }]);
        f.call("transaction.post", &json!({ "datePosted": date.to_string(), "description": "transfer", "splits": splits }));
        let cents = i128::from(cents);
        match kind {
            0 => (gain + cents, loss, internal),
            1 => (gain, loss, internal + cents),
            _ => (gain, loss + cents, internal),
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, ..ProptestConfig::default() })]

    /// On books with transfers added, each row's profit is its gain less its loss and its average lies between its
    /// minimum and maximum (AB:163-172, :179-189). Over the period the gains and losses are the selected splits'
    /// positive and negative amounts, an internal transfer counted on both sides unless excluded (AB:220-233), so
    /// the profits add up to the change in balance either way.
    #[test]
    fn rows_add_up(transfers in proptest::collection::vec((0u32..181, 0u8..3, 1u32..1_000_000), 0..8)) {
        let mut f = Fixture::open("average-balance", "locale", false);
        let (gain, loss, internal) = post(&mut f, &transfers);
        // The fixture's own: $200 in from Income and $25 from Bank to Another Bank.
        let (gain, internal) = (gain + 20_000, internal + 2_500);
        [false, true].into_iter().for_each(|exclude| {
            let options = with(testing_options(), "Accounts", "Exclude transactions between selected accounts", json!(exclude));
            let rows = figures(&f.run(TEMPLATE, &options));
            assert_eq!(rows.len(), 6);
            rows.iter().for_each(|[average, maximum, minimum, row_gain, row_loss, profit]| {
                assert_eq!(cmp(*row_gain, add(*profit, *row_loss)), Ordering::Equal, "profit = gain - loss");
                assert_ne!(cmp(*average, *minimum), Ordering::Less);
                assert_ne!(cmp(*average, *maximum), Ordering::Greater);
            });
            let total = |i: usize| rows.iter().fold((0, 1), |sum, r| add(sum, r[i]));
            let both_sides = if exclude { 0 } else { internal };
            assert_eq!(cmp(total(3), (gain + both_sides, 100)), Ordering::Equal, "gains");
            assert_eq!(cmp(total(4), (loss + both_sides, 100)), Ordering::Equal, "losses");
            assert_eq!(cmp(total(5), (gain - loss, 100)), Ordering::Equal, "profits");
        });
    }
}
