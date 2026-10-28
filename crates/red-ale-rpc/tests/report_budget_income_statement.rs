//! The Budget Income Statement and Budget Profit & Loss (`standard/budget-income-statement.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/budget-income-statement.scm`, cited as BIS) through `report.run` and
//! `report.export`: every Budget Income Statement assertion of 5.17's `test-budget.scm` (TB) on
//! `fixtures/reports/budget.gnucash` (`budget.expected.json`), then hand-worked cases on the fixture and on a book
//! built through the API.
//!
//! Each account shows its own Budget amounts over the budget or the chosen periods, negated for an account shown
//! reversed (BIS:274-276); the revenue table negates them again (BIS:457-460), and the net income is the unsigned
//! amounts summed and negated (BIS:430-435). On the fixture, Income's budget is -$195 and Expenses' $90.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod report_runs;
mod statements;

use serde_json::{Value, json};
use statements::{
    Setup, amount_of, blocks, expected, expected_run, gnucash_text_count, labels, options_param, row_texts, tables, test_settings,
};

const INCOME_STATEMENT: &str = "Budget Income Statement";
const PROFIT_AND_LOSS: &str = "Budget Profit & Loss";

fn fixture() -> Setup {
    Setup::fixture("budget.gnucash", &test_settings("locale"))
}

fn ranged(start: Value, end: Value) -> Value {
    json!({ "General": { "Report for range of budget periods": true, "Range start": start, "Range end": end } })
}

// Expected figures from 5.17 (TB:163-184).

/// Every check of TB:163-184 with the default options, against `template`. GnuCash lays the revenue and expense
/// tables out in rows 1 and 2 of a table of its own (BIS:541-564), and each starts with a row of spacer cells
/// (BIS:476-480); so the count reads both tables two tables deep, and "row 3 of the table in row 1" is the revenue
/// table's second row here, "row 5 of the table in row 2" the expense table's fourth. Guile's `sxpath` lists a
/// cell's texts level by level, so it reads an account's amount before its name, which is a level down in a link.
fn expected_figures(template: &str) {
    let mut s = fixture();
    let e = expected("budget.expected.json");
    let run = expected_run(&e, "budget-is-basic");
    let options = options_param(run);
    let result = s.run(template, &options);
    let html = s.export(template, &options);
    let ts = tables(&result);
    assert_eq!(ts.len(), 2, "the revenue and expense tables");
    let (inc, exp) = (ts[0], ts[1]);
    let mut income = row_texts(inc, &inc["rows"][1]);
    income.rotate_left(1);
    let net = row_texts(exp, &exp["rows"][3]);
    let got = [json!(gnucash_text_count(&ts, 2)), json!(income), json!(net)];
    run["checks"].as_array().unwrap().iter().zip(got).for_each(|(c, got)| {
        assert_eq!(got, c["expected"], "{} (test line {})", c["name"], c["line"]);
    });
    ["$195.00", "$105.00", "Net income for Budget test budget"]
        .iter()
        .for_each(|w| assert!(html.contains(w), "{w} is in the HTML"));
}

#[test]
fn expected_figures_of_gnucash_test() {
    expected_figures(INCOME_STATEMENT);
}

// Hand-worked cases.

