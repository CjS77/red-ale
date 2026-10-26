//! The Price Scatterplot (`standard/price-scatter.rhai`) through `report.run` and `report.export`. 5.17's tests only
//! reach its "Identical commodities" warning (`5.17:gnucash/report/reports/standard/test/test-charts.scm:189`);
//! the other cases are hand-worked on `balsheet-pnl.gnucash`, FUNDS in USD, from the Scheme. `PS` stands for
//! `5.17:gnucash/report/reports/standard/price-scatter.scm`, `CU` for `5.17:gnucash/report/commodity-utilities.scm`.
//!
//! Each point's x is its time less the start of the first day, over the step's seconds (PS:106-113, :218-229): a
//! month is 2,628,000 s, so a split posted at 10:59 on 1 January 1970 is at 39,540 / 2,628,000 = 659/43,800. Its
//! hover title is x to two places, the step's name and the price as `gnc:monetary->string` prints it (PS:260-267).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod report_runs;

use report_runs::{Fixture, in_html, only, rational};
use serde_json::{Value, json};

const TEMPLATE: &str = "Price Scatterplot";

/// Accounts of `balsheet-pnl.gnucash`.
const CURRENT: &str = "a0000000000000000000000000000007";
const FUNDS: &str = "a000000000000000000000000000000c";

/// FUNDS in USD from 1970 to 1975, by the actual transactions (the default source, PS:69-75).
fn funds_options() -> Value {
    json!({
        "Price": { "Price of Commodity": "FUNDS::FUNDS" },
        "General": { "Start Date": { "absolute": "1970-01-01" }, "End Date": { "absolute": "1975-12-31" } },
    })
}

/// `options` with `section`/`name` set to `value`.
fn with(mut options: Value, section: &str, name: &str, value: Value) -> Value {
    options[section][name] = value;
    options
}

/// A point as `(x, y, hover title, display of y)`.
type Point = (String, String, String, String);

/// The chart's points, after checking the exported page carries each one's hover title.
fn points(f: &mut Fixture, options: &Value) -> Vec<Point> {
    let result = f.run(TEMPLATE, options);
    let chart = only(&result, "chart");
    let series = chart["series"].as_array().unwrap();
    assert_eq!(series.len(), 1);
    let text = |v: &Value| v.as_str().unwrap().to_owned();
    let points: Vec<Point> = series[0]["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (rational(&p["x"]), rational(&p["y"]), text(&p["label"]), text(&p["display"])))
        .collect();
    let labels: Vec<&str> = points.iter().map(|p| p.2.as_str()).collect();
    in_html(&f.export(TEMPLATE, options), &labels);
    points
}

/// `(x, y, title)` triples as owned points without the display, for comparing.
fn without_display(points: &[Point]) -> Vec<(&str, &str, &str)> {
    points.iter().map(|(x, y, label, _)| (x.as_str(), y.as_str(), label.as_str())).collect()
}

/// The one warning of a run, `(title, text)`, after checking the exported page shows its title.
fn warning(f: &mut Fixture, options: &Value) -> (String, String) {
    let result = f.run(TEMPLATE, options);
    let warning = only(&result, "warning");
    let title = warning["title"].as_str().unwrap().to_owned();
    in_html(&f.export(TEMPLATE, options), &[&title]);
    (title, warning["text"].as_str().unwrap().to_owned())
}

/// The FUNDS dates of the fixture's purchases and sales, at 10:59 on the first of each month from January to July
/// 1970, as months since the start.
const MONTHS: [&str; 7] = ["659/43800", "45299/43800", "85619/43800", "130259/43800", "173459/43800", "218099/43800", "261299/43800"];

// Expected figures from 5.17.

/// test-charts.scm:189 renders every template with `default-testing-options` (:163-175): for the Price Scatterplot,
/// the calendar year and its default commodity, USD (`gnc-locale-default-iso-currency-code`, PS:63-67), which is
/// also the report's currency, so the report shows the "Identical commodities" warning (PS:193-199), the case
/// `design/reports.md` §13 says the 5.17 tests reach.
#[test]
fn test_charts_default_testing_options_are_identical_commodities() {
    let mut f = Fixture::open("charts-daily", "iso", true);
    let options = json!({ "General": { "Start Date": { "relative": "start-cal-year" }, "End Date": { "relative": "end-cal-year" } } });
    let (title, text) = warning(&mut f, &options);
    assert_eq!(title, "Identical commodities");
    assert_eq!(
        text,
        "Your selected commodity and the currency of the report are identical. \
         It doesn't make sense to show prices for identical commodities."
    );
}

// Hand-worked cases.

/// The options of `options-generator` (PS:50-96), in GnuCash's order, with the General options every template
/// gets; with no book open they come from an empty one.
#[test]
fn options_are_gnucash_s() {
    let want = [
        r#"Display/Marker Color "b" "color" "b22222""#,
        r#"Display/Plot Width "ca" "plotSize" {"percent":100}"#,
        r#"Display/Plot Height "cb" "plotSize" {"percent":100}"#,
        r#"General/Report name "0a" "string" "Price Scatterplot""#,
        r#"General/Stylesheet "0b" "multichoice" "Default""#,
        r#"General/Add options summary "0c" "multichoice" "never""#,
        r#"General/Start Date "aa" "date" {"relative":"start-accounting-period"}"#,
        r#"General/End Date "ab" "date" {"relative":"end-accounting-period"}"#,
        r#"General/Step Size "b" "multichoice" "MonthDelta""#,
        r#"Price/Report's currency "d" "currency" "CURRENCY::USD""#,
        r#"Price/Price of Commodity "e" "commodity" "CURRENCY::USD""#,
        r#"Price/Price Source "f" "multichoice" "actual-transactions""#,
        r#"Price/Invert prices "g" "boolean" false"#,
    ];
    assert_eq!(report_runs::described_options(TEMPLATE), want);
}

/// The default options are the same case on any book: USD in USD.
#[test]
fn default_options_are_identical_commodities() {
    let mut f = Fixture::open("balsheet-pnl", "iso", true);
    let result = f.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], "Price Scatterplot");
    assert_eq!(only(&result, "warning")["title"], "Identical commodities");
    assert!(result["sections"][0]["blocks"].as_array().unwrap().len() == 1, "no chart (PS:193-199)");
}

