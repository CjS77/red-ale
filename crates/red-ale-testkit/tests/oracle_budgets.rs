//! Budgets against GnuCash 5.17 (`oracle/budgets.py`): GnuCash writes a book with budgets holding every kind of
//! amount and note, red-ale reads it, regenerates each budget exactly, and after an unrelated edit saves a book in which
//! GnuCash reads every budget as it read its own.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, dispatcher, oracle};
use serde_json::{Value, json};

fn script(args: &[&str]) -> String {
    oracle::check_version().unwrap();
    oracle::run_script(&oracle::scripts_dir().join("budgets.py"), args).unwrap_or_else(|e| panic!("oracle: {e}"))
}

/// Every budget as GnuCash reads it.
fn gnucash_budgets(book: &TempBook) -> Value {
    serde_json::from_str(&script(&["dump", &book.path_str()])).unwrap()
}

#[test]
fn budgets_red_ale_saved_read_back_in_gnucash_unchanged() {
    let book = TempBook::copy("synthetic/rich.gnucash");
    script(&["make", &book.path_str()]);
    let expected = gnucash_budgets(&book);
    let plan = &expected["00000000000000000000000000b0d6e7"];
    assert_eq!(plan["amounts"]["1feca2abf92347288c268ebf93753ec4"], json!({ "0": "1234/100", "1": "0/1" }));
    assert_eq!(plan["recurrence"]["weekendAdjust"], "forward");
    assert_eq!(plan["amounts"]["a8cf578f585b4708856fdf5b8928e552"], json!({ "11": "-5/3" }));
    assert_eq!(plan["notes"]["1feca2abf92347288c268ebf93753ec4"], json!({ "2": "Groceries & <more>" }));
    let existing = &expected["0123456789abcdef0123456789abcdef"]["notes"]["1feca2abf92347288c268ebf93753ec4"];
    assert_eq!(*existing, json!({ "0": "January food", "11": "December" }));

    let (read, residue) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    assert_eq!(read.budgets.len(), 2);
    assert_eq!(residue.originals_kept(), 0, "red-ale generates GnuCash's budgets as GnuCash wrote them");

    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    call(&mut d, "account.update", &json!({ "account": "Assets:Cash", "description": "petty" }));
    call(&mut d, "book.close", &json!({ "save": true }));
    let (text, _) = red_ale_xml::book::decode(&std::fs::read(&book.path).unwrap()).unwrap();
    assert!(text.contains("<act:description>petty</act:description>"), "the book was rewritten");
    assert!(text.contains("<slot:key>15</slot:key>"), "the amount past the last period is kept");
    assert_eq!(gnucash_budgets(&book), expected);
}
