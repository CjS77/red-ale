//! The SQLite Backend: an anti-corruption layer between GnuCash 5.17's SQLite books and the core model
//! (`design/sql-fidelity.md`).
//!
//! Three layers: the connection ([`connection`]), the schema check ([`schema`]), and pure row decoders ([`rows`],
//! [`slots`], [`cell`], [`double`]) between a row's cells and the core model. Table and column names, `_num`/`_denom`
//! pairs, timestamp forms and slot rows stay in this crate, as element names stay in `red-ale-xml`.
//!
//! This version reads only: books open read-only, and nothing is ever written to the file or beside it.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod cell;
mod connection;
pub mod double;
pub mod encode;
pub mod error;
mod read;
pub mod residue;
pub mod rows;
pub mod schema;
pub mod slots;
pub mod tables;

use std::path::{Path, PathBuf};

use red_ale_core::Book;

pub use error::SqlError;
pub use residue::SqlResidue;

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

/// An open SQLite book: its path and its SQL Residue.
#[derive(Debug, Clone)]
pub struct SqliteBook {
    path: PathBuf,
    residue: SqlResidue,
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
    };
    Ok((book, opened, findings))
}
