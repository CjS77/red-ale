//! The Budget Chart (`standard/budget-barchart.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/budget-barchart.scm`, cited as BC) through `report.run` and `report.export`.
//! GnuCash has no test of it, so every case is hand-worked from the lines it cites.
//!
//! The report draws a chart per account: for each budget period in the range, both ends inclusive, the account's
//! actual and its rolled-up Budget amount, each a running sum from period 0 by default (BC:155-222).
//!
//! On `fixtures/reports/budget.gnucash` the budget, "test budget", has six monthly periods from 2024-03-01, so on the
//! pinned now, 2024-03-15, the current period is 0 and the next 1. Bank's amounts are $20, $40, unset, $60, unset and
//! unset, its actuals $35, -$20, $67, $77, $0 and $0: running sums of $20, $60, $60, $120, $120 and $120 budgeted and
//! $35, $15, $82, $159, $159 and $159 actual.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod report_runs;
mod statements;

use report_runs::values;
use serde_json::{Value, json};
use statements::{Setup, blocks, test_settings};

const TEMPLATE: &str = "Budget Chart";

fn fixture() -> Setup {
    Setup::fixture("budget.gnucash", &test_settings("iso"))
}

/// The charts of a run.
fn charts(result: &Value) -> Vec<&Value> {
    blocks(result).into_iter().filter(|b| b["kind"] == "chart").collect()
}

/// The chart titled with `account`'s name.
fn chart<'a>(result: &'a Value, account: &str) -> &'a Value {
    charts(result)
        .into_iter()
        .find(|c| c["title"][0] == account)
        .unwrap_or_else(|| panic!("no chart of {account}"))
}

/// A chart's labels, then its "Actual" and "Budget" values.
fn figures(chart: &Value) -> (Vec<String>, Vec<String>, Vec<String>) {
    let labels = chart["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_owned())
        .collect();
    (labels, values(&chart["series"][0]), values(&chart["series"][1]))
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

fn general(general: Value) -> Value {
    json!({ "General": general })
}

/// The options of `options-generator` (BC:72-146), in GnuCash's order, with the General options every template gets.
#[test]
fn options_are_gnucash_s() {
    assert_eq!(
        report_runs::described_options(TEMPLATE),
        [
            r#"Accounts/Accounts "c" "accountList" []"#,
            r#"Accounts/Levels of Subaccounts "d" "multichoice" 6"#,
            r#"Display/Running Sum "a" "boolean" true"#,
            r#"Display/Chart Type "b" "multichoice" "bars""#,
            r#"Display/Plot Width "ca" "plotSize" {"percent":100}"#,
            r#"Display/Plot Height "cb" "plotSize" {"percent":100}"#,
            r#"General/Report name "0a" "string" "Budget Chart""#,
            r#"General/Stylesheet "0b" "multichoice" "Default""#,
            r#"General/Add options summary "0c" "multichoice" "never""#,
            r#"General/Budget "a" "budget" null"#,
            r#"General/Range start "g1.1" "multichoice" "current""#,
            r#"General/Exact start period "g1.2" "numberRange" 1"#,
            r#"General/Range end "g2.1" "multichoice" "next""#,
            r#"General/Exact end period "g2.2" "numberRange" 1"#,
        ]
    );
}

/// The defaults: the bank, asset and liability accounts (BC:114-119), each a leaf within six levels, get a chart in
/// that order (BC:304-317); Root and Asset have children. The range is the current period to the next, 0 to 1
/// (BC:246-270), each a point at the period's start (BC:207, :218). The "Actual" series comes first, red, then the
/// "Budget" series, blue, filled bars (BC:195-204), on an axis in the account's currency (BC:176-178). With running
/// sums the title gives the totals over every period, those outside the range too (BC:185-193).
#[test]
fn budget_against_actual_per_account() {
    let mut s = fixture();
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], TEMPLATE);
    let titles: Vec<&Value> = charts(&result).iter().map(|c| &c["title"]).collect();
    assert_eq!(
        titles,
        [
            &json!(["Bank", "Bgt: $120.00 Act: $159.00"]),
            &json!(["GBP Bank", "Bgt: £0.00 Act: £0.00"]),
            &json!(["Wallet", "Bgt: $0.00 Act: $0.00"]),
            &json!(["Liabilities", "Bgt: $0.00 Act: $0.00"]),
        ]
    );
    let bank = chart(&result, "Bank");
    assert_eq!(
        figures(bank),
        (
            strings(&["2024-03-01", "2024-04-01"]),
            strings(&["35", "15"]),
            strings(&["20", "60"])
        )
    );
    let series = |c: &Value, i: usize| {
        (
            c["series"][i]["label"].clone(),
            c["series"][i]["color"].clone(),
            c["series"][i]["fill"].clone(),
        )
    };
    assert_eq!(series(bank, 0), (json!("Actual"), json!("#ff4136"), json!(true)));
    assert_eq!(series(bank, 1), (json!("Budget"), json!("#0074d9"), json!(true)));
    let usd_axis = json!({ "label": "USD", "format": "currency", "commodity": "CURRENCY::USD" });
    assert_eq!((&bank["type"], &bank["yAxis"]), (&json!("bar"), &usd_axis));
    assert_eq!(chart(&result, "GBP Bank")["yAxis"]["commodity"], "CURRENCY::GBP");

    let html = s.export(TEMPLATE, &json!({}));
    assert!(html.contains("<svg"), "the charts are drawn");
    ["Bgt: $120.00 Act: $159.00", "Bgt: £0.00 Act: £0.00", "Actual", "Budget"]
        .iter()
        .for_each(|w| assert!(html.contains(w), "{w} is in the HTML"));
}

