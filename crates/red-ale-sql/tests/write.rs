//! Saving and creating SQLite books (`design/sql-fidelity.md` §4, §6, §7, §9), on copies of the fixtures in
//! temporary directories.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use common::{copy, dump, env, fixture, gnclock, me, read, without_lock, writable};
use red_ale_core::{Book, Frame, SlotValue};
use red_ale_sql::{Faults, SqlError, SqliteBook, create, open, open_read_only};
use rusqlite::types::Value::{Integer, Text};

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
    assert!(
        changed[0].starts_with("transactions|1|") && changed[0].contains("'Edited'"),
        "{changed:?}"
    );
    drop(opened);
    assert_eq!(read(&path), edited);
}

#[test]
fn another_writer_or_a_broken_lock_refuses_the_save_and_writes_nothing() {
    for breaker in [
        "DELETE FROM gnclock",
        "UPDATE transactions SET description = 'GnuCash' WHERE rowid = 2",
    ] {
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

/// The lines of two debug dumps that differ, for a readable failure.
fn differing(a: &str, b: &str) -> Vec<String> {
    a.lines()
        .zip(b.lines())
        .enumerate()
        .filter(|(_, (x, y))| x != y)
        .take(12)
        .map(|(i, (x, y))| format!("{i}: {x} | {y}"))
        .collect()
}

/// The fixture's Book written into a new book, from its root alone: every row of every entity is inserted.
fn rebuild(name: &str) -> (Book, Book) {
    let original = read(&fixture(name));
    let (_, residue, _) = open_read_only(&fixture(name)).unwrap();
    let mut skeleton = Book::new_empty(&mut env());
    skeleton.guid = original.guid;
    skeleton.root = original.root;
    // A new book's root has no commodity; the save gives it the fixture's, if any.
    let root = red_ale_core::Account::new(original.root, "Root Account", red_ale_core::AccountType::Root, None, None);
    skeleton.accounts = [(original.root, Arc::new(root))].into_iter().collect();
    skeleton.slots = Frame::new();
    skeleton.templates.root = Some(residue.residue().template_root);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.gnucash");
    let mut made = create(&path, &skeleton, &me(), &mut env()).unwrap();
    skeleton.templates.root = None;
    made.save(&skeleton, &original, &mut env()).unwrap();
    drop(made);
    (original, read(&path))
}

#[test]
fn every_fixture_written_whole_into_a_new_book_reads_back_the_same() {
    let root = fixture("");
    let mut names: Vec<String> = ["gnucash", "piecash", "reports", "synthetic"]
        .iter()
        .flat_map(|d| {
            std::fs::read_dir(root.join(d))
                .unwrap()
                .map(move |e| format!("{d}/{}", e.unwrap().file_name().to_string_lossy()))
        })
        .filter(|n| n.ends_with(".gnucash"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 37);
    names.iter().for_each(|name| {
        let (mut original, rebuilt) = rebuild(name);
        // A new book has its root account first, as GnuCash's `sync` writes it; a fixture may not.
        let root = original.accounts.shift_remove(&original.root).unwrap();
        original.accounts.shift_insert(0, root.guid, root);
        let (a, b) = (format!("{original:#?}"), format!("{rebuilt:#?}"));
        assert!(a == b, "{name}: {:#?}", differing(&a, &b));
    });
}

#[test]
fn a_created_book_has_gnucash_s_schema_versions_and_pragmas() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.gnucash");
    let mut env = env();
    let mut book = Book::new_empty(&mut env);
    // The service gives a new book its template root, which the `books` row names and the read keeps.
    red_ale_core::ops::scheduled::template_root(&mut book, &mut env);
    let made = create(&path, &book, &me(), &mut env).unwrap();
    assert!(made.holds_lock());
    assert!(matches!(create(&path, &book, &me(), &mut env), Err(SqlError::Io(_))));
    drop(made);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let pragma = |p: &str| {
        conn.query_row(&format!("PRAGMA {p}"), [], |r| r.get::<_, rusqlite::types::Value>(0))
            .unwrap()
    };
    assert_eq!(
        [
            "page_size",
            "encoding",
            "auto_vacuum",
            "journal_mode",
            "user_version",
            "application_id"
        ]
        .map(pragma),
        [
            Integer(4096),
            Text("UTF-8".into()),
            Integer(0),
            Text("delete".into()),
            Integer(0),
            Integer(0)
        ]
    );
    drop(conn);
    let (read_back, _, findings) = open_read_only(&path).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(read_back, book);
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["new.gnucash"]);
}

#[test]
fn a_backup_holds_the_rows_and_no_lock() {
    let (dir, path) = copy("synthetic/rich.gnucash");
    let before = dump(&path);
    let (_, opened, _) = open(&path, &me(), &writable).unwrap();
    let backup = dir.path().join("backup.gnucash");
    opened.backup_to(&backup).unwrap();
    assert_eq!(dump(&backup), before);
    assert!(gnclock(&backup).is_empty());
    assert_eq!(without_lock(dump(&path)), without_lock(before));
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

#[test]
fn what_an_sqlite_book_cannot_hold_is_refused() {
    let book = read(&fixture("synthetic/rich.gnucash"));
    let refused = |edit: &dyn Fn(&mut Book)| {
        let mut after = book.clone();
        edit(&mut after);
        SqliteBook::unstorable(&book, &after)
    };
    assert_eq!(refused(&|_| {}), None);
    let deferred = refused(&|b| {
        let sx = Arc::make_mut(b.scheduled.get_index_mut(0).unwrap().1);
        sx.deferred.push(red_ale_core::DeferredInstance {
            last: None,
            remaining: 0,
            instance_count: 1,
        });
    });
    assert!(deferred.is_some_and(|r| r.contains("postponed")));
    let nested = refused(&|b| b.slots.set("zz", SlotValue::List(vec![SlotValue::List(Vec::new())])));
    assert!(nested.is_some_and(|r| r.contains("list inside a list")));
    let nul = refused(&|b| Arc::make_mut(b.transactions.get_index_mut(0).unwrap().1).description = Some("a\0b".into()));
    assert!(nul.is_some_and(|r| r.contains("NUL")));
    let late = refused(&|b| Arc::make_mut(b.transactions.get_index_mut(0).unwrap().1).date_posted = red_ale_core::Time64(253_402_214_400));
    assert!(late.is_some_and(|r| r.contains("outside the range")));
    let nan = refused(&|b| b.slots.set("zz", SlotValue::Double("nan".into())));
    assert!(nan.is_some_and(|r| r.contains("not a number")));
}
