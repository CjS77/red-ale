//! A one-transaction save of a large SQLite book writes only that transaction's rows (`design/sql-fidelity.md` §9,
//! "Scale"), on the 5,000-transaction synthetic book written as SQLite. The example `memory` (`rows`) runs the same
//! check on the 100,000-transaction book.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::TempBook;
use red_ale_testkit::memory::one_transaction_save;
use red_ale_testkit::synthetic::write_synthetic_sqlite;
use rusqlite::types::Value;

#[test]
fn a_one_transaction_save_of_the_synthetic_book_writes_only_that_transactions_rows() {
    let book = TempBook::new_path();
    write_synthetic_sqlite(5_000, &book.path);
    let (posted, changed) = one_transaction_save(&book.path);
    let tables: Vec<&str> = changed.keys().map(String::as_str).collect();
    assert_eq!(tables, ["slots", "splits", "sqlite_sequence", "transactions"], "{changed:#?}");
    let new = |table: &str| changed[table].new.len();
    assert_eq!((new("transactions"), new("splits"), new("slots")), (1, 2, 1), "{changed:#?}");
    let owned = Value::Text(posted);
    ["transactions", "splits", "slots"].iter().for_each(|table| {
        let rows = &changed[*table];
        assert!(rows.gone.is_empty(), "{table}: {:?}", rows.gone);
        assert!(
            rows.new.iter().all(|r| r.contains(&owned)),
            "{table}: a row not of {owned:?}: {:?}",
            rows.new
        );
    });
    // The slot row's `id` is `AUTOINCREMENT`: inserting it moves the `slots` row of SQLite's own sequence table on.
    let sequence = &changed["sqlite_sequence"];
    let slots = Value::Text("slots".into());
    assert!(
        sequence.new.len() == 1 && sequence.gone.len() == 1 && sequence.new[0].contains(&slots) && sequence.gone[0].contains(&slots),
        "{sequence:?}"
    );
}
