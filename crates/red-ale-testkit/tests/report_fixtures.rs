//! The report fixture books (`fixtures/reports/`, rebuilt from GnuCash 5.17's report tests by
//! `scripts/report-fixtures/`) open in red-ale for writing and come back byte for byte.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{call, fixture, open, report_fixtures};
use red_ale_xml::{read_book, write_book_text};
use serde_json::json;

#[test]
fn every_report_fixture_is_listed() {
    let names = report_fixtures();
    let expected = [
        "average-balance",
        "balsheet-pnl",
        "category-average",
        "category-daily",
        "category-liability",
        "category-multi",
        "charts-daily",
        "charts-net",
        "net-days",
        "test-data",
    ];
    assert_eq!(names, expected.map(|n| format!("reports/{n}.gnucash")));
}

#[test]
fn every_report_fixture_opens_for_writing() {
    report_fixtures().iter().for_each(|name| {
        let (_book, mut d) = open(name);
        let info = call(&mut d, "book.info", &json!({}));
        assert_eq!(info["readOnly"], false, "{name}: {info}");
        call(&mut d, "book.close", &json!({ "discard": true }));
    });
}

#[test]
fn every_report_fixture_round_trips_byte_for_byte() {
    report_fixtures().iter().for_each(|name| {
        let bytes = std::fs::read(fixture(name)).unwrap();
        let text = String::from_utf8(bytes.clone()).expect("report fixtures are written uncompressed");
        let (book, mut residue) = read_book(&bytes).unwrap();
        assert!(residue.notices.unrecognised.is_empty(), "{name}: {:?}", residue.notices.unrecognised);
        assert!(write_book_text(&book, &residue) == text, "{name}: changed when written with its original elements");
        residue.forget_originals();
        assert!(write_book_text(&book, &residue) == text, "{name}: changed when every entity was regenerated");
    });
}
