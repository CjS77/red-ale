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

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod report_runs;

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
const V04_REPORTS: [&str; 2] = ["Budget Report", "Future Scheduled Transactions Summary"];

/// FNV-1a, 64 bits: a digest with no dependency, enough to see that two outputs differ.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// One line per export: `book date-format report <tab> outcome`, and the outcomes' HTML by key.
fn exports(locale: Option<&str>) -> Vec<(String, String, Option<String>)> {
    BOOKS
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
