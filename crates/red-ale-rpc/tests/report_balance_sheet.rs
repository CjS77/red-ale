//! The Balance Sheet (`standard/balance-sheet.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/balance-sheet.scm`, cited as BS) through `report.run` and `report.export`:
//! the figures of GnuCash's own test on `fixtures/reports/balsheet-pnl.gnucash`, hand-worked cases on books built
//! through the API, and the property that a balance sheet balances.
//!
//! The fixture (`standard/test/test-balsheet-pnl.scm:51-166`): Root (ASSET) holds Asset (Bank1 with Savings $100,
//! Bonds $2,000, Empty and Current $2,609; House $100,000; ForeignBank with ForeignSavings #200; Broker $2,000 with
//! Funds, 30 FUNDS), Liability (Bank2 with Loan -$9,000 and CreditCard -$500), Equity and Income (with Income-GBP).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use std::collections::BTreeSet;

use proptest::prelude::*;
use serde_json::{Value, json};
use statements::{
    Check, Setup, amount_of, assert_checks, assert_rows_at, blocks, expected, expected_run, labels, options_param, rates,
    rows_labelled, tables, test_settings,
};

const TEMPLATE: &str = "Balance Sheet";

/// A named check of a run of `balsheet-pnl.expected.json`: the figure checks, and where GnuCash's test reads the rows.
struct Run {
    id: &'static str,
    checks: Vec<Check<'static>>,
    rows: Vec<(&'static str, u32, &'static str)>,
}

fn check(name: &'static str, line: u32, label: &'static str, want: &'static [&'static str]) -> Check<'static> {
    Check { name, line, label, want }
}

/// GnuCash reads cells by HTML position (`standard/test/test-balsheet-pnl.scm`), and a figure in a nested commodity
/// table is read twice when the outer and inner cell numbers coincide: the multilevel and recursive "funds" and
/// "foreignsavings" checks at column 3 end with the converted figure again. Here those cells are read once, as
/// the `commodities` lines they are.
fn multilevel_run() -> Run {
    let checks = vec![
        check("multilevel. root = $0.00", 219, "Root", &["$0.00"]),
        check("multilevel. assets = $0.00", 222, "Asset", &["$0.00"]),
        check("multilevel. bank1 = $0.00", 225, "Bank1", &["$0.00"]),
        check("multilevel. bonds = $2,000.00", 228, "Bonds", &["$2,000.00"]),
        check("multilevel. current = $2609.00", 231, "Current", &["$2,609.00"]),
        check("multilevel. empty = $0.00", 234, "Empty", &["$0.00"]),
        check("multilevel. savings = $100.00", 237, "Savings", &["$100.00"]),
        check("multilevel. total bank1 = $4709", 240, "Total Bank1", &["$4,709.00"]),
        check("multilevel. broker = $2,000.00", 243, "Broker", &["$2,000.00"]),
        check("multilevel. funds = $14,424.52", 246, "Funds", &["30. FUNDS", "$14,424.52"]),
        check("multilevel. total broker = $16,424.52", 249, "Total Broker", &["$16,424.52"]),
        check("multilevel. foreign = $0.00", 252, "ForeignBank", &["$0.00"]),
        check("multilevel. foreignsavings = #200.00 = $340", 255, "ForeignSavings", &["#200.00", "$340.00"]),
        check("multilevel. total foreign = $340", 258, "Total ForeignBank", &["$340.00"]),
        check("multilevel. house = $100,000", 261, "House", &["$100,000.00"]),
        check("multilevel. total asset = $121,473.52", 264, "Total Asset", &["$121,473.52"]),
        check("multilevel. total root = $121,473.52", 267, "Total Root", &["$121,473.52"]),
        check("multilevel. total assets = $121,473.52", 270, "Total Assets", &["$121,473.52"]),
    ];
    let rows = vec![
        ("multilevel. root", 3, "Root"),
        ("multilevel. assets", 4, "Asset"),
        ("multilevel. bank1", 5, "Bank1"),
        ("multilevel. bonds", 6, "Bonds"),
        ("multilevel. current", 7, "Current"),
        ("multilevel. empty", 8, "Empty"),
        ("multilevel. savings", 9, "Savings"),
        ("multilevel. total bank1", 10, "Total Bank1"),
        ("multilevel. broker", 11, "Broker"),
        ("multilevel. funds", 12, "Funds"),
        ("multilevel. total broker", 13, "Total Broker"),
        ("multilevel. foreign", 14, "ForeignBank"),
        ("multilevel. foreignsavings", 15, "ForeignSavings"),
        ("multilevel. total foreign", 16, "Total ForeignBank"),
        ("multilevel. house", 17, "House"),
        ("multilevel. total asset", 18, "Total Asset"),
        ("multilevel. total root", 19, "Total Root"),
        ("multilevel. total assets", 20, "Total Assets"),
    ];
    Run { id: "balsheet-multilevel", checks, rows }
}

