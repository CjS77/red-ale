//! The Advanced Portfolio (`standard/advanced-portfolio.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/advanced-portfolio.scm`, cited as AP) through `report.run` and
//! `report.export`: the checks of GnuCash's own test (`standard/test/test-portfolios.scm`, cited as TP) on
//! `fixtures/reports/stock-test-data.gnucash` and its SQLite copy, and hand-worked cases for the branches that test
//! does not reach, on books built through the API. Figures are worked from AP by hand
//! (`design/notes/report-investments.md` §3).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use serde_json::{Value, json};
use statements::{Setup, blocks, expected, expected_run, options_param, tables, test_settings};

const ADVANCED: &str = "Advanced Portfolio";
const FIXTURE: &str = "stock-test-data.gnucash";

/// The fixture and its SQLite copy, each opened read-only, with GnuCash's default date format (TP sets none).
fn both_formats() -> [(&'static str, Setup); 2] {
    [
        ("xml", Setup::fixture(FIXTURE, &test_settings("locale"))),
        ("sqlite", Setup::sqlite_fixture(FIXTURE, &test_settings("locale"))),
    ]
}

/// A cell's texts as GnuCash's test reads them (`sxml->table-row-col` trims whitespace, so the marker `" "` gives
/// none): a text cell's text, else the display form; nothing for an empty cell.
fn cell_texts(cell: &Value) -> Vec<String> {
    if cell.is_null() {
        return vec![];
    }
    let text = cell["text"].as_str().or_else(|| cell["display"].as_str()).unwrap().trim();
    if text.is_empty() { vec![] } else { vec![text.to_owned()] }
}

/// The rows of the table, each as `role: texts`.
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
    want: [&'static str; 17],
}

fn ported() -> [Ported; 3] {
    let p = |id, name, line, want| Ported { id, name, line, want };
    [
        p(
            "advanced-average",
            "advanced: average basis",
            87,
            [
                "AAPL", "AAPL", "NASDAQ", "42.00", "$6.00", "$484.88", "$252.00", "$800.00", "$543.00", "$227.88", "-$232.88", "-$5.00",
                "-0.63%", "$4.00", "$10.00", "-$1.00", "-0.13%",
            ],
        ),
        p(
            "advanced-fifo",
            "advanced: fifo basis",
            95,
            [
                "AAPL", "AAPL", "NASDAQ", "42.00", "$6.00", "$543.94", "$252.00", "$800.00", "$543.00", "$286.94", "-$291.94", "-$5.00",
                "-0.63%", "$4.00", "$10.00", "-$1.00", "-0.13%",
            ],
        ),
        p(
            "advanced-filo",
            "advanced: filo basis",
            103,
            [
                "AAPL", "AAPL", "NASDAQ", "42.00", "$6.00", "$400.00", "$252.00", "$800.00", "$543.00", "$143.00", "-$148.00", "-$5.00",
                "-0.63%", "$4.00", "$10.00", "-$1.00", "-0.13%",
            ],
        ),
    ]
}

/// Every check of TP:84-107 on the XML fixture and its SQLite copy, through `report.run` and `report.export`.
#[test]
fn expected_figures_of_gnucash_test() {
    let e = expected("stock-test-data.expected.json");
    for (format, mut s) in both_formats() {
        ported().iter().for_each(|c| {
            let options = options_param(expected_run(&e, c.id));
            let result = s.run(ADVANCED, &options);
            assert_eq!(
                first_row(&result),
                c.want,
                "{format}: {} (test line {}): {:#?}",
                c.name,
                c.line,
                rows(&result)
            );
            let html = s.export(ADVANCED, &options);
            c.want
                .iter()
                .for_each(|w| assert!(html.contains(w), "{format}: {} (test line {}): {w} is in the HTML", c.name, c.line));
        });
    }
}

/// The checks above are exactly the Advanced Portfolio's checks of `stock-test-data.expected.json`, with its names,
/// lines and strings; its null test is `the_null_test_renders`.
#[test]
fn every_check_of_the_expected_figures_is_ported() {
    let e = expected("stock-test-data.expected.json");
    let runs: Vec<&Value> = e["runs"].as_array().unwrap().iter().filter(|r| r["template"] == ADVANCED).collect();
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
        .filter(|r| r["template"] == ADVANCED)
        .collect();
    assert_eq!(null_tests.len(), 1);
    assert_eq!(null_tests[0]["line"], 27);
}

/// TP:27 renders the report with its defaults on an empty book: no STOCK or MUTUAL account, so the default account
/// list is empty and the report is the no-account warning (AP:1175-1179) under the title, Report name and the date.
#[test]
fn the_null_test_renders() {
    let mut s = Setup::new_book(&test_settings("locale"));
    let result = s.run(ADVANCED, &json!({}));
    let b = blocks(&result);
    assert_eq!(b.len(), 1);
    assert_eq!(
        (b[0]["kind"].as_str(), b[0]["title"].as_str()),
        (Some("warning"), Some("No accounts selected"))
    );
    assert_eq!(result["title"], "Advanced Portfolio 03/15/24");
    assert!(s.export(ADVANCED, &json!({})).contains("No accounts selected"));
}

/// The texts of the notes after the table, one string per text block.
fn notes(result: &Value) -> Vec<String> {
    blocks(result)
        .iter()
        .filter(|b| b["kind"] == "text")
        .map(|b| b["text"].as_str().unwrap().to_owned())
        .collect()
}

const NOTE_TXN: &str = "* this commodity data was built using transaction pricing instead of the price list.\n\
                        If you are in a multi-currency situation, the exchanges may not be correct.";
const NOTE_NO_PRICE: &str = "** this commodity has no price and a price of 1 has been used.";

/// Every row of the defaults (AP:1036-1160): AAPL's row, linked to its account and its price, the marker `" "`; a
/// rule over the 18 columns; the "Total" row, `" "` under Symbol, Listing, Shares, Price and the marker, then the
/// totals through the exchange function. MSFT and TSLA hold no shares and are left out, so no note follows.
#[test]
fn rows_rule_and_total() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("locale"));
    let result = s.run(ADVANCED, &json!({}));
    assert_eq!(
        rows(&result),
        [
            "data: AAPL | AAPL | NASDAQ | 42.00 | $6.00 | $484.88 | $252.00 | $800.00 | $543.00 | $227.88 | -$232.88 | \
             -$5.00 | -0.63% | $4.00 | $10.00 | -$1.00 | -0.13%",
            "rule: ",
            "grandTotal: Total | $484.88 | $252.00 | $800.00 | $543.00 | $227.88 | -$232.88 | -$5.00 | -0.63% | $4.00 | \
             $10.00 | -$1.00 | -0.13%",
        ]
    );
    let table = tables(&result)[0];
    let labels: Vec<&str> = table["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        [
            "Account",
            "Symbol",
            "Listing",
            "Shares",
            "Price",
            " ",
            "Basis",
            "Value",
            "Money In",
            "Money Out",
            "Realized Gain",
            "Unrealized Gain",
            "Total Gain",
            "Rate of Gain",
            "Income",
            "Brokerage Fees",
            "Total Return",
            "Rate of Return",
        ]
    );
    let row = &table["rows"][0];
    assert_eq!(row["cells"][0]["account"], "a0000000000000000000000000000008");
    assert_eq!(row["cells"][4]["link"], json!({ "price": "e0000000000000000000000000000007" }));
    assert_eq!(
        row["cells"][4]["currency"], "CURRENCY::USD",
        "a price cell (gnc:default-price-renderer, AP:900-903)"
    );
    assert_eq!(row["cells"][5]["text"], " ");
    assert_eq!(row["cells"][3]["numberFormat"], "share");
    assert_eq!(table["rows"][1]["cells"].as_array().unwrap().len(), 18);
    assert!(notes(&result).is_empty());
    assert_eq!(result["title"], "Advanced Portfolio 03/15/24");
}

