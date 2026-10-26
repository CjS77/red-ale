//! The Income Statement and Profit & Loss (`standard/income-statement.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/income-statement.scm`, cited as IS) through `report.run` and
//! `report.export`: the figures of GnuCash's own test on `fixtures/reports/balsheet-pnl.gnucash`, run against both
//! templates, and hand-worked cases on books built through the API.
//!
//! In 1980 the fixture's Income earns $250 and Income-GBP #600 (#500 in GBP and #100 for $170); at the latest price
//! GBP is $1.70 (`standard/test/test-balsheet-pnl.scm:160-166`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use std::collections::BTreeSet;

use serde_json::{Value, json};
use statements::{
    Check, Setup, amount_of, assert_checks, assert_rows_at, blocks, expected, expected_run, labels, options_param, rates,
    rows_labelled, tables, test_settings,
};

const INCOME_STATEMENT: &str = "Income Statement";
const PROFIT_AND_LOSS: &str = "Profit & Loss";

/// A run of `balsheet-pnl.expected.json`: the figure checks, and where GnuCash's test reads their rows.
struct Run {
    id: &'static str,
    checks: Vec<Check<'static>>,
    rows: Vec<(&'static str, u32, &'static str)>,
}

fn check(name: &'static str, line: u32, label: &'static str, want: &'static [&'static str]) -> Check<'static> {
    Check { name, line, label, want }
}

/// Total Revenue under one price source: GnuCash's row 4 (Revenues, Income, Income-GBP, Total Revenue).
fn revenue_run(id: &'static str, name: &'static str, line: u32, want: &'static [&'static str]) -> Run {
    Run { id, checks: vec![check(name, line, "Total Revenue", want)], rows: vec![(name, 4, "Total Revenue")] }
}

const INCOME_ALL: &[&str] = &["-#600.00", "-$1,020.00", "-$250.00", "-$250.00"];

fn runs() -> Vec<Run> {
    vec![
        Run {
            id: "pnl-default",
            checks: vec![
                check("total revenue  = $1,270.00", 373, "Total Revenue", &["$1,270.00"]),
                check("total expenses  = $0.00", 377, "Total Expenses", &["$0.00"]),
            ],
            rows: vec![("total revenue", 4, "Total Revenue"), ("total expenses", 7, "Total Expenses")],
        },
        revenue_run("pnl-weighted-average", "weighted average revenue = $1160.36", 384, &["$1,160.36"]),
        revenue_run("pnl-average-cost", "average-cost revenue = $976", 391, &["$976.00"]),
        revenue_run("pnl-pricedb-nearest", "pricedb-nearest revenue = $1270", 398, &["$1,270.00"]),
        revenue_run("pnl-pricedb-latest", "pricedb-latest revenue = $1270", 405, &["$1,270.00"]),
        Run {
            id: "pnl-multilevel",
            checks: vec![
                check("multilevel. income = -$250.00", 415, "Income", &["-$250.00"]),
                check("multilevel. income-GBP = -#600", 419, "Income-GBP", &["-#600.00", "-$1,020.00"]),
                check("multilevel. total income = -$1,270.00", 423, "Total Income", &["-$1,270.00"]),
                check("multilevel. total revenue = $1,270.00", 427, "Total Revenue", &["$1,270.00"]),
                check("multilevel. expenses = $0.00", 431, "Total Expenses", &["$0.00"]),
                check("multilevel. net-income = $1,270", 435, "Net income for Period", &["$1,270.00"]),
            ],
            rows: vec![
                ("multilevel. income", 2, "Income"),
                ("multilevel. income-GBP", 3, "Income-GBP"),
                ("multilevel. total income", 4, "Total Income"),
                ("multilevel. total revenue", 5, "Total Revenue"),
                ("multilevel. expenses", 8, "Total Expenses"),
                ("multilevel. net-income", 10, "Net income for Period"),
            ],
        },
        Run {
            id: "pnl-recursive",
            checks: vec![
                check("recursive. income = $1020+250", 444, "Income", INCOME_ALL),
                check("recursive. income-gbp = $1020", 447, "Income-GBP", &["-#600.00", "-$1,020.00"]),
                check("recursive. total revenue = $1270", 450, "Total Revenue", &["$1,270.00"]),
            ],
            rows: vec![
                ("recursive. income", 2, "Income"),
                ("recursive. income-gbp", 3, "Income-GBP"),
                ("recursive. total revenue", 4, "Total Revenue"),
            ],
        },
        Run {
            id: "pnl-disable show-fcur show-rates",
            checks: vec![check("show-fcur disabled", 457, "Income", &["-$1,270.00"])],
            rows: vec![("show-fcur disabled", 2, "Income")],
        },
        Run {
            id: "pnl-enable show-fcur show-rates",
            checks: vec![check("show-fcur enabled", 467, "Income", INCOME_ALL)],
            rows: vec![("show-fcur enabled", 2, "Income")],
        },
    ]
}

