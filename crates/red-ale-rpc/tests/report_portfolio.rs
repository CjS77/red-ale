//! The Investment Portfolio (`standard/portfolio.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/portfolio.scm`, cited as PF) through `report.run` and `report.export`: the
//! checks of GnuCash's own test (`standard/test/test-portfolios.scm`, cited as TP) on
//! `fixtures/reports/stock-test-data.gnucash` and its SQLite copy, and hand-worked cases for the branches that test
//! does not reach, on the same book and on books built through the API.
//!
//! `stock-test-data` is `create-stock-test-data` (`5.17:bindings/guile/test/test-engine-extras.scm:952-1059`): AAPL
//! (NASDAQ, SCU 1) is bought, sold, split 1:10 twice (1980-10-01, 1980-11-01) and 3:1 in reverse with cash in lieu
//! (1980-12-01), bought again and sold, so it holds 1 share from 1980-05-01, 10 from 1980-10-01, 100 from 1980-11-01,
//! 33 from 1980-12-01, 43 from 1981-01-01 and 42 from 1981-03-01. Its prices in USD, at midnight UTC: $100
//! (1980-02-01), $200 (1980-03-01), $400 (1980-05-01), $40, $4 and $12 at the three splits, $50 (1981-01-01) and $6
//! (1981-03-01).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use serde_json::{Value, json};
use statements::{Setup, blocks, expected, expected_run, options_param, tables, test_settings};

const PORTFOLIO: &str = "Investment Portfolio";
const FIXTURE: &str = "stock-test-data.gnucash";

/// The fixture and its SQLite copy, each opened read-only, with GnuCash's default date format (TP sets none).
fn both_formats() -> [(&'static str, Setup); 2] {
    [
        ("xml", Setup::fixture(FIXTURE, &test_settings("locale"))),
        ("sqlite", Setup::sqlite_fixture(FIXTURE, &test_settings("locale"))),
    ]
}

/// A cell's text as GnuCash's test reads it: a text cell's text, else the display form; nothing for an empty cell.
fn cell_texts(cell: &Value) -> Vec<String> {
    if cell.is_null() {
        return vec![];
    }
    let text = cell["text"].as_str().or_else(|| cell["display"].as_str()).unwrap();
    vec![text.to_owned()]
}

/// The rows of the portfolio table, each as `role: texts`.
fn rows(result: &Value) -> Vec<String> {
    tables(result)[0]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let texts: Vec<String> = r["cells"].as_array().unwrap().iter().flat_map(cell_texts).collect();
            format!("{}: {}", r["role"].as_str().unwrap_or("data"), texts.join(" | "))
        })
        .collect()
}