/// The rates are exact (AP:914-922, :928-936): −$5.00 and −$1.00 on $800.00 are −5/8 % and −1/8 %, ties that Guile's
/// `~,2f` rounds away from zero, as the number cell does with `halfUp`.
#[test]
fn rates_are_exact_and_round_half_up() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("locale"));
    let result = s.run(ADVANCED, &json!({}));
    let cells = &tables(&result)[0]["rows"][0]["cells"];
    assert_eq!(cells[13]["number"], json!({ "num": "-5", "denom": "8" }));
    assert_eq!(cells[17]["number"], json!({ "num": "-1", "denom": "8" }));
    assert_eq!(
        (&cells[13]["rounding"], &cells[13]["suffix"], &cells[13]["places"]),
        (&json!("halfUp"), &json!("%"), &json!(2))
    );
}

/// "Set preference for price list data" off (AP:448-480): the price is the newest split with two commodities up to
/// the Date with units and value, the sale of 1981-03-01 at $3.00 a share (the gain split has no units), shown as a
/// Monetary linked to its transaction and marked `*`; the value is 42 × $3.00, and the first note follows
/// (AP:1163-1167). Total gain −$131.00 and return −$127.00 on $800.00 are −16.375 % and −15.875 %, rounded away from
/// zero.
#[test]
fn transaction_pricing_without_the_price_list() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("locale"));
    let result = s.run(ADVANCED, &json!({ "General": { "Set preference for price list data": false } }));
    assert_eq!(
        rows(&result)[0],
        "data: AAPL | AAPL | NASDAQ | 42.00 | $3.00 | * | $484.88 | $126.00 | $800.00 | $543.00 | $227.88 | -$358.88 | \
         -$131.00 | -16.38% | $4.00 | $10.00 | -$127.00 | -15.88%"
    );
    let price = &tables(&result)[0]["rows"][0]["cells"][4];
    assert_eq!(price["link"], json!({ "transaction": "c0000000000000000000000000000008" }));
    assert!(price["monetary"].is_object());
    assert_eq!(notes(&result), [NOTE_TXN]);
    // The search ends at the Date for "Nearest to report date" and at now for "Most recent" (AP:452-458): on
    // 1980-12-25 the splits since the sale of 1980-05-01 have no value, so its $400 is the price; now finds the $3.
    let at = |source: &str| {
        json!({ "General": { "Date": { "absolute": "1980-12-25" }, "Price Source": source,
                             "Set preference for price list data": false } })
    };
    let price_at = |s: &mut Setup, source: &str| tables(&s.run(ADVANCED, &at(source)))[0]["rows"][0]["cells"][4]["display"].clone();
    assert_eq!(
        (price_at(&mut s, "pricedb-nearest"), price_at(&mut s, "pricedb-latest")),
        (json!("$400.00"), json!("$3.00"))
    );
}

/// "Include accounts with no shares" (AP:852): MSFT and TSLA have no price and no transaction to take one from, so
/// their price is 1 in the report's currency, unlinked and marked `**` (AP:482-490), their figures zero and their
/// rates empty (no money in); the second note follows. With the price list off as well, AAPL's `*` note comes first
/// and the two are a line apart (AP:1169-1172). Rows alternate from `normal-row`.
#[test]
fn accounts_with_no_shares_have_no_price() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("locale"));
    let result = s.run(ADVANCED, &json!({ "Accounts": { "Include accounts with no shares": true } }));
    let zero = "$0.00 | $0.00 | $0.00 | $0.00 | $0.00 | $0.00 | $0.00 | $0.00 | $0.00 | $0.00";
    assert_eq!(
        rows(&result)[1..3],
        [
            format!("data: MSFT | MSFT | NASDAQ | 0.00 | $1.00 | ** | {zero}"),
            format!("data: TSLA | TSLA | NASDAQ | 0.00 | $1.00 | ** | {zero}")
        ]
    );
    let table = tables(&result)[0];
    let alternate: Vec<&Value> = table["rows"].as_array().unwrap()[..3].iter().map(|r| &r["alternate"]).collect();
    assert_eq!(alternate, [false, true, false]);
    assert!(table["rows"][1]["cells"][4]["link"].is_null());
    assert!(table["rows"][1]["cells"][13].is_null() && table["rows"][1]["cells"][17].is_null());
    assert_eq!(notes(&result), [NOTE_NO_PRICE]);
    let both =
        json!({ "General": { "Set preference for price list data": false }, "Accounts": { "Include accounts with no shares": true } });
    let result = s.run(ADVANCED, &both);
    assert_eq!(notes(&result), [format!("{NOTE_TXN}\n{NOTE_NO_PRICE}")]);
    let html = s.export(ADVANCED, &both);
    assert!(
        html.contains("instead of the price list.<br>If you are") && html.contains("not be correct.<br>** this"),
        "{html}"
    );
}

