//! Round-trip tests: books written by GnuCash 5.17 must come back byte for byte, both when unchanged entities are
//! written from their original elements and when every entity is generated from the model.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_xml::{read_book, write_book_text};

/// Books in GnuCash 5.17's canonical form: GnuCash itself loads and saves each one byte for byte.
const CANONICAL: [&str; 12] = [
    "gnucash/sample1.gnucash",
    "gnucash/bug-746937-sx-split-order.gnucash",
    "synthetic/business.gnucash",
    "synthetic/business-editable.gnucash",
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
        let line = expected
            .lines()
            .zip(actual.lines())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| expected.lines().count().min(actual.lines().count()));
        let context = |s: &str| s.lines().skip(line.saturating_sub(3)).take(6).collect::<Vec<_>>().join("\n");
        panic!(
            "{name} differs at line {}:\n--- expected\n{}\n--- actual\n{}",
            line + 1,
            context(expected),
            context(actual)
        );
    }
}

#[test]
fn unchanged_books_round_trip_byte_for_byte() {
    for name in CANONICAL {
        let bytes = fixture(name);
        let (book, residue) = read_book(&bytes).unwrap();
        assert!(
            residue.notices.unrecognised.is_empty(),
            "{name}: {:?}",
            residue.notices.unrecognised
        );
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

#[test]
fn a_split_with_no_account_round_trips() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    let start = text.find("<split:account type=\"guid\">").unwrap() + "<split:account type=\"guid\">".len();
    let orphaned = format!("{}{}{}", &text[..start], "0".repeat(32), &text[start + 32..]);
    let (book, mut residue) = read_book(orphaned.as_bytes()).unwrap();
    assert!(book.transactions.values().flat_map(|t| &t.splits).any(|s| s.account.is_none()));
    residue.forget_originals();
    let (again, _) = read_book(write_book_text(&book, &residue).as_bytes()).unwrap();
    assert_eq!(again, book);
}

#[test]
fn books_gnucash_wrote_keep_no_original_text() {
    for name in CANONICAL {
        let (_, residue) = read_book(&fixture(name)).unwrap();
        assert_eq!(residue.originals_kept(), 0, "{name}");
    }
}

#[test]
fn an_entity_generated_differently_keeps_its_original_text() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    // GnuCash 5.17 writes an empty num as nothing, so `<trn:num></trn:num>` from another writer survives only as text.
    let at = text.find("  <trn:date-posted>").unwrap();
    let other = format!("{}  <trn:num></trn:num>\n{}", &text[..at], &text[at..]);
    let (book, residue) = read_book(other.as_bytes()).unwrap();
    assert_eq!(residue.originals_kept(), 1);
    assert_same("rich with an empty num", &other, &write_book_text(&book, &residue));
}

#[test]
fn budgets_are_read_into_the_book_and_written_from_it() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    let (mut book, residue) = read_book(text.as_bytes()).unwrap();
    let budget = book.budgets.values().next().unwrap();
    let food = red_ale_core::Guid::parse("1feca2abf92347288c268ebf93753ec4").unwrap();
    assert_eq!((budget.name.as_str(), budget.num_periods), ("Budget 2024", 12));
    assert_eq!(budget.note(food, 0), Some("January food"));
    assert!(residue.out_of_scope_elements().all(|e| e.name != "gnc:budget"));
    let budget = std::sync::Arc::make_mut(book.budgets.get_index_mut(0).unwrap().1);
    budget.name = "Budget 2025".into();
    let written = write_book_text(&book, &residue);
    assert_eq!(written, text.replacen("<bgt:name>Budget 2024</bgt:name>", "<bgt:name>Budget 2025</bgt:name>", 1));
}

#[test]
fn a_budget_generated_differently_keeps_its_original_text() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    // The budget's recurrence is the one indented by four spaces; the scheduled transaction's is indented by six.
    let explicit_none = "\n    </recurrence:start>\n    <recurrence:weekend_adj>none</recurrence:weekend_adj>\n";
    let other = text.replacen("\n    </recurrence:start>\n", explicit_none, 1);
    assert_ne!(other, text);
    let (book, residue) = read_book(other.as_bytes()).unwrap();
    assert_eq!(residue.originals_kept(), 1);
    assert_same("rich with an explicit weekend adjustment", &other, &write_book_text(&book, &residue));
}

#[test]
fn an_escaped_book_version_is_written_back_escaped() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    let other = text.replacen("<gnc:book version=\"2.0.0\">", "<gnc:book version=\"2.0.0&amp;&quot;x&lt;\">", 1);
    assert_ne!(other, text);
    let (book, residue) = read_book(other.as_bytes()).unwrap();
    let written = write_book_text(&book, &residue);
    assert_same("rich with an escaped book version", &other, &written);
    assert_eq!(read_book(written.as_bytes()).unwrap().0, book);
}

/// Every XML fixture, canonical or not: SQL books and version 1 files are refused, so they are left out.
fn xml_fixtures() -> Vec<String> {
    let root = format!("{}/../../fixtures", env!("CARGO_MANIFEST_DIR"));
    let mut names: Vec<String> = ["gnucash", "piecash", "synthetic"]
        .iter()
        .flat_map(|dir| {
            std::fs::read_dir(format!("{root}/{dir}"))
                .unwrap()
                .map(move |e| format!("{dir}/{}", e.unwrap().file_name().to_string_lossy()))
        })
        .filter(|n| n.ends_with(".gnucash") && !n.ends_with(".sqlite.gnucash") && n != "gnucash/currency_tree_xml.gnucash")
        .collect();
    names.sort();
    names
}

#[test]
fn every_xml_fixture_reads_back_as_the_same_book() {
    let names = xml_fixtures();
    assert!(names.len() >= 15, "{names:?}");
    names.iter().for_each(|name| {
        let (book, mut residue) = read_book(&fixture(name)).unwrap();
        let (kept, _) = read_book(write_book_text(&book, &residue).as_bytes()).unwrap();
        assert!(kept == book, "{name}: changed when written with its original elements");
        residue.forget_originals();
        let (regenerated, _) = read_book(write_book_text(&book, &residue).as_bytes()).unwrap();
        assert!(regenerated == book, "{name}: changed when every entity was regenerated");
    });
}
