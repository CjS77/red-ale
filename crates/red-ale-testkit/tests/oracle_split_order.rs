//! A transaction's splits in a book GnuCash didn't write: GnuCash sorts them as it loads the book, those with a value
//! of zero or more first and otherwise as the file has them (`5.17:libgnucash/engine/Transaction.cpp:540-553`,
//! `:1508-1510`), and saves them in that order. red-ale reads them in that order and, saving with no edits, writes
//! what GnuCash writes.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, fixture, oracle};
use red_ale_xml::{read_book, write_book_text};

/// `text` with the splits of the first transaction after `marker` put in `order`, given as indices into the order
/// the file has them in.
fn reorder_splits(text: &str, marker: &str, order: &[usize]) -> String {
    let start = text.find(marker).unwrap();
    let open = start + text[start..].find("  <trn:splits>\n").unwrap() + "  <trn:splits>\n".len();
    let close = open + text[open..].find("  </trn:splits>\n").unwrap();
    let blocks: Vec<&str> = text[open..close].split_inclusive("    </trn:split>\n").collect();
    assert_eq!(blocks.len(), order.len());
    let reordered: String = order.iter().map(|&i| blocks[i]).collect();
    format!("{}{reordered}{}", &text[..open], &text[close..])
}

/// `rich.gnucash`, which GnuCash 5.17 saves byte for byte, with two transactions' splits out of GnuCash's order: Buy
/// ACME's as 9.95, -1009.95, 1000.00, and the Rent template's given values -4.56 then 4.56.
fn unsorted_rich() -> String {
    let text = std::fs::read_to_string(fixture("synthetic/rich.gnucash")).unwrap();
    let text = reorder_splits(&text, "<trn:description>Buy ACME</trn:description>", &[1, 2, 0]);
    let rent = "<trn:description>Rent</trn:description>\n  <trn:splits>";
    let (head, tail) = text.split_at(text.find(rent).unwrap());
    let tail = tail
        .replacen("<split:value>0/100</split:value>", "<split:value>-456/100</split:value>", 1)
        .replacen("<split:value>0/100</split:value>", "<split:value>456/100</split:value>", 1);
    format!("{head}{tail}")
}

#[test]
fn red_ale_saves_unsorted_splits_as_gnucash_saves_them() {
    oracle::check_version().unwrap();
    let book = TempBook::new_path();
    let text = unsorted_rich();
    std::fs::write(&book.path, &text).unwrap();
    let resaved = book.dir.path().join("resaved.gnucash");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/oracle-resave.py");
    oracle::run_script(&script, &[&book.path_str(), &resaved.display().to_string()]).unwrap();
    let gnucash = std::fs::read_to_string(&resaved).unwrap();
    assert_ne!(gnucash, text, "GnuCash sorted the splits");
    let (ours, residue) = read_book(text.as_bytes()).unwrap();
    let written = write_book_text(&ours, &residue);
    let first = gnucash.lines().zip(written.lines()).position(|(a, b)| a != b);
    assert!(written == gnucash, "red-ale wrote another order; first difference at line {first:?}");
}
