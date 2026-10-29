//! The Trial Balance (`standard/trial-balance.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/trial-balance.scm`, cited as TB) through `report.run` and `report.export`:
//! the figures of GnuCash's own test (`standard/test/test-trial-balance.scm`) on `fixtures/reports/adjusting-entries`
//! and `adjusting-entries-fx`, each as XML and as its SQLite copy, and hand-worked cases on books built through the API
//! for the options and branches that test does not reach.
//!
//! A row is read as GnuCash's test reads it: its label, then each amount cell's display strings, `""` for a blank
//! cell. In the work sheet the columns are, from 1, Trial Balance, Adjustments, Adjusted Trial Balance, Income
//! Statement and Balance Sheet, each a debit and a credit. Rows come in display order, where a BANK account sorts
//! after its other siblings: GnuCash's `typeorder` lists 14 types in an array of 15, and the unlisted entry, 0, is
//! ACCT_TYPE_BANK, so `revorder` ranks BANK last (`5.17:libgnucash/engine/Account.cpp:2348-2400`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use serde_json::{Value, json};
use statements::{Setup, blocks, cell_text, displays, expected, options_param, tables, test_settings};

const TRIAL_BALANCE: &str = "Trial Balance";

/// The statement table's body: the rows below the work sheet's headings, each as its label and its amount cells'
/// display strings (a commodity table's lines joined by a space), `""` when blank.
fn body(result: &Value) -> Vec<Vec<String>> {
    tables(result)[0]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["role"] != "heading")
        .map(|r| {
            let cells = r["cells"].as_array().unwrap();
            std::iter::once(cell_text(&cells[0]))
                .chain(cells[1..].iter().map(|c| displays(c).join(" ")))
                .collect()
        })
        .collect()
}

/// The row labelled `label`.
fn row(result: &Value, label: &str) -> Vec<String> {
    body(result)
        .into_iter()
        .find(|r| r[0] == label)
        .unwrap_or_else(|| panic!("no row {label}: {:#?}", body(result)))
}

fn strings(want: &[&str]) -> Vec<String> {
    want.iter().map(|s| (*s).to_owned()).collect()
}

/// The body as GnuCash's HTML lays it out: the work sheet's Net Income/Loss figure in the Adjustments column, where
/// `gnc:html-table-add-labeled-amount-line!`'s padding puts it (`5.17:gnucash/report/html-acct-table.scm:899-901`),
/// not in the Income Statement column the Scheme names (TB:821-826) and red-ale uses.
fn as_gnucash_lays_it_out(mut rows: Vec<Vec<String>>) -> Vec<Vec<String>> {
    rows.iter_mut()
        .filter(|r| r[0] == "Net Income" || r[0] == "Net Loss")
        .for_each(|r| {
            let (from, to) = if r[0] == "Net Income" { (7, 3) } else { (8, 4) };
            r[to] = std::mem::take(&mut r[from]);
        });
    rows
}

/// What `sxml->table-row-col` reads (`5.17:gnucash/report/test/test-report-extras.scm:96-109`) from table 1 at
/// `select`: a column from 1 top to bottom, a row from 1 (negative from the end), or every text; blank cells give
/// nothing.
fn select(rows: &[Vec<String>], select: &Value) -> Vec<String> {
    let texts = |r: &Vec<String>| r.iter().filter(|c| !c.is_empty()).cloned().collect::<Vec<_>>();
    match (select["row"].as_i64(), select["column"].as_u64()) {
        (None, Some(col)) => rows
            .iter()
            .map(|r| r[usize::try_from(col).unwrap() - 1].clone())
            .filter(|c| !c.is_empty())
            .collect(),
        (Some(n), None) => {
            let len = i64::try_from(rows.len()).unwrap();
            texts(&rows[usize::try_from(if n < 0 { len + n } else { n - 1 }).unwrap()])
        }
        (None, None) => rows.iter().flat_map(texts).collect(),
        _ => panic!("selection {select}"),
    }
}

