//! The Budget Balance Sheet (`standard/budget-balance-sheet.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/budget-balance-sheet.scm`, cited as BBS) through `report.run` and
//! `report.export`: every Budget Balance Sheet assertion of 5.17's `test-budget.scm` (TB) on
//! `fixtures/reports/budget.gnucash` (`budget.expected.json`), then hand-worked cases on a book built through the API.
//!
//! The sheet is dated at the budget's start (BBS:272): each account shows its balance strictly before then plus its
//! own Budget amounts over every period (BBS:226-262), and the totals are collector arithmetic on those
//! (BBS:478-576).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod report_runs;
mod statements;

use serde_json::{Value, json};
use statements::{
    Setup, amount_of, blocks, expected, expected_run, gnucash_text_count, labels, options_param, row_texts, tables, test_settings,
};

const TEMPLATE: &str = "Budget Balance Sheet";

// Expected figures from 5.17 (TB:186-253).

/// Every check of TB:186-253 with the default options: the text count of the page's one table, and each line
/// GnuCash's test reads by its row, which here is that row less two (GnuCash counts from 1, and its table starts
/// with a row of spacer cells, BBS:613-616). Every figure is also in the exported page.
#[test]
fn expected_figures_of_gnucash_test() {
    let mut s = Setup::fixture("budget.gnucash", &test_settings("locale"));
    let e = expected("budget.expected.json");
    let run = expected_run(&e, "budget-bs-basic");
    let options = options_param(run);
    let result = s.run(TEMPLATE, &options);
    let html = s.export(TEMPLATE, &options);
    let ts = tables(&result);
    assert_eq!(ts.len(), 1, "the single-column form");
    let rows = ts[0]["rows"].as_array().unwrap();
    run["checks"].as_array().unwrap().iter().for_each(|c| {
        let (name, line) = (c["name"].as_str().unwrap(), &c["line"]);
        if c["select"]["count"] == true {
            assert_eq!(json!(gnucash_text_count(&ts, 1)), c["expected"], "{name} (test line {line})");
            return;
        }
        let row = usize::try_from(c["select"]["row"].as_u64().unwrap() - 2).unwrap();
        let got = row_texts(ts[0], &rows[row]);
        assert_eq!(json!(got), c["expected"], "{name} (test line {line})");
        assert!(html.contains(&got[1]), "{name}: {} is in the HTML", got[1]);
    });
}

// Hand-worked cases.

/// The options of `budget-balance-sheet-options-generator` (BBS:112-212), in GnuCash's order, with the General
/// options every template gets. With no book open there is no budget to default to, and no account.
#[test]
fn options_are_gnucash_s() {
    let want = [
        r#"Accounts/Accounts "a" "accountList" []"#,
        r#"Accounts/Levels of Subaccounts "b" "multichoice" 3"#,
        r#"Accounts/Flatten list to depth limit "c" "boolean" false"#,
        r#"Commodities/Report's currency "a" "currency" "CURRENCY::USD""#,
        r#"Commodities/Price Source "b" "multichoice" "pricedb-nearest""#,
        r#"Commodities/Show Foreign Currencies "c" "boolean" true"#,
        r#"Commodities/Show Exchange Rates "d" "boolean" false"#,
        r#"Display/Include accounts with zero total balances "a" "boolean" true"#,
        r#"Display/Omit zero balance figures "b" "boolean" false"#,
        r#"Display/Parent account balances "ca" "multichoice" "immediate-bal""#,
        r#"Display/Parent account subtotals "cb" "multichoice" "f""#,
        r#"Display/Display accounts as hyperlinks "d" "boolean" true"#,
        r#"Display/Show accounting-style rules "e" "boolean" false"#,
        r#"Display/Label the assets section "f" "boolean" true"#,
        r#"Display/Include assets total "g" "boolean" true"#,
        r#"Display/Label the liabilities section "h" "boolean" true"#,
        r#"Display/Include liabilities total "i" "boolean" true"#,
        r#"Display/Label the equity section "j" "boolean" true"#,
        r#"Display/Include equity total "k" "boolean" true"#,
        r#"Display/Include new/existing totals "l" "boolean" true"#,
        r#"General/Report name "0a" "string" "Budget Balance Sheet""#,
        r#"General/Stylesheet "0b" "multichoice" "Default""#,
        r#"General/Add options summary "0c" "multichoice" "never""#,
        r#"General/Report Title "a" "string" "Budget Balance Sheet""#,
        r#"General/Single column Balance Sheet "c" "boolean" true"#,
        r#"General/Budget "d" "budget" null"#,
    ];
    assert_eq!(report_runs::described_options(TEMPLATE), want);
}

