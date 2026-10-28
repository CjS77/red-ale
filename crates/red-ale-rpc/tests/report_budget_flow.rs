//! The Budget Flow (`standard/budget-flow.rhai`, translated from `5.17:gnucash/report/reports/standard/budget-flow.scm`,
//! cited as BF) through `report.run` and `report.export`. GnuCash has no test of it, so every case is hand-worked
//! from the lines it cites.
//!
//! The report shows one period of a budget: a row for each selected account without children, with its own Budget
//! amount, unset as 0 and not rolled up, and its actual (BF:124-152); each kind of account totalled in the report's
//! currency (BF:155-162); then the kinds' totals, last kind first (BF:190-194), and a grand total (BF:231-240).
//!
//! On `fixtures/reports/budget.gnucash` the budget, "test budget", has six monthly periods from 2024-03-01. Bank's
//! amounts are $20, $40, unset and $60 in periods 0-3 and its actuals $35, -$20, $67 and $77; Income's amounts -$55,
//! unset, -$65 and -$75 and its actuals -$55, $0 (period 1's transaction is a closing one, which actuals leave out),
//! -$67 and -$77; Expenses' amounts unset, $30, $20 and $40 and its actuals $20 and $20. Root and Asset have children,
//! so they get no row.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod report_runs;
mod statements;

use serde_json::{Value, json};
use statements::{Setup, blocks, tables, test_settings};

const TEMPLATE: &str = "Budget Flow";

const BANK: &str = "a0000000000000000000000000000003";

/// The rows of the default period on the fixture: amounts in each account's commodity, each kind totalled in
/// dollars. The kinds' totals are listed last kind first, Trading Accounts with no accounts included (BF:179-197).
const PERIOD_1: [&str; 27] = [
    "Root:Asset:Bank | $20.00 | $35.00",
    "Root:Asset:GBP Bank | £0.00 | £0.00",
    "Root:Asset:Wallet | $0.00 | $0.00",
    "Total: | $20.00 | $35.00",
    "---",
    "Root:Liabilities | $0.00 | $0.00",
    "Total: | $0.00 | $0.00",
    "---",
    "Root:Equity | $0.00 | $0.00",
    "Total: | $0.00 | $0.00",
    "---",
    "Root:Income | -$55.00 | -$55.00",
    "Root:Income-GBP | £0.00 | £0.00",
    "Total: | -$55.00 | -$55.00",
    "---",
    "Root:Expenses | $0.00 | $20.00",
    "Total: | $0.00 | $20.00",
    "---",
    "Total: | $0.00 | $0.00",
    "---",
    "Trading Accounts | $0.00 | $0.00",
    "Expenses | $0.00 | $20.00",
    "Income | -$55.00 | -$55.00",
    "Equities | $0.00 | $0.00",
    "Liabilities | $0.00 | $0.00",
    "Assets | $20.00 | $35.00",
    "Total: | -$35.00 | $0.00",
];

fn fixture() -> Setup {
    Setup::fixture("budget.gnucash", &test_settings("iso"))
}

fn period(v: Value) -> Value {
    json!({ "General": { "Period": v } })
}

/// Each row of the run's table as `label | budgeted | actual`; a rule as `---`.
fn rows(result: &Value) -> Vec<String> {
    let shown = |c: &Value| c["display"].as_str().or_else(|| c["text"].as_str()).unwrap().to_owned();
    let row = |r: &Value| {
        if r["role"] == "rule" {
            return "---".to_owned();
        }
        r["cells"].as_array().unwrap().iter().map(shown).collect::<Vec<_>>().join(" | ")
    };
    tables(result)[0]["rows"].as_array().unwrap().iter().map(row).collect()
}

/// The options of `budget-report-options-generator` (BF:48-88), in GnuCash's order, with the General options every
/// template gets.
#[test]
fn options_are_gnucash_s() {
    assert_eq!(
        report_runs::described_options(TEMPLATE),
        [
            r#"Accounts/Account "ac" "accountList" []"#,
            r#"General/Report name "0a" "string" "Budget Flow""#,
            r#"General/Stylesheet "0b" "multichoice" "Default""#,
            r#"General/Add options summary "0c" "multichoice" "never""#,
            r#"General/Budget "a" "budget" null"#,
            r#"General/Period "b" "numberRange" 1"#,
            r#"General/Price Source "c" "multichoice" "pricedb-latest""#,
            r#"General/Report's currency "d" "currency" "CURRENCY::USD""#,
        ]
    );
}

/// A single period, the default first one (BF:57-67), with every account (BF:79-83): period 0's amounts and
/// actuals (BF:131-136). The grand total adds the kinds' totals: $20 - $55 budgeted, $35 - $55 + $20 actual. The title
/// is the report name, the budget's name and the period's start (BF:295-299). An account's row and label name the
/// account (BF:143-148), and the exported page shows the same figures.
#[test]
fn a_single_period() {
    let mut s = fixture();
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], "Budget Flow: test budget - 2024-03-01");
    assert_eq!(rows(&result), PERIOD_1);
    let bank = &tables(&result)[0]["rows"][0];
    assert_eq!(
        (&bank["role"], &bank["account"], &bank["cells"][0]["account"]),
        (&json!("account"), &json!(BANK), &json!(BANK))
    );
    let html = s.export(TEMPLATE, &json!({}));
    ["Budget Flow: test budget - 2024-03-01", "Root:Asset:Bank", "-$35.00", "Trading Accounts"]
        .iter()
        .for_each(|w| assert!(html.contains(w), "{w} is in the HTML"));
}