/// The exchange-rates table checks: absent with "Show Exchange Rates" off, else #1.00 = $1.70.
fn rates_checks() -> Vec<(&'static str, &'static str, u32, Vec<&'static str>)> {
    vec![
        ("pnl-disable show-fcur show-rates", "show-rates disabled", 460, vec![]),
        ("pnl-enable show-fcur show-rates", "show-rates enabled", 470, vec!["#1.00", "$1.70"]),
    ]
}

fn fixture() -> Setup {
    Setup::fixture("balsheet-pnl.gnucash", &test_settings("iso"))
}

/// Every Income Statement check of `test-balsheet-pnl.scm:362-476` against `template`, through `report.run` and
/// `report.export`, each figure by its row label and each row where GnuCash's test reads it.
fn expected_figures(template: &str) {
    let mut s = fixture();
    let e = expected("balsheet-pnl.expected.json");
    runs().iter().for_each(|r| {
        let options = options_param(expected_run(&e, r.id));
        let result = s.run(template, &options);
        let html = s.export(template, &options);
        assert_checks(&result, &html, &r.checks);
        assert_rows_at(&result, &r.rows, 1);
    });
    rates_checks().iter().for_each(|(id, name, line, want)| {
        let options = options_param(expected_run(&e, id));
        let result = s.run(template, &options);
        assert_eq!(rates(&result), *want, "{name} (test line {line})");
        let html = s.export(template, &options);
        want.iter().for_each(|w| assert!(html.contains(w), "{name}: {w} is in the HTML"));
        assert_eq!(tables(&result).len(), if want.is_empty() { 1 } else { 2 }, "{name}");
    });
}