/// The two price sources on 1980-12-25, when AAPL holds 33 shares at an average $4.54545455 after the reverse split
/// (basis $150.00): "Nearest to report date" takes the $50 of 1981-01-01, 6½ days on; "Most recent" ignores the Date
/// and takes the $6 of 1981-03-01 (AP:1011-1025). Money in $300.00, out $390.00, realized $240.00, income the $4.00 of
/// cash in lieu, fees $10.00.
#[test]
fn price_sources() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("locale"));
    let at = |source: &str| json!({ "General": { "Date": { "absolute": "1980-12-25" }, "Price Source": source } });
    let cases = [
        (
            "pricedb-nearest",
            "data: AAPL | AAPL | NASDAQ | 33.00 | $50.00 | $150.00 | $1,650.00 | $300.00 | $390.00 | $240.00 | $1,500.00 | \
             $1,740.00 | 580.00% | $4.00 | $10.00 | $1,744.00 | 581.33%",
        ),
        (
            "pricedb-latest",
            "data: AAPL | AAPL | NASDAQ | 33.00 | $6.00 | $150.00 | $198.00 | $300.00 | $390.00 | $240.00 | $48.00 | $288.00 | \
             96.00% | $4.00 | $10.00 | $292.00 | 97.33%",
        ),
    ];
    cases.iter().for_each(|(source, want)| {
        let result = s.run(ADVANCED, &at(source));
        assert_eq!(rows(&result)[0], *want, "{source}");
        assert_eq!(result["title"], "Advanced Portfolio 12/25/80", "{source}");
    });
}

