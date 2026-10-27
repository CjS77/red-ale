//! The Account Summary (`standard/account-summary.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/account-summary.scm`, cited as AS) through `report.run` and `report.export`,
//! on a copy of `fixtures/reports/test-data.gnucash`.
//!
//! The book (`create-test-data`, `5.17:bindings/guile/test/test-engine-extras.scm:389-480`): Root (ASSET) holds Asset
//! (Bank, GBP Bank in GBP, Wallet), Liabilities, Income, Income-GBP (GBP), Expenses and Equity. Today, 2024-03-15,
//! the balances are Bank $2,186, GBP Bank #608, Wallet $20, Liabilities -$3, Income -$2,573, Income-GBP -#612,
//! Expenses $264 and Equity $111; GBP is $1.50 (3/2, the newest price). On 1970-01-01 only Bank ($24), Income
//! (-$29) and Expenses ($5) have balances.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod statements;

use serde_json::{Value, json};
use statements::{
    Check, Setup, amount_of, assert_checks, expected, expected_run, labels, options_param, rates, rows_labelled, tables, test_settings,
};

const TEMPLATE: &str = "Account Summary";

fn setup() -> Setup {
    Setup::fixture("test-data.gnucash", &test_settings("locale"))
}

/// The account rows of the one table, as `label: amounts`.
fn rows(result: &Value) -> Vec<String> {
    let t = tables(result)[0];
    let label = t["columns"].as_array().unwrap().iter().position(|c| c["role"] == "label").unwrap();
    let amount = t["columns"].as_array().unwrap().iter().position(|c| c["role"] == "amount");
    t["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let shown = amount.map(|a| statements::displays(&r["cells"][a])).unwrap_or_default();
            format!("{} {}: {}", r["depth"], statements::cell_text(&r["cells"][label]), shown.join(" "))
        })
        .collect()
}

fn run(options: &Value) -> Value {
    setup().run(TEMPLATE, options)
}

/// `test-account-summary.scm:51-65`, default options. GnuCash reads the cells at HTML positions, where the commodity
/// tables nested in a cell repeat their text; here each figure is read from its account's row: a `commodities` cell
/// gives each line's amount and its value in the report currency.
#[test]
fn expected_figures_of_gnucash_test() {
    let mut s = setup();
    let e = expected("test-data.expected.json");
    let run = expected_run(&e, "account-summary");
    let options = options_param(run);
    let result = s.run(TEMPLATE, &options);
    let html = s.export(TEMPLATE, &options);
    let checks = [
        Check {
            name: "accsum col 1, col 3: GBP Bank",
            line: 54,
            label: "GBP Bank",
            want: &["#608.00", "$912.00"],
        },
        Check {
            name: "accsum col 1, col 3: Income-GBP",
            line: 54,
            label: "Income-GBP",
            want: &["-#612.00", "-$918.00"],
        },
        Check {
            name: "accsum col 3: Bank",
            line: 61,
            label: "Bank",
            want: &["$2,186.00"],
        },
        Check {
            name: "accsum col 3: Wallet",
            line: 61,
            label: "Wallet",
            want: &["$20.00"],
        },
    ];
    assert_checks(&result, &html, &checks);
    let names = [
        "Root",
        "Asset",
        "Bank",
        "GBP Bank",
        "Wallet",
        "Liabilities",
        "Income",
        "Income-GBP",
        "Expenses",
        "Equity",
    ];
    assert_eq!(
        labels(tables(&result)[0]),
        names,
        "accsum col 2 (test line 57): every account, in order"
    );
    names
        .iter()
        .for_each(|n| assert!(html.contains(&format!(">{n}<")), "{n} is in the HTML"));
    assert_eq!(
        run["checks"].as_array().unwrap().len(),
        3,
        "every check of the run is covered above"
    );
}

