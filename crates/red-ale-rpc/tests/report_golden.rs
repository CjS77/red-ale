//! Done-when check of milestone 3: under the default (C) locale, every v0.2 report gives the same bytes as v0.2 did.
//!
//! `tests/report_golden/v0.2.txt` holds, for every built-in report of v0.2 on every `fixtures/reports/` book, with
//! the `us` and `locale` date formats and default options, the length and FNV-1a digest of `report.export`'s HTML
//! (or the error code). It was written by this file's generator run against the v0.2 tree (`main` at the end of
//! milestone 2):
//!
//! ```text
//! RED_ALE_GOLDEN_WRITE=/tmp/v0.2.txt cargo test -p red-ale-rpc --test report_golden
//! ```
//!
//! The test runs the same exports now, once with no `locale` param (the Configuration's default, C) and once with
//! `"locale": "C"`, and compares. On a mismatch the HTML is written to a temporary directory for diffing.
//!
//! Reports added since keep their own record, `tests/report_golden/v0.4.txt`: every v0.4 report on every book,
//! `budget` included, written when the report was translated and checked against its tests. Lines are looked up by
//! key, so each report's lines are appended as a block when it is added:
//!
//! ```text
//! RED_ALE_GOLDEN_WRITE_V04=/tmp/v0.4.txt cargo test -p red-ale-rpc --test report_golden
//! ```
//!
//! The v0.5 reports keep `tests/report_golden/v0.5.txt` the same way, on every book of `BOOKS` and of `V05_BOOKS`.
//! Translating a report adds its name to `V05_REPORTS`; the writer then writes only the lines `v0.5.txt` does not
//! have yet, which are appended to it as the report's block:
//!
//! ```text
//! RED_ALE_GOLDEN_WRITE_V05=/tmp/new.txt cargo test -p red-ale-rpc --test report_golden v05 && cat /tmp/new.txt >> …/v0.5.txt
//! ```
//!
//! The v0.6 reports keep `tests/report_golden/v0.6.txt` the same way: each on every book of `BOOKS` and `V05_BOOKS`
//! with its defaults, and the Printable Invoice also for every document of the books that have them
//! (`INVOICE_BOOKS`), keyed by the book's path below `fixtures/` and the document's GUID:
//!
//! ```text
//! RED_ALE_GOLDEN_WRITE_V06=/tmp/new.txt cargo test -p red-ale-rpc --test report_golden v06 && cat /tmp/new.txt >> …/v0.6.txt
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod report_runs;

use std::collections::BTreeSet;
use std::fmt::Write as _;

use report_runs::Fixture;
use serde_json::json;

const BOOKS: [&str; 11] = [
    "average-balance",
    "balsheet-pnl",
    "budget",
    "category-average",
    "category-daily",
    "category-liability",
    "category-multi",
    "charts-daily",
    "charts-net",
    "net-days",
    "test-data",
];

const DATE_FORMATS: [&str; 2] = ["us", "locale"];

/// The books and reports v0.2 had, which `v0.2.txt` covers.
const V02_BOOKS: usize = 10;
const V02_REPORTS: usize = 19;

/// The built-in reports added in v0.4, which `v0.4.txt` covers.
const V04_REPORTS: [&str; 8] = [
    "Budget Report",
    "Budget Balance Sheet",
    "Budget Income Statement",
    "Budget Profit & Loss",
    "Budget Flow",
    "Budget Chart",
    "Future Scheduled Transactions Summary",
    "Balance Forecast",
];

/// The built-in reports added in v0.5, which `v0.5.txt` covers. A translated report appends its name.
const V05_REPORTS: &[&str] = &[
    "Equity Statement",
    "Trial Balance",
    "Cash Flow",
    "Cash Flow Barchart",
    "Balance Sheet (Multicolumn)",
    "Income Statement (Multicolumn)",
    "Transaction Report",
    "Investment Portfolio",
    "General Ledger",
    "Reconciliation Report",
    "General Journal",
    "Advanced Portfolio",
    "IFRS weighted-average cost basis report",
    "Investment Lots",
    "Transaction Breakdown Report",
    "Income vs. Day of Week",
    "Expenses vs. Day of Week",
    "Multicolumn View",
];

/// The fixture books added in v0.5, on which `v0.5.txt` records the v0.5 reports besides `BOOKS`.
const V05_BOOKS: &[&str] = &[
    "adjusting-entries",
    "adjusting-entries-fx",
    "cash-flow",
    "cashflow-barchart",
    "balsheet-pnl-extras",
    "balsheet-pnl-extras-trading",
    "transaction",
    "stock-test-data",
    "ifrs-cost-basis",
    "investment-lots",
    "lots-reverse-split",
    "lots-baseline",
    "lots-reverse-split-sale",
    "lots-postsplit-baseline",
];