/// Actual Transactions (CU:230-275): each FUNDS split's value over its shares, from the Funds account, the only
/// STOCK, MUTUAL or CURRENCY account (PS:148-150): $2,000 for 10 FUNDS is $200, and so on.
#[test]
fn actual_transactions() {
    let mut f = Fixture::open("balsheet-pnl", "iso", true);
    let points = points(&mut f, &funds_options());
    let want = [
        (MONTHS[0], "200", ".02 Months  =  $200.00"),
        (MONTHS[1], "210", "1.03 Months  =  $210.00"),
        (MONTHS[2], "225", "1.95 Months  =  $225.00"),
        (MONTHS[3], "244", "2.97 Months  =  $244.00"),
        (MONTHS[4], "264", "3.96 Months  =  $264.00"),
        (MONTHS[5], "255", "4.98 Months  =  $255.00"),
        (MONTHS[6], "250", "5.97 Months  =  $250.00"),
    ];
    assert_eq!(without_display(&points), want);
    assert_eq!(points[0].3, "$200.00", "the y value as the axis's currency shows it");
}

/// Weighted Average (CU:122-193): the running total of the values over the running total of the shares, sales
/// counted at their absolute amounts: 6,350 / 30 = 635/3 after March, printed as GnuCash prints a fraction that is not
/// a decimal, `$211 + 2/3`.
#[test]
fn weighted_average() {
    let mut f = Fixture::open("balsheet-pnl", "iso", true);
    let options = with(funds_options(), "Price", "Price Source", json!("weighted-average"));
    let points = points(&mut f, &options);
    let want = [
        (MONTHS[0], "200", ".02 Months  =  $200.00"),
        (MONTHS[1], "205", "1.03 Months  =  $205.00"),
        (MONTHS[2], "635/3", "1.95 Months  =  $211 + 2/3"),
        (MONTHS[3], "879/4", "2.97 Months  =  $219.75"),
        (MONTHS[4], "1143/5", "3.96 Months  =  $228.60"),
        (MONTHS[5], "233", "4.98 Months  =  $233.00"),
        (MONTHS[6], "1648/7", "5.97 Months  =  $235 + 3/7"),
    ];
    assert_eq!(without_display(&points), want);
}

/// Price Database (PS:211-216): the forward prices of FUNDS in USD, newest first as `gnc-pricedb-get-prices` gives
/// them, each at local midnight: 1 January 1975 is 1,826 days after the start, 1,826 × 86,400 / 2,628,000 =
/// 21,912/365 months.
#[test]
fn price_database() {
    let mut f = Fixture::open("balsheet-pnl", "iso", true);
    let options = with(funds_options(), "Price", "Price Source", json!("pricedb"));
    let points = points(&mut f, &options);
    let want = [
        ("21912/365", "50005/104", "60.03 Months  =  $480 + 85/104"),
        ("17532/365", "45004/103", "48.03 Months  =  $436 + 96/103"),
        ("13152/365", "40003/102", "36.03 Months  =  $392 + 19/102"),
        ("24", "35002/101", "24.00 Months  =  $346 + 56/101"),
        ("12", "30001/100", "12.00 Months  =  $300.01"),
        ("2172/365", "250", "5.95 Months  =  $250.00"),
        ("1812/365", "255", "4.96 Months  =  $255.00"),
        ("288/73", "264", "3.95 Months  =  $264.00"),
        ("216/73", "244", "2.96 Months  =  $244.00"),
        ("708/365", "225", "1.94 Months  =  $225.00"),
        ("372/365", "210", "1.02 Months  =  $210.00"),
        ("0", "200", ".00 Months  =  $200.00"),
    ];
    assert_eq!(without_display(&points), want);
}

