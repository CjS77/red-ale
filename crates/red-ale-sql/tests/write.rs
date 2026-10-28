//! Saving SQLite books (`design/sql-fidelity.md` §4, §9), on copies of the fixtures in
//! temporary directories.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use common::{copy, dump, env, me, read, writable};
use red_ale_core::{Book, SlotValue};
use red_ale_sql::{Faults, SqlError, SqliteBook, open};

/// Edit the description of the first transaction.
fn edit_description(book: &Book, text: &str) -> Book {
    let mut next = book.clone();
    let t = next.transactions.values_mut().next().unwrap();
    Arc::make_mut(t).description = Some(text.into());
    next
}

#[test]
fn a_save_writes_only_the_changed_row_and_reads_back() {
    let (_dir, path) = copy("synthetic/rich.gnucash");
    let (book, mut opened, _) = open(&path, &me(), &writable).unwrap();
    let before = dump(&path);
    let edited = edit_description(&book, "Edited");
    assert!(SqliteBook::has_changes(&book, &edited));
    assert!(!SqliteBook::has_changes(&book, &book.clone()));
    opened.save(&book, &edited, &mut env()).unwrap();
    let after = dump(&path);
    let changed: Vec<_> = after.iter().filter(|r| !before.contains(r)).collect();
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert!(changed[0].starts_with("transactions|1|") && changed[0].contains("'Edited'"), "{changed:?}");
    drop(opened);
    assert_eq!(read(&path), edited);
}

#[test]
fn another_writer_or_a_broken_lock_refuses_the_save_and_writes_nothing() {
    for breaker in ["DELETE FROM gnclock", "UPDATE transactions SET description = 'GnuCash' WHERE rowid = 2"] {
        let (_dir, path) = copy("synthetic/rich.gnucash");
        let (book, mut opened, _) = open(&path, &me(), &writable).unwrap();
        rusqlite::Connection::open(&path).unwrap().execute(breaker, []).unwrap();
        let before = dump(&path);
        let err = opened.save(&book, &edit_description(&book, "Edited"), &mut env()).unwrap_err();
        assert!(matches!(err, SqlError::BookChanged(_)), "{breaker}: {err}");
        assert_eq!(dump(&path), before, "{breaker}");
    }
}

#[test]
fn a_failure_at_any_statement_leaves_every_row_as_it_was() {
    let (_dir, path) = copy("synthetic/rich.gnucash");
    let (book, mut opened, _) = open(&path, &me(), &writable).unwrap();
    let before = dump(&path);
    let mut edited = edit_description(&book, "Edited");
    let t = Arc::make_mut(edited.transactions.values_mut().nth(1).unwrap());
    t.slots.set("notes", SlotValue::String("new note".into()));
    let removed = *edited.transactions.keys().nth(2).unwrap();
    edited.transactions.shift_remove(&removed);
    opened.set_faults(Faults {
        fail_statement: None,
        corrupt: true,
    });
    let err = opened.save(&book, &edited, &mut env()).unwrap_err();
    assert!(matches!(err, SqlError::Verification(_)), "{err}");
    assert_eq!(dump(&path), before);
    let statements = (1..500)
        .take_while(|n| {
            opened.set_faults(Faults {
                fail_statement: Some(*n),
                corrupt: false,
            });
            let failed = opened.save(&book, &edited, &mut env()).is_err();
            if failed {
                assert_eq!(dump(&path), before, "statement {n}");
            }
            failed
        })
        .count();
    assert!(statements > 5, "{statements}");
    drop(opened);
    assert_eq!(read(&path), edited);
}

#[test]
fn guid_reuse_deletes_the_orphans_an_entity_would_adopt() {
    let (_dir, path) = copy("synthetic/rich.gnucash");
    let (book, mut opened, _) = open(&path, &me(), &writable).unwrap();
    let mut edited = book.clone();
    let (guid, tx) = edited.transactions.shift_remove_index(0).unwrap();
    opened.save(&book, &edited, &mut env()).unwrap();
    let orphan_slot = format!("INSERT INTO slots (obj_guid, name, slot_type, int64_val) VALUES ('{guid}', 'stray', 1, 5)");
    // An orphan only another program could leave: red-ale removed the transaction and its slots.
    drop(opened);
    rusqlite::Connection::open(&path).unwrap().execute(&orphan_slot, []).unwrap();
    let (book, mut opened, findings) = open(&path, &me(), &writable).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let mut restored = book.clone();
    restored.transactions.insert(guid, tx);
    opened.save(&book, &restored, &mut env()).unwrap();
    drop(opened);
    assert_eq!(read(&path).transactions[&guid].slots, restored.transactions[&guid].slots);
}
