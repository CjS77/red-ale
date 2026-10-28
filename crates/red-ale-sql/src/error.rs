//! Errors from opening and reading SQLite books.

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
}