/// The texts of the table's first row, as `sxml->table-row-col` reads row 1 of table 1.
fn first_row(result: &Value) -> Vec<String> {
    tables(result)[0]["rows"][0]["cells"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(cell_texts)
        .collect()
}

/// One check of TP: row 1 of the run `id` of `stock-test-data.expected.json` holds `want`.
struct Ported {
    id: &'static str,
    name: &'static str,
    line: u64,
    want: [&'static str; 6],
}

fn ported() -> [Ported; 4] {
    let p = |id, name, line, want| Ported { id, name, line, want };
    [
        p(
            "portfolio-latest",
            "portfolio: pricedb-latest",
            56,
            ["AAPL", "AAPL", "NASDAQ", "42.00", "$6.00", "$252.00"],
        ),
        p(
            "portfolio-nearest",
            "portfolio: pricedb-nearest",
            63,
            ["AAPL", "AAPL", "NASDAQ", "2.00", "$200.00", "$400.00"],
        ),
        p(
            "portfolio-average-cost",
            "portfolio: average-cost",
            70,
            ["AAPL", "AAPL", "NASDAQ", "1.00", "$200.00", "$200.00"],
        ),
        p(
            "portfolio-weighted-average",
            "portfolio: weighted-average",
            76,
            ["AAPL", "AAPL", "NASDAQ", "1.00", "$233.33", "$233.33"],
        ),
    ]
}

/// Every check of TP:53-78 on the XML fixture and its SQLite copy, through `report.run` and `report.export`.
#[test]
fn expected_figures_of_gnucash_test() {
    let e = expected("stock-test-data.expected.json");
    for (format, mut s) in both_formats() {
        ported().iter().for_each(|c| {
            let options = options_param(expected_run(&e, c.id));
            let result = s.run(PORTFOLIO, &options);
            assert_eq!(
                first_row(&result),
                c.want,
                "{format}: {} (test line {}): {:#?}",
                c.name,
                c.line,
                rows(&result)
            );
            let html = s.export(PORTFOLIO, &options);
            c.want
                .iter()
                .for_each(|w| assert!(html.contains(w), "{format}: {} (test line {}): {w} is in the HTML", c.name, c.line));
        });
    }
}

/// The checks above are exactly the Investment Portfolio's checks of `stock-test-data.expected.json`, with its names,
/// lines and strings; its null test is `the_null_test_renders`.
#[test]
fn every_check_of_the_expected_figures_is_ported() {
    let e = expected("stock-test-data.expected.json");
    let runs: Vec<&Value> = e["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["template"] == PORTFOLIO)
        .collect();
    let theirs: Vec<Value> = runs
        .iter()
        .flat_map(|r| {
            r["checks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| json!([r["id"], c["name"], c["line"], c["select"], c["expected"]]))
        })
        .collect();
    let ours: Vec<Value> = ported()
        .iter()
        .map(|c| json!([c.id, c.name, c.line, { "table": 1, "row": 1, "column": null }, c.want]))
        .collect();
    assert_eq!(ours, theirs);
    let null_tests: Vec<&Value> = e["renderOnly"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["template"] == PORTFOLIO)
        .collect();
    assert_eq!(null_tests.len(), 1);
    assert_eq!(null_tests[0]["line"], 26);
}

/// TP:26 renders the report with its defaults on an empty book: no STOCK or MUTUAL account, so the default account
/// list is empty and the report is the no-account warning (PF:276-281) under the title, Report name and the date.
#[test]
fn the_null_test_renders() {
    let mut s = Setup::new_book(&test_settings("locale"));
    let result = s.run(PORTFOLIO, &json!({}));
    let b = blocks(&result);
    assert_eq!(b.len(), 1);
    assert_eq!(
        (b[0]["kind"].as_str(), b[0]["title"].as_str()),
        (Some("warning"), Some("No accounts selected"))
    );
    assert_eq!(result["title"], "Investment Portfolio 03/15/24");
    assert!(s.export(PORTFOLIO, &json!({})).contains("No accounts selected"));
}

/// Every row of the latest run of TP:56 (PF:242-273): AAPL's row, linked to its account and its price; a rule over
/// the six columns; one "Total" row, the figure spanning five columns. MSFT and TSLA, selected by default as STOCK
/// accounts (PF:61-68), hold no shares and are left out.
#[test]
fn rows_rule_and_total() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("locale"));
    let result = s.run(PORTFOLIO, &json!({}));
    assert_eq!(
        rows(&result),
        [
            "data: AAPL | AAPL | NASDAQ | 42.00 | $6.00 | $252.00",
            "rule: ",
            "grandTotal: Total | $252.00"
        ]
    );
    let table = tables(&result)[0];
    let labels: Vec<&str> = table["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["Account", "Symbol", "Listing", "Units", "Price", "Value"]);
    let row = &table["rows"][0];
    assert_eq!(row["cells"][0]["account"], "a0000000000000000000000000000008");
    assert_eq!(row["cells"][4]["link"], json!({ "price": "e0000000000000000000000000000007" }));
    assert_eq!(row["cells"][3]["numberFormat"], "share");
    assert_eq!(table["rows"][2]["cells"][1]["span"], 5);
    assert_eq!(result["title"], "Investment Portfolio 03/15/24");
}

/// "Include accounts with no shares" (PF:128) shows MSFT and TSLA, which have no price: `(#f . 0)`, so their price
/// is $0.00 in the report's currency, unlinked, and their value 0. Rows alternate from `normal-row` (PF:150).
#[test]
fn accounts_with_no_shares() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("locale"));
    let result = s.run(PORTFOLIO, &json!({ "Accounts": { "Include accounts with no shares": true } }));
    assert_eq!(
        rows(&result),
        [
            "data: AAPL | AAPL | NASDAQ | 42.00 | $6.00 | $252.00",
            "data: MSFT | MSFT | NASDAQ | 0.00 | $0.00 | $0.00",
            "data: TSLA | TSLA | NASDAQ | 0.00 | $0.00 | $0.00",
            "rule: ",
            "grandTotal: Total | $252.00",
        ]
    );
    let table = tables(&result)[0];
    let alternate: Vec<&Value> = table["rows"].as_array().unwrap()[..3].iter().map(|r| &r["alternate"]).collect();
    assert_eq!(alternate, [false, true, false]);
    assert!(table["rows"][1]["cells"][4]["link"].is_null());
}

/// The price-database sources on 1980-12-25, when AAPL holds 33 shares: "before" takes the newest price at or
/// before local noon of the Date, the $12 of the 1980-12-01 split (PF:219-229), and values at the price at or before
/// the end of the day; "nearest" takes the $50 of 1981-01-01, 6½ days on, and so values. "Most recent" ignores the
/// Date and takes the newest price at or before now, the $6 of 1981-03-01, for both (PF:201-218,
/// `5.17:gnucash/report/commodity-utilities.scm:686-699`).
#[test]
fn price_database_sources() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("locale"));
    let at = |source: &str| json!({ "General": { "Date": { "absolute": "1980-12-25" }, "Price Source": source } });
    let cases = [
        ("pricedb-before", ["AAPL", "AAPL", "NASDAQ", "33.00", "$12.00", "$396.00"]),
        ("pricedb-nearest", ["AAPL", "AAPL", "NASDAQ", "33.00", "$50.00", "$1,650.00"]),
        ("pricedb-latest", ["AAPL", "AAPL", "NASDAQ", "33.00", "$6.00", "$198.00"]),
    ];
    cases.iter().for_each(|(source, want)| {
        let result = s.run(PORTFOLIO, &at(source));
        assert_eq!(first_row(&result), want, "{source}");
        assert_eq!(result["title"], "Investment Portfolio 12/25/80", "{source}");
    });
}