/// Default options (AS:138-234): depth 3, summarized; parents show their own balance (OU's "immediate-bal" default),
/// which is $0.00 for Root and Asset; the code column is shown and empty; the title is the company name (none),
/// the report title and the date (AS:296-299) in the C locale's `%m/%d/%y`; no exchange-rates table.
#[test]
fn default_options() {
    let result = run(&json!({}));
    assert_eq!(result["title"], " Account Summary 03/15/24");
    assert_eq!(
        rows(&result),
        [
            "0 Root: $0.00",
            "1 Asset: $0.00",
            "2 Bank: $2,186.00",
            "2 GBP Bank: #608.00 $912.00",
            "2 Wallet: $20.00",
            "1 Liabilities: -$3.00",
            "1 Income: -$2,573.00",
            "1 Income-GBP: -#612.00 -$918.00",
            "1 Expenses: $264.00",
            "1 Equity: $111.00",
        ]
    );
    let table = tables(&result)[0];
    let headers: Vec<&str> = table["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert_eq!(headers, ["Account Code", "Account title", "Balance"], "AS:373-393");
    assert_eq!(table["columns"][2]["levels"], 3, "one staircase level per level of the tree depth");
    assert!(
        table["rows"].as_array().unwrap().iter().all(|r| r["cells"][0]["text"] == ""),
        "no account has a code"
    );
    assert_eq!(tables(&result).len(), 1, "Show Exchange Rates is off by default (AS:191-193)");
    let (_, bank) = rows_labelled(&result, "Bank")[0];
    assert_eq!((bank["level"].as_u64(), bank["role"].as_str()), (Some(2), Some("account")));
    assert_eq!(
        bank["cells"][1]["account"], "a0000000000000000000000000000003",
        "labels link to their account (AS:326)"
    );
}

/// One level: "summarize" gives Root its whole tree's balance (HAT:1018-1022), a commodity per line in the
/// collector's order (GBP was added after USD, so it comes first): GBP 608 - 612 = -#4.00, -$6.00 at $1.50; USD
/// 2,186 + 20 - 3 - 2,573 + 264 + 111 = $5.00. "flatten" raises every account to the first level, each with its own
/// balance (HAT:661, HAT:1023-1024); "truncate" drops the deeper accounts and shows Root as any parent, its own
/// $0.00 (HAT:1025-1026).
#[test]
fn depth_limit_summarize_flatten_truncate() {
    let one = |behavior: &str| run(&json!({ "Accounts": { "Levels of Subaccounts": 1, "Depth limit behavior": behavior } }));
    assert_eq!(rows(&one("summarize")), ["0 Root: -#4.00 -$6.00 $5.00 $5.00"]);
    assert_eq!(
        rows(&one("flatten")),
        [
            "0 Root: $0.00",
            "0 Asset: $0.00",
            "0 Bank: $2,186.00",
            "0 GBP Bank: #608.00 $912.00",
            "0 Wallet: $20.00",
            "0 Liabilities: -$3.00",
            "0 Income: -$2,573.00",
            "0 Income-GBP: -#612.00 -$918.00",
            "0 Expenses: $264.00",
            "0 Equity: $111.00",
        ]
    );
    assert_eq!(rows(&one("truncate")), ["0 Root: $0.00"]);
    let two = run(&json!({ "Accounts": { "Levels of Subaccounts": 2 } }));
    assert_eq!(
        rows(&two)[..3],
        [
            "0 Root: $0.00",
            "1 Asset: #608.00 $912.00 $2,206.00 $2,206.00",
            "1 Liabilities: -$3.00"
        ]
    );
}

/// "Parent account subtotals" (AS:266-268, HAT:778-787): a "Total" row after a parent's children with its
/// recursive balance, each commodity rounded to the cent and added (CU:900-910): Asset #608 at $1.50 plus $2,206 is
/// $3,118.00; Root -$6.00 + $5.00 = -$1.00. With "Show accounting-style rules" those rows carry a rule (HAT:1041-1043).
/// "Calculate Subtotal" shows a parent's recursive balance on its own row.
#[test]
fn parent_subtotals_and_balances() {
    let result = run(&json!({ "Display": { "Parent account subtotals": "t", "Show accounting-style rules": true } }));
    assert_eq!(amount_of(&result, "Total Asset"), ["$3,118.00"]);
    assert_eq!(amount_of(&result, "Total Root"), ["-$1.00"]);
    assert_eq!(
        rows(&result)[4..6],
        ["2 Wallet: $20.00".to_owned(), "1 Total Asset: $3,118.00".to_owned()]
    );
    let (table, total) = rows_labelled(&result, "Total Asset")[0];
    assert_eq!(
        (total["role"].as_str(), total["cells"][2]["rule"].as_bool()),
        (Some("subtotal"), Some(true))
    );
    assert!(
        table["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["role"] == "account")
            .all(|r| r["cells"][2]["rule"].is_null())
    );
    let recursive = run(&json!({ "Display": { "Parent account balances": "recursive-bal" } }));
    assert_eq!(amount_of(&recursive, "Asset"), ["#608.00", "$912.00", "$2,206.00", "$2,206.00"]);
    let omitted = run(&json!({ "Display": { "Parent account balances": "omit-bal" } }));
    assert_eq!(rows(&omitted)[..3], ["0 Root: ", "1 Asset: ", "2 Bank: $2,186.00"]);
}

/// "Omit zero balance figures" (AS:340-342, HAT:1007-1013) leaves Root's and Asset's own $0.00 blank; "Include
/// accounts with zero total balances" off (AS:323-325, HAT:742-744) drops every account whose whole tree is zero on
/// 1970-01-01: Root's tree adds up to $24 - $29 + $5 = 0, so only Asset (its own balance $0.00), Bank, Income and
/// Expenses remain.
#[test]
fn zero_balances() {
    let omit = run(&json!({ "Display": { "Omit zero balance figures": true } }));
    assert_eq!(rows(&omit)[..3], ["0 Root: ", "1 Asset: ", "2 Bank: $2,186.00"]);
    let day = json!({ "absolute": "1970-01-01" });
    let all = run(&json!({ "General": { "Date": day } }));
    assert_eq!(labels(tables(&all)[0]).len(), 10);
    let nonzero = run(&json!({ "General": { "Date": day }, "Display": { "Include accounts with zero total balances": false } }));
    assert_eq!(
        rows(&nonzero),
        ["1 Asset: $0.00", "2 Bank: $24.00", "1 Income: -$29.00", "1 Expenses: $5.00"]
    );
    assert_eq!(nonzero["title"], " Account Summary 01/01/70");
}

/// "Show Exchange Rates" (AS:413-418) adds the rates of the commodities of the accounts and their descendants
/// (HU:226-265): one, GBP at its nearest price to today, 3/2. "Show Foreign Currencies" off (AS:343) shows every
/// balance in the report currency only.
#[test]
fn exchange_rates_and_foreign_currencies() {
    let mut s = setup();
    let options = json!({ "Commodities": { "Show Exchange Rates": true } });
    let result = s.run(TEMPLATE, &options);
    assert_eq!(rates(&result), ["#1.00", "$1.50"]);
    assert_eq!(tables(&result)[1]["columns"][0]["label"], "Exchange rate");
    assert!(s.export(TEMPLATE, &options).contains("$1.50"));
    let plain = run(&json!({ "Commodities": { "Show Foreign Currencies": false } }));
    assert_eq!(amount_of(&plain, "GBP Bank"), ["$912.00"]);
    assert_eq!(amount_of(&plain, "Income-GBP"), ["-$918.00"]);
}

/// "Price Source" (AS:183-185, :285-286; CU:769-795) on 1990-01-01, when GBP Bank holds the twelve #51 incomes of
/// 1978, #612. The book's two prices are both later: USD at #5/7 on 2000-01-15 and GBP at $3/2 on 2000-02-15.
/// "pricedb-before" finds none and exchanges to $0.00; "pricedb-latest" takes the newest, $1.50, $918.00;
/// "pricedb-nearest" the nearer one, inverted, $7/5, $856.80 (`5.17:libgnucash/engine/gnc-pricedb.cpp:2408-2527`).
#[test]
fn price_sources() {
    let at = |source: &str| {
        let options = json!({ "General": { "Date": { "absolute": "1990-01-01" } }, "Commodities": { "Price Source": source } });
        amount_of(&run(&options), "GBP Bank")
    };
    assert_eq!(at("pricedb-before"), ["#612.00", "$0.00"]);
    assert_eq!(at("pricedb-latest"), ["#612.00", "$918.00"]);
    assert_eq!(at("pricedb-nearest"), ["#612.00", "$856.80"]);
}

/// Under `reverseBalancedAccounts: "credit"` (`gnc-reverse-balance`, HAT:1014-1015) LIABILITY, INCOME and EQUITY
/// balances are negated, commodity by commodity; ASSET and EXPENSE ones are not.
#[test]
fn reversed_signs_under_credit() {
    let mut config = test_settings("locale");
    config["reports"]["reverseBalancedAccounts"] = json!("credit");
    let mut s = Setup::fixture("test-data.gnucash", &config);
    let result = s.run(TEMPLATE, &json!({}));
    assert_eq!(
        rows(&result)[5..],
        [
            "1 Liabilities: $3.00",
            "1 Income: $2,573.00",
            "1 Income-GBP: #612.00 $918.00",
            "1 Expenses: $264.00",
            "1 Equity: -$111.00",
        ]
    );
    assert_eq!(amount_of(&result, "Bank"), ["$2,186.00"]);
}

/// The column options (AS:373-409): code, type and description before the account title, the balance, then notes;
/// subtotal rows repeat their account's cells as `add-col` copies them. Without links the labels name no account
/// (AS:326).
#[test]
fn columns() {
    let display = json!({ "Account Code": false, "Account Type": true, "Account Description": true, "Account Notes": true,
                          "Display accounts as hyperlinks": false, "Parent account subtotals": "t" });
    let result = run(&json!({ "Display": display }));
    let table = tables(&result)[0];
    let headers: Vec<&str> = table["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert_eq!(headers, ["Type", "Description", "Account title", "Balance", "Notes"]);
    let types: Vec<&str> = table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["cells"][0]["text"].as_str().unwrap())
        .collect();
    assert_eq!(types[..5], ["Asset", "Asset", "Asset", "Asset", "Asset"]);
    assert_eq!(rows_labelled(&result, "Total Asset")[0].1["cells"][0]["text"], "Asset");
    assert_eq!(rows_labelled(&result, "Income")[0].1["cells"][0]["text"], "Income");
    assert!(table["rows"].as_array().unwrap().iter().all(|r| r["cells"][2]["account"].is_null()));
    let no_balance = run(&json!({ "Display": { "Account Balance": false } }));
    let headers: Vec<&str> = tables(&no_balance)[0]["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert_eq!(headers, ["Account Code", "Account title"], "AS:387-393");
}

/// The report title option (AS:141-143, :296-299), and the warning when no account is selected (AS:301-307): on a
/// book with no accounts the option's default is empty, and an empty value reads as the default
/// (`5.17:libgnucash/engine/gnc-option-impl.cpp:418-421`), so that is how a report gets none.
#[test]
fn title_and_no_accounts() {
    let result = run(&json!({ "General": { "Report Title": "Chart" } }));
    assert_eq!(result["title"], " Chart 03/15/24");
    let none = Setup::new_book(&test_settings("locale")).run(TEMPLATE, &json!({}));
    let block = &none["sections"][0]["blocks"][0];
    assert_eq!(
        (block["kind"].as_str(), block["title"].as_str()),
        (Some("warning"), Some("No accounts selected"))
    );
}

/// Running the report leaves the Book as it was and writes nothing; two exports are the same bytes, with no
/// script.
#[test]
fn read_only_and_deterministic() {
    let mut s = Setup::with_config(&test_settings("locale"));
    std::fs::copy(
        format!("{}/../../fixtures/reports/test-data.gnucash", env!("CARGO_MANIFEST_DIR")),
        s.book(),
    )
    .unwrap();
    let book = s.book().display().to_string();
    s.call("book.open", json!({ "path": book }));
    let before = s.d.context().book().unwrap().clone();
    let files = s.files();
    let options = json!({ "Commodities": { "Show Exchange Rates": true }, "Display": { "Parent account subtotals": "t" } });
    s.run(TEMPLATE, &options);
    let (a, b) = (s.export(TEMPLATE, &options), s.export(TEMPLATE, &options));
    assert_eq!(a, b);
    assert!(!a.to_ascii_lowercase().contains("<script"));
    assert_eq!(s.d.context().book().unwrap(), &before);
    assert_eq!(s.call("book.info", json!({}))["dirty"], false);
    assert_eq!(s.files(), files, "no journal or other file was written");
}