/// With the fixture open, the budget defaults to its only one (`gnc-budget-get-default`, BBS:123-125) and the
/// accounts to every account of the thirteen types (BBS:128-138), which is all ten.
#[test]
fn defaults_come_from_the_book() {
    let mut s = Setup::fixture("budget.gnucash", &test_settings("locale"));
    let described = s.call("report.options", json!({ "report": TEMPLATE }));
    let options = described["options"].as_array().unwrap();
    let option = |name: &str| options.iter().find(|o| o["name"] == name).unwrap()["default"].clone();
    assert_eq!(option("Budget"), json!("f0000000000000000000000000000000"));
    assert_eq!(option("Accounts").as_array().unwrap().len(), 10);
}

/// A book of six accounts and a budget "Plan" of three monthly periods from 2024-01-01 (`budget.create`):
///
/// - 2023-12-01 Opening to Checking $1,000; 2023-12-05 Checking to Euro $110 for €100; 2023-12-10 Card to Food
///   $50; 2023-12-20 Salary to Checking $300; 2024-01-10, after the budget's start, Salary to Checking $200;
/// - €1 = $1.20 on 2023-12-31;
/// - Budget amounts: Checking 100 and 50 in periods 0 and 1; Card -20 in period 0; Salary -400 in periods 0 and 1;
///   Food 150 in period 0 and 100 in period 2.
fn plan_book(settings: &Value) -> Setup {
    let mut s = Setup::new_book(settings);
    s.account("Checking", "BANK", None, "CURRENCY::USD");
    s.account("Euro", "BANK", None, "CURRENCY::EUR");
    s.account("Card", "CREDIT", None, "CURRENCY::USD");
    s.account("Opening", "EQUITY", None, "CURRENCY::USD");
    s.account("Salary", "INCOME", None, "CURRENCY::USD");
    s.account("Food", "EXPENSE", None, "CURRENCY::USD");
    s.transfer("2023-12-01", "Opening balance", "Opening", "Checking", "1000.00");
    let splits = json!([{ "account": "Euro", "value": "110.00", "amount": "100.00" }, { "account": "Checking", "value": "-110.00" }]);
    let euros = json!({ "datePosted": "2023-12-05", "currency": "CURRENCY::USD", "description": "Euros", "splits": splits });
    s.call("transaction.post", euros);
    s.transfer("2023-12-10", "Groceries", "Card", "Food", "50.00");
    s.transfer("2023-12-20", "December pay", "Salary", "Checking", "300.00");
    s.transfer("2024-01-10", "January pay", "Salary", "Checking", "200.00");
    s.call("price.add", json!({ "commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "value": "1.20", "date": "2023-12-31" }));
    let recurrence = json!({ "mult": 1, "periodType": "month", "start": "2024-01-01" });
    let budget = s.call("budget.create", json!({ "name": "Plan", "numPeriods": 3, "recurrence": recurrence }))["guid"].clone();
    let cells = [("Checking", 0, "100"), ("Checking", 1, "50"), ("Card", 0, "-20"), ("Salary", 0, "-400"), ("Salary", 1, "-400"),
                 ("Food", 0, "150"), ("Food", 2, "100")];
    let amounts: Vec<Value> = cells.iter().map(|(a, p, v)| json!({ "account": a, "period": p, "amount": v })).collect();
    s.call("budget.setAmounts", json!({ "budget": budget, "amounts": amounts }));
    s
}

fn figures(result: &Value, rows: &[&str]) -> Vec<(String, Vec<String>)> {
    rows.iter().map(|l| ((*l).to_owned(), amount_of(result, l))).collect()
}

fn want(rows: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    rows.iter().map(|(l, a)| ((*l).to_owned(), a.iter().map(|s| (*s).to_owned()).collect())).collect()
}

/// The plan book with the defaults, worked from BBS:423-576 at the budget's start, 2024-01-01:
///
/// - balances then: Checking $1,190, Euro €100, Card -$50, Opening -$1,000, Salary -$300, Food $50; January's pay is
///   after the start and counts nowhere;
/// - each account's row is that plus its Budget amounts: Checking $1,340, Card -$70 (BBS:257-262);
/// - existing assets $1,190 + €100 at $1.20 = $1,310; allocated $150 (BBS:515-523);
/// - existing retained earnings -(-$300 + $50) = $250, new ones -(-$800 + $250) = $550, Food's period 2 included
///   (BBS:492-511);
/// - unallocated $550 - ($150 - $20) = $420, so total assets $1,880 (BBS:525-535);
/// - the unrealized gain is the assets less what they cost: ($1,190 + €100) - $1,300 = €100 - $110, $10 at $1.20; the
///   card cost what it is worth (BBS:539-553);
/// - existing equity $1,000 + $250 + $10 = $1,260, new $550, total $1,810; with the liabilities' $70, $1,880
///   (BBS:557-576).
#[test]
fn the_sheet_at_the_budget_start() {
    let mut s = plan_book(&test_settings("iso"));
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], " Budget Balance Sheet Plan");
    let table = tables(&result)[0];
    assert_eq!(
        labels(table),
        [
            "Assets", "Checking", "Euro", "Existing Assets", "Allocated Assets", "Unallocated Assets", "Total Assets", "Liabilities",
            "Card", "Existing Liabilities", "New Liabilities", "Total Liabilities", "Equity", "Opening", "Existing Retained Earnings",
            "New Retained Earnings", "Unrealized Gains", "Existing Equity", "New Equity", "Total Equity", "Total Liabilities & Equity",
        ]
    );
    let rows = [
        "Checking", "Euro", "Existing Assets", "Allocated Assets", "Unallocated Assets", "Total Assets", "Card", "Existing Liabilities",
        "New Liabilities", "Total Liabilities", "Opening", "Existing Retained Earnings", "New Retained Earnings", "Unrealized Gains",
        "Existing Equity", "New Equity", "Total Equity", "Total Liabilities & Equity",
    ];
    assert_eq!(
        figures(&result, &rows),
        want(&[
            ("Checking", &["$1,340.00"]),
            ("Euro", &["€100.00", "$120.00"]),
            ("Existing Assets", &["$1,310.00"]),
            ("Allocated Assets", &["$150.00"]),
            ("Unallocated Assets", &["$420.00"]),
            ("Total Assets", &["$1,880.00"]),
            ("Card", &["-$70.00"]),
            ("Existing Liabilities", &["-$50.00"]),
            ("New Liabilities", &["-$20.00"]),
            ("Total Liabilities", &["-$70.00"]),
            ("Opening", &["-$1,000.00"]),
            ("Existing Retained Earnings", &["$250.00"]),
            ("New Retained Earnings", &["$550.00"]),
            ("Unrealized Gains", &["$10.00"]),
            ("Existing Equity", &["$1,260.00"]),
            ("New Equity", &["$550.00"]),
            ("Total Equity", &["$1,810.00"]),
            ("Total Liabilities & Equity", &["$1,880.00"]),
        ])
    );
    let rows = table["rows"].as_array().unwrap();
    let rules: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r["role"] == "rule").map(|(i, _)| i).collect();
    // Two rules after the assets, one after the liabilities and one after the equity (BBS:647-650, :680, :743).
    assert_eq!(rules, [7, 8, 14, 23]);
}