/// A book of NYSE shares with an SCU of 1,000, held under Broker and bought from Bank on 2024-01-10: 10 XYZ for
/// $12,345.67, 0.125 ABC for $5.00 and 5 NOP for $50.00; EMP is never bought. XYZ is priced $1,200 on 2024-02-01 and
/// €1,100 on 2024-03-01; ABC only the other way round, USD at 0.025 ABC on 2024-02-01 ($40 an ABC); NOP and EMP
/// have no price.
fn shares_book() -> Setup {
    let mut s = Setup::new_book(&test_settings("locale"));
    s.account("Bank", "BANK", None, "CURRENCY::USD");
    s.account("Broker", "ASSET", None, "CURRENCY::USD");
    ["XYZ", "EMP", "ABC", "NOP"].iter().for_each(|m| {
        s.call(
            "commodity.create",
            json!({ "namespace": "NYSE", "mnemonic": m, "fullname": m, "fraction": 1000 }),
        );
        s.account(m, "STOCK", Some("Broker"), &format!("NYSE::{m}"));
    });
    [("XYZ", "10", "12345.67"), ("ABC", "0.125", "5.00"), ("NOP", "5", "50.00")]
        .iter()
        .for_each(|(m, units, cost)| {
            let splits = json!([{ "account": format!("Broker:{m}"), "value": cost, "amount": units },
                            { "account": "Bank", "value": format!("-{cost}") }]);
            s.call(
                "transaction.post",
                json!({ "datePosted": "2024-01-10", "currency": "CURRENCY::USD", "splits": splits }),
            );
        });
    [
        ("NYSE::XYZ", "CURRENCY::USD", "1200", "2024-02-01"),
        ("NYSE::XYZ", "CURRENCY::EUR", "1100", "2024-03-01"),
        ("CURRENCY::USD", "NYSE::ABC", "0.025", "2024-02-01"),
    ]
    .iter()
    .for_each(|(commodity, currency, value, date)| {
        s.call(
            "price.add",
            json!({ "commodity": commodity, "currency": currency, "value": value, "date": date }),
        );
    });
    s
}

/// `shares_book`'s accounts in the option's order, at `date` with `source`, and the `more` options.
fn shares_options(source: &str, date: &str, more: Value) -> Value {
    let mut options = json!({
        "General": { "Date": { "absolute": date }, "Price Source": source },
        "Accounts": { "Accounts": ["Broker:XYZ", "Broker:EMP", "Broker:ABC", "Broker:NOP"] },
    });
    more.as_object().unwrap().iter().for_each(|(section, values)| {
        values
            .as_object()
            .unwrap()
            .iter()
            .for_each(|(k, v)| options[section][k] = v.clone());
    });
    options
}