/// Root with its sub-accounts at the latest prices: #200.00 ($340.00), 30 FUNDS ($14,424.52) and $106,709.00.
const ROOT_ALL: &[&str] = &["#200.00", "$340.00", "30. FUNDS", "$14,424.52", "$106,709.00", "$106,709.00"];

fn recursive_run() -> Run {
    let checks = vec![
        check("recursive. root = $760+14424.52+106709", 278, "Root", ROOT_ALL),
        check("recursive. assets = $760+14424.52+106709", 281, "Asset", ROOT_ALL),
        check("recursive. bank1 = $4,709.00", 284, "Bank1", &["$4,709.00"]),
        check("recursive. bonds = $2,000.00", 287, "Bonds", &["$2,000.00"]),
        check("recursive. current = $2609.00", 290, "Current", &["$2,609.00"]),
        check("recursive. empty = $0.00", 293, "Empty", &["$0.00"]),
        check("recursive. savings = $100.00", 296, "Savings", &["$100.00"]),
        check("recursive. broker = $14424.52+$2000.00", 299, "Broker", &["30. FUNDS", "$14,424.52", "$2,000.00", "$2,000.00"]),
        check("recursive. funds = $14424.52", 302, "Funds", &["30. FUNDS", "$14,424.52"]),
        check("recursive. foreign = $340.00", 305, "ForeignBank", &["#200.00", "$340.00"]),
        check("recursive. foreignsavings = #200.00 = $340", 308, "ForeignSavings", &["#200.00", "$340.00"]),
        check("recursive. house = $100,000", 311, "House", &["$100,000.00"]),
        check("recursive. total assets = $121,473.52", 314, "Total Assets", &["$121,473.52"]),
    ];
    let rows = vec![
        ("recursive. root", 3, "Root"),
        ("recursive. assets", 4, "Asset"),
        ("recursive. bank1", 5, "Bank1"),
        ("recursive. bonds", 6, "Bonds"),
        ("recursive. current", 7, "Current"),
        ("recursive. empty", 8, "Empty"),
        ("recursive. savings", 9, "Savings"),
        ("recursive. broker", 10, "Broker"),
        ("recursive. funds", 11, "Funds"),
        ("recursive. foreign", 12, "ForeignBank"),
        ("recursive. foreignsavings", 13, "ForeignSavings"),
        ("recursive. house", 14, "House"),
        ("recursive. total assets", 15, "Total Assets"),
    ];
    Run { id: "balsheet-recursive", checks, rows }
}

