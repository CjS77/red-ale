//! GnuCash's SQL lock (`design/sql-fidelity.md` §5): taken in the transaction that reads the book, only for a book
//! that may be written, and released, only red-ale's own row, on close and on drop.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{copy, dump, gnclock, me, read, writable};
use red_ale_sql::{SqlError, open};

#[test]
fn a_normal_open_and_close_leaves_every_row_as_it_was() {
    for name in ["synthetic/rich.gnucash", "synthetic/business.gnucash", "gnucash/sample1.gnucash", "piecash/book_schtx.gnucash"] {
        let (_dir, path) = copy(name);
        let before = dump(&path);
        let (_, mut opened, findings) = open(&path, &me(), &writable).unwrap();
        assert!(findings.is_empty() && opened.holds_lock(), "{name}");
        assert_eq!(gnclock(&path), [("red-ale-test".to_owned(), 4242)], "{name}");
        opened.release_lock().unwrap();
        assert!(!opened.holds_lock());
        assert_eq!(dump(&path), before, "{name}");
    }
}

#[test]
fn dropping_the_book_releases_the_lock() {
    let (_dir, path) = copy("synthetic/rich.gnucash");
    let (_, opened, _) = open(&path, &me(), &writable).unwrap();
    assert!(matches!(open(&path, &me(), &writable), Err(SqlError::Locked(_))));
    drop(opened);
    assert!(gnclock(&path).is_empty());
}

#[test]
fn a_row_gnucash_holds_refuses_the_open_and_stays() {
    let (_dir, path) = copy("synthetic/rich.gnucash");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("INSERT INTO gnclock VALUES ('otherhost', 99)", []).unwrap();
    drop(conn);
    let before = dump(&path);
    let err = open(&path, &me(), &writable).unwrap_err();
    assert!(matches!(&err, SqlError::Locked(h) if h.hostname == "otherhost" && h.pid == 99), "{err}");
    assert_eq!(dump(&path), before);
    // A read-only open works while it is held.
    read(&path);
}

#[test]
fn a_book_that_turns_out_read_only_is_never_written() {
    let (_dir, path) = copy("synthetic/rich.gnucash");
    let before = std::fs::read(&path).unwrap();
    let (_, opened, _) = open(&path, &me(), &|_, _| false).unwrap();
    assert!(!opened.holds_lock());
    drop(opened);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn a_book_without_gnclock_takes_no_lock() {
    let (_dir, path) = copy("synthetic/rich.gnucash");
    rusqlite::Connection::open(&path).unwrap().execute("DROP TABLE gnclock", []).unwrap();
    let (_, opened, findings) = open(&path, &me(), &writable).unwrap();
    assert!(findings.unrecognised.iter().any(|f| f.contains("gnclock")), "{findings:?}");
    assert!(!opened.holds_lock());
}

#[test]
fn only_red_ales_own_row_is_released() {
    let (_dir, path) = copy("synthetic/rich.gnucash");
    let (_, mut opened, _) = open(&path, &me(), &writable).unwrap();
    // GnuCash's "Open Anyway" deletes every row and inserts its own.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DELETE FROM gnclock; INSERT INTO gnclock VALUES ('otherhost', 99);").unwrap();
    opened.release_lock().unwrap();
    assert_eq!(gnclock(&path), [("otherhost".to_owned(), 99)]);
}