/// The price cell shows the price as the price database holds it (PF:116-119), with every price-database source:
/// "Most recent" takes the first of the latest-any-currency list, which is newest first, so XYZ's euro price, and
/// ABC's price of USD in ABC as stored, 0.025 ABC, though PF:206-215 inverts it for a value it never shows; "nearest"
/// and "before" never invert (PF:219-240), which the note's Q9 keeps. The values convert through the price database
/// into the report's currency, inverting where needed: 10 × $1,200 and 0.125 × $40. NOP has no price, so its price is
/// $0.00 and its value 0; EMP holds nothing and is left out.
#[test]
fn prices_as_stored_in_any_currency() {
    let mut s = shares_book();
    let want = [
        "data: XYZ | XYZ | NYSE | 10.00 | €1,100.00 | $12,000.00",
        "data: ABC | ABC | NYSE | 0.13 | 0.025 ABC | $5.00",
        "data: NOP | NOP | NYSE | 5.00 | $0.00 | $0.00",
        "rule: ",
        "grandTotal: Total | $12,005.00",
    ];
    ["pricedb-latest", "pricedb-nearest", "pricedb-before"].iter().for_each(|source| {
        let result = s.run(PORTFOLIO, &shares_options(source, "2024-03-10", json!({})));
        assert_eq!(rows(&result), want, "{source}");
    });
    let html = s.export(PORTFOLIO, &shares_options("pricedb-latest", "2024-03-10", json!({})));
    ["€1,100.00", "0.025 ABC", "$12,005.00"]
        .iter()
        .for_each(|w| assert!(html.contains(w), "{w} is in the HTML"));
}

/// On 2024-02-20 the euro price is in the future: "before" finds only the $1,200 of 2024-02-01; "nearest" lists each
/// currency's nearest price, newest first (`5.17:libgnucash/engine/gnc-pricedb.cpp:1962-1980`), and takes the first,
/// the euro price, though the dollar price is nearer. The values take the dollar price either way.
#[test]
fn nearest_takes_the_newest_of_each_currency_s_nearest() {
    let mut s = shares_book();
    let before = s.run(PORTFOLIO, &shares_options("pricedb-before", "2024-02-20", json!({})));
    assert_eq!(rows(&before)[0], "data: XYZ | XYZ | NYSE | 10.00 | $1,200.00 | $12,000.00");
    let nearest = s.run(PORTFOLIO, &shares_options("pricedb-nearest", "2024-02-20", json!({})));
    assert_eq!(rows(&nearest)[0], "data: XYZ | XYZ | NYSE | 10.00 | €1,100.00 | $12,000.00");
}

/// The averages price 10,000 units through the exchange function and divide by 10,000, to five significant figures
/// (PF:191-200; `sigfigs_denom`, `5.17:libgnucash/engine/gnc-numeric.cpp:468-486`): XYZ cost $1,234.567 a share, so
/// its price is $1,234.6, shown $1,234.60, while its value is the exact 10 × 1,234.567. ABC cost $40, NOP $10. With
/// one purchase each, average cost and the weighted average agree. No price is linked.
#[test]
fn averages_round_the_price_to_five_figures() {
    let mut s = shares_book();
    ["average-cost", "weighted-average"].iter().for_each(|source| {
        let result = s.run(PORTFOLIO, &shares_options(source, "2024-03-10", json!({})));
        assert_eq!(
            rows(&result),
            [
                "data: XYZ | XYZ | NYSE | 10.00 | $1,234.60 | $12,345.67",
                "data: ABC | ABC | NYSE | 0.13 | $40.00 | $5.00",
                "data: NOP | NOP | NYSE | 5.00 | $10.00 | $50.00",
                "rule: ",
                "grandTotal: Total | $12,400.67",
            ],
            "{source}"
        );
        assert!(tables(&result)[0]["rows"][0]["cells"][4]["link"].is_null(), "{source}");
    });
}

/// The units of the first three rows with `places` share decimal places.
fn units_with(s: &mut Setup, places: u32) -> Vec<String> {
    let more = json!({ "General": { "Share decimal places": places } });
    let result = s.run(PORTFOLIO, &shares_options("pricedb-latest", "2024-03-10", more));
    tables(&result)[0]["rows"].as_array().unwrap()[..3]
        .iter()
        .map(|r| r["cells"][3]["display"].as_str().unwrap().to_owned())
        .collect()
}

/// "Share decimal places" (PF:99-101; `gnc_share_print_info_places`, `5.17:libgnucash/app-utils/gnc-ui-util.cpp:1013-1023`):
/// exactly that many decimals, rounded half away from zero, the point always printed: 0.125 ABC is `0.13` with two
/// places, `0.125` with three and `0.` with none, where XYZ is `10.`.
#[test]
fn share_decimal_places() {
    let mut s = shares_book();
    assert_eq!(units_with(&mut s, 2), ["10.00", "0.13", "5.00"]);
    assert_eq!(units_with(&mut s, 3), ["10.000", "0.125", "5.000"]);
    assert_eq!(units_with(&mut s, 0), ["10.", "0.", "5."]);
}

/// The shade of each data row.
fn shades(result: &Value) -> Vec<Value> {
    tables(result)[0]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["role"].is_null() || r["role"] == "data")
        .map(|r| r["alternate"].clone())
        .collect()
}