/// Invert prices (PS:151-153, :228): FUNDS per USD, so the amounts are in FUNDS, printed with its symbol after the
/// number, and the title and axis swap commodities (PS:168-183).
#[test]
fn inverted_prices() {
    let mut f = Fixture::open("balsheet-pnl", "iso", true);
    let options = with(funds_options(), "Price", "Invert prices", json!(true));
    let points = points(&mut f, &options);
    let want = [
        (MONTHS[0], "1/200", ".02 Months  =  0.005 FUNDS"),
        (MONTHS[1], "1/210", "1.03 Months  =  1/210 FUNDS"),
        (MONTHS[2], "1/225", "1.95 Months  =  1/225 FUNDS"),
        (MONTHS[3], "1/244", "2.97 Months  =  1/244 FUNDS"),
        (MONTHS[4], "1/264", "3.96 Months  =  1/264 FUNDS"),
        (MONTHS[5], "1/255", "4.98 Months  =  1/255 FUNDS"),
        (MONTHS[6], "1/250", "5.97 Months  =  0.004 FUNDS"),
    ];
    assert_eq!(without_display(&points), want);
    let result = f.run(TEMPLATE, &options);
    let chart = only(&result, "chart");
    assert_eq!(chart["title"], json!(["Price Scatterplot", "USD - 1970-01-01 to 1975-12-31"]));
    assert_eq!(chart["yAxis"], json!({ "label": "FUNDS", "format": "currency", "commodity": "FUNDS::FUNDS" }));
}

/// The chart itself (PS:166-187, :269-274): titled by the report and the commodity over the period, x in months on
/// a linear axis, y in the report's currency, the points in the marker colour joined by a #4bb2c5 line. Step Size
/// and the Display options change it: in years (31,536,000 s) the 1971 price is at x = 1.
#[test]
fn chart_options() {
    let mut f = Fixture::open("balsheet-pnl", "iso", true);
    let result = f.run(TEMPLATE, &funds_options());
    let chart = only(&result, "chart");
    assert_eq!(chart["type"], "scatter");
    assert_eq!(chart["title"], json!(["Price Scatterplot", "FUNDS - 1970-01-01 to 1975-12-31"]));
    assert_eq!(chart["xAxis"], json!({ "label": "Months", "scale": "linear" }));
    assert_eq!(chart["yAxis"], json!({ "label": "USD", "format": "currency", "commodity": "CURRENCY::USD" }));
    let series = &chart["series"][0];
    assert_eq!((&series["label"], &series["color"], &series["line"]), (&json!("Price"), &json!("#b22222"), &json!("#4bb2c5")));

    let options = with(with(funds_options(), "Price", "Price Source", json!("pricedb")), "General", "Step Size", json!("YearDelta"));
    let options = with(options, "Display", "Marker Color", json!("00ff00"));
    let options = with(options, "Display", "Plot Width", json!({ "pixels": 500 }));
    let points = points(&mut f, &options);
    assert_eq!(without_display(&points)[4], ("1", "30001/100", "1.00 Years  =  $300.01"));
    let result = f.run(TEMPLATE, &options);
    let chart = only(&result, "chart");
    assert_eq!(chart["xAxis"]["label"], "Years");
    assert_eq!(chart["series"][0]["color"], "#00ff00");
    assert_eq!(chart["width"], json!({ "pixels": 500 }));
}

/// "No data" (PS:232-236): no FUNDS price in the database in 1976; and no GBP price from transactions, since GBP is
/// held only in ForeignSavings, an ASSET account, which `gnc:account-has-shares?` leaves out (PS:148-150). The price
/// database gives forward prices only: USD in GBP (1980) is not a price of GBP in USD.
#[test]
fn no_data() {
    let mut f = Fixture::open("balsheet-pnl", "iso", true);
    let later = json!({ "absolute": "1976-01-01" });
    let options = with(with(funds_options(), "Price", "Price Source", json!("pricedb")), "General", "Start Date", later);
    assert_eq!(warning(&mut f, &options).0, "No data");
    let gbp = with(funds_options(), "Price", "Price of Commodity", json!("CURRENCY::GBP"));
    assert_eq!(warning(&mut f, &gbp).0, "No data");
    let reverse = json!({
        "Price": { "Price of Commodity": "CURRENCY::GBP", "Price Source": "pricedb" },
        "General": { "Start Date": { "absolute": "1980-01-01" }, "End Date": { "absolute": "1980-12-31" } },
    });
    assert_eq!(warning(&mut f, &reverse).0, "No data");
}