fn runs() -> Vec<Run> {
    let total_assets = |name, line, want| (vec![check(name, line, "Total Assets", want)], vec![(name, 15, "Total Assets")]);
    let (wa, wa_rows) = total_assets("weighted average assets = $114,071.66", 192, &["$114,071.66"]);
    let (ac, ac_rows) = total_assets("average-cost assets = $113,100", 198, &["$113,100.00"]);
    let (pn, pn_rows) = total_assets("pricedb-nearest assets = $116,006.33", 204, &["$116,006.33"]);
    let (latest, latest_rows) = total_assets("pricedb-latest assets = $121,473.52", 210, &["$121,473.52"]);
    vec![
        Run {
            id: "balsheet-default",
            checks: vec![
                check("total assets = $116,006.33", 180, "Total Assets", &["$116,006.33"]),
                check("total liabilities = $9,500.00", 183, "Total Liabilities", &["$9,500.00"]),
                check("total equity  = $106,506.33", 186, "Total Equity", &["$106,506.33"]),
            ],
            rows: vec![
                ("total assets", 15, "Total Assets"),
                ("total liabilities", 23, "Total Liabilities"),
                ("total equity", 28, "Total Equity"),
            ],
        },
        Run { id: "balsheet-weighted-average", checks: wa, rows: wa_rows },
        Run { id: "balsheet-average-cost", checks: ac, rows: ac_rows },
        Run { id: "balsheet-pricedb-nearest", checks: pn, rows: pn_rows },
        Run { id: "balsheet-pricedb-latest", checks: latest, rows: latest_rows },
        multilevel_run(),
        recursive_run(),
        Run {
            id: "balsheet-disable show-fcur show-rates",
            checks: vec![check("show-fcur disabled", 321, "Root", &["$121,473.52"])],
            rows: vec![("show-fcur disabled", 3, "Root")],
        },
        Run {
            id: "balsheet-enable show-fcur show-rates",
            checks: vec![check("show-fcur enabled", 331, "Root", ROOT_ALL)],
            rows: vec![("show-fcur enabled", 3, "Root")],
        },
        Run {
            id: "balsheet-incl-zb-accts=#f omit-zb-bals=#t",
            checks: vec![check("omit-zb-bals=#t", 345, "Root", &[]), check("incl-zb-accts=#f", 348, "Savings", &["$100.00"])],
            rows: vec![("omit-zb-bals=#t", 3, "Root"), ("incl-zb-accts=#f", 8, "Savings")],
        },
        Run {
            id: "balsheet-incl-zb-accts=#t omit-zb-bals=#f",
            checks: vec![check("omit-zb-bals=#f", 355, "Root", &["$0.00"]), check("incl-zb-accts=#t", 358, "Empty", &["$0.00"])],
            rows: vec![("omit-zb-bals=#f", 3, "Root"), ("incl-zb-accts=#t", 8, "Empty")],
        },
    ]
}

/// The exchange-rates table checks (table 2 of GnuCash's document): absent when "Show Exchange Rates" is off; else
/// #1.00 = $1.70 (the latest price, the 1980-02-01 transfer's) and 1 FUNDS = $480 + 85/104 (50005/104).
fn rates_checks() -> Vec<(&'static str, &'static str, u32, Vec<&'static str>)> {
    vec![
        ("balsheet-disable show-fcur show-rates", "show-rates disabled", 324, vec![]),
        ("balsheet-enable show-fcur show-rates", "show-rates enabled", 334, vec!["#1.00", "$1.70", "1. FUNDS", "$480 + 85/104"]),
    ]
}

fn fixture() -> Setup {
    Setup::fixture("balsheet-pnl.gnucash", &test_settings("iso"))
}

