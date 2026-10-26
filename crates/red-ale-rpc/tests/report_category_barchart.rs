//! The four "Over Time" charts of `standard/category-barchart.rhai`
//! (`5.17:gnucash/report/reports/standard/category-barchart.scm`, cited below by line): the figures of GnuCash
//! 5.17's own test, ported in `fixtures/reports/category-*.expected.json`, hand-worked cases and a property test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod chart_reports;

use chart_reports::{Charts, Expected, blocks_of, chart, check_read_only, check_run, table, values};
use proptest::prelude::*;
use serde_json::{Value, json};

const INCOME_OVER_TIME: &str = "44f81bee049b4b3ea908f8dac9a9474e";
const EXPENSE_OVER_TIME: &str = "b1f15b2052c149df93e698fe85a81ea6";
const ASSETS_OVER_TIME: &str = "e9cf815f79db44bcb637d0295093ae3d";
const LIABILITIES_OVER_TIME: &str = "faf410e8f8da481fbc09e4763da40bcc";
const TEMPLATES: [&str; 4] = [INCOME_OVER_TIME, EXPENSE_OVER_TIME, ASSETS_OVER_TIME, LIABILITIES_OVER_TIME];

/// Every run of `<fixture>.expected.json`, each assertion a named check.
fn expected_runs(fixture: &str, ids: &[&str]) {
    let expected = Expected::load(fixture);
    let mut charts = Charts::open(&format!("{fixture}.gnucash"), "locale");
    let checked: usize = ids.iter().map(|id| check_run(&mut charts, &expected, id)).sum();
    let all = expected.0["runs"].as_array().unwrap();
    assert_eq!(ids.len(), all.len(), "every run of {fixture} is checked");
    let assertions: usize = all.iter().map(|r| r["checks"].as_array().unwrap().len()).sum();
    assert_eq!(checked, assertions);
}

// test-standard-category-report.scm:81-115 (daily amounts) and :263-300 (assets).
#[test]
fn test_standard_category_report_daily_and_assets() {
    expected_runs("category-daily", &["single-txn-test", "asset-test"]);
}

// test-standard-category-report.scm:117-207: averages for every step and averaging pair.
#[test]
fn test_standard_category_report_averages() {
    let ids = ["single-txn-test-average-week", "single-txn-test-average-month", "single-txn-test-average-day",
               "single-txn-test-weekly-average-day", "single-txn-test-weekly-average-week", "single-txn-test-weekly-average-month"];
    expected_runs("category-average", &ids);
}

// test-standard-category-report.scm:211-259: depth and percentages.
#[test]
fn test_standard_category_report_multi_account() {
    expected_runs("category-multi", &["multi--test", "ratios"]);
}

// test-standard-category-report.scm:302-343: a credit-card liability.
#[test]
fn test_standard_category_report_liability() {
    expected_runs("category-liability", &["liability-test"]);
}

// Hand-worked cases on category-multi (scripts/report-fixtures/category_report.py:69-77): the top-level expense
// tree Expenses > Groceries, Rent, Auto > Tax, Parking, Petrol, in that book order, each account getting $N on day
// N of March 2024, so $496 in March.

const EXPENSES: &str = "a0000000000000000000000000000001";
const GROCERIES: &str = "a0000000000000000000000000000002";
const RENT: &str = "a0000000000000000000000000000003";
const AUTO: &str = "a0000000000000000000000000000004";
const TAX: &str = "a0000000000000000000000000000005";
const PARKING: &str = "a0000000000000000000000000000006";
const PETROL: &str = "a0000000000000000000000000000007";

fn multi() -> Charts {
    Charts::open("category-multi.gnucash", "locale")
}

/// March 2024 by week with `display`.
fn march(display: Value) -> Value {
    let general = json!({ "Start Date": { "absolute": "2024-03-01" }, "End Date": { "absolute": "2024-03-31" }, "Step Size": "WeekDelta" });
    json!({ "General": general, "Display": display })
}