/// The options of `budget-income-statement-options-generator-internal` (BIS:127-256), in GnuCash's order, with the
/// General options every template gets; the Budget Profit & Loss has the same options with its own name as the
/// title's default (BIS:585-586).
#[test]
fn options_are_gnucash_s() {
    let want = |name: &str| {
        [
            r#"Accounts/Accounts "a" "accountList" []"#.to_owned(),
            r#"Accounts/Levels of Subaccounts "b" "multichoice" 3"#.to_owned(),
            r#"Accounts/Flatten list to depth limit "c" "boolean" false"#.to_owned(),
            r#"Commodities/Report's currency "a" "currency" "CURRENCY::USD""#.to_owned(),
            r#"Commodities/Price Source "b" "multichoice" "pricedb-nearest""#.to_owned(),
            r#"Commodities/Show Foreign Currencies "c" "boolean" true"#.to_owned(),
            r#"Commodities/Show Exchange Rates "d" "boolean" false"#.to_owned(),
            r#"Display/Include accounts with zero total balances "a" "boolean" true"#.to_owned(),
            r#"Display/Omit zero balance figures "b" "boolean" false"#.to_owned(),
            r#"Display/Parent account balances "ca" "multichoice" "immediate-bal""#.to_owned(),
            r#"Display/Parent account subtotals "cb" "multichoice" "f""#.to_owned(),
            r#"Display/Display accounts as hyperlinks "d" "boolean" true"#.to_owned(),
            r#"Display/Show accounting-style rules "e" "boolean" false"#.to_owned(),
            r#"Display/Label the revenue section "f" "boolean" true"#.to_owned(),
            r#"Display/Include revenue total "g" "boolean" true"#.to_owned(),
            r#"Display/Label the expense section "h" "boolean" true"#.to_owned(),
            r#"Display/Include expense total "i" "boolean" true"#.to_owned(),
            r#"Display/Display as a two column report "j" "boolean" false"#.to_owned(),
            r#"Display/Display in standard, income first, order "k" "boolean" true"#.to_owned(),
            format!(r#"General/Report name "0a" "string" "{name}""#),
            r#"General/Stylesheet "0b" "multichoice" "Default""#.to_owned(),
            r#"General/Add options summary "0c" "multichoice" "never""#.to_owned(),
            format!(r#"General/Report Title "a" "string" "{name}""#),
            r#"General/Budget "c" "budget" null"#.to_owned(),
            r#"General/Report for range of budget periods "d" "boolean" false"#.to_owned(),
            r#"General/Range start "e" "numberRange" 1"#.to_owned(),
            r#"General/Range end "f" "numberRange" 1"#.to_owned(),
        ]
    };
    assert_eq!(report_runs::described_options(INCOME_STATEMENT), want(INCOME_STATEMENT));
    assert_eq!(report_runs::described_options(PROFIT_AND_LOSS), want(PROFIT_AND_LOSS));
}

/// The defaults on the fixture (BIS:127-256): the whole budget, three levels, income first in one column; the
/// title is the company name (none), the report title and "for Budget test budget" (BIS:500-503, :531-532). The
/// revenue total is the signed amounts, -$195, while Income's row shows them negated, $195 (BIS:424-425,
/// :457-460); the net income goes below the expenses (BIS:534-539).
#[test]
fn default_options() {
    let result = fixture().run(INCOME_STATEMENT, &json!({}));
    assert_eq!(result["title"], " Budget Income Statement for Budget test budget");
    let ts = tables(&result);
    assert_eq!(labels(ts[0]), ["Revenues", "Income", "Income-GBP", "Total Revenue"]);
    assert_eq!(labels(ts[1]), ["Expenses", "Expenses", "Total Expenses", "Net income for Budget test budget"]);
    assert_eq!(amount_of(&result, "Total Revenue"), ["-$195.00"]);
    assert_eq!(amount_of(&result, "Income-GBP"), ["£0.00", "$0.00"], "a foreign account's commodity table (HAT:1035-1039)");
    assert_eq!(amount_of(&result, "Total Expenses"), ["$90.00"]);
    let net = &ts[1]["rows"][3];
    assert_eq!((net["role"].as_str(), net["level"].as_u64()), (Some("grandTotal"), Some(0)), "the rightmost level (BIS:524-527)");
}

/// The Budget Profit & Loss is the Budget Income Statement under another name (BIS:577-609): the same statement,
/// whole or ranged, and only the title's default differs. Its renderer passes the Income Statement's name
/// (BIS:587-588), so with no account it warns as the Budget Income Statement.
#[test]
fn profit_and_loss_is_the_income_statement_renamed() {
    expected_figures(PROFIT_AND_LOSS);
    let mut s = fixture();
    [json!({}), ranged(json!(2), json!(4))].iter().for_each(|options| {
        let pnl = s.run(PROFIT_AND_LOSS, options);
        let is = s.run(INCOME_STATEMENT, options);
        assert_eq!(pnl["sections"], is["sections"]);
        assert_eq!(pnl["report"]["guid"], "e5fa5ce805e840ecbeca4dba3fa4ead9");
        assert_eq!(pnl["title"], is["title"].as_str().unwrap().replace(INCOME_STATEMENT, PROFIT_AND_LOSS));
    });
    let mut s = Setup::new_book(&test_settings("locale"));
    let result = s.run(PROFIT_AND_LOSS, &json!({}));
    assert_eq!(result["title"], INCOME_STATEMENT);
    assert_eq!(blocks(&result)[0]["title"], "No accounts selected");
}

/// A book with a Budget "Plan" of three monthly periods from 2024-01-01 and no transactions: Salary -400 in each
/// period, Consulting (in euros) -100 in periods 0 and 1, Food 150 in period 0 and 100 in period 2, Rent 500 in each;
/// €1 is $1.10 on 2024-01-01 and $1.30 on 2024-02-01.
fn plan_book() -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    s.account("Checking", "BANK", None, "CURRENCY::USD");
    s.account("Salary", "INCOME", None, "CURRENCY::USD");
    s.account("Consulting", "INCOME", None, "CURRENCY::EUR");
    s.account("Food", "EXPENSE", None, "CURRENCY::USD");
    s.account("Rent", "EXPENSE", None, "CURRENCY::USD");
    [("2024-01-01", "1.10"), ("2024-02-01", "1.30")].iter().for_each(|(date, value)| {
        s.call("price.add", json!({ "commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "value": value, "date": date }));
    });
    let recurrence = json!({ "mult": 1, "periodType": "month", "start": "2024-01-01" });
    let budget = s.call("budget.create", json!({ "name": "Plan", "numPeriods": 3, "recurrence": recurrence }))["guid"].clone();
    let cells = [
        ("Salary", 0, "-400"), ("Salary", 1, "-400"), ("Salary", 2, "-400"), ("Consulting", 0, "-100"), ("Consulting", 1, "-100"),
        ("Food", 0, "150"), ("Food", 2, "100"), ("Rent", 0, "500"), ("Rent", 1, "500"), ("Rent", 2, "500"),
    ];
    let amounts: Vec<Value> = cells.iter().map(|(a, p, v)| json!({ "account": a, "period": p, "amount": v })).collect();
    s.call("budget.setAmounts", json!({ "budget": budget, "amounts": amounts }));
    s
}

/// The figures of the rows labelled `rows`, as `label: displays`.
fn figures(result: &Value, rows: &[&str]) -> Vec<String> {
    rows.iter().map(|l| format!("{l}: {}", amount_of(result, l).join(" "))).collect()
}

/// The period range on the plan book (BIS:286-309, :500-510), each worked from BIS:414-435 with prices at the
/// first reported period's start (BIS:304-309, :366-369):
///
/// - the whole budget, at €1 = $1.10: Consulting €200 ($220) and Salary $1,200, so revenue -$1,420; Food $250 and
///   Rent $1,500; net -(-€200 - $1,200 + $1,750) = €200 - $550, a loss of $330;
/// - "Range start" and "Range end" 2, period 1 alone, at $1.30: Consulting €100 ($130), Salary $400, revenue -$530;
///   Food $0 (an unset amount reads 0, `5.17:gnucash/report/report-utilities.scm:751-760`), Rent $500; net €100 -
///   $100, an income of $30;
/// - 1 to 2, periods 0 and 1, at $1.10: revenue -$1,020, expenses $1,150, net €200 - $350, a loss of $130.
#[test]
fn period_range() {
    let mut s = plan_book();
    let rows = ["Consulting", "Salary", "Total Revenue", "Food", "Rent", "Total Expenses"];
    let result = s.run(INCOME_STATEMENT, &json!({}));
    assert_eq!(result["title"], " Budget Income Statement for Budget Plan");
    assert_eq!(
        figures(&result, &rows),
        ["Consulting: €200.00 $220.00", "Salary: $1,200.00", "Total Revenue: -$1,420.00", "Food: $250.00", "Rent: $1,500.00",
         "Total Expenses: $1,750.00"]
    );
    assert_eq!(amount_of(&result, "Net loss for Budget Plan"), ["$330.00"]);

    let result = s.run(INCOME_STATEMENT, &ranged(json!(2), json!(2)));
    assert_eq!(result["title"], " Budget Income Statement for Budget Plan Period 2");
    assert_eq!(
        figures(&result, &rows),
        ["Consulting: €100.00 $130.00", "Salary: $400.00", "Total Revenue: -$530.00", "Food: $0.00", "Rent: $500.00",
         "Total Expenses: $500.00"]
    );
    assert_eq!(amount_of(&result, "Net income for Budget Plan Period 2"), ["$30.00"]);

    let result = s.run(INCOME_STATEMENT, &ranged(json!(1), json!(2)));
    assert_eq!(result["title"], " Budget Income Statement for Budget Plan Periods 1 - 2");
    assert_eq!(figures(&result, &["Total Revenue", "Total Expenses"]), ["Total Revenue: -$1,020.00", "Total Expenses: $1,150.00"]);
    assert_eq!(amount_of(&result, "Net loss for Budget Plan Periods 1 - 2"), ["$130.00"]);
}

/// Period numbers are truncated to whole numbers (BIS:288-299): 2.9 to 3.9 is periods 2 to 3. A range that ends
/// before it starts, or past the budget's six periods, is a warning titled by the report title (BIS:397-410).
#[test]
fn range_truncation_and_errors() {
    let mut s = fixture();
    let result = s.run(INCOME_STATEMENT, &ranged(json!("2.9"), json!("3.9")));
    assert_eq!(result["title"], " Budget Income Statement for Budget test budget Periods 2 - 3");
    assert_eq!(amount_of(&result, "Income"), ["$65.00"], "period 2 as the user counts them has no amount; period 3 has -$65");
    let warning = |result: &Value| (result["title"].clone(), blocks(result)[0]["title"].clone(), blocks(result)[0]["text"].clone());
    let result = s.run(INCOME_STATEMENT, &ranged(json!(3), json!(2)));
    assert_eq!(
        warning(&result),
        (json!(INCOME_STATEMENT), json!("Budget Income Statement:"), json!("Reporting range end period cannot be less than start period."))
    );
    let mut options = ranged(json!(1), json!(7));
    options["General"]["Report Title"] = json!("Plan");
    let result = s.run(INCOME_STATEMENT, &options);
    assert_eq!(
        warning(&result),
        (json!("Plan"), json!("Plan:"), json!("Reporting range end period cannot exceed number of periods in budget"))
    );
}

/// Under `credit` Income's amounts are negated (BIS:274-276), so the revenue total turns positive, $195, while the row
/// is negated twice more, by the revenue table (BIS:457-460) and by the account table for a reversed account
/// (HAT:1029-1030), and still shows $195; the net income takes the unsigned amounts and is unchanged (BIS:430-435).
#[test]
fn reversed_accounts() {
    let mut settings = test_settings("locale");
    settings["reports"]["reverseBalancedAccounts"] = json!("credit");
    let mut s = Setup::fixture("budget.gnucash", &settings);
    let result = s.run(INCOME_STATEMENT, &json!({}));
    assert_eq!(
        figures(&result, &["Income", "Total Revenue", "Total Expenses", "Net income for Budget test budget"]),
        ["Income: $195.00", "Total Revenue: $195.00", "Total Expenses: $90.00", "Net income for Budget test budget: $105.00"]
    );
}

/// The four layouts (BIS:534-558): the net income goes below the expenses in the standard order and below the
/// revenue otherwise; the tables stand one above the other, or side by side in a `group` with "Display as a two
/// column report", the revenue first in the standard order.
#[test]
fn layouts() {
    let mut s = fixture();
    let layout = |s: &mut Setup, two: bool, standard: bool| {
        let display = json!({ "Display as a two column report": two, "Display in standard, income first, order": standard });
        let result = s.run(INCOME_STATEMENT, &json!({ "Display": display }));
        let grouped = result["sections"][0]["blocks"][0]["kind"] == "group";
        let firsts: Vec<String> = tables(&result).iter().map(|t| labels(t)[0].clone()).collect();
        let net_in = tables(&result).iter().position(|t| labels(t).last().unwrap().starts_with("Net income")).unwrap();
        (grouped, firsts, net_in)
    };
    let (rev, exp) = ("Revenues".to_owned(), "Expenses".to_owned());
    assert_eq!(layout(&mut s, false, true), (false, vec![rev.clone(), exp.clone()], 1));
    assert_eq!(layout(&mut s, false, false), (false, vec![exp.clone(), rev.clone()], 1));
    assert_eq!(layout(&mut s, true, true), (true, vec![rev.clone(), exp.clone()], 1));
    assert_eq!(layout(&mut s, true, false), (true, vec![exp, rev], 1));
}

/// With no budget the report warns, titled by the report title (BIS:392-395); the exchange rates follow the
/// statement when asked (BIS:566-570), at the first reported period's start.
#[test]
fn budget_warning_and_rates() {
    let mut s = Setup::fixture("test-data.gnucash", &test_settings("locale"));
    let result = s.run(INCOME_STATEMENT, &json!({}));
    assert_eq!(result["title"], INCOME_STATEMENT);
    assert_eq!(blocks(&result)[0]["text"], "No budgets exist. You must create at least one budget.");
    let mut s = plan_book();
    let mut options = ranged(json!(2), json!(3));
    options["Commodities"] = json!({ "Show Exchange Rates": true });
    assert_eq!(statements::rates(&s.run(INCOME_STATEMENT, &options)), ["€1.00", "$1.30"]);
}
