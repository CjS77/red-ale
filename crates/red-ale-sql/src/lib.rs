//! The SQLite Backend: an anti-corruption layer between GnuCash 5.17's SQLite books and the core model
//! (`design/sql-fidelity.md`).
//!
//! Three layers: the connection ([`connection`]), the schema check ([`schema`]), and pure row decoders ([`rows`],
//! [`slots`], [`cell`], [`double`]) between a row's cells and the core model. Table and column names, `_num`/`_denom`
//! pairs, timestamp forms and slot rows stay in this crate, as element names stay in `red-ale-xml`.
//!
//! A read-only open writes nothing to the file or beside it. A normal open takes GnuCash's `gnclock` lock in the
//! transaction that reads the book ([`open`]); saving arrives with the row writers.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod cell;
mod connection;
pub mod double;
pub mod encode;
pub mod error;
mod lock;
mod read;
pub mod residue;
pub mod rows;
pub mod schema;
pub mod slots;
pub mod tables;

use std::path::{Path, PathBuf};

use red_ale_core::Book;

use rusqlite::{Connection, TransactionBehavior};

pub use error::SqlError;
pub use lock::LockOwner;
pub use residue::SqlResidue;

use lock::HeldLock;

/// What the schema check and the row readers found; each entry makes the book read-only
/// (`design/sql-fidelity.md` §1, "What opens read-only").
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Findings {
    /// Content red-ale can read but not edit safely, each naming its table and, for a row, its `rowid` and column.
    pub unrecognised: Vec<String>,
    /// `versions` rows other than GnuCash 5.17's.
    pub schema_versions: Vec<String>,
}

impl Findings {
    /// True when nothing was found.
    pub fn is_empty(&self) -> bool {
        self.unrecognised.is_empty() && self.schema_versions.is_empty()
    }
}

/// An open SQLite book: its path, its SQL Residue and, while the session may write, its connection and lock row.
/// Dropping it releases the lock.
#[derive(Debug)]
pub struct SqliteBook {
    path: PathBuf,
    residue: SqlResidue,
    /// The connection, kept for the session so the lock row and the data version belong to it. `None` once released,
    /// and for a read-only session.
    conn: Option<Connection>,
    lock: Option<HeldLock>,
}

impl SqliteBook {
    /// The book file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What the book holds that the Book does not.
    pub fn residue(&self) -> &SqlResidue {
        &self.residue
    }

    /// True while this session holds the book's lock.
    pub fn holds_lock(&self) -> bool {
        self.lock.is_some()
    }

    /// Delete this session's lock row, and only it, and close the connection. Does nothing if no lock is held; a row
    /// another program already deleted is not an error.
    pub fn release_lock(&mut self) -> Result<(), SqlError> {
        let Some(held) = self.lock.take() else { return Ok(()) };
        let mut conn = self.conn.take().ok_or_else(|| SqlError::NotABook("the lock is held without a connection".into()))?;
        lock::release(&mut conn, &held)
    }
}

impl Drop for SqliteBook {
    fn drop(&mut self) {
        // Nothing useful can be done with a failure here; `release_lock` reports it when called explicitly.
        let _ = self.release_lock();
    }
}

/// Read the book at `path` read-only, from one consistent snapshot. The file is opened with `SQLITE_OPEN_READONLY`
/// and left byte for byte as it was; a book in write-ahead-log mode or with a hot journal is refused, since even
/// reading it would write.
pub fn open_read_only(path: &Path) -> Result<(Book, SqliteBook, Findings), SqlError> {
    let mut conn = connection::open_read_only(path)?;
    let snapshot = conn.transaction().map_err(SqlError::from_sqlite)?;
    let (book, residue, findings) = read::read(&snapshot)?;
    snapshot.rollback().map_err(SqlError::from_sqlite)?;
    let opened = SqliteBook {
        path: path.to_owned(),
        residue,
        conn: None,
        lock: None,
    };
    Ok((book, opened, findings))
}

/// Open the book for writing: inside one `BEGIN IMMEDIATE`, refuse it if `gnclock` holds a row, read it, and insert
/// `owner`'s lock row only if `writable` agrees, then commit (`design/sql-fidelity.md` §5). The existence check, the
/// read and the insert are atomic, and a book that turns out read-only is never written at all: its transaction is
/// rolled back and its connection closed.
pub fn open(
    path: &Path,
    owner: &LockOwner,
    writable: &dyn Fn(&Book, &Findings) -> bool,
) -> Result<(Book, SqliteBook, Findings), SqlError> {
    let mut conn = connection::open_read_write(path)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(SqlError::from_write)?;
    lock::check_free(&tx)?;
    let (book, residue, findings) = read::read(&tx)?;
    let lock = if writable(&book, &findings) {
        let held = lock::insert(&tx, owner)?;
        tx.commit().map_err(SqlError::from_write)?;
        Some(held)
    } else {
        tx.rollback().map_err(SqlError::from_sqlite)?;
        None
    };
    let opened = SqliteBook {
        path: path.to_owned(),
        residue,
        conn: lock.is_some().then_some(conn),
        lock,
    };
    Ok((book, opened, findings))
}
