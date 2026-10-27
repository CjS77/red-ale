//! The synthetic books used to measure memory are in GnuCash 5.17's canonical form: GnuCash loads one and saves it
//! back byte for byte, so red-ale measured on it is measured on a book GnuCash wrote.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::synthetic::synthetic_book_text;
use red_ale_testkit::{TempBook, oracle};

#[test]
fn gnucash_saves_a_synthetic_book_byte_for_byte() {
    oracle::check_version().unwrap();
    let book = TempBook::new_path();
    let text = synthetic_book_text(500);
    std::fs::write(&book.path, &text).unwrap();
    let resaved = book.dir.path().join("resaved.gnucash");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/oracle-resave.py");
    oracle::run_script(&script, &[&book.path_str(), &resaved.display().to_string()]).unwrap();
    let gnucash = std::fs::read_to_string(&resaved).unwrap();
    let first = text.lines().zip(gnucash.lines()).position(|(a, b)| a != b);
    assert!(
        gnucash == text,
        "GnuCash rewrote the synthetic book; first difference at line {first:?}"
    );
}
