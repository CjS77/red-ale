//! A Since Last Run creating 5,000 transactions is one Mutation (`design/milestone4.md`, "Done when"): ten daily
//! scheduled transactions of 500 occurrences each, added to a synthetic book, all fall due by the dispatcher's clock,
//! and one `scheduledTransaction.apply` creates every one. The example `memory` (`since-last-run`) times and measures
//! the same run on the 100,000-transaction book.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::TempBook;
use red_ale_testkit::since_last_run::{add_daily_schedules, run};
use red_ale_testkit::synthetic::{synthetic_book_text, write_synthetic_sqlite};

const SCHEDULES: usize = 10;
const OCCURRENCES: usize = 500;
const CREATED: usize = SCHEDULES * OCCURRENCES;

#[test]
fn a_since_last_run_creating_5000_transactions_is_one_mutation() {
    let book = TempBook::new_path();
    std::fs::write(&book.path, synthetic_book_text(100)).unwrap();
    add_daily_schedules(&book.path, SCHEDULES, OCCURRENCES);
    let r = run(&book.path, None);
    assert_eq!((r.to_create, r.created), (CREATED, CREATED));
    assert_eq!(r.transactions, (100, 100 + CREATED as u64));
    assert_eq!(r.revisions, (0, 1), "the apply is one step of the Book revision");
    // The journal is GnuCash's, a record per transaction change, not per Mutation: a `B` and a `C` record for each
    // transaction added, all written once the Mutation has succeeded.
    assert_eq!((r.journal_records, r.journal_transactions), (2 * CREATED, CREATED));
}

/// The same on the SQLite copy of the book, written by red-ale's SQLite writer: one Mutation, and the save writes the
/// 5,000 transactions' rows.
#[test]
fn a_since_last_run_creating_5000_transactions_on_an_sqlite_book_is_one_mutation() {
    let book = TempBook::new_path();
    write_synthetic_sqlite(100, &book.path);
    add_daily_schedules(&book.path, SCHEDULES, OCCURRENCES);
    let r = run(&book.path, None);
    assert_eq!((r.to_create, r.created), (CREATED, CREATED));
    assert_eq!(r.transactions, (100, 100 + CREATED as u64));
    assert_eq!(r.revisions, (0, 1), "the apply is one step of the Book revision");
    assert_eq!((r.journal_records, r.journal_transactions), (2 * CREATED, CREATED));
}