/// Rows alternate over the shown rows only (PF:107-150): with EMP left out ABC takes the second shade; with it shown,
/// EMP does, and its price and value are 0.
#[test]
fn rows_alternate_over_shown_rows() {
    let mut s = shares_book();
    let hidden = s.run(PORTFOLIO, &shares_options("pricedb-latest", "2024-03-10", json!({})));
    assert_eq!(shades(&hidden), [false, true, false]);
    let more = json!({ "Accounts": { "Include accounts with no shares": true } });
    let shown = s.run(PORTFOLIO, &shares_options("pricedb-latest", "2024-03-10", more));
    assert_eq!(shades(&shown), [false, true, false, true]);
    assert_eq!(rows(&shown)[1], "data: EMP | EMP | NYSE | 0.00 | $0.00 | $0.00");
}

/// With every selected account empty the table is the rule alone: the collector holds nothing, so there is no Total
/// row (PF:255-273).
#[test]
fn no_shown_rows_leave_the_rule() {
    let mut s = shares_book();
    let options = json!({ "Accounts": { "Accounts": ["Broker:EMP"] } });
    assert_eq!(rows(&s.run(PORTFOLIO, &options)), ["rule: "]);
}

/// The options of PF:39-76 in GnuCash's order, with the General options every template gets. With no book open the
/// default account list is empty.
#[test]
fn options_are_gnucash_s() {
    let mut s = Setup::with_config(&test_settings("locale"));
    let described = s.call("report.options", json!({ "report": PORTFOLIO }));
    let got: Vec<String> = described["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}/{} {} {} {}",
                o["section"].as_str().unwrap(),
                o["name"].as_str().unwrap(),
                o["key"],
                o["type"],
                o["default"]
            )
        })
        .collect();
    let want = [
        r#"Accounts/Accounts "b" "accountList" []"#,
        r#"Accounts/Include accounts with no shares "e" "boolean" false"#,
        r#"General/Report name "0a" "string" "Investment Portfolio""#,
        r#"General/Stylesheet "0b" "multichoice" "Default""#,
        r#"General/Add options summary "0c" "multichoice" "never""#,
        r#"General/Date "a" "date" {"relative":"end-accounting-period"}"#,
        r#"General/Report's currency "c" "currency" "CURRENCY::USD""#,
        r#"General/Price Source "d" "multichoice" "pricedb-latest""#,
        r#"General/Share decimal places "e" "numberRange" 2"#,
    ];
    assert_eq!(got, want);
    let mut s = Setup::fixture(FIXTURE, &test_settings("locale"));
    let described = s.call("report.options", json!({ "report": PORTFOLIO }));
    let accounts = described["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "Accounts")
        .unwrap()
        .clone();
    let aapl_msft_tsla = [
        "a0000000000000000000000000000008",
        "a0000000000000000000000000000009",
        "a000000000000000000000000000000a",
    ];
    assert_eq!(
        accounts["default"],
        json!(aapl_msft_tsla),
        "STOCK and MUTUAL accounts in display order"
    );
}

/// Running the report with each price source leaves the Book, its revision and its file as they were and writes
/// nothing beside it, on XML and SQLite; two exports are the same bytes, with no script.
#[test]
fn a_run_changes_nothing() {
    for (format, mut s) in both_formats() {
        let bytes = std::fs::read(s.book()).unwrap();
        let before = s.d.context().book().unwrap().clone();
        let revision = s.d.context().session().unwrap().revision();
        let files = s.files();
        [
            "pricedb-latest",
            "pricedb-nearest",
            "pricedb-before",
            "average-cost",
            "weighted-average",
        ]
        .iter()
        .for_each(|source| {
            let options = json!({ "General": { "Price Source": source }, "Accounts": { "Include accounts with no shares": true } });
            s.run(PORTFOLIO, &options);
            let (a, b) = (s.export(PORTFOLIO, &options), s.export(PORTFOLIO, &options));
            assert_eq!(a, b, "{format} {source}");
            assert!(!a.to_ascii_lowercase().contains("<script"), "{format}");
        });
        assert_eq!(s.d.context().book().unwrap(), &before, "{format}");
        assert_eq!(s.d.context().session().unwrap().revision(), revision, "{format}");
        assert_eq!(s.call("book.info", json!({}))["dirty"], false, "{format}");
        assert_eq!(s.files(), files, "{format}: no journal or other file was written");
        assert_eq!(std::fs::read(s.book()).unwrap(), bytes, "{format}: the file is unchanged");
    }
}