/// Without "Running Sum" each point is its own period's figures (BC:208-213), and the title only names the account
/// (BC:193).
#[test]
fn running_sum_off() {
    let mut s = fixture();
    let result = s.run(TEMPLATE, &json!({ "Display": { "Running Sum": false } }));
    let bank = chart(&result, "Bank");
    assert_eq!(bank["title"], json!(["Bank"]));
    assert_eq!(
        figures(bank),
        (
            strings(&["2024-03-01", "2024-04-01"]),
            strings(&["35", "-20"]),
            strings(&["20", "40"])
        )
    );
}

/// "Line Chart" draws lines, unfilled (BC:175, :199, :204); the figures are the same as the bars'.
#[test]
fn bar_and_line() {
    let mut s = fixture();
    let bars = s.run(TEMPLATE, &json!({}));
    let lines = s.run(TEMPLATE, &json!({ "Display": { "Chart Type": "lines" } }));
    let (bar, line) = (chart(&bars, "Bank"), chart(&lines, "Bank"));
    assert_eq!(line["type"], "line");
    let filled = |c: &Value, i: usize| c["series"][i]["fill"].as_bool().unwrap_or(false);
    assert_eq!(
        (filled(line, 0), filled(line, 1)),
        (false, false),
        "a false flag is left out of the result"
    );
    assert_eq!(figures(line), figures(bar));
}

/// A period range (BC:263-273, :214): manual periods 2 to 4, as the user counts them, are periods 1 to 3, running sums
/// still counted from period 0; without running sums, those periods' own figures. "First" to "last" is every period.
/// Exact periods round, ties to even (BC:272-273): 2.5 and 3.5 are 2 and 4, so periods 1 to 3 again, where the Budget
/// Report's truncation would give 1 to 2. "Previous" in period 0 stays at 0 (BC:261).
#[test]
fn a_period_range() {
    let mut s = fixture();
    let manual = |start: Value, end: Value| json!({ "Range start": "manual", "Exact start period": start, "Range end": "manual", "Exact end period": end });
    let april_to_june = strings(&["2024-04-01", "2024-05-01", "2024-06-01"]);
    let result = s.run(TEMPLATE, &general(manual(json!(2), json!(4))));
    assert_eq!(
        figures(chart(&result, "Bank")),
        (april_to_june.clone(), strings(&["15", "82", "159"]), strings(&["60", "60", "120"]))
    );

    let mut options = general(manual(json!(2), json!(4)));
    options["Display"] = json!({ "Running Sum": false });
    let result = s.run(TEMPLATE, &options);
    assert_eq!(
        figures(chart(&result, "Bank")),
        (april_to_june.clone(), strings(&["-20", "67", "77"]), strings(&["40", "0", "60"]))
    );

    let result = s.run(TEMPLATE, &general(json!({ "Range start": "first", "Range end": "last" })));
    let (labels, actual, budget) = figures(chart(&result, "Bank"));
    assert_eq!(labels.len(), 6);
    assert_eq!(
        (actual, budget),
        (
            strings(&["35", "15", "82", "159", "159", "159"]),
            strings(&["20", "60", "60", "120", "120", "120"])
        )
    );

    let result = s.run(TEMPLATE, &general(manual(json!("2.5"), json!("3.5"))));
    assert_eq!(figures(chart(&result, "Bank")).0, april_to_june);

    let result = s.run(TEMPLATE, &general(json!({ "Range start": "previous", "Range end": "current" })));
    assert_eq!(
        figures(chart(&result, "Bank")),
        (strings(&["2024-03-01"]), strings(&["35"]), strings(&["20"]))
    );
}