/// Budgeted against actual in periods 2 and 3, as the user counts them (`(1- period)`, BF:131-136). Period 2: Bank's
/// $40 against -$20 and Expenses' $30 against $20; Income has no amount, and the closing transaction leaves its actual
/// at $0, as it leaves Equity's. Period 3: Bank's amount is unset, read as $0, against $67; Income -$65 against -$67;
/// Expenses $20 against nothing spent.
#[test]
fn budgeted_against_actual() {
    let mut s = fixture();
    let result = s.run(TEMPLATE, &period(json!(2)));
    assert_eq!(result["title"], "Budget Flow: test budget - 2024-04-01");
    let two = rows(&result);
    assert_eq!(two[0], "Root:Asset:Bank | $40.00 | -$20.00");
    assert_eq!(two[8], "Root:Equity | $0.00 | $0.00");
    assert_eq!(two[11], "Root:Income | $0.00 | $0.00");
    assert_eq!(two[15], "Root:Expenses | $30.00 | $20.00");
    assert_eq!(
        two[20..],
        [
            "Trading Accounts | $0.00 | $0.00",
            "Expenses | $30.00 | $20.00",
            "Income | $0.00 | $0.00",
            "Equities | $0.00 | $0.00",
            "Liabilities | $0.00 | $0.00",
            "Assets | $40.00 | -$20.00",
            "Total: | $70.00 | $0.00",
        ]
    );

    let three = rows(&s.run(TEMPLATE, &period(json!(3))));
    assert_eq!(three[0], "Root:Asset:Bank | $0.00 | $67.00");
    assert_eq!(three[11], "Root:Income | -$65.00 | -$67.00");
    assert_eq!(three[15], "Root:Expenses | $20.00 | $0.00");
    assert_eq!(three.last().unwrap(), "Total: | -$45.00 | $0.00");
}

/// The period is not limited to the budget's six (BF:57-67): period 7 reads no amounts, which are 0 past the count
/// (`5.17:libgnucash/engine/gnc-budget.cpp:587-597`), and the actuals of the recurrence's seventh month, September,
/// when nothing was posted.
#[test]
fn a_period_past_the_budget() {
    let mut s = fixture();
    let result = s.run(TEMPLATE, &period(json!(7)));
    assert_eq!(result["title"], "Budget Flow: test budget - 2024-09-01");
    let shown = rows(&result);
    let zero = |r: &&String| r.as_str() == "---" || r.ends_with("$0.00 | $0.00") || r.ends_with("£0.00 | £0.00");
    assert!(shown.iter().all(|r| zero(&r)), "{shown:#?}");
}

/// The report never reverses signs: budget-flow.scm calls neither `gnc-reverse-balance` nor a helper that does, so
/// under `credit` Income's figures stay negative.
#[test]
fn reversed_accounts() {
    let mut settings = test_settings("iso");
    settings["reports"]["reverseBalancedAccounts"] = json!("credit");
    let mut s = Setup::fixture("budget.gnucash", &settings);
    assert_eq!(rows(&s.run(TEMPLATE, &json!({}))), PERIOD_1);
}

