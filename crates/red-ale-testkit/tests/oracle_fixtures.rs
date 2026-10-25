//! Every fixture, as GnuCash 5.17 reads it, matches red-ale's reading; and a book red-ale writes reads the same in
//! GnuCash.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_rpc::Dispatcher;
use red_ale_service::Settings;
use red_ale_testkit::{TempBook, diff, model, oracle, xml_fixtures};
use red_ale_xml::{read_book, write_book};
use serde_json::json;

fn compare(name: &str, gnucash: &serde_json::Value, ours: &serde_json::Value) -> Vec<String> {
    diff(gnucash, ours, &model::NOT_YET_COMPUTED).into_iter().take(10).map(|d| format!("{name}: {d}")).collect()
}

#[test]
fn oracle_is_the_pinned_version() {
    oracle::check_version().unwrap();
}

#[test]
fn every_fixture_reads_as_gnucash_reads_it() {
    let failures: Vec<String> = xml_fixtures()
        .iter()
        .flat_map(|name| {
            let book = TempBook::copy(name);
            let gnucash = oracle::dump(&book.path).unwrap_or_else(|e| panic!("{name}: {e}"));
            let (ours, _) = read_book(&std::fs::read(&book.path).unwrap()).unwrap();
            compare(name, &gnucash, &model::dump(&ours))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_fixture_regenerated_by_red_ale_reads_the_same_in_gnucash() {
    let failures: Vec<String> = xml_fixtures()
        .iter()
        .flat_map(|name| {
            let book = TempBook::copy(name);
            let (ours, mut residue) = read_book(&std::fs::read(&book.path).unwrap()).unwrap();
            residue.forget_originals();
            std::fs::write(&book.path, write_book(&ours, &residue, residue.compressed).unwrap()).unwrap();
            let gnucash = oracle::dump(&book.path).unwrap_or_else(|e| panic!("{name}: {e}"));
            compare(name, &gnucash, &model::dump(&ours))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_new_book_opens_in_gnucash() {
    let book = TempBook::new_path();
    let mut d = Dispatcher::new(Settings::default());
    d.call("book.create", json!({ "path": book.path_str() })).unwrap();
    d.call("book.close", json!({})).unwrap();
    let gnucash = oracle::dump(&book.path).unwrap();
    let (ours, _) = read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    let failures = compare("new book", &gnucash, &model::dump(&ours));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_writable_fixture_saved_unedited_is_the_same_book_to_gnucash() {
    let failures: Vec<String> = xml_fixtures()
        .iter()
        .flat_map(|name| {
            let book = TempBook::copy(name);
            let before = oracle::dump(&book.path).unwrap_or_else(|e| panic!("{name}: {e}"));
            let mut d = red_ale_testkit::dispatcher();
            red_ale_testkit::call(&mut d, "book.open", &json!({ "path": book.path_str() }));
            if red_ale_testkit::call(&mut d, "book.info", &json!({}))["readOnly"] == true {
                return Vec::new();
            }
            red_ale_testkit::call(&mut d, "book.save", &json!({}));
            red_ale_testkit::call(&mut d, "book.close", &json!({}));
            let after = oracle::dump(&book.path).unwrap_or_else(|e| panic!("{name}: {e}"));
            diff(&before, &after, &[]).into_iter().take(10).map(|d| format!("{name}: {d}")).collect()
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