/// Every Balance Sheet check of `test-balsheet-pnl.scm:168-360`, through `report.run` and `report.export`, each
/// figure by its row label and each row at the place GnuCash's test reads it (its table starts with a row of spacer
/// cells, BS:444-447).
#[test]
fn expected_figures_of_gnucash_test() {
    let mut s = fixture();
    let e = expected("balsheet-pnl.expected.json");
    let runs = runs();
    runs.iter().for_each(|r| {
        let options = options_param(expected_run(&e, r.id));
        let result = s.run(TEMPLATE, &options);
        let html = s.export(TEMPLATE, &options);
        assert_checks(&result, &html, &r.checks);
        assert_rows_at(&result, &r.rows, 2);
    });
    rates_checks().iter().for_each(|(id, name, line, want)| {
        let options = options_param(expected_run(&e, id));
        let result = s.run(TEMPLATE, &options);
        assert_eq!(rates(&result), *want, "{name} (test line {line})");
        let html = s.export(TEMPLATE, &options);
        want.iter().for_each(|w| assert!(html.contains(w), "{name}: {w} is in the HTML"));
        assert_eq!(tables(&result).len(), if want.is_empty() { 1 } else { 2 }, "{name}");
    });
    let ours: BTreeSet<&str> =
        runs.iter().flat_map(|r| r.checks.iter().map(|c| c.name)).chain(rates_checks().iter().map(|c| c.1)).collect();
    let theirs: BTreeSet<&str> = e["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["template"] == TEMPLATE)
        .flat_map(|r| r["checks"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()))
        .collect();
    assert_eq!(ours, theirs, "every Balance Sheet check of the expected figures is run");
}

/// The default options (BS:140-241) on the fixture at 1971-01-01, every level shown: the asset section, then the
/// liabilities, then the equity with the unrealized gain, which is what the assets are worth at the nearest prices
/// ($116,006.33) less what they cost (the book's values, $113,100), $2,906.33 (BS:394-401); the total of
/// liabilities and equity equals the total assets. Section totals sit at level 0, the rightmost (BS:326-329).
#[test]
fn default_layout() {
    let result = fixture().run(TEMPLATE, &json!({ "General": { "Balance Sheet Date": { "absolute": "1971-01-01" } },
                                                    "Accounts": { "Levels of Subaccounts": "all" } }));
    assert_eq!(result["title"], " Balance Sheet 1971-01-01");
    let table = tables(&result)[0];
    assert_eq!(labels(table), [
        "Assets", "Root", "Asset", "Bank1", "Bonds", "Current", "Empty", "Savings", "Broker", "Funds", "ForeignBank",
        "ForeignSavings", "House", "Total Assets", "Liabilities", "Liability", "Bank2", "CreditCard", "Loan",
        "Total Liabilities", "Equity", "Equity", "Unrealized Gains", "Total Equity", "Total Liabilities & Equity",
    ]);
    assert_eq!(amount_of(&result, "Unrealized Gains"), ["$2,906.33"]);
    assert_eq!(amount_of(&result, "Total Liabilities & Equity"), ["$116,006.33"]);
    assert_eq!(table["columns"][1]["levels"], 5, "the tree depth, Root Account counted (RU:255-257)");
    let roles: Vec<(&str, u64)> = ["Assets", "Total Assets", "Unrealized Gains", "Total Liabilities & Equity", "Root", "Bonds"]
        .iter()
        .map(|l| {
            let (_, r) = rows_labelled(&result, l).into_iter().next().unwrap();
            (r["role"].as_str().unwrap(), r["level"].as_u64().unwrap())
        })
        .collect();
    assert_eq!(roles, [("heading", 0), ("total", 0), ("subtotal", 0), ("grandTotal", 0), ("account", 0), ("account", 3)]);
    let ruled = table["rows"].as_array().unwrap().iter().filter(|r| r["role"] == "rule").count();
    assert_eq!(ruled, 4, "two after the assets in one column (BS:456-458), one after the liabilities and the equity");
}

/// The two-column form (BS:413-415, :503-513): the assets in a table on the left, without the two rules, and the
/// liabilities and equity on the right, side by side in a group; the figures are the same.
#[test]
fn two_column_layout() {
    let general = json!({ "Balance Sheet Date": { "absolute": "1971-01-01" }, "Single column Balance Sheet": false });
    let result = fixture().run(TEMPLATE, &json!({ "General": general, "Accounts": { "Levels of Subaccounts": "all" } }));
    assert_eq!(result["sections"][0]["blocks"][0]["kind"], "group");
    let (left, right) = (tables(&result)[0], tables(&result)[1]);
    assert_eq!(labels(left).last().unwrap(), "Total Assets");
    assert_eq!(labels(right)[..2], ["Liabilities", "Liability"]);
    assert_eq!(labels(right).last().unwrap(), "Total Liabilities & Equity");
    assert_eq!(amount_of(&result, "Total Assets"), ["$116,006.33"]);
    assert_eq!(amount_of(&result, "Total Liabilities & Equity"), ["$116,006.33"]);
}

/// "Use standard US layout" off (BS:461-464, :494-497): assets, equity, then liabilities, the grand total last.
#[test]
fn non_us_layout() {
    let general = json!({ "Balance Sheet Date": { "absolute": "1971-01-01" }, "Use standard US layout": false });
    let result = fixture().run(TEMPLATE, &json!({ "General": general, "Accounts": { "Levels of Subaccounts": 2 } }));
    let shown: Vec<String> = labels(tables(&result)[0]).into_iter().filter(|l| !l.is_empty()).collect();
    assert_eq!(shown, [
        "Assets", "Root", "Asset", "Total Assets", "Equity", "Equity", "Unrealized Gains", "Total Equity", "Liabilities",
        "Liability", "Bank2", "Total Liabilities", "Total Liabilities & Equity",
    ]);
    // Levels 2 summarizes the second level (HAT:1018-1022). Root, selected, is the first level of the assets; it is
    // not a liability, so Liability is the first level of the liabilities and Bank2 the second.
    assert_eq!(amount_of(&result, "Asset"), ["#200.00", "$297.03", "30. FUNDS", "$9,000.30", "$106,709.00", "$106,709.00"]);
    assert_eq!(amount_of(&result, "Liability"), ["$0.00"]);
    assert_eq!(amount_of(&result, "Bank2"), ["-$9,500.00"]);
}

/// The section label and total options (BS:217-236), flattening (BS:175-177, :422), links (BS:428) and rules (BS:436).
#[test]
fn display_options() {
    let date = json!({ "Balance Sheet Date": { "absolute": "1971-01-01" } });
    let display = json!({ "Label the assets section": false, "Include assets total": false, "Label the liabilities section": false,
                          "Include liabilities total": false, "Label the equity section": false, "Include equity total": false,
                          "Display accounts as hyperlinks": false, "Show accounting-style rules": true,
                          "Parent account subtotals": "t" });
    let accounts = json!({ "Levels of Subaccounts": 2, "Flatten list to depth limit": true });
    let options = json!({ "General": date, "Display": display, "Accounts": accounts });
    let result = fixture().run(TEMPLATE, &options);
    let shown: Vec<String> = labels(tables(&result)[0]).into_iter().filter(|l| !l.is_empty()).collect();
    assert_eq!(shown[..4], ["Root", "Asset", "Bank1", "Bonds"], "flattened to the second level");
    ["Assets", "Total Assets", "Liabilities", "Total Liabilities", "Total Equity"]
        .iter()
        .for_each(|l| assert!(rows_labelled(&result, l).is_empty(), "{l} is left out"));
    let (_, bonds) = rows_labelled(&result, "Bonds")[0];
    assert_eq!((bonds["depth"].as_u64(), bonds["cells"][0]["account"].is_null()), (Some(1), true));
    let (_, total) = rows_labelled(&result, "Total Asset")[0];
    assert_eq!(total["cells"][1]["rule"], true);
    assert_eq!(amount_of(&result, "Total Liabilities & Equity"), ["$116,006.33"]);
}

/// A book through the API: USD checking and a EUR account under Assets, and equity; $1,000 opened on 2024-02-01,
/// €100 bought for $110 on 2024-03-01; EUR at $0.90 on 2024-03-02 and $1.20 on 2024-03-14. Trading accounts on
/// when `trading`, before the exchange, so it gets its trading splits.
fn exchange_book(trading: bool) -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    if trading {
        s.call("book.setOptions", json!({ "useTradingAccounts": true }));
    }
    s.account("Assets", "ASSET", None, "CURRENCY::USD");
    s.account("Checking", "BANK", Some("Assets"), "CURRENCY::USD");
    s.account("Euro", "BANK", Some("Assets"), "CURRENCY::EUR");
    s.account("Equity", "EQUITY", None, "CURRENCY::USD");
    s.transfer("2024-02-01", "Opening", "Equity", "Assets:Checking", "1000.00");
    let splits = json!([{ "account": "Assets:Euro", "value": "110.00", "amount": "100.00" },
                        { "account": "Assets:Checking", "value": "-110.00" }]);
    let buy = json!({ "datePosted": "2024-03-01", "currency": "CURRENCY::USD", "description": "Buy EUR", "splits": splits });
    s.call("transaction.post", buy);
    [("2024-03-02", "0.90"), ("2024-03-14", "1.20")].iter().for_each(|(date, value)| {
        s.call("price.add", json!({ "commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "value": value, "date": date }));
    });
    s
}

fn on(date: &str) -> Value {
    json!({ "General": { "Balance Sheet Date": { "absolute": date } } })
}

/// With trading accounts (BS:266, :389-392) the equity has a trading line and no unrealized gain: the trading
/// accounts hold -€100 and +$110, negated €100 and -$110; at $1.20 on 2024-03-15 that is $120 - $110, "Trading Gains"
/// $10.00, and at $0.90 on 2024-03-03 $90 - $110, "Trading Losses" $20.00 (BS:318-329, :478-482). Without trading
/// accounts the same difference is the unrealized gain: the assets (€100 and $890) less their value ($1,000)
/// (BS:394-401, :483-487). The trading accounts themselves get no rows: they are not assets, liabilities or equity.
#[test]
fn trading_line_or_unrealized_gains() {
    let mut trading = exchange_book(true);
    let gains = trading.run(TEMPLATE, &on("2024-03-15"));
    assert_eq!(amount_of(&gains, "Euro"), ["€100.00", "$120.00"]);
    assert_eq!(amount_of(&gains, "Total Assets"), ["$1,010.00"]);
    assert_eq!(amount_of(&gains, "Trading Gains"), ["$10.00"]);
    assert_eq!(amount_of(&gains, "Total Equity"), ["$1,010.00"]);
    assert_eq!(amount_of(&gains, "Total Liabilities & Equity"), ["$1,010.00"]);
    assert!(rows_labelled(&gains, "Unrealized Gains").is_empty());
    assert!(rows_labelled(&gains, "Trading").is_empty() && rows_labelled(&gains, "EUR").is_empty());
    let mut with_rates = on("2024-03-15");
    with_rates["Commodities"] = json!({ "Show Exchange Rates": true });
    assert_eq!(rates(&trading.run(TEMPLATE, &with_rates)), ["€1.00", "$1.20"]);
    let losses = trading.run(TEMPLATE, &on("2024-03-03"));
    assert_eq!(amount_of(&losses, "Trading Losses"), ["$20.00"]);
    assert_eq!(amount_of(&losses, "Total Liabilities & Equity"), ["$980.00"]);

    let mut plain = exchange_book(false);
    let gains = plain.run(TEMPLATE, &on("2024-03-15"));
    assert_eq!(amount_of(&gains, "Unrealized Gains"), ["$10.00"]);
    assert_eq!(amount_of(&gains, "Total Equity"), ["$1,010.00"]);
    assert!(rows_labelled(&gains, "Trading Gains").is_empty());
    let losses = plain.run(TEMPLATE, &on("2024-03-03"));
    assert_eq!(amount_of(&losses, "Unrealized Losses"), ["$20.00"]);
    assert_eq!(amount_of(&losses, "Total Assets"), ["$980.00"]);
}

/// Retained earnings are the negated income and expense balances (BS:385-387), labelled "Retained Losses" and shown
/// positive when negative (BS:473-477). $1,000 opened, $300 earned on 2024-03-01 and $500 spent on 2024-03-10: on
/// 2024-03-05 the earnings are $300.00; on 2024-03-15 the loss is $200.00, and the equity $1,000 - $200 = $800.00.
/// No liability account: the section has its label and a $0.00 total.
#[test]
fn retained_earnings_and_losses() {
    let mut s = Setup::new_book(&test_settings("iso"));
    s.account("Checking", "BANK", None, "CURRENCY::USD");
    s.account("Equity", "EQUITY", None, "CURRENCY::USD");
    s.account("Income", "INCOME", None, "CURRENCY::USD");
    s.account("Expenses", "EXPENSE", None, "CURRENCY::USD");
    s.transfer("2024-02-01", "Opening", "Equity", "Checking", "1000.00");
    s.transfer("2024-03-01", "Fees", "Income", "Checking", "300.00");
    s.transfer("2024-03-10", "Rent", "Checking", "Expenses", "500.00");
    let early = s.run(TEMPLATE, &on("2024-03-05"));
    assert_eq!(amount_of(&early, "Retained Earnings"), ["$300.00"]);
    assert_eq!(amount_of(&early, "Total Equity"), ["$1,300.00"]);
    let late = s.run(TEMPLATE, &on("2024-03-15"));
    assert_eq!(amount_of(&late, "Retained Losses"), ["$200.00"]);
    assert!(rows_labelled(&late, "Retained Earnings").is_empty());
    assert_eq!(amount_of(&late, "Equity"), ["-$1,000.00"]);
    assert_eq!(amount_of(&late, "Total Equity"), ["$800.00"]);
    assert_eq!(amount_of(&late, "Total Assets"), ["$800.00"]);
    assert_eq!(amount_of(&late, "Total Liabilities"), ["$0.00"]);
    assert_eq!(amount_of(&late, "Total Liabilities & Equity"), ["$800.00"]);
    assert!(rows_labelled(&late, "Unrealized Gains").is_empty(), "a zero unrealized gain is left out (BS:470-472)");
}

/// The test's null-test (`standard/test/test-balsheet-pnl.scm:87`): the report renders on a book with no accounts,
/// with the no-account warning (BS:366-373).
#[test]
fn empty_book() {
    let result = Setup::new_book(&test_settings("iso")).run(TEMPLATE, &json!({}));
    assert_eq!(blocks(&result)[0]["title"], "No accounts selected");
}

/// Running the report leaves the Book as it was and writes nothing; two exports are the same bytes, with no
/// script.
#[test]
fn read_only_and_deterministic() {
    let mut s = Setup::with_config(&test_settings("iso"));
    std::fs::copy(format!("{}/../../fixtures/reports/balsheet-pnl.gnucash", env!("CARGO_MANIFEST_DIR")), s.book()).unwrap();
    let book = s.book().display().to_string();
    s.call("book.open", json!({ "path": book }));
    let before = s.d.context().book().unwrap().clone();
    let files = s.files();
    let options = json!({ "Commodities": { "Show Exchange Rates": true }, "General": { "Single column Balance Sheet": false } });
    s.run(TEMPLATE, &options);
    let (a, b) = (s.export(TEMPLATE, &options), s.export(TEMPLATE, &options));
    assert_eq!(a, b);
    assert!(!a.to_ascii_lowercase().contains("<script"));
    assert_eq!(s.d.context().book().unwrap(), &before);
    assert_eq!(s.call("book.info", json!({}))["dirty"], false);
    assert_eq!(s.files(), files, "no journal or other file was written");
}

/// A user script that prints the Balance Sheet's asset and liabilities-plus-equity collectors (BS:376-408) for
/// every account of the book at the end of today, each as its sorted non-zero `commodity=amount` pairs.
const PROPERTY_SCRIPT: &str = r#"
import "standard/balance-sheet" as bs;
fn templates() { [#{ guid: "0000000000000000000000000000b501", name: "Balance property" }] }
fn options(params, o, book) { }
fn pairs(c) {
    let out = [];
    for p in c {
        if p[1] != 0 { out.push(`${p[0].id}=${p[1]}`); }
    }
    out.sort();
    out
}
fn render(params, book, opts, ctx) {
    let b = bs::statement_balances(book.root().descendants_sorted(), ctx.end_of_day(ctx.now), book.uses_trading_accounts());
    print(`${pairs(b.asset)}`);
    print(`${pairs(b.liability_plus_equity)}`);
    #{ title: "", sections: [] }
}
"#;

/// One generated transaction: a kind, a value in cents, a second amount and a day of March 2024.
type Txn = (u8, u32, u32, u32);

/// Post `t`: USD between the USD accounts, or a USD value for an amount of EUR or ACME shares.
fn post(s: &mut Setup, (kind, cents, other, day): Txn) {
    let value = format!("{}.{:02}", cents / 100, cents % 100);
    let date = format!("2024-03-{day:02}");
    let pair = |from: &str, to: &str| [(to.to_owned(), value.clone(), None), (from.to_owned(), format!("-{value}"), None)];
    let from_cash =
        |to: &str, amount: String| [(to.to_owned(), value.clone(), Some(amount)), ("Assets:Cash".to_owned(), format!("-{value}"), None)];
    let splits = match kind {
        0 => pair("Equity", "Assets:Cash"),
        1 => pair("Income", "Assets:Cash"),
        2 => pair("Assets:Cash", "Expenses"),
        3 => pair("Liabilities:Card", "Expenses"),
        4 => pair("Assets:Cash", "Liabilities:Card"),
        5 => from_cash("Assets:Euro", format!("{other}.00")),
        6 => from_cash("Assets:ACME", format!("{other}")),
        _ => [("Expenses".to_owned(), value.clone(), None), ("Assets:Euro".to_owned(), format!("-{value}"), Some(format!("-{other}.00")))],
    };
    let splits: Vec<Value> = splits
        .iter()
        .map(|(account, value, amount)| match amount {
            Some(a) => json!({ "account": account, "value": value, "amount": a }),
            None => json!({ "account": account, "value": value }),
        })
        .collect();
    s.call("transaction.post", json!({ "datePosted": date, "currency": "CURRENCY::USD", "splits": splits }));
}

/// A book of USD, EUR and ACME accounts of every kind the statement shows, with `txns` and prices for EUR and ACME,
/// trading accounts on when `trading`.
fn generated_book(trading: bool, txns: &[Txn], eur: u32, acme: u32) -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    s.script("property.rhai", PROPERTY_SCRIPT);
    if trading {
        s.call("book.setOptions", json!({ "useTradingAccounts": true }));
    }
    s.call("commodity.create", json!({ "namespace": "NASDAQ", "mnemonic": "ACME", "fullname": "Acme", "fraction": 1000 }));
    [
        ("Assets", "ASSET", None, "CURRENCY::USD"),
        ("Cash", "BANK", Some("Assets"), "CURRENCY::USD"),
        ("Euro", "BANK", Some("Assets"), "CURRENCY::EUR"),
        ("ACME", "STOCK", Some("Assets"), "NASDAQ::ACME"),
        ("Liabilities", "LIABILITY", None, "CURRENCY::USD"),
        ("Card", "CREDIT", Some("Liabilities"), "CURRENCY::USD"),
        ("Equity", "EQUITY", None, "CURRENCY::USD"),
        ("Income", "INCOME", None, "CURRENCY::USD"),
        ("Expenses", "EXPENSE", None, "CURRENCY::USD"),
    ]
    .iter()
    .for_each(|(name, kind, parent, commodity)| s.account(name, kind, *parent, commodity));
    txns.iter().for_each(|t| post(&mut s, *t));
    [("CURRENCY::EUR", eur), ("NASDAQ::ACME", acme)].iter().for_each(|(commodity, cents)| {
        let value = format!("{}.{:02}", cents / 100, cents % 100);
        s.call("price.add", json!({ "commodity": commodity, "currency": "CURRENCY::USD", "value": value, "date": "2024-03-10" }));
    });
    s
}

fn txn() -> impl Strategy<Value = Txn> {
    (0_u8..8, 1_u32..500_000, 1_u32..2_000, 1_u32..29)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    /// `design/reports.md` §13: a Balance Sheet's assets equal its liabilities plus equity as multi-commodity
    /// collectors, before any rounding, with trading accounts or without; so its two totals show the same figure.
    #[test]
    fn a_balance_sheet_balances(trading in any::<bool>(), txns in proptest::collection::vec(txn(), 1..10),
                                eur in 1_u32..300, acme in 1_u32..100_000) {
        let mut s = generated_book(trading, &txns, eur, acme);
        let log = s.run("Balance property", &json!({}))["log"].clone();
        prop_assert_eq!(&log[0]["text"], &log[1]["text"], "assets, then liabilities and equity");
        let result = s.run(TEMPLATE, &json!({}));
        prop_assert_eq!(amount_of(&result, "Total Assets"), amount_of(&result, "Total Liabilities & Equity"));
    }
}
