//! What the SQLite write tests share: fixtures copied into temporary directories, a lock owner, and table dumps.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use red_ale_core::{Book, FixedEnv, Time64};
use red_ale_sql::{Findings, LockOwner, open_read_only};

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sqlite").join(name)
}

pub fn copy(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(fixture(name), &path).unwrap();
    (dir, path)
}

pub fn me() -> LockOwner {
    LockOwner::new("red-ale-test", 4242)
}

pub fn writable(_: &Book, f: &Findings) -> bool {
    f.is_empty()
}

pub fn env() -> FixedEnv {
    FixedEnv::new(Time64(1_717_412_340), 7)
}

/// Every table's rows (`rowid`, then `quote()` of each column) and `sqlite_master`, as the oracle's `dump.py` does.
pub fn dump(path: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut out: Vec<String> = conn
        .prepare("SELECT type || '|' || name || '|' || coalesce(sql, '') FROM sqlite_master ORDER BY rowid")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    tables.iter().for_each(|t| {
        let columns: Vec<String> = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{t}')"))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let quoted = columns.iter().map(|c| format!("quote(\"{c}\")")).collect::<Vec<_>>().join(" || '|' || ");
        let sql = format!("SELECT '{t}|' || rowid || '|' || {quoted} FROM \"{t}\" ORDER BY rowid");
        out.extend(conn.prepare(&sql).unwrap().query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap));
    });
    out
}

pub fn without_lock(rows: Vec<String>) -> Vec<String> {
    rows.into_iter().filter(|r| !r.starts_with("gnclock|")).collect()
}

pub fn gnclock(path: &Path) -> Vec<(String, i64)> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.prepare("SELECT Hostname, PID FROM gnclock ORDER BY rowid")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

pub fn read(path: &Path) -> Book {
    let (book, _, findings) = open_read_only(path).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    book
}