/// The built-in reports added in v0.6, which `v0.6.txt` covers.
const V06_REPORTS: &[&str] = &["Printable Invoice"];

/// The books holding business documents, by path below `fixtures/`, and the documents the Printable Invoice prints:
/// `create-test-invoice-data`'s eight, and the business book's four as XML and as SQLite.
const INVOICE_BOOKS: [(&str, &[&str]); 3] = [
    (
        "reports/transaction.gnucash",
        &[
            "30000000000000000000000000000000",
            "30000000000000000000000000000001",
            "30000000000000000000000000000002",
            "30000000000000000000000000000003",
            "30000000000000000000000000000004",
            "30000000000000000000000000000005",
            "30000000000000000000000000000006",
            "30000000000000000000000000000007",
        ],
    ),
    ("synthetic/business.gnucash", &BUSINESS_DOCUMENTS),
    ("sqlite/synthetic/business.gnucash", &BUSINESS_DOCUMENTS),
];

const BUSINESS_DOCUMENTS: [&str; 4] = [
    "35ad2d2ebe3847ea9a415e7cc53cc09f",
    "6193d3b14af34c25b389fc13ac14d277",
    "f6772a8b0ea543679c940e23904f9ed9",
    "fc2cd829ac2442269c964a2ee27244cc",
];

/// FNV-1a, 64 bits: a digest with no dependency, enough to see that two outputs differ.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// One line per export of every built-in report on every book of `BOOKS`: `book date-format report <tab> outcome`,
/// and the outcomes' HTML by key.
fn exports(locale: Option<&str>) -> Vec<(String, String, Option<String>)> {
    exports_of(&BOOKS, locale, |_| true)
}