/// A book with a Checking account and two budgets of three monthly periods, $10 a period each: "Future" from
/// 2024-06-01 and "Past" from 2023-01-01.
fn two_budgets() -> (Setup, Value, Value) {
    let mut s = Setup::new_book(&test_settings("iso"));
    s.account("Checking", "BANK", None, "CURRENCY::USD");
    let mut budget = |name: &str, start: &str| {
        let recurrence = json!({ "mult": 1, "periodType": "month", "start": start });
        let guid = s.call("budget.create", json!({ "name": name, "numPeriods": 3, "recurrence": recurrence }))["guid"].clone();
        let amounts: Vec<Value> = (0..3)
            .map(|p| json!({ "account": "Checking", "period": p, "amount": "10" }))
            .collect();
        s.call("budget.setAmounts", json!({ "budget": guid, "amounts": amounts }));
        guid
    };
    let (future, past) = (budget("Future", "2024-06-01"), budget("Past", "2023-01-01"));
    (s, future, past)
}

/// Relative periods outside the budget (BC:252-253): before it, "current" and "next" are both period 1, the second,
/// so the chart has one point, July's, at the running sum of $20; after it both are the number of periods, 3, past
/// the last, so the chart has no point while its title still sums every period.
#[test]
fn relative_periods_outside_the_budget() {
    let (mut s, future, past) = two_budgets();
    let result = s.run(TEMPLATE, &general(json!({ "Budget": future })));
    assert_eq!(
        figures(chart(&result, "Checking")),
        (strings(&["2024-07-01"]), strings(&["0"]), strings(&["20"]))
    );
    let result = s.run(TEMPLATE, &general(json!({ "Budget": past })));
    let checking = chart(&result, "Checking");
    assert_eq!(checking["title"], json!(["Checking", "Bgt: $30.00 Act: $0.00"]));
    assert_eq!(figures(checking), (vec![], vec![], vec![]));
    assert!(
        s.export(TEMPLATE, &general(json!({ "Budget": past })))
            .contains("Bgt: $30.00 Act: $0.00")
    );
}

/// "Levels of Subaccounts" (BC:304-312): an account at the limit gets a chart whatever its children, a leaf above it
/// too, and under "all" only leaves. The budget is rolled up from sub-accounts when an account has no amount of its
/// own (`gnc:get-account-period-rolledup-budget-value`, `5.17:gnucash/report/report-utilities.scm:819-838`), and the
/// actual includes them.
///
/// - At two levels, of Root and Asset only Asset is charted: its budget is Bank's, $20 and $40, and its actual Bank's
///   with GBP Bank's and Wallet's, which are 0.
/// - At one level Root is charted: its budget sums its children's, Asset's $20 and Income's -$55 in period 0, Asset's
///   $40 and Expenses' $30 in period 1, so running sums of -$35 and $35; over all periods also Income's -$65 and
///   Expenses' $20 in period 2 and Bank's $60, Expenses' $40 and Income's -$75 in period 3, $15 in all. Its actuals
///   are its whole tree's, which balances: $0.
/// - Under "all", of Root, Asset and Bank only Bank is charted.
#[test]
fn levels_and_rolled_up_budgets() {
    let mut s = fixture();
    let run = |s: &mut Setup, accounts: &[&str], depth: Value| {
        s.run(
            TEMPLATE,
            &json!({ "Accounts": { "Accounts": accounts, "Levels of Subaccounts": depth } }),
        )
    };
    let result = run(&mut s, &["Root", "Root:Asset"], json!(2));
    assert_eq!(charts(&result).len(), 1);
    assert_eq!(figures(chart(&result, "Asset")).1, strings(&["35", "15"]));
    assert_eq!(figures(chart(&result, "Asset")).2, strings(&["20", "60"]));

    let result = run(&mut s, &["Root"], json!(1));
    let root = chart(&result, "Root");
    assert_eq!(root["title"], json!(["Root", "Bgt: $15.00 Act: $0.00"]));
    assert_eq!((figures(root).1, figures(root).2), (strings(&["0", "0"]), strings(&["-35", "35"])));

    let result = run(&mut s, &["Root", "Root:Asset", "Root:Asset:Bank"], json!("all"));
    let titles: Vec<&Value> = charts(&result).iter().map(|c| &c["title"][0]).collect();
    assert_eq!(titles, [&json!("Bank")]);
}

/// With no account to choose, then with no budget in the book, the report only warns (BC:289-298), the first under
/// the report's title, the second under its template name.
#[test]
fn warnings() {
    let mut s = Setup::new_book(&test_settings("iso"));
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], TEMPLATE);
    assert_eq!(blocks(&result)[0]["title"], "No accounts selected");

    let mut s = Setup::fixture("test-data.gnucash", &test_settings("iso"));
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], TEMPLATE);
    assert_eq!(blocks(&result)[0]["text"], "No budgets exist. You must create at least one budget.");
}
