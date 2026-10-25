//! Round-trip tests: books written by GnuCash 5.17 must come back byte for byte, both when unchanged entities are
//! written from their original elements and when every entity is generated from the model.

use red_ale_xml::{read_book, write_book_text};

const CANONICAL: [&str; 3] = ["gnucash/sample1.gnucash", "gnucash/bug-746937-sx-split-order.gnucash", "synthetic/business.gnucash"];

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