/// Every Trial Balance check of `<book>.expected.json` on the book, as XML and as SQLite, through `report.run` and
/// `report.export`. Returns the number of checks run on each.
fn expected_checks(book: &str) -> usize {
    let e = expected(&format!("{book}.expected.json"));
    let runs: Vec<&Value> = e["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["template"] == TRIAL_BALANCE)
        .collect();
    let name = format!("{book}.gnucash");
    let settings = test_settings("locale");
    for (format, mut s) in [
        ("xml", Setup::fixture(&name, &settings)),
        ("sqlite", Setup::sqlite_fixture(&name, &settings)),
    ] {
        runs.iter().for_each(|run| {
            let options = options_param(run);
            let result = s.run(TRIAL_BALANCE, &options);
            let html = s.export(TRIAL_BALANCE, &options);
            let rows = as_gnucash_lays_it_out(body(&result));
            run["checks"].as_array().unwrap().iter().for_each(|c| {
                let what = format!("{format} {}: {} (test line {})", run["id"], c["name"], c["line"]);
                let got = select(&rows, &c["select"]);
                if let Some(count) = c["count"].as_u64() {
                    assert_eq!(got.len(), usize::try_from(count).unwrap(), "{what}: {rows:#?}");
                } else {
                    let want: Vec<String> = serde_json::from_value(c["expected"].clone()).unwrap();
                    assert_eq!(got, want, "{what}: {rows:#?}");
                    want.iter().for_each(|w| assert!(html.contains(w), "{what}: {w} is in the HTML"));
                }
            });
        });
    }
    runs.iter().map(|r| r["checks"].as_array().unwrap().len()).sum()
}

/// test-trial-balance.scm:78-166: the current, pre-adjustment and work sheet runs, 20 checks.
#[test]
fn expected_figures_of_gnucash_test() {
    assert_eq!(expected_checks("adjusting-entries"), 20);
}

/// test-trial-balance.scm:168-179: after the two foreign transfers, the unrealized gains above the totals, 1 check.
#[test]
fn expected_unrealized_gain_of_gnucash_test() {
    assert_eq!(expected_checks("adjusting-entries-fx"), 1);
}

/// The options of TB:170-277 in GnuCash's order.
#[test]
fn options_are_gnucash_s() {
    let mut s = Setup::with_config(&test_settings("iso"));
    let described = s.call("report.options", json!({ "report": TRIAL_BALANCE }));
    let got: Vec<String> = described["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}/{} {} {}",
                o["section"].as_str().unwrap(),
                o["name"].as_str().unwrap(),
                o["key"],
                o["default"]
            )
        })
        .collect();
    let want = [
        r#"Accounts/Accounts "a" []"#,
        r#"Accounts/Levels of Subaccounts "b" 1"#,
        r#"Commodities/Report's currency "a" "CURRENCY::USD""#,
        r#"Commodities/Price Source "b" "average-cost""#,
        r#"Commodities/Show Foreign Currencies "c" false"#,
        r#"Commodities/Show Exchange Rates "d" false"#,
        r#"Display/Display accounts as hyperlinks "e" true"#,
        r#"Entries/Adjusting Entries pattern "a" "Adjusting Entries""#,
        r#"Entries/Adjusting Entries pattern is case-sensitive "b" false"#,
        r#"Entries/Adjusting Entries Pattern is regular expression "c" false"#,
        r#"Entries/Closing Entries pattern "d" "Closing Entries""#,
        r#"Entries/Closing Entries pattern is case-sensitive "e" false"#,
        r#"Entries/Closing Entries Pattern is regular expression "f" false"#,
        r#"General/Report name "0a" "Trial Balance""#,
        r#"General/Stylesheet "0b" "Default""#,
        r#"General/Add options summary "0c" "never""#,
        r#"General/Report Title "a" "Trial Balance""#,
        r#"General/Start of Adjusting/Closing "ca" {"relative":"start-accounting-period"}"#,
        r#"General/Date of Report "cb" {"relative":"end-accounting-period"}"#,
        r#"General/Report variation "d" "current""#,
        r#"Merchandising/Gross adjustment accounts. "c" []"#,
        r#"Merchandising/Income summary accounts "d" []"#,
    ];
    assert_eq!(got, want);
}

/// A book with no account gives the no-account warning only, under the usual title (TB:367-389).
#[test]
fn no_accounts_give_the_warning() {
    let mut s = Setup::new_book(&test_settings("iso"));
    let result = s.run(TRIAL_BALANCE, &json!({ "Commodities": { "Show Exchange Rates": true } }));
    let b = blocks(&result);
    assert_eq!((b.len(), b[0]["title"].as_str()), (1, Some("No accounts selected")));
    assert_eq!(result["title"], " Trial Balance 2024-03-15");
}

