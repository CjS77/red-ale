//! The Printable Invoice (`standard/invoice.rhai`) on a Book changed in place, for what the dispatcher cannot set: the
//! Book's Fancy Date Format, which no method writes. INV stands for `5.17:gnucash/report/reports/standard/invoice.scm`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Host, fixture, registry_with};
use red_ale_core::SlotValue;
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;
use red_ale_report::result::{Block, Table};
use serde_json::json;

const PRINTABLE_INVOICE: &str = "5123a759ceb9483abf2182d01c140e8d";

/// inv-8 of `reports/transaction.gnucash`, posted 1980-09-08 10:59 UTC and due at midnight before it.
const INV_8: &str = "30000000000000000000000000000007";

/// The first two cells' texts of each row of the invoice details table.
fn details(book: red_ale_core::Book) -> Vec<(String, String)> {
    let reg = registry_with(Library::builtin(), &[], Limits::default());
    let r = Host::new(book)
        .run(&reg, PRINTABLE_INVOICE, &json!({ "General": { "Invoice Number": INV_8 } }))
        .unwrap();
    let Block::Group(row) = &r.sections[0].blocks[1] else {
        panic!("the first layout row is a group")
    };
    let Block::Table(t) = &row.blocks[1] else {
        panic!("Row 1 Right is the details table")
    };
    rows(t)
}

fn rows(t: &Table) -> Vec<(String, String)> {
    let text = |c: Option<&Option<red_ale_report::result::Cell>>| c.and_then(Option::as_ref).and_then(|c| c.text.clone());
    t.rows
        .iter()
        .map(|r| (text(r.cells.first()).unwrap_or_default(), text(r.cells.get(1)).unwrap_or_default()))
        .collect()
}

/// `make-date-row` prints with `gnc:options-fancy-date` (INV:222-228, :237): the Book's Fancy Date Format's `custom`
/// string as a strftime format (`5.17:bindings/guile/options.scm:403-407`), or `qof-print-date`'s text when the Book
/// has none.
#[test]
fn dates_follow_the_books_fancy_date_format() {
    let book = fixture("transaction.gnucash");
    let plain = details(book.clone());
    assert_eq!(
        &plain[..2],
        [
            ("Date:".to_owned(), "09/08/80".to_owned()),
            ("Due Date:".to_owned(), "09/08/80".to_owned())
        ]
    );
    let mut fancy = book;
    fancy
        .slots
        .set("options/Business/Fancy Date Format/custom", SlotValue::String("%d %B %Y".into()));
    let dated = details(fancy);
    assert_eq!(
        &dated[..2],
        [
            ("Date:".to_owned(), "08 September 1980".to_owned()),
            ("Due Date:".to_owned(), "08 September 1980".to_owned())
        ]
    );
}