/// A book with a Budget "Plan" of three monthly periods from 2024-01-01: Checking and Savings (in euros), Salary and
/// Consulting (in euros), and Expenses with a child, Food. Period 0's amounts: Savings €50, Salary -$400, Consulting
/// -€100, Expenses $999 and Food $150. In January $400 of salary reaches Checking and $120 goes on food; €90 of
/// consulting reaches Savings. €1 is $1.10 on 2024-01-20, $1.30 on 2024-02-25 and $1.50 on 2024-06-01.
fn plan_book() -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    [
        ("Checking", "BANK", None, "CURRENCY::USD"),
        ("Savings", "BANK", None, "CURRENCY::EUR"),
        ("Salary", "INCOME", None, "CURRENCY::USD"),
        ("Consulting", "INCOME", None, "CURRENCY::EUR"),
        ("Expenses", "EXPENSE", None, "CURRENCY::USD"),
        ("Food", "EXPENSE", Some("Expenses"), "CURRENCY::USD"),
    ]
    .iter()
    .for_each(|(name, kind, parent, commodity)| s.account(name, kind, *parent, commodity));
    [("2024-01-20", "1.10"), ("2024-02-25", "1.30"), ("2024-06-01", "1.50")].iter().for_each(|(date, value)| {
        let price = json!({ "commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "value": value, "date": date });
        s.call("price.add", price);
    });
    let recurrence = json!({ "mult": 1, "periodType": "month", "start": "2024-01-01" });
    let budget = s.call("budget.create", json!({ "name": "Plan", "numPeriods": 3, "recurrence": recurrence }))["guid"].clone();
    let cells = [("Savings", "50"), ("Salary", "-400"), ("Consulting", "-100"), ("Expenses", "999"), ("Expenses:Food", "150")];
    let amounts: Vec<Value> = cells.iter().map(|(a, v)| json!({ "account": a, "period": 0, "amount": v })).collect();
    s.call("budget.setAmounts", json!({ "budget": budget, "amounts": amounts }));
    s.transfer("2024-01-15", "Salary", "Salary", "Checking", "400");
    s.transfer("2024-01-16", "Food", "Checking", "Expenses:Food", "120");
    let splits = json!([{ "account": "Savings", "value": "90" }, { "account": "Consulting", "value": "-90" }]);
    let t = json!({ "datePosted": "2024-01-17", "currency": "CURRENCY::EUR", "description": "Consulting", "splits": splits });
    s.call("transaction.post", t);
    s
}

/// Totals in the report's currency (BF:155-158, :231-236): the euro figures are exchanged by the price source. With
/// the default, the latest price, €1 = $1.50: assets $75 budgeted, $280 + $135 = $415 actual; income -$400 - $150 =
/// -$550 and -$400 - $135 = -$535; Food's $150 and $120; the grand totals -$325 and $0. Expenses has a child, so its
/// $999 is neither shown nor counted (BF:119-127).
///
/// The nearest price is taken at the end of the period after the reported one (`gnc-budget-get-period-end-date
/// budget period` with the 1-based period, BF:286-288): 2024-02-29, nearest to February's $1.30, where January's end
/// would be nearer the $1.10 of 2024-01-20. Assets are then $65 and $280 + $117 = $397, income -$530 and -$517, the
/// grand totals -$315 and $0.
#[test]
fn totals_in_the_report_currency() {
    let mut s = plan_book();
    assert_eq!(
        rows(&s.run(TEMPLATE, &json!({}))),
        [
            "Checking | $0.00 | $280.00",
            "Savings | €50.00 | €90.00",
            "Total: | $75.00 | $415.00",
            "---",
            "Total: | $0.00 | $0.00",
            "---",
            "Total: | $0.00 | $0.00",
            "---",
            "Consulting | -€100.00 | -€90.00",
            "Salary | -$400.00 | -$400.00",
            "Total: | -$550.00 | -$535.00",
            "---",
            "Expenses:Food | $150.00 | $120.00",
            "Total: | $150.00 | $120.00",
            "---",
            "Total: | $0.00 | $0.00",
            "---",
            "Trading Accounts | $0.00 | $0.00",
            "Expenses | $150.00 | $120.00",
            "Income | -$550.00 | -$535.00",
            "Equities | $0.00 | $0.00",
            "Liabilities | $0.00 | $0.00",
            "Assets | $75.00 | $415.00",
            "Total: | -$325.00 | $0.00",
        ]
    );
    let nearest = rows(&s.run(TEMPLATE, &json!({ "General": { "Price Source": "pricedb-nearest" } })));
    assert_eq!(nearest[2], "Total: | $65.00 | $397.00");
    assert_eq!(nearest[10], "Total: | -$530.00 | -$517.00");
    assert_eq!(nearest.last().unwrap(), "Total: | -$315.00 | $0.00");
}

/// Only the selected accounts get rows, and a selected parent none (BF:124-127): with Expenses and Food chosen, Food
/// alone shows; every kind still has its total line.
#[test]
fn selected_accounts() {
    let mut s = plan_book();
    let shown = rows(&s.run(TEMPLATE, &json!({ "Accounts": { "Account": ["Expenses", "Expenses:Food"] } })));
    assert_eq!(shown[..3], ["Total: | $0.00 | $0.00", "---", "Total: | $0.00 | $0.00"]);
    assert_eq!(shown.iter().filter(|r| r.contains("Food")).collect::<Vec<_>>(), ["Expenses:Food | $150.00 | $120.00"]);
    assert_eq!(shown.last().unwrap(), "Total: | $150.00 | $120.00");
}

/// With no account selected, then with no budget in the book, the report only warns (BF:270-281), naming the report
/// by its template name; accounts are checked first, so a book with neither warns of the accounts. A period that is
/// not whole fails, as `(1- period)` reaches the C calls as a fraction in GnuCash (BF:261-262, :131-136).
#[test]
fn warnings_and_a_fractional_period() {
    let mut s = Setup::new_book(&test_settings("iso"));
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], TEMPLATE);
    assert_eq!(blocks(&result)[0]["title"], "No accounts selected");

    let mut s = fixture();
    let err = s.d.call("report.run", json!({ "report": TEMPLATE, "options": period(json!("1.5")) })).unwrap_err();
    assert_eq!(err.code, red_ale_rpc::codes::REPORT_SCRIPT);

    let mut s = Setup::fixture("test-data.gnucash", &test_settings("iso"));
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], TEMPLATE);
    assert_eq!(blocks(&result)[0]["text"], "No budgets exist. You must create at least one budget.");
}
