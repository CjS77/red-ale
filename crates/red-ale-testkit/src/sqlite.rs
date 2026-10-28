//! SQLite books for tests and measurements: a Book written whole into a new SQLite book, and the rows that differ
//! between two SQLite books.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use red_ale_core::ops::business::contacts::set_feature_used;
use red_ale_core::{Account, AccountType, Book, Env, Frame};
use red_ale_service::backend::ISO_DATES_FEATURE;
use red_ale_sql::LockOwner;
use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags};

/// Write `book` as a new SQLite book at `path`, which must not exist: create it from the root alone, as `book.create`
/// does, then save the whole Book into it, as `red-ale-sql`'s `every_fixture_written_whole_into_a_new_book…` test
/// does. The copy gets the ISO-dates Book feature GnuCash adds on every SQLite load, so the first save after opening
/// it writes nothing for that. Frame and template-root GUIDs come from `env`.
pub fn write_book(book: &Book, path: &Path, env: &mut dyn Env) {
    let mut whole = book.clone();
    set_feature_used(&mut whole, ISO_DATES_FEATURE);
    let mut skeleton = Book::new_empty(env);
    skeleton.guid = book.guid;
    skeleton.root = book.root;
    let root = Account::new(book.root, "Root Account", AccountType::Root, None, None);
    skeleton.accounts = [(book.root, Arc::new(root))].into_iter().collect();
    skeleton.slots = Frame::new();
    let owner = LockOwner::new("red-ale-testkit", i64::from(std::process::id()));
    let mut made = red_ale_sql::create(path, &skeleton, &owner, env).unwrap_or_else(|e| panic!("creating {}: {e}", path.display()));
    made.save(&skeleton, &whole, env)
        .unwrap_or_else(|e| panic!("writing the Book into {}: {e}", path.display()));
    made.release_lock()
        .unwrap_or_else(|e| panic!("releasing the lock of {}: {e}", path.display()));
}

/// True when the file at `path` is an SQLite database.
pub fn is_sqlite(path: &Path) -> bool {
    const MAGIC: &[u8; 16] = b"SQLite format 3\0";
    let mut start = [0; 16];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut start)).is_ok() && &start == MAGIC
}

/// The rows of one table that differ between two SQLite books, each as its `rowid` and cells.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChangedRows {
    /// Rows of the second book the first lacks: added, or the new form of a changed row.
    pub new: Vec<Vec<Value>>,
    /// Rows of the first book the second lacks: deleted, or the old form of a changed row.
    pub gone: Vec<Vec<Value>>,
}

/// The rows that differ between two SQLite books, by table, comparing each row's `rowid` and every column. Tables
/// that differ in neither direction are left out. SQLite compares the rows, so only the differing rows are read into
/// memory, whatever the books' size.
pub fn changed_rows(before: &Path, after: &Path) -> BTreeMap<String, ChangedRows> {
    let conn = Connection::open_with_flags(after, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap_or_else(|e| panic!("opening {}: {e}", after.display()));
    conn.execute("ATTACH DATABASE ?1 AS before", [before.display().to_string()])
        .unwrap_or_else(|e| panic!("attaching {}: {e}", before.display()));
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM main.sqlite_master WHERE type = 'table' ORDER BY name")
        .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
        .expect("listing the tables");
    tables
        .into_iter()
        .map(|t| {
            let rows = ChangedRows {
                new: only(&conn, &t, "main", "before"),
                gone: only(&conn, &t, "before", "main"),
            };
            (t, rows)
        })
        .filter(|(_, rows)| *rows != ChangedRows::default())
        .collect()
}

/// The rows of `table` in schema `a` that schema `b` lacks, in `rowid` order.
fn only(conn: &Connection, table: &str, a: &str, b: &str) -> Vec<Vec<Value>> {
    let sql = format!("SELECT rowid, * FROM {a}.\"{table}\" EXCEPT SELECT rowid, * FROM {b}.\"{table}\" ORDER BY 1");
    let mut statement = conn.prepare(&sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
    let columns = statement.column_count();
    statement
        .query_map([], |r| (0..columns).map(|i| r.get::<_, Value>(i)).collect())
        .and_then(Iterator::collect)
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}
