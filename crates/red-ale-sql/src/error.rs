//! Errors from opening, reading and writing SQLite books.

use thiserror::Error;

/// Why an SQLite book could not be read at all. Content red-ale can read but not edit safely is never an error: it
/// is a [`crate::Findings`] entry, and the book opens read-only.
#[derive(Debug, Error)]
pub enum SqlError {
    /// The file could not be read.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// The file is not an SQLite database.
    #[error("not an SQLite database: {0}")]
    NotADatabase(String),
    /// The database is in write-ahead-log mode, which GnuCash never sets and in which even reading can write beside
    /// the file (`design/sql-fidelity.md` §1, "Refused outright").
    #[error("the database is in write-ahead-log (WAL) mode, which GnuCash never uses; red-ale does not open it")]
    WriteAheadLog,
    /// A journal left by a crash: SQLite would roll it back on open, which writes the file.
    #[error("a hot journal ({0}) was left by a program that stopped while writing; open the book normally to recover it")]
    HotJournal(String),
    /// Another program holds a lock on the database past the busy timeout.
    #[error("the database is busy: another program is writing it")]
    Busy,
    /// The database is an SQLite file but not a GnuCash book that can be built.
    #[error("not a GnuCash SQL book: {0}")]
    NotABook(String),
    /// Another program holds the book's lock: its `gnclock` row (`design/sql-fidelity.md` §5).
    #[error("the book is locked by {} (process {}); close it there first", .0.hostname, .0.pid)]
    Locked(crate::LockOwner),
    /// red-ale may not write the file or the directory it is in, so it cannot take the lock.
    #[error("cannot write the book to lock it: {0}")]
    CannotWrite(String),
    /// Another program wrote the book or broke its lock since red-ale opened or last saved it; nothing was written.
    #[error("another program changed the book since red-ale opened it ({0}); nothing was written")]
    BookChanged(String),
    /// What a save wrote did not read back as written, so the save was rolled back.
    #[error("{0}")]
    Verification(String),
    /// The Book holds something an SQLite book cannot store (`design/sql-fidelity.md` §8).
    #[error("{0}")]
    Unstorable(String),
    /// SQLite failed.
    #[error("SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl SqlError {
    /// The error for a failure SQLite reports, recognising a busy database and a file that is not one.
    pub(crate) fn from_sqlite(e: rusqlite::Error) -> Self {
        match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => Self::Busy,
            Some(rusqlite::ErrorCode::NotADatabase) => Self::NotADatabase(e.to_string()),
            _ => Self::Sqlite(e),
        }
    }

    /// [`SqlError::from_sqlite`] for a write, where a file or directory red-ale may not write is its own error.
    pub(crate) fn from_write(e: rusqlite::Error) -> Self {
        match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::ReadOnly | rusqlite::ErrorCode::CannotOpen | rusqlite::ErrorCode::PermissionDenied) => {
                Self::CannotWrite(e.to_string())
            }
            _ => Self::from_sqlite(e),
        }
    }
}
