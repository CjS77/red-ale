//! Round-trip tests: books written by GnuCash 5.17 must come back byte for byte, both when unchanged entities are
//! written from their original elements and when every entity is generated from the model.

use red_ale_xml::{read_book, write_book_text};

/// Books in GnuCash 5.17's canonical form: GnuCash itself loads and saves each one byte for byte.
const CANONICAL: [&str; 11] = [
    "gnucash/sample1.gnucash",
    "gnucash/bug-746937-sx-split-order.gnucash",
    "synthetic/business.gnucash",
    "synthetic/rich.gnucash",
    "piecash/all_account_types.gnucash",
    "piecash/book_prices.gnucash",
    "piecash/book_schtx.gnucash",
    "piecash/ghost_kvp_scheduled_transaction.gnucash",
    "piecash/investment.gnucash",
    "piecash/invoices.gnucash",
    "piecash/simple_sample.gnucash",
];

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn text(bytes: &[u8]) -> String {
    red_ale_xml::book::decode(bytes).unwrap().0
}

fn assert_same(name: &str, expected: &str, actual: &str) {
    if expected != actual {
        let line = expected.lines().zip(actual.lines()).position(|(a, b)| a != b).unwrap_or_else(|| expected.lines().count().min(actual.lines().count()));
        let context = |s: &str| s.lines().skip(line.saturating_sub(3)).take(6).collect::<Vec<_>>().join("\n");
        panic!("{name} differs at line {}:\n--- expected\n{}\n--- actual\n{}", line + 1, context(expected), context(actual));
    }
}

#[test]
fn unchanged_books_round_trip_byte_for_byte() {
    for name in CANONICAL {
        let bytes = fixture(name);
        let (book, residue) = read_book(&bytes).unwrap();
        assert!(residue.notices.unrecognised.is_empty(), "{name}: {:?}", residue.notices.unrecognised);
        assert_same(name, &text(&bytes), &write_book_text(&book, &residue));
    }
}

#[test]
fn generators_reproduce_gnucash_output() {
    for name in CANONICAL {
        let bytes = fixture(name);
        let (book, mut residue) = read_book(&bytes).unwrap();
        residue.forget_originals();
        assert_same(name, &text(&bytes), &write_book_text(&book, &residue));
    }
}

#[test]
fn edits_are_written_and_only_the_edited_entity_changes() {
    let bytes = fixture("synthetic/rich.gnucash");
    let (mut book, residue) = read_book(&bytes).unwrap();
    let guid = *book.accounts.keys().nth(1).unwrap();
    std::sync::Arc::make_mut(book.accounts.get_mut(&guid).unwrap()).name = "Renamed & <new>".into();
    let out = write_book_text(&book, &residue);
    assert!(out.contains("<act:name>Renamed &amp; &lt;new&gt;</act:name>"));
    let original = text(&bytes);
    let changed: Vec<_> = original.lines().zip(out.lines()).filter(|(a, b)| a != b).collect();
    assert_eq!(changed.len(), 1, "{changed:?}");
}