#[test]
fn expected_figures_of_gnucash_test() {
    expected_figures(INCOME_STATEMENT);
    let e = expected("balsheet-pnl.expected.json");
    let runs = runs();
    let ours: BTreeSet<&str> =
        runs.iter().flat_map(|r| r.checks.iter().map(|c| c.name)).chain(rates_checks().iter().map(|c| c.1)).collect();
    let theirs: BTreeSet<&str> = e["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["template"] == INCOME_STATEMENT)
        .flat_map(|r| r["checks"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()))
        .collect();
    assert_eq!(ours, theirs, "every Income Statement check of the expected figures is run");
}

/// GnuCash's "pnl" test runs the Income Statement's GUID (`standard/test/test-balsheet-pnl.scm:19`); Profit & Loss
/// is the same report under another name (IS:574-582), so the same checks hold for it, and only the title differs
/// (IS:369-374).
#[test]
fn profit_and_loss_is_the_income_statement_renamed() {
    expected_figures(PROFIT_AND_LOSS);
    let period = json!({ "General": { "Start Date": { "absolute": "1980-01-01" }, "End Date": { "absolute": "1981-01-01" } } });
    let mut s = fixture();
    let pnl = s.run(PROFIT_AND_LOSS, &period);
    assert_eq!(pnl["title"], " Profit & Loss For Period Covering 1980-01-01 to 1981-01-01");
    assert_eq!(pnl["report"]["guid"], "8758ba23984c40dea5527f5f0ca2779e");
    let is = s.run(INCOME_STATEMENT, &period);
    assert_eq!(is["title"], " Income Statement For Period Covering 1980-01-01 to 1981-01-01");
    assert_eq!(pnl["sections"], is["sections"]);
}

/// The default layout (IS:504-517): revenue, a rule, expenses, a rule, and the net income at the rightmost level;
/// the accounts are the income and expense ones (IS:149-152), three levels deep.
#[test]
fn default_layout() {
    let period = json!({ "General": { "Start Date": { "absolute": "1980-01-01" }, "End Date": { "absolute": "1981-01-01" } } });
    let result = fixture().run(INCOME_STATEMENT, &period);
    let table = tables(&result)[0];
    assert_eq!(labels(table), ["Revenues", "Income", "Income-GBP", "Total Revenue", "Expenses", "Total Expenses", "Net income for Period"]);
    let roles: Vec<&str> = table["rows"].as_array().unwrap().iter().map(|r| r["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["heading", "account", "account", "total", "rule", "heading", "total", "rule", "grandTotal"]);
    let (_, net) = rows_labelled(&result, "Net income for Period")[0];
    assert_eq!(net["level"], 0);
    assert_eq!(table["columns"][1]["levels"], 3);
    assert_eq!(amount_of(&result, "Income-GBP"), ["-#600.00", "-$1,020.00"], "the nearest price, $1.70");
}

fn layout(display: Value) -> Value {
    let general = json!({ "Start Date": { "absolute": "1980-01-01" }, "End Date": { "absolute": "1981-01-01" } });
    fixture().run(INCOME_STATEMENT, &json!({ "General": general, "Display": display }))
}

/// Two columns (IS:484-502): revenue and expense tables side by side in the standard order, the net income under
/// the expenses after a rule; with no trading account there is no trading table. Expense first (IS:519-531): the
/// sections swap, and in two columns the net income goes under the revenue.
#[test]
fn two_column_and_expense_first_layouts() {
    let two = layout(json!({ "Display as a two column report": true }));
    assert_eq!(two["sections"][0]["blocks"][0]["kind"], "group");
    let columns: Vec<Vec<String>> = tables(&two).iter().map(|t| labels(t)).collect();
    assert_eq!(columns, [
        vec!["Revenues", "Income", "Income-GBP", "Total Revenue"],
        vec!["Expenses", "Total Expenses", "Net income for Period"],
    ]);
    assert_eq!(amount_of(&two, "Net income for Period"), ["$1,270.00"]);

    let expense_first = layout(json!({ "Display in standard, income first, order": false }));
    assert_eq!(labels(tables(&expense_first)[0]), [
        "Expenses", "Total Expenses", "Revenues", "Income", "Income-GBP", "Total Revenue", "Net income for Period",
    ]);
    let both = layout(json!({ "Display as a two column report": true, "Display in standard, income first, order": false }));
    let columns: Vec<Vec<String>> = tables(&both).iter().map(|t| labels(t)).collect();
    assert_eq!(columns, [
        vec!["Expenses", "Total Expenses"],
        vec!["Revenues", "Income", "Income-GBP", "Total Revenue", "Net income for Period"],
    ]);
    let unlabelled = layout(json!({ "Label the revenue section": false, "Include revenue total": false,
                                    "Label the expense section": false, "Include expense total": false }));
    assert_eq!(labels(tables(&unlabelled)[0]), ["Income", "Income-GBP", "Net income for Period"]);
}

/// A book through the API, for March 2024: $500 of fees on the 1st, $120 of rent on the 5th, both closed into
/// Equity on the 20th by transactions described "Closing Entries", and $600 of repairs on the 25th.
fn closing_book() -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    s.account("Checking", "BANK", None, "CURRENCY::USD");
    s.account("Equity", "EQUITY", None, "CURRENCY::USD");
    s.account("Income", "INCOME", None, "CURRENCY::USD");
    s.account("Expenses", "EXPENSE", None, "CURRENCY::USD");
    s.transfer("2024-03-01", "Fees", "Income", "Checking", "500.00");
    s.transfer("2024-03-05", "Rent", "Checking", "Expenses", "120.00");
    s.transfer("2024-03-20", "Closing Entries", "Equity", "Income", "500.00");
    s.transfer("2024-03-20", "Closing Entries", "Expenses", "Equity", "120.00");
    s.transfer("2024-03-25", "Repairs", "Checking", "Expenses", "600.00");
    s
}

fn march(entries: Value) -> Value {
    json!({ "General": { "Start Date": { "absolute": "2024-03-01" }, "End Date": { "absolute": "2024-03-31" } }, "Entries": entries })
}

/// `(revenue, expenses, Income's row, Expenses' row, net line)` of a run.
fn figures(result: &Value) -> (String, String, String, String, String) {
    let one = |label: &str| amount_of(result, label).join(" ");
    let net = ["Net income for Period", "Net loss for Period"].iter().find(|l| !rows_labelled(result, l).is_empty()).unwrap();
    (one("Total Revenue"), one("Total Expenses"), one("Income"), one("Expenses"), format!("{net} {}", one(net)))
}

/// The closing-entries pattern (IS:232-240, :324-328) picks out the closing transactions, which the totals leave out
/// (IS:386-398) and the account rows take back off ("pre-closing", HAT:612-651). When it matches, revenue is the
/// closing entry less the period's change: $500 - ($-500 + $500) = $500; expenses are the change less the closing
/// entry: ($120 - $120 + $600) - (-$120) = $720; the net is a loss of $220 (IS:445-457). When it does not, the
/// closing entries count as ordinary ones: revenue $0, expenses $600, a loss of $600. The default pattern,
/// "Closing Entries", matches without regard to case; case-sensitive, "closing entries" does not; as a regular
/// expression `^Clos.*ries$` does, and as plain text it does not.
#[test]
fn closing_entries_pattern() {
    let mut s = closing_book();
    let matched = want(["$500.00", "$720.00", "-$500.00", "$720.00", "Net loss for Period $220.00"]);
    let unmatched = want(["$0.00", "$600.00", "$0.00", "$600.00", "Net loss for Period $600.00"]);
    let cases = [
        (json!({}), &matched),
        (json!({ "Closing Entries pattern": "closing entries" }), &matched),
        (json!({ "Closing Entries pattern": "closing entries", "Closing Entries pattern is case-sensitive": true }), &unmatched),
        (json!({ "Closing Entries pattern": "^Clos.*ries$", "Closing Entries Pattern is regular expression": true }), &matched),
        (json!({ "Closing Entries pattern": "^Clos.*ries$" }), &unmatched),
        (json!({ "Closing Entries pattern": "Nothing" }), &unmatched),
    ];
    cases.iter().for_each(|(entries, wanted)| {
        assert_eq!(&figures(&s.run(INCOME_STATEMENT, &march(entries.clone()))), *wanted, "{entries}");
    });
}

fn want([revenue, expenses, income, expense, net]: [&str; 5]) -> (String, String, String, String, String) {
    (revenue.to_owned(), expenses.to_owned(), income.to_owned(), expense.to_owned(), net.to_owned())
}

/// Transactions that close the books carry the `book_closing` slot, which the pattern matches whatever their
/// description (the `closing` key of IS:328, RU:707-714): the same figures as a matching pattern.
#[test]
fn closing_transactions_match_whatever_their_description() {
    let mut s = Setup::new_book(&test_settings("iso"));
    s.account("Checking", "BANK", None, "CURRENCY::USD");
    s.account("Equity", "EQUITY", None, "CURRENCY::USD");
    s.account("Income", "INCOME", None, "CURRENCY::USD");
    s.account("Expenses", "EXPENSE", None, "CURRENCY::USD");
    s.transfer("2024-03-01", "Fees", "Income", "Checking", "500.00");
    s.transfer("2024-03-05", "Rent", "Checking", "Expenses", "120.00");
    let close = json!({ "date": "2024-03-20", "incomeTotalAccount": "Equity", "expenseTotalAccount": "Equity", "description": "Year end" });
    s.call("book.closeBooks", close);
    s.transfer("2024-03-25", "Repairs", "Checking", "Expenses", "600.00");
    let result = s.run(INCOME_STATEMENT, &march(json!({ "Closing Entries pattern": "Nothing" })));
    assert_eq!(figures(&result), want(["$500.00", "$720.00", "-$500.00", "$720.00", "Net loss for Period $220.00"]));
}

/// A trading book: $50 of fees on 2024-03-02 and €100 bought for $110 on 2024-03-05, EUR at $1.20 on 2024-03-14;
/// Income is top-level, so the trading accounts' parents are in USD (`5.17:libgnucash/engine/Scrub.cpp:493-516`).
fn trading_book() -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    s.call("book.setOptions", json!({ "useTradingAccounts": true }));
    s.account("Checking", "BANK", None, "CURRENCY::USD");
    s.account("Euro", "BANK", None, "CURRENCY::EUR");
    s.account("Income", "INCOME", None, "CURRENCY::USD");
    s.transfer("2024-03-02", "Fees", "Income", "Checking", "50.00");
    let splits = json!([{ "account": "Euro", "value": "110.00", "amount": "100.00" }, { "account": "Checking", "value": "-110.00" }]);
    s.call("transaction.post", json!({ "datePosted": "2024-03-05", "currency": "CURRENCY::USD", "splits": splits }));
    s.call("price.add", json!({ "commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "value": "1.20", "date": "2024-03-14" }));
    s
}

fn with_trading(display: Value) -> Value {
    let period = json!({ "Start Date": { "absolute": "2024-03-01" }, "End Date": { "absolute": "2024-03-31" } });
    let accounts = json!(["Income", "Trading:CURRENCY:EUR", "Trading:CURRENCY:USD"]);
    let options = json!({ "General": period, "Accounts": { "Accounts": accounts }, "Display": display });
    trading_book().run(INCOME_STATEMENT, &options)
}

/// The trading section (IS:88-93, :400-402, :475-481), when trading accounts are selected: the trading accounts hold
/// -€100 and +$110 for the period, -$120 + $110 at $1.20, "Total Trading" -$10.00 (no "loss" label, IS:480); the net
/// income is the revenue plus trading less expenses, $50 - $10 = $40 (IS:404-407). Its accounts sit at the first
/// level, their parents not being selected. Two columns put it between revenue and expenses (IS:500-501).
#[test]
fn trading_section() {
    let result = with_trading(json!({}));
    assert_eq!(labels(tables(&result)[0]), [
        "Revenues", "Income", "Total Revenue", "Trading", "EUR", "USD", "Total Trading", "Expenses", "Total Expenses",
        "Net income for Period",
    ]);
    assert_eq!(amount_of(&result, "EUR"), ["-€100.00", "-$120.00"]);
    assert_eq!(amount_of(&result, "USD"), ["$110.00"]);
    assert_eq!(amount_of(&result, "Total Trading"), ["-$10.00"]);
    assert_eq!(amount_of(&result, "Net income for Period"), ["$40.00"]);
    assert_eq!(rows_labelled(&result, "EUR")[0].1["depth"], 0);
    let two = with_trading(json!({ "Display as a two column report": true }));
    let firsts: Vec<String> = tables(&two).iter().map(|t| labels(t)[0].clone()).collect();
    assert_eq!(firsts, ["Revenues", "Trading", "Expenses"]);
    let quiet = with_trading(json!({ "Label the trading accounts section": false, "Include trading accounts total": false }));
    assert!(rows_labelled(&quiet, "Trading").is_empty() && rows_labelled(&quiet, "Total Trading").is_empty());
    assert_eq!(amount_of(&quiet, "Net income for Period"), ["$40.00"]);
}

/// The no-account warning on a book with no income or expense account (IS:376-383).
#[test]
fn empty_book() {
    let result = Setup::new_book(&test_settings("iso")).run(INCOME_STATEMENT, &json!({}));
    assert_eq!(blocks(&result)[0]["title"], "No accounts selected");
}

/// Running either template leaves the Book as it was and writes nothing; two exports are the same bytes, with no
/// script.
#[test]
fn read_only_and_deterministic() {
    let mut s = Setup::with_config(&test_settings("iso"));
    std::fs::copy(format!("{}/../../fixtures/reports/balsheet-pnl.gnucash", env!("CARGO_MANIFEST_DIR")), s.book()).unwrap();
    let book = s.book().display().to_string();
    s.call("book.open", json!({ "path": book }));
    let before = s.d.context().book().unwrap().clone();
    let files = s.files();
    let options = json!({ "Commodities": { "Show Exchange Rates": true }, "Display": { "Display as a two column report": true } });
    [INCOME_STATEMENT, PROFIT_AND_LOSS].iter().for_each(|t| {
        s.run(t, &options);
        let (a, b) = (s.export(t, &options), s.export(t, &options));
        assert_eq!(a, b);
        assert!(!a.to_ascii_lowercase().contains("<script"));
    });
    assert_eq!(s.d.context().book().unwrap(), &before);
    assert_eq!(s.call("book.info", json!({}))["dirty"], false);
    assert_eq!(s.files(), files, "no journal or other file was written");
}