/// The budget's start dates the sheet (BBS:272): moved to 2024-02-01, January's $200 pay is a balance before it, so
/// Checking's row is $1,540, existing assets $1,510 and existing retained earnings -(-$500 + $50) = $450. The amounts
/// stay keyed by period, and the €1 price of $1.20 is still the nearest (BBS:337-340).
#[test]
fn the_budget_start_dates_the_sheet() {
    let mut s = plan_book(&test_settings("iso"));
    let budget = s.call("budget.list", json!({}))["budgets"][0]["guid"].clone();
    let recurrence = json!({ "mult": 1, "periodType": "month", "start": "2024-02-01" });
    s.call("budget.update", json!({ "budget": budget, "recurrence": recurrence }));
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(
        figures(&result, &["Checking", "Existing Assets", "Existing Retained Earnings", "Total Assets", "Total Liabilities & Equity"]),
        want(&[
            ("Checking", &["$1,540.00"]),
            ("Existing Assets", &["$1,510.00"]),
            ("Existing Retained Earnings", &["$450.00"]),
            ("Total Assets", &["$2,080.00"]),
            ("Total Liabilities & Equity", &["$2,080.00"]),
        ])
    );
}

/// Retained earnings come from the selected income and expense accounts only (BBS:492-511). Without Salary they are
/// losses: existing -$50, new -$250, each shown negated under its "Losses" label (BBS:342-353, :701-715), and the
/// unallocated assets -$250 - $130 = -$380, which has no such label. Without "Include new/existing totals" only their
/// total is shown, -$300 as "Total Retained Losses" $300, and none of the existing and new lines (BBS:633-638,
/// :669-675, :702-720, :732-738).
#[test]
fn retained_losses_and_totals_alone() {
    let mut s = plan_book(&test_settings("iso"));
    let accounts = json!(["Checking", "Euro", "Card", "Opening", "Food"]);
    let result = s.run(TEMPLATE, &json!({ "Accounts": { "Accounts": accounts } }));
    assert_eq!(
        figures(&result, &["Existing Retained Losses", "New Retained Losses", "Unallocated Assets", "Total Assets"]),
        want(&[
            ("Existing Retained Losses", &["$50.00"]),
            ("New Retained Losses", &["$250.00"]),
            ("Unallocated Assets", &["-$380.00"]),
            ("Total Assets", &["$1,080.00"]),
        ])
    );
    let result = s.run(TEMPLATE, &json!({ "Accounts": { "Accounts": accounts }, "Display": { "Include new/existing totals": false } }));
    assert_eq!(
        labels(tables(&result)[0]),
        [
            "Assets", "Checking", "Euro", "Unallocated Assets", "Total Assets", "Liabilities", "Card", "Total Liabilities", "Equity",
            "Opening", "Total Retained Losses", "Unrealized Gains", "Total Equity", "Total Liabilities & Equity",
        ]
    );
    assert_eq!(amount_of(&result, "Total Retained Losses"), ["$300.00"]);
}

