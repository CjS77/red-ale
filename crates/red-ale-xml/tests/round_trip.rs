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
    let budget = std::sync::Arc::make_mut(book.budgets.get_index_mut(0).unwrap().1);
    budget.name = "Budget 2025".into();
    let written = write_book_text(&book, &residue);
    assert_eq!(
        written,
        text.replacen("<bgt:name>Budget 2024</bgt:name>", "<bgt:name>Budget 2025</bgt:name>", 1)
    );
}

#[test]
fn scheduled_and_template_transactions_are_read_into_the_book_and_written_from_it() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    let (mut book, residue) = read_book(text.as_bytes()).unwrap();
    let template_account = red_ale_core::Guid::parse("22f2905cbcfa0b910c0dbc348cfc1161").unwrap();
    let sx = book.scheduled.values().next().unwrap();
    assert_eq!((sx.name.as_str(), sx.template_account), ("Monthly rent", Some(template_account)));
    assert_eq!(book.scheduled_of_template_account(template_account).map(|s| s.guid), Some(sx.guid));
    let templates = &book.templates;
    assert_eq!((templates.accounts.len(), templates.transactions.len()), (2, 1));
    assert!(
        templates.accounts.keys().all(|g| !book.accounts.contains_key(g)),
        "not in the Account tree"
    );
    assert!(templates.transactions.keys().all(|g| !book.transactions.contains_key(g)));
    let sx = std::sync::Arc::make_mut(book.scheduled.get_index_mut(0).unwrap().1);
    sx.name = "Rent".into();
    sx.advance_remind_days = 6;
    let t = std::sync::Arc::make_mut(book.templates.transactions.get_index_mut(0).unwrap().1);
    t.description = Some("Rent & rates".into());
    let expected = text
        .replacen("<sx:name>Monthly rent</sx:name>", "<sx:name>Rent</sx:name>", 1)
        .replacen("<sx:advanceRemindDays>5<", "<sx:advanceRemindDays>6<", 1)
        .replacen(
            "<trn:description>Rent</trn:description>",
            "<trn:description>Rent &amp; rates</trn:description>",
            1,
        );
    assert_same("rich with an edited schedule", &expected, &write_book_text(&book, &residue));
}

#[test]
fn template_and_scheduled_transactions_generated_differently_keep_their_original_text() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    let other = text
        .replacen("<sx:enabled>y</sx:enabled>", "<sx:enabled>yes</sx:enabled>", 1)
        .replacen(
            "  <trn:description>Rent</trn:description>",
            "  <trn:num></trn:num>\n  <trn:description>Rent</trn:description>",
            1,
        );
    let (book, residue) = read_book(other.as_bytes()).unwrap();
    assert!(!book.scheduled[0].enabled, "only y is true");
    assert_eq!(residue.originals_kept(), 2);
    assert_same("rich with other spellings", &other, &write_book_text(&book, &residue));
}

#[test]
fn a_book_with_no_scheduled_transactions_writes_no_template_block() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    let (mut book, residue) = read_book(text.as_bytes()).unwrap();
    let root = book.templates.root.unwrap();
    book.scheduled.clear();
    book.templates.transactions.clear();
    book.templates.accounts.retain(|g, _| *g == root);
    let written = write_book_text(&book, &residue);
    assert!(!written.contains("<gnc:template-transactions>") && !written.contains("<gnc:schedxaction"));
    assert!(!written.contains("cd:type=\"schedxaction\""));
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
    assert_same(
        "rich with an explicit weekend adjustment",
        &other,
        &write_book_text(&book, &residue),
    );
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

const BUY_ACME: &str = "<trn:description>Buy ACME</trn:description>";

/// GnuCash sorts a transaction's splits as it loads it, those with a value of zero or more first and otherwise as
/// the file has them (`5.17:libgnucash/backend/xml/gnc-transaction-xml-v2.cpp:565-572`;
/// `5.17:libgnucash/engine/Transaction.cpp:540-553`, `:1508-1510`), so it writes them back in that order.
#[test]
fn splits_another_writer_left_unsorted_are_read_and_written_as_gnucash_sorts_them() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    let (gnucash, _) = read_book(text.as_bytes()).unwrap();
    // Buy ACME is 1000.00, 9.95, -1009.95 as GnuCash wrote it; another writer put the negative split first.
    let other = reorder_splits(&text, BUY_ACME, &[2, 0, 1]);
    assert_ne!(other, text);
    let (book, residue) = read_book(other.as_bytes()).unwrap();
    assert!(book == gnucash, "the splits are in GnuCash's order");
    assert_eq!(residue.originals_kept(), 0, "the order alone keeps no original text");
    assert_same("rich with unsorted splits", &text, &write_book_text(&book, &residue));
}

/// The sort is stable: splits on the same side keep the order the file gives them, even where GnuCash wrote another.
#[test]
fn the_split_sort_keeps_the_file_order_on_each_side() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    let (book, residue) = read_book(reorder_splits(&text, BUY_ACME, &[1, 2, 0]).as_bytes()).unwrap();
    let expected = reorder_splits(&text, BUY_ACME, &[1, 0, 2]);
    assert!(read_book(expected.as_bytes()).unwrap().0 == book, "9.95 stays before 1000.00");
    assert_same("rich with 9.95 first", &expected, &write_book_text(&book, &residue));
}

/// A transaction kept as its original text for another reason is kept with its splits in GnuCash's order.
#[test]
fn original_text_kept_for_a_transaction_has_its_splits_sorted() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    let with_num = text.replacen(&format!("  {BUY_ACME}"), &format!("  <trn:num></trn:num>\n  {BUY_ACME}"), 1);
    let (book, residue) = read_book(reorder_splits(&with_num, BUY_ACME, &[2, 0, 1]).as_bytes()).unwrap();
    assert_eq!(residue.originals_kept(), 1);
    assert_same("rich with an empty num and unsorted splits", &with_num, &write_book_text(&book, &residue));
}

/// Template transactions are read by the same parser and committed the same way, so they are sorted too
/// (`5.17:libgnucash/backend/xml/gnc-schedxaction-xml-v2.cpp:828-841`; `io-gncxml-v2.cpp:271-283`, `:336-340`).
#[test]
fn template_transaction_splits_are_sorted_as_they_are_read() {
    let text = String::from_utf8(fixture("synthetic/rich.gnucash")).unwrap();
    let rent = "<trn:description>Rent</trn:description>\n  <trn:splits>";
    let (head, tail) = text.split_at(text.find(rent).unwrap());
    // The template's two splits are zero; give them opposite signs, the negative one first.
    let tail = tail
        .replacen("<split:value>0/100</split:value>", "<split:value>-456/100</split:value>", 1)
        .replacen("<split:value>0/100</split:value>", "<split:value>456/100</split:value>", 1);
    let negative_first = format!("{head}{tail}");
    let (book, residue) = read_book(negative_first.as_bytes()).unwrap();
    let values: Vec<String> = book.templates.transactions[0].splits.iter().map(|s| s.value.to_string()).collect();
    assert_eq!(values, ["456/100", "-456/100"]);
    let sorted = reorder_splits(&negative_first, rent, &[1, 0]);
    assert_same("rich with a sorted template", &sorted, &write_book_text(&book, &residue));
}