/// The Display options (AP:890-904, :1037-1051): without symbols, listings, shares and prices the row and the Total
/// row lose those cells, the marker stays; shares print with the chosen places.
#[test]
fn display_options() {
    let mut s = Setup::fixture(FIXTURE, &test_settings("locale"));
    let off = json!({ "Display": {
        "Show ticker symbols": false, "Show listings": false, "Show number of shares": false, "Show prices": false } });
    let result = s.run(ADVANCED, &off);
    let table = tables(&result)[0];
    let labels: Vec<&str> = table["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels[..3], ["Account", " ", "Basis"]);
    assert_eq!(labels.len(), 14);
    assert_eq!(
        rows(&result),
        [
            "data: AAPL | $484.88 | $252.00 | $800.00 | $543.00 | $227.88 | -$232.88 | -$5.00 | -0.63% | $4.00 | $10.00 | \
             -$1.00 | -0.13%",
            "rule: ",
            "grandTotal: Total | $484.88 | $252.00 | $800.00 | $543.00 | $227.88 | -$232.88 | -$5.00 | -0.63% | $4.00 | \
             $10.00 | -$1.00 | -0.13%",
        ]
    );
    assert_eq!(table["rows"][2]["cells"].as_array().unwrap().len(), 14);
    let mut places = |n: u32| {
        let result = s.run(ADVANCED, &json!({ "Display": { "Share decimal places": n } }));
        tables(&result)[0]["rows"][0]["cells"][3]["display"].as_str().unwrap().to_owned()
    };
    assert_eq!((places(0), places(3)), ("42.".to_owned(), "42.000".to_owned()));
}

// Books built through the API. Each holds Bank (BANK) and Broker (ASSET), both in USD, with NYSE stocks under Broker.

/// A new book with Bank, Broker and the `stocks` under Broker, of SCU `fraction`.
fn book_with(stocks: &[&str], fraction: u32) -> Setup {
    let mut s = Setup::new_book(&test_settings("locale"));
    s.account("Bank", "BANK", None, "CURRENCY::USD");
    s.account("Broker", "ASSET", None, "CURRENCY::USD");
    stocks.iter().for_each(|m| stock(&mut s, m, "Broker", fraction));
    s
}

/// The NYSE stock `m` and its STOCK account under `parent`.
fn stock(s: &mut Setup, m: &str, parent: &str, fraction: u32) {
    s.call(
        "commodity.create",
        json!({ "namespace": "NYSE", "mnemonic": m, "fullname": m, "fraction": fraction }),
    );
    s.account(m, "STOCK", Some(parent), &format!("NYSE::{m}"));
}

/// A transaction on `date` in `currency` with `splits`, each `[account, value, amount]` (amount `""` for the value).
fn post(s: &mut Setup, date: &str, currency: &str, splits: &[[&str; 3]]) {
    let splits: Vec<Value> = splits
        .iter()
        .map(|[account, value, amount]| {
            let mut split = json!({ "account": account, "value": value });
            if !amount.is_empty() {
                split["amount"] = json!(amount);
            }
            split
        })
        .collect();
    s.call(
        "transaction.post",
        json!({ "datePosted": date, "currency": currency, "splits": splits }),
    );
}

/// A price of `commodity` in `currency`.
fn price(s: &mut Setup, commodity: &str, currency: &str, value: &str, date: &str) {
    s.call(
        "price.add",
        json!({ "commodity": commodity, "currency": currency, "value": value, "date": date }),
    );
}

/// Options for `accounts` on 2024-03-10, and `more`.
fn options_for(accounts: &[&str], more: Value) -> Value {
    let mut options = json!({ "General": { "Date": { "absolute": "2024-03-10" } }, "Accounts": { "Accounts": accounts } });
    more.as_object().unwrap().iter().for_each(|(section, values)| {
        values
            .as_object()
            .unwrap()
            .iter()
            .for_each(|(k, v)| options[section][k] = v.clone());
    });
    options
}

/// Ten shares of A, B, C and D, each bought for $1,000 on 2024-01-10, and their prices: A only in euros, €100, with a
/// euro at $1.10; B €100 on 2024-03-01 and $105 on 2024-02-01; C only the other way round, a dollar at 0.008 C; D
/// only in francs, which have no price in dollars.
fn prices_book() -> Setup {
    let mut s = book_with(&["A", "B", "C", "D"], 100);
    ["A", "B", "C", "D"].iter().for_each(|m| {
        post(
            &mut s,
            "2024-01-10",
            "CURRENCY::USD",
            &[[&format!("Broker:{m}"), "1000", "10"], ["Bank", "-1000", ""]],
        );
    });
    price(&mut s, "CURRENCY::EUR", "CURRENCY::USD", "1.10", "2024-03-01");
    price(&mut s, "NYSE::A", "CURRENCY::EUR", "100", "2024-03-01");
    price(&mut s, "NYSE::B", "CURRENCY::EUR", "100", "2024-03-01");
    price(&mut s, "NYSE::B", "CURRENCY::USD", "105", "2024-02-01");
    price(&mut s, "CURRENCY::USD", "NYSE::C", "0.008", "2024-03-01");
    price(&mut s, "NYSE::D", "CURRENCY::CHF", "90", "2024-03-01");
    s
}

/// `find-price` and the price checks (AP:324-336, :438-490): A's euro price stays, shown in euros and linked to it, and
/// its value is 10 × €100 converted at $1.10; B's dollar price replaces the newer euro one; C's price of the dollar
/// is inverted to $125, linked to the price stored; D's franc price converts 100 francs to nothing, so it is dropped
/// and the purchase gives $100.00, marked `*` and linked to its transaction. The Total row adds the four.
#[test]
fn prices_in_other_currencies() {
    let mut s = prices_book();
    let result = s.run(ADVANCED, &options_for(&["Broker:A", "Broker:B", "Broker:C", "Broker:D"], json!({})));
    assert_eq!(
        rows(&result),
        [
            "data: A | A | NYSE | 10.00 | €100.00 | $1,000.00 | $1,100.00 | $1,000.00 | $0.00 | $0.00 | $100.00 | $100.00 | \
             10.00% | $0.00 | $0.00 | $100.00 | 10.00%",
            "data: B | B | NYSE | 10.00 | $105.00 | $1,000.00 | $1,050.00 | $1,000.00 | $0.00 | $0.00 | $50.00 | $50.00 | \
             5.00% | $0.00 | $0.00 | $50.00 | 5.00%",
            "data: C | C | NYSE | 10.00 | $125.00 | $1,000.00 | $1,250.00 | $1,000.00 | $0.00 | $0.00 | $250.00 | $250.00 | \
             25.00% | $0.00 | $0.00 | $250.00 | 25.00%",
            "data: D | D | NYSE | 10.00 | $100.00 | * | $1,000.00 | $1,000.00 | $1,000.00 | $0.00 | $0.00 | $0.00 | $0.00 | \
             0.00% | $0.00 | $0.00 | $0.00 | 0.00%",
            "rule: ",
            "grandTotal: Total | $4,000.00 | $4,400.00 | $4,000.00 | $0.00 | $0.00 | $400.00 | $400.00 | 10.00% | $0.00 | \
             $0.00 | $400.00 | 10.00%",
        ]
    );
    let table = tables(&result)[0];
    let price_of = |row: usize| &table["rows"][row]["cells"][4];
    assert_eq!(price_of(0)["currency"], "CURRENCY::EUR");
    assert!(price_of(0)["link"]["price"].is_string());
    assert!(
        price_of(2)["link"]["price"].is_string(),
        "the inverted price links the price stored"
    );
    assert!(price_of(3)["link"]["transaction"].is_string());
    assert_eq!(notes(&result), [NOTE_TXN]);
    let html = s.export(ADVANCED, &options_for(&["Broker:A", "Broker:B", "Broker:C", "Broker:D"], json!({})));
    ["€100.00", "$125.00", "$4,400.00"]
        .iter()
        .for_each(|w| assert!(html.contains(w), "{w} is in the HTML"));
}

/// XYZ (SCU 1,000) bought on 2024-01-10, 10 for $1,000 with a $10 commission, and 4 sold on 2024-02-10 for $600
/// with $6; priced $120 on 2024-03-01.
fn fees_book() -> Setup {
    let mut s = book_with(&["XYZ"], 1000);
    s.account("Commission", "EXPENSE", None, "CURRENCY::USD");
    post(
        &mut s,
        "2024-01-10",
        "CURRENCY::USD",
        &[["Broker:XYZ", "1000", "10"], ["Commission", "10", ""], ["Bank", "-1010", ""]],
    );
    post(
        &mut s,
        "2024-02-10",
        "CURRENCY::USD",
        &[["Broker:XYZ", "-600", "-4"], ["Commission", "6", ""], ["Bank", "594", ""]],
    );
    price(&mut s, "NYSE::XYZ", "CURRENCY::USD", "120", "2024-03-01");
    s
}

/// "How to report brokerage fees" (AP:626-640, :722-726, :849-850, :925-926). The fees are $16.
/// - In the basis: money in takes the purchase's $10 ($1,010), money out loses the sale's $6 ($594); the basis is
///   $101 a share, so 6 left are $606 and the sale realizes $594 − $404 = $190.
/// - In the gain: money in and out the same; the basis $100 a share ($600), the sale realizes $600 − $400 = $200, less
///   the $16 of fees: $184.
/// - Omitted: money in $1,000 and out $600, realized $200, and no Brokerage Fees column.
#[test]
fn brokerage_fees() {
    let mut s = fees_book();
    let with = |fees: &str| options_for(&["Broker:XYZ"], json!({ "General": { "How to report brokerage fees": fees } }));
    let cases = [
        (
            "include-in-basis",
            "data: XYZ | XYZ | NYSE | 6.00 | $120.00 | $606.00 | $720.00 | $1,010.00 | $594.00 | $190.00 | $114.00 | $304.00 | \
             30.10% | $0.00 | $16.00 | $304.00 | 30.10%",
        ),
        (
            "include-in-gain",
            "data: XYZ | XYZ | NYSE | 6.00 | $120.00 | $600.00 | $720.00 | $1,010.00 | $594.00 | $184.00 | $120.00 | $304.00 | \
             30.10% | $0.00 | $16.00 | $304.00 | 30.10%",
        ),
        (
            "ignore-brokerage",
            "data: XYZ | XYZ | NYSE | 6.00 | $120.00 | $600.00 | $720.00 | $1,000.00 | $600.00 | $200.00 | $120.00 | $320.00 | \
             32.00% | $0.00 | $320.00 | 32.00%",
        ),
    ];
    cases.iter().for_each(|(fees, want)| {
        let result = s.run(ADVANCED, &with(fees));
        assert_eq!(rows(&result)[0], *want, "{fees}");
        let labels: Vec<&str> = tables(&result)[0]["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["label"].as_str().unwrap())
            .collect();
        assert_eq!(labels.contains(&"Brokerage Fees"), *fees != "ignore-brokerage", "{fees}");
    });
}

/// L: 10 bought for $1,000 on 2024-01-10, 10 for $1,200 on 2024-02-10, 15 sold for $1,950 on 2024-02-20; priced $140
/// on 2024-03-01.
fn lots_book() -> Setup {
    let mut s = book_with(&["L"], 100);
    post(
        &mut s,
        "2024-01-10",
        "CURRENCY::USD",
        &[["Broker:L", "1000", "10"], ["Bank", "-1000", ""]],
    );
    post(
        &mut s,
        "2024-02-10",
        "CURRENCY::USD",
        &[["Broker:L", "1200", "10"], ["Bank", "-1200", ""]],
    );
    post(
        &mut s,
        "2024-02-20",
        "CURRENCY::USD",
        &[["Broker:L", "-1950", "-15"], ["Bank", "1950", ""]],
    );
    price(&mut s, "NYSE::L", "CURRENCY::USD", "140", "2024-03-01");
    s
}

/// The basis methods (AP:202-319) on L. The average holds 20 at $110 and keeps $110 for the 5 left: basis $550,
/// realized $1,950 − $1,650. FIFO sells all of the first pair and 5 of the second, keeping 5 at $120: $600, realized
/// $350. LIFO sells all of the second and 5 of the first, keeping 5 at $100: $500, realized $250. The total gain is
/// $450 every way, 450/2,200 = 20.45 %.
#[test]
fn basis_methods() {
    let mut s = lots_book();
    let with = |method: &str| options_for(&["Broker:L"], json!({ "General": { "Basis calculation method": method } }));
    let row = |basis: &str, realized: &str, unrealized: &str| {
        format!(
            "data: L | L | NYSE | 5.00 | $140.00 | {basis} | $700.00 | $2,200.00 | $1,950.00 | {realized} | {unrealized} | \
             $450.00 | 20.45% | $0.00 | $0.00 | $450.00 | 20.45%"
        )
    };
    let cases = [
        ("average-basis", row("$550.00", "$300.00", "$150.00")),
        ("fifo-basis", row("$600.00", "$350.00", "$100.00")),
        ("filo-basis", row("$500.00", "$250.00", "$200.00")),
    ];
    cases
        .iter()
        .for_each(|(method, want)| assert_eq!(rows(&s.run(ADVANCED, &with(method)))[0], *want, "{method}"));
}

/// The rate at a tie (the note's Q1, `design/reports.md` §14): one share bought for $8.00 and priced $8.29 gains
/// $0.29, 3.625 % exactly. GnuCash divides doubles, 0.29 / 8.0 × 100 = 3.6249999…, and prints `3.62%`; red-ale
/// rounds the exact rate half away from zero, as `~,2f` does a tie, and prints `3.63%`.
#[test]
fn a_rate_at_a_tie() {
    let mut s = book_with(&["T"], 100);
    post(&mut s, "2024-01-10", "CURRENCY::USD", &[["Broker:T", "8", "1"], ["Bank", "-8", ""]]);
    price(&mut s, "NYSE::T", "CURRENCY::USD", "8.29", "2024-03-01");
    let result = s.run(ADVANCED, &options_for(&["Broker:T"], json!({})));
    assert_eq!(
        rows(&result)[0],
        "data: T | T | NYSE | 1.00 | $8.29 | $8.00 | $8.29 | $8.00 | $0.00 | $0.00 | $0.29 | $0.29 | 3.63% | $0.00 | $0.00 | \
         $0.29 | 3.63%"
    );
    assert_eq!(
        tables(&result)[0]["rows"][0]["cells"][13]["number"],
        json!({ "num": "29", "denom": "8" })
    );
}

/// The 10⁻⁸ and SCU roundings (AP:57-59, :231-235, :714-725). R: 3,000,000 shares for $10,000,000.00 are $3.33333333
/// each to 10⁻⁸, so the basis is $9,999,999.99, a cent below what was paid. F: one transaction buys three single
/// shares at $100 with a $10 commission; each split's share of it, $10/3, is $3.33 at the dollar's SCU, so the basis
/// is $309.99 while money in is $310.00.
#[test]
fn the_ten_to_the_eighth_and_scu_roundings() {
    let mut s = book_with(&["R", "F"], 100);
    s.account("Commission", "EXPENSE", None, "CURRENCY::USD");
    post(
        &mut s,
        "2024-01-10",
        "CURRENCY::USD",
        &[["Broker:R", "10000000", "3000000"], ["Bank", "-10000000", ""]],
    );
    post(
        &mut s,
        "2024-01-10",
        "CURRENCY::USD",
        &[
            ["Broker:F", "100", "1"],
            ["Broker:F", "100", "1"],
            ["Broker:F", "100", "1"],
            ["Commission", "10", ""],
            ["Bank", "-310", ""],
        ],
    );
    price(&mut s, "NYSE::R", "CURRENCY::USD", "3.40", "2024-03-01");
    price(&mut s, "NYSE::F", "CURRENCY::USD", "100", "2024-03-01");
    let result = s.run(ADVANCED, &options_for(&["Broker:R", "Broker:F"], json!({})));
    assert_eq!(
        rows(&result)[..2],
        [
            "data: R | R | NYSE | 3,000,000.00 | $3.40 | $9,999,999.99 | $10,200,000.00 | $10,000,000.00 | $0.00 | $0.00 | \
             $200,000.01 | $200,000.01 | 2.00% | $0.00 | $0.00 | $200,000.01 | 2.00%",
            "data: F | F | NYSE | 3.00 | $100.00 | $309.99 | $300.00 | $310.00 | $0.00 | $0.00 | -$9.99 | -$9.99 | -3.22% | \
             $0.00 | $10.00 | -$9.99 | -3.22%",
        ]
    );
}

/// Accounts for the events of the walk, each under Broker with SCU 1: S sells 5 it never had; Z buys 10 for $100,
/// sells 15 for $180 and buys 5 for $60; W buys 10 for $500 and becomes worthless (a split of −10 with no value);
/// P has a spin-off of $20 with no shares; G gives 2 of 10 bought for $1,000 to charity, priced $130.
fn events_book() -> Setup {
    let mut s = book_with(&["S", "Z", "W", "P", "G"], 1);
    s.account("Charity", "EXPENSE", None, "CURRENCY::USD");
    let usd = "CURRENCY::USD";
    post(&mut s, "2024-01-10", usd, &[["Broker:S", "-250", "-5"], ["Bank", "250", ""]]);
    post(&mut s, "2024-01-10", usd, &[["Broker:Z", "100", "10"], ["Bank", "-100", ""]]);
    post(&mut s, "2024-01-20", usd, &[["Broker:Z", "-180", "-15"], ["Bank", "180", ""]]);
    post(&mut s, "2024-01-30", usd, &[["Broker:Z", "60", "5"], ["Bank", "-60", ""]]);
    post(&mut s, "2024-01-10", usd, &[["Broker:W", "500", "10"], ["Bank", "-500", ""]]);
    post(&mut s, "2024-02-10", usd, &[["Broker:W", "0", "-10"]]);
    post(&mut s, "2024-01-10", usd, &[["Broker:P", "-20", "0"], ["Bank", "20", ""]]);
    post(&mut s, "2024-01-10", usd, &[["Broker:G", "1000", "10"], ["Bank", "-1000", ""]]);
    post(&mut s, "2024-02-10", usd, &[["Broker:G", "-240", "-2"], ["Charity", "240", ""]]);
    price(&mut s, "NYSE::G", usd, "130", "2024-03-01");
    s
}

/// A sale with nothing held leaves the average basis empty (AP:237-277) and realizes all it brings in (AP:740-752):
/// S's price is its own sale's, $50, its value −$250; with no money in its rates are empty. A gift to charity, two
/// splits whose other is this account, is no brokerage (AP:541-546): G's basis keeps $100 a share for the 8 left and
/// the gift realizes $240 − $200. A stock split to nothing makes the stock worthless (AP:280-297): W's basis goes to
/// zero and realizes −$500, shown with the accounts with no shares.
#[test]
fn sales_gifts_and_worthless_stock() {
    let mut s = events_book();
    let options = options_for(
        &["Broker:S", "Broker:G", "Broker:W"],
        json!({ "Accounts": { "Include accounts with no shares": true } }),
    );
    let result = s.run(ADVANCED, &options);
    assert_eq!(
        rows(&result),
        [
            "data: S | S | NYSE | -5.00 | $50.00 | * | $0.00 | -$250.00 | $0.00 | $250.00 | $250.00 | -$250.00 | $0.00 | $0.00 | \
             $0.00 | $0.00",
            "data: G | G | NYSE | 8.00 | $130.00 | $800.00 | $1,040.00 | $1,000.00 | $240.00 | $40.00 | $240.00 | $280.00 | \
             28.00% | $0.00 | $0.00 | $280.00 | 28.00%",
            "data: W | W | NYSE | 0.00 | $50.00 | * | $0.00 | $0.00 | $500.00 | $0.00 | -$500.00 | $0.00 | -$500.00 | -100.00% | \
             $0.00 | $0.00 | -$500.00 | -100.00%",
            "rule: ",
            "grandTotal: Total | $800.00 | $790.00 | $1,500.00 | $490.00 | -$210.00 | -$10.00 | -$220.00 | -14.67% | $0.00 | \
             $0.00 | -$220.00 | -14.67%",
        ]
    );
}

/// A division by zero stops the walk (AP:1076-1086): Z sold 15 of 10, holding −5, so buying 5 makes the average's
/// divisor zero, "buying 5.0000 share units" (AP:224-228). The error comes before the table, which keeps S's row and
/// totals and has no row for Z or G after it. A spin-off with no basis throws too, "spinoff of 0.00 currency units"
/// (AP:302-308).
#[test]
fn division_by_zero_keeps_the_rows_before() {
    let mut s = events_book();
    let result = s.run(ADVANCED, &options_for(&["Broker:S", "Broker:Z", "Broker:G"], json!({})));
    let b = blocks(&result);
    assert_eq!((b[0]["kind"].as_str(), b[0]["title"].as_str()), (Some("warning"), Some("Error")));
    assert_eq!(
        b[0]["text"],
        "There is an error processing the transaction 'buying 5.0000 share units'. This may to be caused by a sell \
         transaction causing a negative stock balance, and a subsequent buy transaction causing a zero balance. This leads \
         to a division-by-zero error. It can be fixed by preventing negative stock balances."
    );
    assert_eq!(
        rows(&result),
        [
            "data: S | S | NYSE | -5.00 | $50.00 | * | $0.00 | -$250.00 | $0.00 | $250.00 | $250.00 | -$250.00 | $0.00 | $0.00 | \
             $0.00 | $0.00",
            "rule: ",
            "grandTotal: Total | $0.00 | -$250.00 | $0.00 | $250.00 | $250.00 | -$250.00 | $0.00 | $0.00 | $0.00 | $0.00",
        ]
    );
    assert_eq!(notes(&result), [NOTE_TXN]);
    let spun = s.run(ADVANCED, &options_for(&["Broker:P"], json!({})));
    assert!(
        blocks(&spun)[0]["text"]
            .as_str()
            .unwrap()
            .contains("'spinoff of 0.00 currency units'")
    );
    assert!(
        s.export(ADVANCED, &options_for(&["Broker:P"], json!({})))
            .contains("spinoff of 0.00 currency units")
    );
}

/// Income and fees booked to the parent account (AP:778-838): Broker, an ASSET account in dollars, receives a $30
/// dividend from Income:XYZ and pays $2 to Expenses:XYZ, both named as the stock account; they count as XYZ's income
/// and brokerage. The $7 from Income:Other and a $9 dividend after the Date do not.
#[test]
fn income_and_fees_of_the_parent_account() {
    let mut s = book_with(&["XYZ"], 100);
    s.account("Income", "INCOME", None, "CURRENCY::USD");
    s.account("XYZ", "INCOME", Some("Income"), "CURRENCY::USD");
    s.account("Other", "INCOME", Some("Income"), "CURRENCY::USD");
    s.account("Expenses", "EXPENSE", None, "CURRENCY::USD");
    s.account("XYZ", "EXPENSE", Some("Expenses"), "CURRENCY::USD");
    let usd = "CURRENCY::USD";
    post(&mut s, "2024-01-10", usd, &[["Broker:XYZ", "1000", "10"], ["Bank", "-1000", ""]]);
    post(&mut s, "2024-02-15", usd, &[["Broker", "30", ""], ["Income:XYZ", "-30", ""]]);
    post(&mut s, "2024-02-16", usd, &[["Broker", "-2", ""], ["Expenses:XYZ", "2", ""]]);
    post(&mut s, "2024-02-17", usd, &[["Broker", "7", ""], ["Income:Other", "-7", ""]]);
    post(&mut s, "2024-03-20", usd, &[["Broker", "9", ""], ["Income:XYZ", "-9", ""]]);
    price(&mut s, "NYSE::XYZ", usd, "100", "2024-03-01");
    let result = s.run(ADVANCED, &options_for(&["Broker:XYZ"], json!({})));
    assert_eq!(
        rows(&result)[0],
        "data: XYZ | XYZ | NYSE | 10.00 | $100.00 | $1,000.00 | $1,000.00 | $1,000.00 | $0.00 | $0.00 | $0.00 | $0.00 | 0.00% | \
         $30.00 | $2.00 | $30.00 | 3.00%"
    );
    let in_gain = s.run(
        ADVANCED,
        &options_for(
            &["Broker:XYZ"],
            json!({ "General": { "How to report brokerage fees": "include-in-gain" } }),
        ),
    );
    assert_eq!(
        rows(&in_gain)[0],
        "data: XYZ | XYZ | NYSE | 10.00 | $100.00 | $1,000.00 | $1,000.00 | $1,000.00 | $0.00 | -$2.00 | $0.00 | -$2.00 | \
         -0.20% | $30.00 | $2.00 | $28.00 | 2.80%"
    );
}

/// Reinvested dividends (AP:580-593, :642-683), each stock bought for $1,000 on 2024-01-10 and priced $100. AAA
/// reinvests $50 on 2024-02-10 buying $45 and leaving $5 in Cash1, its sibling, then $50 on 2024-03-05 buying $55
/// with that $5: money in stays $1,000. BBB leaves the $5 in Savings, which is not Broker2's, then takes it from
/// Cash2: the holding account is forgotten and the $5 counts as money in. CCC reinvests $50 buying $55, taking $5
/// from Cash3 first: the holding starts at −$5 and the $5 counts as money in.
#[test]
fn reinvested_dividends() {
    let mut s = Setup::new_book(&test_settings("locale"));
    s.account("Bank", "BANK", None, "CURRENCY::USD");
    s.account("Income", "INCOME", None, "CURRENCY::USD");
    s.account("Savings", "ASSET", None, "CURRENCY::USD");
    ["1", "2", "3"].iter().for_each(|n| {
        s.account(&format!("Broker{n}"), "ASSET", None, "CURRENCY::USD");
        s.account(&format!("Cash{n}"), "ASSET", Some(&format!("Broker{n}")), "CURRENCY::USD");
    });
    stock(&mut s, "AAA", "Broker1", 100);
    stock(&mut s, "BBB", "Broker2", 100);
    stock(&mut s, "CCC", "Broker3", 100);
    let usd = "CURRENCY::USD";
    ["Broker1:AAA", "Broker2:BBB", "Broker3:CCC"].iter().for_each(|a| {
        post(&mut s, "2024-01-10", usd, &[[a, "1000", "10"], ["Bank", "-1000", ""]]);
    });
    post(
        &mut s,
        "2024-02-10",
        usd,
        &[["Income", "-50", ""], ["Broker1:AAA", "45", "0.45"], ["Broker1:Cash1", "5", ""]],
    );
    post(
        &mut s,
        "2024-03-05",
        usd,
        &[["Income", "-50", ""], ["Broker1:AAA", "55", "0.55"], ["Broker1:Cash1", "-5", ""]],
    );
    post(
        &mut s,
        "2024-02-10",
        usd,
        &[["Income", "-50", ""], ["Broker2:BBB", "45", "0.45"], ["Savings", "5", ""]],
    );
    post(
        &mut s,
        "2024-03-05",
        usd,
        &[["Income", "-50", ""], ["Broker2:BBB", "55", "0.55"], ["Broker2:Cash2", "-5", ""]],
    );
    post(
        &mut s,
        "2024-02-10",
        usd,
        &[["Income", "-50", ""], ["Broker3:CCC", "55", "0.55"], ["Broker3:Cash3", "-5", ""]],
    );
    ["AAA", "BBB", "CCC"]
        .iter()
        .for_each(|m| price(&mut s, &format!("NYSE::{m}"), usd, "100", "2024-03-06"));
    let result = s.run(ADVANCED, &options_for(&["Broker1:AAA", "Broker2:BBB", "Broker3:CCC"], json!({})));
    assert_eq!(
        rows(&result)[..3],
        [
            "data: AAA | AAA | NYSE | 11.00 | $100.00 | $1,100.00 | $1,100.00 | $1,000.00 | $0.00 | $0.00 | $0.00 | $0.00 | \
             0.00% | $100.00 | $0.00 | $100.00 | 10.00%",
            "data: BBB | BBB | NYSE | 11.00 | $100.00 | $1,100.00 | $1,100.00 | $1,005.00 | $0.00 | $0.00 | $0.00 | $0.00 | \
             0.00% | $100.00 | $0.00 | $100.00 | 9.95%",
            "data: CCC | CCC | NYSE | 10.55 | $100.00 | $1,055.00 | $1,055.00 | $1,005.00 | $0.00 | $0.00 | $0.00 | $0.00 | \
             0.00% | $50.00 | $0.00 | $50.00 | 4.98%",
        ]
    );
}

/// A purchase in another currency (AP:709-711, :853-856): XYZ is bought for €1,000 and priced in dollars; the basis
/// and money in are the euros at the euro's price nearest the Date, $1.10.
#[test]
fn a_purchase_in_another_currency() {
    let mut s = book_with(&["XYZ"], 100);
    s.account("Euros", "BANK", None, "CURRENCY::EUR");
    post(
        &mut s,
        "2024-01-10",
        "CURRENCY::EUR",
        &[["Broker:XYZ", "1000", "10"], ["Euros", "-1000", ""]],
    );
    price(&mut s, "CURRENCY::EUR", "CURRENCY::USD", "1.10", "2024-03-01");
    price(&mut s, "NYSE::XYZ", "CURRENCY::USD", "120", "2024-03-01");
    let result = s.run(ADVANCED, &options_for(&["Broker:XYZ"], json!({})));
    assert_eq!(
        rows(&result)[0],
        "data: XYZ | XYZ | NYSE | 10.00 | $120.00 | $1,100.00 | $1,200.00 | $1,100.00 | $0.00 | $0.00 | $100.00 | $100.00 | \
         9.09% | $0.00 | $0.00 | $100.00 | 9.09%"
    );
}

/// The options of AP:61-140 in GnuCash's order, with the General options every template gets.
#[test]
fn options_are_gnucash_s() {
    let mut s = Setup::with_config(&test_settings("locale"));
    let described = s.call("report.options", json!({ "report": ADVANCED }));
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
        r#"Display/Show ticker symbols "a" "boolean" true"#,
        r#"Display/Show listings "b" "boolean" true"#,
        r#"Display/Show number of shares "c" "boolean" true"#,
        r#"Display/Share decimal places "d" "numberRange" 2"#,
        r#"Display/Show prices "e" "boolean" true"#,
        r#"General/Report name "0a" "string" "Advanced Portfolio""#,
        r#"General/Stylesheet "0b" "multichoice" "Default""#,
        r#"General/Add options summary "0c" "multichoice" "never""#,
        r#"General/Date "a" "date" {"relative":"end-accounting-period"}"#,
        r#"General/Report's currency "c" "currency" "CURRENCY::USD""#,
        r#"General/Price Source "d" "multichoice" "pricedb-nearest""#,
        r#"General/Basis calculation method "e" "multichoice" "average-basis""#,
        r#"General/Set preference for price list data "f" "boolean" true"#,
        r#"General/How to report brokerage fees "g" "multichoice" "include-in-basis""#,
    ];
    assert_eq!(got, want);
    let mut s = Setup::fixture(FIXTURE, &test_settings("locale"));
    let described = s.call("report.options", json!({ "report": ADVANCED }));
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

/// Running the report with each source, method and fee option leaves the Book, its revision and its file as they
/// were and writes nothing beside it, on XML and SQLite; two exports are the same bytes, with no script.
#[test]
fn a_run_changes_nothing() {
    for (format, mut s) in both_formats() {
        let bytes = std::fs::read(s.book()).unwrap();
        let before = s.d.context().book().unwrap().clone();
        let revision = s.d.context().session().unwrap().revision();
        let files = s.files();
        let general = [
            json!({ "Price Source": "pricedb-latest", "Basis calculation method": "fifo-basis" }),
            json!({ "Basis calculation method": "filo-basis", "How to report brokerage fees": "include-in-gain" }),
            json!({ "Set preference for price list data": false, "How to report brokerage fees": "ignore-brokerage" }),
        ];
        general.iter().for_each(|g| {
            let options = json!({ "General": g, "Accounts": { "Include accounts with no shares": true } });
            s.run(ADVANCED, &options);
            let (a, b) = (s.export(ADVANCED, &options), s.export(ADVANCED, &options));
            assert_eq!(a, b, "{format} {g}");
            assert!(!a.to_ascii_lowercase().contains("<script"), "{format}");
        });
        assert_eq!(s.d.context().book().unwrap(), &before, "{format}");
        assert_eq!(s.d.context().session().unwrap().revision(), revision, "{format}");
        assert_eq!(s.call("book.info", json!({}))["dirty"], false, "{format}");
        assert_eq!(s.files(), files, "{format}: no journal or other file was written");
        assert_eq!(std::fs::read(s.book()).unwrap(), bytes, "{format}: the file is unchanged");
    }
}