/// The exports of the built-in reports `wanted` picks by name, on `books`.
fn exports_of(books: &[&str], locale: Option<&str>, wanted: impl Fn(&str) -> bool) -> Vec<(String, String, Option<String>)> {
    books
        .iter()
        .flat_map(|book| DATE_FORMATS.iter().map(move |format| (*book, *format)))
        .flat_map(|(book, format)| {
            let mut f = Fixture::open(book, format, true);
            let listed = f.call("report.list", &json!({}));
            let names: Vec<String> = listed["reports"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["kind"] == "builtin")
                .map(|r| r["name"].as_str().unwrap().to_owned())
                .filter(|name| wanted(name))
                .collect();
            names
                .into_iter()
                .map(|report| {
                    let mut params = json!({ "report": report, "options": {} });
                    if let Some(locale) = locale {
                        params["locale"] = json!(locale);
                    }
                    let key = format!("{book} {format} {report}");
                    match f.d.call("report.export", params) {
                        Ok(v) => {
                            let html = v["html"].as_str().unwrap().to_owned();
                            (key, format!("{} {:016x}", html.len(), fnv1a(html.as_bytes())), Some(html))
                        }
                        Err(e) => (key, format!("error {}", e.code), None),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn lines(exports: &[(String, String, Option<String>)]) -> String {
    exports.iter().fold(String::new(), |mut out, (key, outcome, _)| {
        let _ = writeln!(out, "{key}\t{outcome}");
        out
    })
}

fn golden() -> Vec<(String, String)> {
    recorded(include_str!("report_golden/v0.2.txt"))
}

fn recorded(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(|l| l.split_once('\t').unwrap())
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn assert_as_recorded(label: &str, expected: &[(String, String)], now: &[(String, String, Option<String>)]) {
    let dir = std::env::temp_dir().join(format!("red-ale-report-golden-{label}"));
    let differ: Vec<&str> = expected
        .iter()
        .filter_map(|(key, outcome)| {
            let found = now.iter().find(|(k, ..)| k == key);
            match found {
                Some((_, o, _)) if o == outcome => None,
                Some((_, _, html)) => {
                    std::fs::create_dir_all(&dir).unwrap();
                    std::fs::write(
                        dir.join(format!("{}.html", key.replace(['/', ' '], "_"))),
                        html.as_deref().unwrap_or_default(),
                    )
                    .unwrap();
                    Some(key.as_str())
                }
                None => Some(key.as_str()),
            }
        })
        .collect();
    assert!(
        differ.is_empty(),
        "{label}: {} of {} exports differ from the record (HTML in {}): {differ:#?}",
        differ.len(),
        expected.len(),
        dir.display()
    );
}

#[test]
fn every_v02_report_gives_the_same_bytes_under_the_c_locale() {
    if let Ok(path) = std::env::var("RED_ALE_GOLDEN_WRITE") {
        std::fs::write(path, lines(&exports(None))).unwrap();
        return;
    }
    let expected = golden();
    assert_eq!(expected.len(), V02_BOOKS * DATE_FORMATS.len() * V02_REPORTS, "19 reports in v0.2");
    assert!(
        expected.iter().filter(|(_, o)| !o.starts_with("error")).count() > expected.len() / 2,
        "most exports succeed"
    );
    assert_as_recorded("default", &expected, &exports(None));
    assert_as_recorded("c", &expected, &exports(Some("C")));
}

/// The exports of the v0.4 reports among `all`.
fn v04(all: Vec<(String, String, Option<String>)>) -> Vec<(String, String, Option<String>)> {
    all.into_iter()
        .filter(|(key, ..)| V04_REPORTS.iter().any(|r| key.ends_with(&format!(" {r}"))))
        .collect()
}

#[test]
fn every_v04_report_gives_its_recorded_bytes() {
    if let Ok(path) = std::env::var("RED_ALE_GOLDEN_WRITE_V04") {
        std::fs::write(path, lines(&v04(exports(None)))).unwrap();
        return;
    }
    let expected = recorded(include_str!("report_golden/v0.4.txt"));
    assert_eq!(expected.len(), BOOKS.len() * DATE_FORMATS.len() * V04_REPORTS.len());
    assert_as_recorded("v0.4", &expected, &v04(exports(None)));
}

/// The exports of the v0.5 reports on `BOOKS` and `V05_BOOKS`.
fn v05() -> Vec<(String, String, Option<String>)> {
    let books: Vec<&str> = BOOKS.iter().chain(V05_BOOKS).copied().collect();
    exports_of(&books, None, |name| V05_REPORTS.contains(&name))
}

#[test]
fn every_v05_report_gives_its_recorded_bytes() {
    let expected = recorded(include_str!("report_golden/v0.5.txt"));
    let now = v05();
    if let Ok(path) = std::env::var("RED_ALE_GOLDEN_WRITE_V05") {
        let new: Vec<_> = now
            .into_iter()
            .filter(|(key, ..)| !expected.iter().any(|(k, _)| k == key))
            .collect();
        std::fs::write(path, lines(&new)).unwrap();
        return;
    }
    let keys = |ks: Vec<&String>| ks.into_iter().cloned().collect::<BTreeSet<String>>();
    assert_eq!(
        keys(expected.iter().map(|(k, _)| k).collect()),
        keys(now.iter().map(|(k, ..)| k).collect()),
        "v0.5.txt records every v0.5 report on every book, and nothing else"
    );
    assert_as_recorded("v0.5", &expected, &now);
}

/// The Printable Invoice of every document of `INVOICE_BOOKS`, under each date format.
fn invoice_exports() -> Vec<(String, String, Option<String>)> {
    INVOICE_BOOKS
        .iter()
        .flat_map(|(path, documents)| DATE_FORMATS.iter().map(move |format| (*path, *documents, *format)))
        .flat_map(|(path, documents, format)| {
            let mut f = Fixture::open_fixture(path, format, true);
            documents
                .iter()
                .map(|guid| {
                    let params = json!({ "report": "Printable Invoice", "options": { "General": { "Invoice Number": guid } } });
                    let html = f.call("report.export", &params)["html"].as_str().unwrap().to_owned();
                    let outcome = format!("{} {:016x}", html.len(), fnv1a(html.as_bytes()));
                    (format!("{path} {format} Printable Invoice {guid}"), outcome, Some(html))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The exports of the v0.6 reports: on `BOOKS` and `V05_BOOKS` with their defaults, and the invoices.
fn v06() -> Vec<(String, String, Option<String>)> {
    let books: Vec<&str> = BOOKS.iter().chain(V05_BOOKS).copied().collect();
    let mut all = exports_of(&books, None, |name| V06_REPORTS.contains(&name));
    all.extend(invoice_exports());
    all
}

#[test]
fn every_v06_report_gives_its_recorded_bytes() {
    let expected = recorded(include_str!("report_golden/v0.6.txt"));
    let now = v06();
    if let Ok(path) = std::env::var("RED_ALE_GOLDEN_WRITE_V06") {
        let new: Vec<_> = now
            .into_iter()
            .filter(|(key, ..)| !expected.iter().any(|(k, _)| k == key))
            .collect();
        std::fs::write(path, lines(&new)).unwrap();
        return;
    }
    let keys = |ks: Vec<&String>| ks.into_iter().cloned().collect::<BTreeSet<String>>();
    assert_eq!(
        keys(expected.iter().map(|(k, _)| k).collect()),
        keys(now.iter().map(|(k, ..)| k).collect()),
        "v0.6.txt records every v0.6 report on every book, and nothing else"
    );
    assert_as_recorded("v0.6", &expected, &now);
}