/// "Only one price" (PS:238-243): the database's one FUNDS price in the first half of 1971.
#[test]
fn only_one_price() {
    let mut f = Fixture::open("balsheet-pnl", "iso", true);
    let options = json!({
        "Price": { "Price of Commodity": "FUNDS::FUNDS", "Price Source": "pricedb" },
        "General": { "Start Date": { "absolute": "1971-01-01" }, "End Date": { "absolute": "1971-06-30" } },
    });
    assert_eq!(warning(&mut f, &options).0, "Only one price");
}

/// "All Prices equal" (PS:245-250) and "All Prices at the same date" (PS:252-257), from purchases posted on a copy
/// of the book: two of 10 FUNDS for $3,000 in 1976, a month apart, then two on one day in 1977 at $3,000 and
/// $3,100. The equal-prices test comes first, so prices both equal and on one day would give it.
#[test]
fn all_prices_equal_or_on_one_day() {
    let mut f = Fixture::open("balsheet-pnl", "iso", false);
    [("1976-01-01", "3000"), ("1976-02-01", "3000"), ("1977-03-01", "3000"), ("1977-03-01", "3100")].into_iter().for_each(|(day, value)| {
        let splits = json!([{ "account": CURRENT, "value": format!("-{value}") }, { "account": FUNDS, "value": value, "amount": "10" }]);
        f.call("transaction.post", &json!({ "currency": "USD", "datePosted": day, "description": "buy 10FUND", "splits": splits }));
    });
    let year = |y: u32| {
        let options = with(funds_options(), "General", "Start Date", json!({ "absolute": format!("{y}-01-01") }));
        with(options, "General", "End Date", json!({ "absolute": format!("{y}-12-31") }))
    };
    let (title, text) = warning(&mut f, &year(1976));
    assert_eq!(title, "All Prices equal");
    assert!(text.starts_with("All the prices found are equal."), "{text}");
    let (title, text) = warning(&mut f, &year(1977));
    assert_eq!(title, "All Prices at the same date");
    assert!(text.starts_with("All the prices found are from the same date."), "{text}");
}

/// The report runs on every report fixture (`design/reports.md` §13: reports never fail on a book red-ale can open),
/// with the default options and for GBP and EUR in USD under each price source, inverted or not.
#[test]
fn runs_on_every_fixture_book() {
    let books = ["average-balance", "balsheet-pnl", "category-average", "category-daily", "category-liability", "category-multi"];
    let books = books.into_iter().chain(["charts-daily", "charts-net", "net-days", "test-data"]);
    books.for_each(|book| {
        let mut f = Fixture::open(book, "iso", true);
        f.run(TEMPLATE, &json!({}));
        let general = json!({ "Start Date": { "absolute": "1970-01-01" }, "End Date": { "absolute": "2025-12-31" } });
        ["CURRENCY::GBP", "CURRENCY::EUR"].into_iter().for_each(|commodity| {
            ["actual-transactions", "weighted-average", "pricedb"].into_iter().for_each(|source| {
                [false, true].into_iter().for_each(|invert| {
                    let price = json!({ "Price of Commodity": commodity, "Price Source": source, "Invert prices": invert });
                    f.run(TEMPLATE, &json!({ "General": general, "Price": price }));
                });
            });
        });
    });
}

/// Running the template leaves the Book as it was, not dirty, with no journal written; two exports are the same
/// bytes and hold no script (`design/reports.md` §13, "Properties and safety").
#[test]
fn runs_only_read_and_exports_are_deterministic() {
    let mut f = Fixture::open("balsheet-pnl", "iso", false);
    let before = f.d.context().book().unwrap().clone();
    let files = f.files();
    ["actual-transactions", "weighted-average", "pricedb"].into_iter().for_each(|source| {
        let options = with(funds_options(), "Price", "Price Source", json!(source));
        f.run(TEMPLATE, &options);
        let (a, b) = (f.export(TEMPLATE, &options), f.export(TEMPLATE, &options));
        assert_eq!(a, b, "{source}");
        assert!(a.contains("<svg") && !a.to_ascii_lowercase().contains("<script"), "{source}");
    });
    assert_eq!(f.d.context().book().unwrap(), &before, "the Book is unchanged");
    assert_eq!(f.call("book.info", &json!({}))["dirty"], false);
    assert_eq!(f.files(), files, "no journal or other file was written");
}