fn labels(result: &Value) -> Vec<String> {
    chart(result)["series"].as_array().unwrap().iter().map(|s| s["label"].as_str().unwrap().to_owned()).collect()
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

/// Twelve monthly values, `march` in March and 0 elsewhere.
fn months(march: &str) -> Vec<String> {
    (1..=12).map(|m| if m == 3 { march.to_owned() } else { "0".to_owned() }).collect()
}

// Defaults (:98-199): this calendar year by month, every expense account, two levels, stacked bars, at most 8, sorted
// by descending amount. At level 1 Expenses shows its own amounts and at level 2 each child with its descendants'
// (:453-477): Auto $1,984, the others $496 each. Ties keep traverse-accounts' order, which puts the children before
// the parent, each level latest first, then reversed: Groceries, Rent, Expenses. Colours from gnc:assign-colors
// (5.17:gnucash/report/html-utilities.scm:226-245).
#[test]
fn expense_over_time_defaults() {
    let mut c = multi();
    let result = c.run("Expense Over Time", &json!({}));
    assert_eq!(result["title"], "", "GnuCash gives the document no title (:259)");
    let ch = chart(&result);
    assert_eq!((ch["type"].as_str(), ch["stacked"].as_bool()), (Some("bar"), Some(true)));
    assert_eq!(ch["title"], json!(["Expense Over Time", "01/01/24 to 12/31/24"]), ":532-535");
    assert_eq!(ch["labels"].as_array().unwrap().len(), 12);
    assert_eq!(ch["labels"][2], "03/01/24");
    assert_eq!(ch["yAxis"], json!({ "label": "USD", "format": "currency", "commodity": "CURRENCY::USD" }), ":547-549, :618-623");
    assert_eq!(ch["tooltip"], json!({ "grouped": false, "nonZeroOnly": true }), ":187-195");
    assert_eq!(labels(&result), ["Auto", "Groceries", "Rent", "Expenses"]);
    let series = ch["series"].as_array().unwrap();
    let colors: Vec<&str> = series.iter().map(|s| s["color"].as_str().unwrap()).collect();
    assert_eq!(colors, ["#ff4136", "#ff851b", "#ffdc00", "#2ecc40"]);
    assert_eq!(values(&series[0]), months("1984"));
    assert_eq!(values(&series[3]), months("496"));
    series.iter().for_each(|s| assert_eq!((s["stack"].as_str(), s["fill"].as_bool()), (Some("default"), Some(true)), ":587-588"));
    assert!(blocks_of(&result, "table").is_empty(), "no table by default (:165-169)");
}

// Series links (:589-609): a parent opens this report on it and its children one level deeper, named after it; an
// account without children opens the account.
#[test]
fn series_link_to_sub_accounts_or_the_account() {
    let mut c = multi();
    let result = c.run("Expense Over Time", &json!({}));
    let ch = chart(&result);
    let auto = &ch["series"][0]["link"]["report"];
    assert_eq!(auto["template"], EXPENSE_OVER_TIME);
    assert_eq!(auto["options"]["Accounts"]["Accounts"], json!([AUTO, TAX, PARKING, PETROL]));
    assert_eq!(auto["options"]["Accounts"]["Levels of Subaccounts"], 3);
    assert_eq!(auto["options"]["General"]["Report name"], "Auto");
    assert_eq!(auto["options"]["Display"]["Sort Method"], "amount", "the other options are copied");
    assert_eq!(ch["series"][1]["link"], json!({ "account": GROCERIES }));
    let expenses = &ch["series"][3]["link"]["report"]["options"];
    assert_eq!(expenses["Accounts"]["Accounts"], json!([EXPENSES, GROCERIES, RENT, AUTO]));
    let linked = c.run(EXPENSE_OVER_TIME, &auto["options"]);
    assert_eq!(chart(&linked)["title"][0], "Auto");
    // At level 3, the limit, the children come in book order (:472-477), before Auto's own amounts (:464-468).
    assert_eq!(labels(&linked), ["Tax", "Parking", "Petrol", "Auto"]);
}

// Levels of Subaccounts (:137-140, :271-273): one level folds everything into Expenses, a single series with no
// Grand Total column (:631, :651-653); "all" is the tree's depth, 4, so every account shows its own amounts, in
// traverse-accounts' order.
#[test]
fn levels_of_subaccounts_fold_accounts_in() {
    let mut c = multi();
    let one = c.run("Expense Over Time", &json!({ "Accounts": { "Levels of Subaccounts": 1 }, "Display": { "Show table": true } }));
    assert_eq!(labels(&one), ["Expenses"]);
    assert_eq!(values(&chart(&one)["series"][0]), months("3472"));
    assert_eq!(table(&one).header, ["Date", "Expenses"]);
    assert_eq!(table(&one).rows[2], ["03/01/24", "$3,472.00"]);
    let all = c.run("Expense Over Time", &json!({ "Accounts": { "Levels of Subaccounts": "all" } }));
    assert_eq!(labels(&all), ["Petrol", "Parking", "Tax", "Auto", "Rent", "Groceries", "Expenses"]);
    chart(&all)["series"].as_array().unwrap().iter().for_each(|s| assert_eq!(values(s), months("496")));
    assert_eq!(chart(&all)["series"][3]["link"]["report"]["options"]["Accounts"]["Levels of Subaccounts"], 5);
}

// Maximum Bars (:160-163, :554-574): past it, the smallest series are summed into "Other", which links to this
// report on the accounts it sums.
#[test]
fn maximum_bars_sums_the_rest_into_other() {
    let mut c = multi();
    let result = c.run("Expense Over Time", &json!({ "Display": { "Maximum Bars": 2, "Show table": true } }));
    assert_eq!(labels(&result), ["Auto", "Other"]);
    let ch = chart(&result);
    assert_eq!(values(&ch["series"][1]), months("1488"));
    assert_eq!(ch["series"][1]["color"], "#ff851b");
    assert_eq!(ch["series"][1]["link"]["report"]["options"]["Accounts"]["Accounts"], json!([GROCERIES, RENT, EXPENSES]));
    let t = table(&result);
    assert_eq!(t.header, ["Date", "Auto", "Other", "Grand Total"]);
    assert_eq!(t.rows[2], ["03/01/24", "$1,984.00", "$1,488.00", "$3,472.00"]);
    assert_eq!(t.rows[0], ["01/01/24", "$0.00", "$0.00", "$0.00"]);
    let err = c.err("report.run", json!({ "report": EXPENSE_OVER_TIME, "options": { "Display": { "Maximum Bars": 1 } } }));
    assert_eq!(err.code, red_ale_rpc::codes::REPORT_OPTION, "below the range");
}

// Use Stacked Charts and Chart Type (:143-158, :529-530, :587-588, :617): unstacked, each series is its own stack;
// a line chart is not filled.
#[test]
fn stacking_and_chart_type() {
    let mut c = multi();
    let result = c.run("Expense Over Time", &json!({ "Display": { "Use Stacked Charts": false, "Chart Type": "linechart" } }));
    let ch = chart(&result);
    assert_eq!((ch["type"].as_str(), ch["stacked"].as_bool()), (Some("line"), Some(false)));
    let stacks: Vec<&str> = ch["series"].as_array().unwrap().iter().map(|s| s["stack"].as_str().unwrap()).collect();
    assert_eq!(stacks, ["0", "1", "2", "3"]);
    assert!(ch["series"][0].get("fill").is_none());
}

// Sort Method (:183-185, :482-498) and Show long account names (:143-145, :353-354): by name; by code, all codes
// being empty, so traverse-accounts' order; and by full name.
#[test]
fn sort_methods_and_long_names() {
    let mut c = multi();
    let sorted = |c: &mut Charts, display: Value| labels(&c.run("Expense Over Time", &json!({ "Display": display })));
    assert_eq!(sorted(&mut c, json!({ "Sort Method": "alphabetical" })), ["Auto", "Expenses", "Groceries", "Rent"]);
    assert_eq!(sorted(&mut c, json!({ "Sort Method": "acct-code" })), ["Groceries", "Rent", "Auto", "Expenses"]);
    let long = sorted(&mut c, json!({ "Sort Method": "alphabetical", "Show long account names": true }));
    assert_eq!(long, ["Expenses", "Expenses:Auto", "Expenses:Groceries", "Expenses:Rent"]);
}

// Show Average (:116-124, :299-328) over March by week: 31 days against 31/7 weeks, so each week's amount is
// divided by 7, and the title says so. The last step runs from 03-29 to the end of 03-31.
#[test]
fn show_average_divides_by_the_period_lengths() {
    let mut c = multi();
    let mut options = march(json!({ "Show table": true }));
    options["General"]["Show Average"] = json!("DayDelta");
    options["Accounts"] = json!({ "Accounts": [GROCERIES] });
    let result = c.run("Expense Over Time", &options);
    let ch = chart(&result);
    assert_eq!(ch["title"], json!(["Expense Over Time Daily Average", "03/01/24 to 03/31/24"]));
    assert_eq!(ch["labels"], json!(["03/01/24", "03/08/24", "03/15/24", "03/22/24", "03/29/24"]));
    assert_eq!(values(&ch["series"][0]), strings(&["4", "11", "18", "25", "90/7"]));
    let t = table(&result);
    assert_eq!(t.rows.iter().map(|r| r[1].as_str()).collect::<Vec<_>>(), ["$4.00", "$11.00", "$18.00", "$25.00", "$12.86"]);
}

// Percentage chart (:173-177, :611, :636-637): each series as a share of the row's total. The chart's series hold
// percentages (red-ale's percent axis shows values as they are, `design/reports.md` §14); the table shows each share
// rounded to two places and the total as an amount.
#[test]
fn percentage_chart_values_are_percentages() {
    let mut c = multi();
    let result = c.run("Expense Over Time", &march(json!({ "Percentage chart": true, "Show table": true })));
    let ch = chart(&result);
    assert_eq!(ch["yAxis"], json!({ "label": "Ratio", "format": "percent" }), ":547-549, :622-623");
    // Week 1: Auto 4 × 28 = $112 of $196.
    assert_eq!(values(&ch["series"][0])[0], "400/7");
    assert_eq!(ch["series"][0]["displays"][0], "57.14%");
    assert_eq!(table(&result).rows[0], ["03/01/24", "57.14 %", "14.29 %", "14.29 %", "14.29 %", "$196.00"]);
}

/// A new book in `c` with a top-level Expenses tree and a Bank, and the transactions `(account, day of January 2024,
/// cents)` from Bank.
fn small_book(c: &mut Charts, children: &[&str], posts: &[(&str, u32, i64)]) {
    let book = c.path().join("small.gnucash").display().to_string();
    c.call("book.create", json!({ "path": book }));
    c.call("account.create", json!({ "name": "Bank", "type": "BANK", "commodity": "USD" }));
    c.call("account.create", json!({ "name": "Expenses", "type": "EXPENSE", "commodity": "USD" }));
    children.iter().for_each(|n| {
        c.call("account.create", json!({ "name": n, "type": "EXPENSE", "commodity": "USD", "parent": "Expenses" }));
    });
    let decimal = |cents: i64| format!("{}{}.{:02}", if cents < 0 { "-" } else { "" }, cents.abs() / 100, cents.abs() % 100);
    posts.iter().for_each(|(account, day, cents)| {
        let splits = json!([{ "account": format!("Expenses:{account}"), "value": decimal(*cents) },
                            { "account": "Bank", "value": decimal(-cents) }]);
        c.call("transaction.post", json!({ "datePosted": format!("2024-01-{day:02}"), "description": "t", "splits": splits }));
    });
}

// A negative amount in a percentage chart gives a warning and the accounts' names (:507-518); with no data in range
// the empty-data warning (:500-505); an end before the start the invalid-dates warning (:283-289); no accounts the
// no-account warning (:276-281).
#[test]
fn warnings() {
    let mut c = Charts::new("locale");
    small_book(&mut c, &["Food", "Fuel"], &[("Food", 10, 5000), ("Fuel", 12, -2000)]);
    let january = json!({ "Start Date": { "absolute": "2024-01-01" }, "End Date": { "absolute": "2024-01-31" } });
    let result = c.run("Expense Over Time", &json!({ "General": january, "Display": { "Percentage chart": true } }));
    let warning = &blocks_of(&result, "warning")[0];
    assert_eq!(warning["title"], "Negative amounts detected");
    assert_eq!(blocks_of(&result, "text")[0]["text"], "Fuel");
    assert!(blocks_of(&result, "chart").is_empty());
    let plain = c.run("Expense Over Time", &json!({ "General": january }));
    assert_eq!(values(&chart(&plain)["series"][1]), ["-20"], "without percentages the refund charts");
    let february = json!({ "Start Date": { "absolute": "2024-02-01" }, "End Date": { "absolute": "2024-02-29" } });
    assert_eq!(blocks_of(&c.run("Expense Over Time", &json!({ "General": february })), "warning")[0]["title"], "No data");
    let backwards = json!({ "Start Date": { "absolute": "2024-01-31" }, "End Date": { "absolute": "2024-01-01" } });
    let invalid = &blocks_of(&c.run("Expense Over Time", &json!({ "General": backwards })), "warning")[0];
    assert_eq!((invalid["title"].as_str(), invalid["text"].as_str()),
               (Some("Invalid dates"), Some("Start date must be earlier than End date")));
    let none = &blocks_of(&c.run("Liabilities Over Time", &json!({})), "warning")[0];
    assert_eq!(none["title"], "No accounts selected", "the book has no liability account, so the default is empty");
}

// Only top-level accounts of the chart's types are walked (:261-264): in test-data every account sits below the
// asset account Root, so the Income Over Time finds no data even for selected income accounts.
#[test]
fn only_top_level_accounts_of_the_type_are_walked() {
    let mut c = Charts::open("test-data.gnucash", "locale");
    let general = json!({ "Start Date": { "absolute": "1978-01-01" }, "End Date": { "absolute": "1978-12-31" } });
    let result = c.run("Income Over Time", &json!({ "General": general }));
    assert_eq!(blocks_of(&result, "warning")[0]["title"], "No data");
}

// Two currencies and the Report's currency (:109-114, :346-351) on test-data's assets (see report_net_charts.rs for
// the book): at three levels Bank, GBP Bank and Wallet ($20 since 1971) show, the parents' own balances being zero.
// Balances at the end of 2000-01-01, 02-01, 03-01 and 03-31 (:331-336), the title saying "Balances" (:534).
#[test]
fn two_currencies_in_the_report_currency() {
    let mut c = Charts::open("test-data.gnucash", "locale");
    let run = |c: &mut Charts, currency: &str| {
        let general = json!({ "Start Date": { "absolute": "2000-01-01" }, "End Date": { "absolute": "2000-03-31" },
                              "Price Source": "pricedb-nearest", "Report's currency": currency });
        let options = json!({ "General": general, "Accounts": { "Levels of Subaccounts": 3 }, "Display": { "Show table": true } });
        c.run("Assets Over Time", &options)
    };
    let usd = run(&mut c, "CURRENCY::USD");
    assert_eq!(chart(&usd)["title"], json!(["Assets Over Time", "Balances 01/01/00 to 03/31/00"]));
    let t = table(&usd);
    assert_eq!(t.header, ["Date", "Bank", "GBP Bank", "Wallet", "Grand Total"]);
    assert_eq!(t.rows[0], ["01/01/00", "$2,181.00", "$856.80", "$20.00", "$3,057.80"]);
    assert_eq!(t.rows[1], ["02/01/00", "$2,195.00", "$903.00", "$20.00", "$3,118.00"]);
    assert_eq!(t.rows[3], ["03/31/00", "$2,186.00", "$912.00", "$20.00", "$3,118.00"]);
    // In pounds: dollars at 10/14 for 01-01, at 6/9 after, each account rounded to the penny before it is added.
    let gbp = table(&run(&mut c, "CURRENCY::GBP"));
    assert_eq!(gbp.rows[0], ["01/01/00", "#1,557.86", "#612.00", "#14.29", "#2,184.15"]);
    assert_eq!(gbp.rows[1], ["02/01/00", "#1,463.33", "#602.00", "#13.33", "#2,078.66"]);
}

// Running every template leaves the Book as it was, writes nothing, and exports the same bytes twice with no script.
#[test]
fn reports_only_read_and_export_deterministically() {
    let mut c = multi();
    check_read_only(&mut c, &TEMPLATES, &json!({ "Display": { "Show table": true } }));
    check_read_only(&mut c, &TEMPLATES, &march(json!({ "Percentage chart": true, "Maximum Bars": 3 })));
}

/// `sum` plus a wire Rational, exactly, as `(num, denom)` in lowest terms.
fn add(sum: (i128, i128), value: &Value) -> (i128, i128) {
    let part = |key: &str| value[key].as_str().unwrap().parse::<i128>().unwrap();
    let (num, den) = (sum.0 * part("denom") + part("num") * sum.1, sum.1 * part("denom"));
    let divisor = gcd(num.abs(), den);
    (num / divisor, den / divisor)
}

fn gcd(x: i128, y: i128) -> i128 {
    if y == 0 { x.max(1) } else { gcd(y, x % y) }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    // design/reports.md §13: a category chart's "Grand Total" column equals the sum of its series, on generated
    // books and options.
    #[test]
    fn grand_total_is_the_sum_of_the_series(
        posts in proptest::collection::vec((0usize..4, 1u32..32, -9000i64..20000), 0..12),
        levels in prop_oneof![Just(json!(2)), Just(json!("all"))],
        max_bars in 2u32..5,
        step in prop_oneof![Just("DayDelta"), Just("WeekDelta"), Just("MonthDelta")],
        average in prop_oneof![Just("None"), Just("DayDelta"), Just("WeekDelta"), Just("MonthDelta")],
        percent in any::<bool>(),
    ) {
        let names = ["Food", "Fuel", "Rent", "Books"];
        let mut c = Charts::new("iso");
        // Two fixed amounts in two accounts make at least two series, so the table has its Grand Total column.
        let fixed = [("Food", 3, 1000), ("Fuel", 20, 725)];
        let generated = posts.iter().filter(|p| p.2 != 0).map(|(a, d, v)| (names[*a], *d, *v));
        let posts: Vec<(&str, u32, i64)> = fixed.into_iter().chain(generated).collect();
        small_book(&mut c, &names, &posts);
        let general = json!({ "Start Date": { "absolute": "2024-01-01" }, "End Date": { "absolute": "2024-01-31" },
                              "Step Size": step, "Show Average": average });
        let display = json!({ "Show table": true, "Maximum Bars": max_bars, "Percentage chart": percent });
        let options = json!({ "General": general, "Accounts": { "Levels of Subaccounts": levels }, "Display": display });
        let result = c.run("Expense Over Time", &options);
        let tables = blocks_of(&result, "table");
        let series = blocks_of(&result, "chart").first().map(|c| c["series"].as_array().unwrap().clone()).unwrap_or_default();
        // No table when a percentage chart meets a negative amount (:507-518), no total column for one series.
        if let Some(t) = tables.first().filter(|_| series.len() > 1) {
            t["rows"].as_array().unwrap().iter().for_each(|row| {
                let cells = row["cells"].as_array().unwrap();
                let total = add((0, 1), &cells[cells.len() - 1]["monetary"]["amount"]);
                let amounts = &cells[1..cells.len() - 1];
                if percent {
                    let shares = amounts.iter().fold((0, 1), |s, a| add(s, &a["number"]));
                    assert!(shares == (0, 1) || shares == (100, 1), "shares {shares:?} of {total:?}");
                } else {
                    let sum = amounts.iter().fold((0, 1), |s, a| add(s, &a["monetary"]["amount"]));
                    assert_eq!(sum, total);
                }
            });
            let labels = chart(&result)["labels"].as_array().unwrap().len();
            (0..labels).for_each(|i| {
                let sum = series.iter().fold((0, 1), |s, x| add(s, &x["values"][i]));
                let total = add((0, 1), &t["rows"][i]["cells"][series.len() + 1]["monetary"]["amount"]);
                if percent {
                    assert!(sum == (0, 1) || sum == (100, 1), "percentages sum to {sum:?}");
                } else {
                    assert_eq!(sum, total, "label {i}");
                }
            });
        }
    }
}