/// Zero retained earnings and a zero unrealized gain are left out, "since they are not present on normal balance
/// sheets" (BBS:699-727): with only Checking and Opening there is no income or expense and nothing foreign. The
/// unallocated assets are then the allocated ones negated, -$150 (BBS:525-531), and the liabilities total $0.
#[test]
fn zero_retained_earnings_and_gains_are_left_out() {
    let mut s = plan_book(&test_settings("iso"));
    let result = s.run(TEMPLATE, &json!({ "Accounts": { "Accounts": ["Checking", "Opening"] } }));
    assert_eq!(
        labels(tables(&result)[0]),
        [
            "Assets", "Checking", "Existing Assets", "Allocated Assets", "Unallocated Assets", "Total Assets", "Liabilities",
            "Existing Liabilities", "New Liabilities", "Total Liabilities", "Equity", "Opening", "Existing Equity", "New Equity",
            "Total Equity", "Total Liabilities & Equity",
        ]
    );
    assert_eq!(
        figures(&result, &["Unallocated Assets", "Total Assets", "Total Liabilities", "Total Liabilities & Equity"]),
        want(&[
            ("Unallocated Assets", &["-$150.00"]),
            ("Total Assets", &["$1,190.00"]),
            ("Total Liabilities", &["$0.00"]),
            ("Total Liabilities & Equity", &["$1,000.00"]),
        ])
    );
}

/// Under `credit` the account table shows Card and Opening negated (HAT:1029-1030), and the liability lines follow
/// the first liability account, Card, which is reversed (`make-sign-handler`, BBS:250-254, :672-678); the equity
/// lines have no sign handler and keep their signs.
#[test]
fn liability_lines_follow_the_first_liability_account() {
    let mut settings = test_settings("iso");
    settings["reports"]["reverseBalancedAccounts"] = json!("credit");
    let mut s = plan_book(&settings);
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(
        figures(&result, &["Card", "Existing Liabilities", "New Liabilities", "Total Liabilities", "Opening", "Total Equity"]),
        want(&[
            ("Card", &["$70.00"]),
            ("Existing Liabilities", &["$50.00"]),
            ("New Liabilities", &["$20.00"]),
            ("Total Liabilities", &["$70.00"]),
            ("Opening", &["$1,000.00"]),
            ("Total Equity", &["$1,810.00"]),
        ])
    );
}

/// The two-column form (BBS:413-414, :751-771): the assets in a table of their own, without the two rules, and the
/// liabilities and equity beside them; the exchange rates follow when asked (BBS:775-778).
#[test]
fn two_column_form_and_rates() {
    let mut s = plan_book(&test_settings("iso"));
    let options = json!({ "General": { "Single column Balance Sheet": false }, "Commodities": { "Show Exchange Rates": true } });
    let result = s.run(TEMPLATE, &options);
    let top = &result["sections"][0]["blocks"];
    assert_eq!(top[0]["kind"], "group");
    let ts = tables(&result);
    assert_eq!(labels(ts[0]).last().unwrap(), "Total Assets");
    assert_eq!(labels(ts[1])[0], "Liabilities");
    assert_eq!(labels(ts[1]).last().unwrap(), "Total Liabilities & Equity");
    assert_eq!(statements::rates(&result), ["€1.00", "$1.20"]);
}

/// With no budget in the book the report warns (BBS:368-371); with no account selected it says so first
/// (BBS:362-367). Both name the report, and so does the title.
#[test]
fn warnings() {
    let mut s = Setup::fixture("test-data.gnucash", &test_settings("locale"));
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], TEMPLATE);
    assert_eq!(blocks(&result)[0]["text"], "No budgets exist. You must create at least one budget.");
    assert_eq!(blocks(&result)[0]["title"], "Budget Balance Sheet:");
    let mut s = Setup::new_book(&test_settings("locale"));
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(result["title"], TEMPLATE);
    assert_eq!(blocks(&result)[0]["title"], "No accounts selected");
}
