//! The SQLite Backend: `red-ale-sql` behind [`Backend`]. This version reads only: every SQLite book opens read-only,
//! so it never takes GnuCash's `gnclock` row and never writes (`design/milestone4.md`, "SQLite books").

use std::path::Path;

use red_ale_core::{Book, Env};
use red_ale_sql::{Findings, SqlError, SqliteBook};
use red_ale_xml::XmlResidue;

use super::{Access, Backend, Format, Opened, ReadOnlyRule, SaveOptions, Saved};
use crate::error::ServiceError;
use crate::session::ReadOnlyReason;

/// An open SQLite book, held read-only.
#[derive(Debug)]
pub struct SqliteBackend {
    book: SqliteBook,
}

impl SqliteBackend {
    /// Read the book read-only. An open for writing gets the reason [`ReadOnlyReason::WritingNotSupported`] after
    /// whatever `rule` finds.
    pub(super) fn open(path: &Path, access: Access, rule: ReadOnlyRule<'_>) -> Result<Opened, ServiceError> {
        let (book, sqlite, found) = red_ale_sql::open_read_only(path).map_err(|e| error(path, e))?;
        let mut read_only = rule(&book, &findings(found));
        if access == Access::ReadWrite {
            read_only.push(ReadOnlyReason::WritingNotSupported);
        }
        Ok(Opened {
            book,
            read_only,
            backend: Box::new(Self { book: sqlite }),
        })
    }
}

/// A failure to read the book, in the Session's terms: I/O and a busy database are not about the file's format.
fn error(path: &Path, e: SqlError) -> ServiceError {
    let path = path.to_owned();
    match e {
        SqlError::Io(source) => ServiceError::Io { path, source },
        SqlError::Busy => ServiceError::Busy { path },
        source => ServiceError::Sql { path, source },
    }
}

/// What the read found that makes the book read-only, in the Session's terms.
fn findings(found: Findings) -> Vec<ReadOnlyReason> {
    let Findings {
        unrecognised,
        schema_versions,
    } = found;
    [
        (!unrecognised.is_empty()).then_some(ReadOnlyReason::UnrecognisedContent(unrecognised)),
        (!schema_versions.is_empty()).then_some(ReadOnlyReason::SchemaVersions(schema_versions)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

impl Backend for SqliteBackend {
    fn format(&self) -> Format {
        Format::Sqlite
    }

    fn path(&self) -> &Path {
        self.book.path()
    }

    /// A read-only book is never locked.
    fn holds_lock(&self) -> bool {
        false
    }

    fn release_lock(&mut self) -> Result<(), ServiceError> {
        Ok(())
    }

    /// The session is read-only, so no Mutation reaches this.
    fn check_storable(&self, _before: &Book, _after: &Book) -> Result<(), ServiceError> {
        Ok(())
    }

    fn prepare_save(&self, _book: &Book) -> Option<Book> {
        None
    }

    /// Never writes. The Session refuses to save a read-only book before it gets here; this is the second guard.
    fn save(&mut self, _saved: &Book, _book: &Book, _options: &SaveOptions, _env: &mut dyn Env) -> Result<Saved, ServiceError> {
        Err(ServiceError::ReadOnly {
            reason: ReadOnlyReason::WritingNotSupported.to_string(),
        })
    }

    fn compressed(&self) -> Option<bool> {
        None
    }

    fn xml_residue(&self) -> Option<&XmlResidue> {
        None
    }
}
