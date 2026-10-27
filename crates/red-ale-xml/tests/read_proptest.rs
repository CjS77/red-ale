//! No file makes the streaming reader panic (`design/milestone3.md`, "No crashes"): a malformed book is an error.
//! Arbitrary bytes, books with bytes changed or cut short, the same gzipped, and the save's read-back of a changed
//! file against the book it should match.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;

use proptest::prelude::*;
use red_ale_xml::{read_back, read_book, read_book_from};

const BOOKS: [&str; 4] = [
    "synthetic/rich.gnucash",
    "synthetic/business.gnucash",
    "gnucash/sample1.gnucash",
    "piecash/invoices.gnucash",
];

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// `name`'s bytes (unzipped if need be) with `edits` applied: each replaces, inserts or deletes one byte, and
/// `cut`, when given, truncates the result there.
fn changed(name: &str, edits: &[(usize, u8, u8)], cut: Option<usize>) -> Vec<u8> {
    let text = red_ale_xml::book::decode(&fixture(name)).unwrap().0.into_bytes();
    let mut bytes = edits.iter().fold(text, |mut bytes, (at, op, byte)| {
        let at = at % (bytes.len() + 1);
        match op % 3 {
            0 if at < bytes.len() => bytes[at] = *byte,
            1 => bytes.insert(at, *byte),
            _ if at < bytes.len() => {
                bytes.remove(at);
            }
            _ => {}
        }
        bytes
    });
    if let Some(cut) = cut {
        bytes.truncate(cut % (bytes.len() + 1));
    }
    bytes
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

/// Markup-heavy text: tags and entities of GnuCash's format in arbitrary order.
fn xml_like() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("<?xml version=\"1.0\" encoding=\"utf-8\" ?>".to_owned()),
        Just("<gnc-v2 xmlns:gnc=\"http://www.gnucash.org/XML/gnc\">".to_owned()),
        Just("<gnc:book version=\"2.0.0\">".to_owned()),
        Just("<gnc:count-data cd:type=\"account\">".to_owned()),
        Just("<gnc:transaction version=\"2.0.0\">".to_owned()),
        Just("<trn:splits><trn:split>".to_owned()),
        Just("<split:value>".to_owned()),
        Just("<slot:value type=\"frame\">".to_owned()),
        Just("<act:id type=\"guid\">".to_owned()),
        "</[a-z:]{1,12}>",
        "&(amp|lt|gt|quot|#[0-9]{1,8}|#x[0-9a-fA-F]{1,6}|[a-z]{0,4});?",
        "[0-9a-f]{32}",
        "-?[0-9]{1,20}/[0-9]{1,20}",
        "\\PC{0,12}",
    ];
    prop::collection::vec(piece, 0..40).prop_map(|pieces| pieces.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn arbitrary_bytes_are_an_error_or_a_book(bytes in prop::collection::vec(any::<u8>(), 0..400)) {
        let _ = read_book(&bytes);
        let _ = read_book(&gzip(&bytes));
    }

    #[test]
    fn markup_in_any_order_is_an_error_or_a_book(text in xml_like()) {
        let _ = read_book(text.as_bytes());
    }

    #[test]
    fn changed_books_are_an_error_or_a_book(
        name in prop::sample::select(BOOKS.to_vec()),
        edits in prop::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 0..6),
        cut in prop::option::of(any::<usize>()),
        zipped in any::<bool>(),
    ) {
        let bytes = changed(name, &edits, cut);
        let bytes = if zipped { gzip(&bytes) } else { bytes };
        let _ = read_book_from(bytes.as_slice());
        let (book, _) = read_book(&fixture(name)).unwrap();
        let _ = read_back(bytes.as_slice(), &book);
    }
}
