//! The SQLite Backend: an anti-corruption layer between GnuCash 5.17's SQLite books and the core model
//! (`design/sql-fidelity.md`).
//!
//! Three layers: the connection ([`connection`]), the schema check ([`schema`]), and pure row decoders ([`rows`],
//! [`slots`], [`cell`], [`double`]) between a row's cells and the core model. Table and column names, `_num`/`_denom`
//! pairs, timestamp forms and slot rows stay in this crate, as element names stay in `red-ale-xml`.
//!
//! A read-only open writes nothing to the file or beside it. A normal open takes GnuCash's `gnclock` lock in the
//! transaction that reads the book ([`open`]); a save writes only the rows of what changed, in one transaction that is
//! read back before it commits.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod cell;
mod connection;
mod create;
pub mod double;
pub mod encode;
pub mod error;
mod lock;
mod read;
pub mod residue;
mod save;
mod storable;
pub mod rows;
pub mod schema;
pub mod slots;
pub mod tables;

use std::path::{Path, PathBuf};

use red_ale_core::{Book, Env};

use rusqlite::{Connection, TransactionBehavior};

pub use error::SqlError;
pub use create::create;
pub use lock::LockOwner;
pub use residue::SqlResidue;

use lock::HeldLock;
#[doc(hidden)]
pub use save::sql::Faults;

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
    /// `PRAGMA data_version` after the open or the last save.
    data_version: i64,
    faults: Faults,
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

    /// Why `after` can't be stored in an SQLite book, if it can't (`design/sql-fidelity.md` §8). Only what changed
    /// from `before` is checked.
    pub fn unstorable(before: &Book, after: &Book) -> Option<String> {
        storable::unstorable(before, after)
    }

    /// Whether a save of `book`, given `saved`, the Book as read or last saved, would write anything.
    pub fn has_changes(saved: &Book, book: &Book) -> bool {
        save::has_changes(saved, book)
    }

    /// Write the rows that differ between `saved` and `book` in one checked transaction (`design/sql-fidelity.md`
    /// §4): `BEGIN IMMEDIATE`; check `gnclock` holds exactly this session's row and no other connection has committed
    /// since the open or the last save (`PRAGMA data_version`), else [`SqlError::BookChanged`]; write; read
    /// everything written back and compare it; `COMMIT`. On any failure the transaction is rolled back and the file is
    /// as it was. Frame and list GUIDs come from `env`.
    pub fn save(&mut self, saved: &Book, book: &Book, env: &mut dyn Env) -> Result<(), SqlError> {
        let faults = self.faults;
        let (Some(conn), Some(held)) = (self.conn.as_mut(), self.lock.as_ref()) else {
            return Err(SqlError::NotABook("the book is not open for writing".into()));
        };
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(SqlError::from_write)?;
        if !lock::is_held(&tx, held)? {
            return Err(SqlError::BookChanged("its gnclock row is not red-ale's alone: the lock was broken".into()));
        }
        if connection::data_version(&tx)? != self.data_version {
            return Err(SqlError::BookChanged("another connection wrote it".into()));
        }
        let mut residue = self.residue.clone();
        save::write(&tx, &mut residue, saved, book, env, faults)?;
        tx.commit().map_err(SqlError::from_write)?;
        self.residue = residue;
        self.data_version = connection::data_version(conn)?;
        Ok(())
    }

    /// Make saves misbehave, for tests of their atomicity.
    #[doc(hidden)]
    pub fn set_faults(&mut self, faults: Faults) {
        self.faults = faults;
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
        data_version: 0,
        faults: Faults::default(),
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
    let data_version = connection::data_version(&conn)?;
    let opened = SqliteBook {
        path: path.to_owned(),
        residue,
        conn: lock.is_some().then_some(conn),
        lock,
        data_version,
        faults: Faults::default(),
    };
    Ok((book, opened, findings))
}