/// A new book with `accounts` (`name:TYPE` or `name:TYPE:COMMODITY`, top level).
fn book_with(accounts: &[&str]) -> Setup {
    let mut s = Setup::new_book(&test_settings("iso"));
    accounts.iter().for_each(|a| {
        let parts: Vec<&str> = a.split(':').collect();
        let commodity = parts.get(2).map_or("CURRENCY::USD".to_owned(), |c| format!("CURRENCY::{c}"));
        s.account(parts[0], parts[1], None, &commodity);
    });
    s
}

fn year(variant: &str, more: Value) -> Value {
    let mut options = json!({ "General": {
        "Start of Adjusting/Closing": { "absolute": "2024-01-01" },
        "Date of Report": { "absolute": "2024-12-31" },
        "Report variation": variant,
    } });
    more.as_object().unwrap().iter().for_each(|(k, v)| options[k] = v.clone());
    options
}

/// TRADING accounts get no row (TB:325-340 never uses the decomposition's TRADING list), but the rates table lists
/// their commodities, since it reads the option's accounts (TB:857-861). With only TRADING accounts selected GnuCash
/// fails reading the columns of a table with no row (TB:520-523); red-ale fails too, saying why.
#[test]
fn trading_accounts_have_no_row() {
    let mut s = book_with(&["Bank:BANK", "Capital:EQUITY", "Trading EUR:TRADING:EUR"]);
    s.transfer("2024-01-02", "Investment", "Capital", "Bank", "1000.00");
    let price = json!({ "commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "value": "1.20", "date": "2024-06-01" });
    s.call("price.add", price);
    let commodities = json!({ "Show Exchange Rates": true, "Price Source": "pricedb-nearest" });
    let both = year(
        "current",
        json!({ "Accounts": { "Accounts": ["Bank", "Trading EUR"] }, "Commodities": commodities }),
    );
    let result = s.run(TRIAL_BALANCE, &both);
    assert_eq!(
        body(&result),
        [strings(&["Bank", "$1,000.00", ""]), strings(&["", "$1,000.00", "$0.00"])]
    );
    let rates = tables(&result)[1];
    assert_eq!(
        rates["rows"][0]["cells"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(displays)
            .collect::<Vec<_>>(),
        ["€1.00", "$1.20"]
    );
    let only = year("current", json!({ "Accounts": { "Accounts": ["Trading EUR"] } }));
    let err = s.err("report.run", json!({ "report": TRIAL_BALANCE, "options": only }));
    assert_eq!(err.code, 3000, "report-script");
    assert!(
        format!("{err} {:?}", err.data).contains("only TRADING accounts are selected"),
        "{err} {:?}",
        err.data
    );
}

/// A trading business's year: $1,000 invested on 2024-01-02, $400 of stock bought on 2024-02-01, $700 of sales on
/// 2024-03-01, and two "Adjusting Entries" on 2024-12-31 that move the opening stock of $400 into Income Summary and
/// bring the closing stock of $250 back out of it.
fn merchandising_book() -> Setup {
    let mut s = book_with(&[
        "Bank:BANK",
        "Inventory:ASSET",
        "Capital:EQUITY",
        "Income Summary:EQUITY",
        "Sales:INCOME",
    ]);
    s.transfer("2024-01-02", "Investment", "Capital", "Bank", "1000.00");
    s.transfer("2024-02-01", "Purchase", "Bank", "Inventory", "400.00");
    s.transfer("2024-03-01", "Sale", "Sales", "Bank", "700.00");
    s.transfer("2024-12-31", "Adjusting Entries", "Inventory", "Income Summary", "400.00");
    s.transfer("2024-12-31", "Adjusting Entries", "Income Summary", "Inventory", "250.00");
    s
}

fn merchandising() -> Value {
    year(
        "work-sheet",
        json!({ "Merchandising": {
        "Gross adjustment accounts.": ["Inventory"],
        "Income summary accounts": ["Income Summary"],
    } }),
    )
}

/// The work sheet with Inventory a gross adjustment account and Income Summary an income summary account
/// (TB:582-660). Inventory's adjustments, +$250 and -$400, show gross (TB:618-621, :649-651), and its adjusted balance
/// is the $250 left (TB:625). Income Summary's adjustments show gross too; with no balance before them, its adjusted
/// trial balance is the debit $400 and credit $250 pair (TB:631-633), in the Income Statement since it is an income
/// summary account (TB:653-657). Credits of $950 against debits of $400 in the Income Statement are a Net Income of
/// $550, and the Balance Sheet's debits exceed its credits by $550 (TB:799-850).
#[test]
fn gross_adjustments_and_income_summary_accounts() {
    let result = merchandising_book().run(TRIAL_BALANCE, &merchandising());
    let rows: Vec<Vec<String>> = [
        [
            "Inventory",
            "$400.00",
            "",
            "$250.00",
            "$400.00",
            "$250.00",
            "",
            "",
            "",
            "$250.00",
            "",
        ],
        ["Sales", "", "$700.00", "", "", "", "$700.00", "", "$700.00", "", ""],
        ["Capital", "", "$1,000.00", "", "", "", "$1,000.00", "", "", "", "$1,000.00"],
        [
            "Income Summary",
            "",
            "",
            "$400.00",
            "$250.00",
            "$400.00",
            "$250.00",
            "$400.00",
            "$250.00",
            "",
            "",
        ],
        ["Bank", "$1,300.00", "", "", "", "$1,300.00", "", "", "", "$1,300.00", ""],
        [
            "",
            "$1,700.00",
            "$1,700.00",
            "$650.00",
            "$650.00",
            "$1,950.00",
            "$1,950.00",
            "$400.00",
            "$950.00",
            "$1,550.00",
            "$1,000.00",
        ],
        ["Net Income", "", "", "", "", "", "", "$550.00", "", "", "$550.00"],
        ["", "", "", "", "", "", "", "$950.00", "$950.00", "$1,550.00", "$1,550.00"],
    ]
    .iter()
    .map(|r| strings(r))
    .collect();
    assert_eq!(body(&result), rows);
    let table = tables(&result)[0];
    let heading: Vec<&str> = table["rows"][0]["cells"].as_array().unwrap()[1..]
        .iter()
        .map(|c| c["text"].as_str().unwrap())
        .collect();
    assert_eq!(
        heading,
        [
            "Trial Balance",
            "Adjustments",
            "Adjusted Trial Balance",
            "Income Statement",
            "Balance Sheet"
        ]
    );
    let roles: Vec<&str> = table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["role"].as_str().unwrap())
        .collect();
    assert_eq!(
        roles,
        [
            "heading",
            "account",
            "account",
            "account",
            "account",
            "account",
            "total",
            "total",
            "grandTotal"
        ]
    );
}

/// An income summary account with a credit balance before its adjustments takes that balance on its credit side
/// (TB:626-630): a $100 credit from 2023 makes Income Summary's adjusted trial balance $400 against $350.
#[test]
fn an_income_summary_account_with_a_credit_balance() {
    let mut s = merchandising_book();
    s.transfer("2023-12-01", "Opening", "Income Summary", "Bank", "100.00");
    let got = row(&s.run(TRIAL_BALANCE, &merchandising()), "Income Summary");
    assert_eq!(
        got,
        strings(&[
            "Income Summary",
            "",
            "$100.00",
            "$400.00",
            "$250.00",
            "$400.00",
            "$350.00",
            "$400.00",
            "$350.00",
            "",
            ""
        ])
    );
}

/// A rent of $300 against $1,000 invested: debits exceed credits in the Income Statement, a Net Loss in its credit
/// column (TB:821-826), and the Balance Sheet's net is negative, so in its debit column (TB:827-830); the grand totals
/// add each net to the smaller side (TB:833-850). With no income or expense the Net Loss figure is blank and the
/// Balance Sheet's net $0.00.
#[test]
fn net_loss_and_no_net_income() {
    let mut s = book_with(&["Bank:BANK", "Capital:EQUITY", "Rent:EXPENSE"]);
    s.transfer("2024-01-02", "Investment", "Capital", "Bank", "1000.00");
    let none = body(&s.run(TRIAL_BALANCE, &year("work-sheet", json!({}))));
    assert_eq!(
        none[3..],
        [
            strings(&[
                "",
                "$1,000.00",
                "$1,000.00",
                "$0.00",
                "$0.00",
                "$1,000.00",
                "$1,000.00",
                "$0.00",
                "$0.00",
                "$1,000.00",
                "$1,000.00"
            ]),
            strings(&["Net Loss", "", "", "", "", "", "", "", "", "", "$0.00"]),
            strings(&["", "", "", "", "", "", "", "$0.00", "$0.00", "$1,000.00", "$1,000.00"]),
        ]
    );
    s.transfer("2024-02-01", "Rent", "Bank", "Rent", "300.00");
    let loss = body(&s.run(TRIAL_BALANCE, &year("work-sheet", json!({}))));
    assert_eq!(
        loss[4..],
        [
            strings(&["Net Loss", "", "", "", "", "", "", "", "$300.00", "$300.00", ""]),
            strings(&["", "", "", "", "", "", "", "$300.00", "$300.00", "$1,000.00", "$1,000.00"]),
        ]
    );
}

/// The patterns book: $1,000 invested on 2024-01-02, $500 of sales on 2024-03-01, $100 of rent described
/// "ADJ: accrued rent" on 2024-06-30, and the sales closed into Capital on 2024-12-30, described "Year-end close".
/// Its balances at the end: Bank $1,400, Capital -$1,500, Rent $100, Sales 0.
fn patterns_book() -> Setup {
    let mut s = book_with(&["Bank:BANK", "Capital:EQUITY", "Rent:EXPENSE", "Sales:INCOME"]);
    s.transfer("2024-01-02", "Investment", "Capital", "Bank", "1000.00");
    s.transfer("2024-03-01", "Sale", "Sales", "Bank", "500.00");
    s.transfer("2024-06-30", "ADJ: accrued rent", "Bank", "Rent", "100.00");
    s.transfer("2024-12-30", "Year-end close", "Capital", "Sales", "500.00");
    s
}

fn entries(start: &str, entries: Value) -> Value {
    let mut options = year("pre-adj", json!({ "Entries": entries }));
    options["General"]["Start of Adjusting/Closing"] = json!({ "absolute": start });
    options
}

/// The pre-adjustment balances (TB:622-624) take out every split of a matching transaction in the window
/// (`5.17:gnucash/report/report-utilities.scm:692-725`), the bank's as well as the rent's: the adjusting pattern
/// ignoring case, case-sensitive, as a regular expression; the closing pattern; a transaction both match, taken out
/// twice; an empty pattern, which matches every description, from the window's start only.
#[test]
fn the_patterns_pick_the_entries() {
    let mut s = patterns_book();
    let none = [
        ["Sales", "", ""],
        ["Rent", "$100.00", ""],
        ["Capital", "", "$1,500.00"],
        ["Bank", "$1,400.00", ""],
    ];
    let adjusted = [
        ["Sales", "", ""],
        ["Rent", "", ""],
        ["Capital", "", "$1,500.00"],
        ["Bank", "$1,500.00", ""],
    ];
    let closed = [
        ["Sales", "", "$500.00"],
        ["Rent", "$100.00", ""],
        ["Capital", "", "$1,000.00"],
        ["Bank", "$1,400.00", ""],
    ];
    let twice = [
        ["Sales", "", "$1,000.00"],
        ["Rent", "$100.00", ""],
        ["Capital", "", "$500.00"],
        ["Bank", "$1,400.00", ""],
    ];
    let all_from_february = [
        ["Sales", "", ""],
        ["Rent", "", ""],
        ["Capital", "", "$1,000.00"],
        ["Bank", "$1,000.00", ""],
    ];
    let cases = [
        ("defaults", entries("2024-01-01", json!({})), none),
        (
            "ignoring case",
            entries("2024-01-01", json!({ "Adjusting Entries pattern": "adj:" })),
            adjusted,
        ),
        (
            "case-sensitive",
            entries(
                "2024-01-01",
                json!({ "Adjusting Entries pattern": "adj:", "Adjusting Entries pattern is case-sensitive": true }),
            ),
            none,
        ),
        (
            "regular expression",
            entries(
                "2024-01-01",
                json!({ "Adjusting Entries pattern": "^ADJ:", "Adjusting Entries Pattern is regular expression": true }),
            ),
            adjusted,
        ),
        (
            "not a regular expression",
            entries("2024-01-01", json!({ "Adjusting Entries pattern": "^ADJ:" })),
            none,
        ),
        (
            "closing",
            entries("2024-01-01", json!({ "Closing Entries pattern": "year-end" })),
            closed,
        ),
        (
            "both",
            entries(
                "2024-01-01",
                json!({ "Closing Entries pattern": "close", "Adjusting Entries pattern": "close" }),
            ),
            twice,
        ),
        (
            "empty from February",
            entries("2024-02-01", json!({ "Adjusting Entries pattern": "" })),
            all_from_february,
        ),
    ];
    cases.iter().for_each(|(name, options, want)| {
        let got = body(&s.run(TRIAL_BALANCE, options));
        assert_eq!(got[..4], want.map(|r| strings(&r)), "{name}");
    });
    let everything = body(&s.run(TRIAL_BALANCE, &entries("2024-01-01", json!({ "Adjusting Entries pattern": "" }))));
    assert!(everything[..4].iter().all(|r| r[1..] == ["", ""]), "{everything:?}");
}

/// A closing transaction is a closing entry whatever its description (the `closing` key, TB:589); the adjusting
/// pattern does not look at the flag (TB:591-596).
#[test]
fn closing_transactions_are_closing_entries() {
    let mut s = book_with(&["Bank:BANK", "Capital:EQUITY", "Sales:INCOME"]);
    s.transfer("2024-01-02", "Investment", "Capital", "Bank", "1000.00");
    s.transfer("2024-03-01", "Sale", "Sales", "Bank", "500.00");
    let close =
        json!({ "date": "2024-12-30", "incomeTotalAccount": "Capital", "expenseTotalAccount": "Capital", "description": "Year end" });
    s.call("book.closeBooks", close);
    let got = body(&s.run(TRIAL_BALANCE, &entries("2024-01-01", json!({}))));
    assert_eq!(
        got[..3],
        [["Sales", "", "$500.00"], ["Capital", "", "$1,000.00"], ["Bank", "$1,500.00", ""]].map(|r| strings(&r))
    );
    let current = body(&s.run(TRIAL_BALANCE, &year("current", json!({}))));
    assert_eq!(
        current[..3],
        [["Sales", "", ""], ["Capital", "", "$1,500.00"], ["Bank", "$1,500.00", ""]].map(|r| strings(&r))
    );
}

/// The unrealized gains of GnuCash's test book after its foreign transfers, by price source (TB:473-500): the book
/// ends at $10 - #6 (`adjusting-entries-fx.expected.json`), $0 at its average cost of $5/3. At the 9/8 price, nearest,
/// before or latest, $3.25 of gains; at the weighted average of the four exchanges, $46/#34, #6 is $8.12 and the gains
/// $1.88; at average cost there is no row (TB:476-478).
#[test]
fn unrealized_gains_by_price_source() {
    let mut s = Setup::fixture("adjusting-entries-fx.gnucash", &test_settings("iso"));
    let cases = [
        ("pricedb-nearest", Some("$3.25")),
        ("pricedb-before", Some("$3.25")),
        ("pricedb-latest", Some("$3.25")),
        ("weighted-average", Some("$1.88")),
        ("average-cost", None),
    ];
    cases.iter().for_each(|(source, want)| {
        let rows = body(&s.run(TRIAL_BALANCE, &json!({ "Commodities": { "Price Source": source } })));
        let unrealized = rows.iter().find(|r| r[0].starts_with("Unrealized"));
        assert_eq!(
            unrealized.map(|r| [r[0].as_str(), r[2].as_str()]),
            want.map(|w| ["Unrealized Gains", w]),
            "{source}"
        );
    });
}

/// In the work sheet the unrealized gains count in the adjusted trial balance and the balance sheet, on their credit
/// side, not in the adjustments or the income statement (TB:700-714).
#[test]
fn unrealized_gains_in_the_work_sheet() {
    let mut s = Setup::fixture("adjusting-entries-fx.gnucash", &test_settings("iso"));
    let options = json!({
        "General": { "Report variation": "work-sheet", "Start of Adjusting/Closing": { "absolute": "2000-01-01" } },
        "Commodities": { "Price Source": "pricedb-nearest" },
    });
    let got = row(&s.run(TRIAL_BALANCE, &options), "Unrealized Gains");
    assert_eq!(
        got,
        strings(&["Unrealized Gains", "", "$3.25", "", "", "", "$3.25", "", "", "", "$3.25"])
    );
}

/// "Show Foreign Currencies" shows a foreign balance as its amount and value (TB:157-158), the absolute value on its
/// side: GBP Bank's #606 at 9/8 and Income-GBP's -#612 as a credit.
#[test]
fn foreign_currencies_shown() {
    let mut s = Setup::fixture("adjusting-entries-fx.gnucash", &test_settings("iso"));
    let options = json!({ "Commodities": { "Price Source": "pricedb-nearest", "Show Foreign Currencies": true } });
    let result = s.run(TRIAL_BALANCE, &options);
    assert_eq!(row(&result, "GBP Bank"), strings(&["GBP Bank", "#606.00 $681.75", ""]));
    assert_eq!(row(&result, "Income-GBP"), strings(&["Income-GBP", "", "#612.00 $688.50"]));
}

/// A foreign account whose splits sum to 0 shows $0.00, one with no split is blank (TB:154-159; collectors keep zero
/// entries, `5.17:gnucash/report/report-utilities.scm:346-351`). Totals are converted once per commodity
/// (TB:446-459), so two €1.00 balances at $1.005 are $1.00 each and $2.01 together.
#[test]
fn foreign_zeros_and_rounded_totals() {
    let mut s = book_with(&[
        "Bank:BANK",
        "Capital:EQUITY",
        "Euro A:BANK:EUR",
        "Euro B:BANK:EUR",
        "Euro C:BANK:EUR",
        "Euro D:BANK:EUR",
    ]);
    s.transfer("2024-01-02", "Investment", "Capital", "Bank", "2.00");
    ["Euro A", "Euro B", "Euro C"].iter().for_each(|a| {
        let splits = json!([{ "account": a, "value": "1.00", "amount": "1.00" }, { "account": "Bank", "value": "-1.00" }]);
        s.call(
            "transaction.post",
            json!({ "datePosted": "2024-02-01", "currency": "CURRENCY::USD", "splits": splits }),
        );
    });
    let back = json!([{ "account": "Euro C", "value": "-1.00", "amount": "-1.00" }, { "account": "Bank", "value": "1.00" }]);
    s.call(
        "transaction.post",
        json!({ "datePosted": "2024-03-01", "currency": "CURRENCY::USD", "splits": back }),
    );
    let price = json!({ "commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "value": "1.005", "date": "2024-06-01" });
    s.call("price.add", price);
    let rows = body(&s.run(
        TRIAL_BALANCE,
        &year("current", json!({ "Commodities": { "Price Source": "pricedb-nearest" } })),
    ));
    assert_eq!(
        rows,
        [
            strings(&["Capital", "", "$2.00"]),
            strings(&["Bank", "", ""]),
            strings(&["Euro A", "$1.00", ""]),
            strings(&["Euro B", "$1.00", ""]),
            strings(&["Euro C", "$0.00", ""]),
            strings(&["Euro D", "", ""]),
            strings(&["Unrealized Gains", "", "$0.01"]),
            strings(&["", "$2.01", "$2.01"]),
        ]
    );
}

/// The labels are indented by their depth among the selected accounts up to the level (TB:505-508, flattened below
/// it), and the unrealized row takes the last account row's (TB:676-677).
#[test]
fn levels_of_subaccounts() {
    let mut s = Setup::fixture("adjusting-entries.gnucash", &test_settings("iso"));
    let depths = |s: &mut Setup, levels: Value| -> Vec<(String, u64)> {
        let options = json!({ "Accounts": { "Levels of Subaccounts": levels }, "Commodities": { "Price Source": "pricedb-nearest" } });
        let result = s.run(TRIAL_BALANCE, &options);
        tables(&result)[0]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["depth"].is_u64())
            .map(|r| (cell_text(&r["cells"][0]), r["depth"].as_u64().unwrap()))
            .collect()
    };
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
        "Unrealized Losses",
    ];
    let with = |d: [u64; 11]| names.iter().zip(d).map(|(n, d)| ((*n).to_owned(), d)).collect::<Vec<_>>();
    assert_eq!(depths(&mut s, json!(1)), with([0; 11]));
    assert_eq!(depths(&mut s, json!(2)), with([0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]));
    assert_eq!(depths(&mut s, json!("all")), with([0, 1, 2, 2, 2, 1, 1, 1, 1, 1, 1]));
}

/// Account labels link to their accounts unless "Display accounts as hyperlinks" is off (TB:514).
#[test]
fn hyperlinks() {
    let mut s = patterns_book();
    let label = |s: &mut Setup, links: bool| {
        let result = s.run(
            TRIAL_BALANCE,
            &year("current", json!({ "Display": { "Display accounts as hyperlinks": links } })),
        );
        tables(&result)[0]["rows"][0]["cells"][0].clone()
    };
    assert!(label(&mut s, true)["account"].is_string());
    assert!(label(&mut s, false)["account"].is_null());
}

/// The title (TB:367-378): the company name, the report title and the date of the report; the other variants name the
/// period.
#[test]
fn titles() {
    let mut s = Setup::with_config(&test_settings("iso"));
    let text = std::fs::read_to_string(format!(
        "{}/../../fixtures/reports/adjusting-entries.gnucash",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let id = "<book:id type=\"guid\">b0000000000000000000000000000000</book:id>\n";
    let slots = "<book:slots>\n  <slot>\n    <slot:key>options</slot:key>\n    <slot:value type=\"frame\">\n      <slot>\n        \
                 <slot:key>Business</slot:key>\n        <slot:value type=\"frame\">\n          <slot>\n            \
                 <slot:key>Company Name</slot:key>\n            <slot:value type=\"string\">Acme Ltd</slot:value>\n          \
                 </slot>\n        </slot:value>\n      </slot>\n    </slot:value>\n  </slot>\n</book:slots>\n";
    std::fs::write(s.book(), text.replacen(id, &format!("{id}{slots}"), 1)).unwrap();
    let book = s.book().display().to_string();
    s.call("book.open", json!({ "path": book, "readOnly": true }));
    let current = s.run(TRIAL_BALANCE, &json!({}));
    assert_eq!(current["title"], "Acme Ltd Trial Balance 2024-03-15");
    let pre_adj = json!({ "Report variation": "pre-adj", "Start of Adjusting/Closing": { "absolute": "2000-01-01" } });
    let pre = s.run(TRIAL_BALANCE, &json!({ "General": pre_adj }));
    assert_eq!(pre["title"], "Acme Ltd Trial Balance For Period Covering 2000-01-01 to 2024-03-15");
}

/// Running every variant leaves the Book, its revision and its file as they were and writes nothing beside it, on XML
/// and SQLite; two exports are the same bytes, with no script.
#[test]
fn a_run_changes_nothing() {
    let settings = test_settings("iso");
    let books = [
        ("xml", Setup::fixture("adjusting-entries-fx.gnucash", &settings)),
        ("sqlite", Setup::sqlite_fixture("adjusting-entries-fx.gnucash", &settings)),
    ];
    for (format, mut s) in books {
        let bytes = std::fs::read(s.book()).unwrap();
        let before = s.d.context().book().unwrap().clone();
        let revision = s.d.context().session().unwrap().revision();
        let files = s.files();
        ["current", "pre-adj", "work-sheet"].iter().for_each(|variant| {
            let options = json!({
                "General": { "Report variation": variant, "Start of Adjusting/Closing": { "absolute": "2000-01-01" } },
                "Commodities": { "Show Exchange Rates": true, "Show Foreign Currencies": true, "Price Source": "pricedb-nearest" },
            });
            s.run(TRIAL_BALANCE, &options);
            let (a, b) = (s.export(TRIAL_BALANCE, &options), s.export(TRIAL_BALANCE, &options));
            assert_eq!(a, b, "{format} {variant}");
            assert!(!a.to_ascii_lowercase().contains("<script"), "{format}");
        });
        assert_eq!(s.d.context().book().unwrap(), &before, "{format}");
        assert_eq!(s.d.context().session().unwrap().revision(), revision, "{format}");
        assert_eq!(s.call("book.info", json!({}))["dirty"], false, "{format}");
        assert_eq!(s.files(), files, "{format}: no journal or other file was written");
        assert_eq!(std::fs::read(s.book()).unwrap(), bytes, "{format}: the file is unchanged");
    }
}
