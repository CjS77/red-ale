//! The SQLite Backend: `red-ale-sql` behind [`Backend`], with GnuCash's `gnclock` lock, a backup before the first save
//! of a session that writes, and saves that write only what changed (`design/sql-fidelity.md`).

use std::cell::RefCell;
use std::path::Path;

use red_ale_core::ops::business::contacts::set_feature_used;
use red_ale_core::{Book, DomainError, Env};
use red_ale_sql::{Findings, LockOwner, SqlError, SqliteBook};
use red_ale_xml::XmlResidue;

use super::{Access, Backend, Format, Opened, ReadOnlyRule, SaveOptions, Saved};
use crate::error::ServiceError;
use crate::files::backup_with;
use crate::session::{ReadOnlyReason, Settings};

/// The Book feature GnuCash sets on every SQLite load (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:879-880`).
pub const ISO_DATES_FEATURE: &str = "ISO-8601 formatted date strings in SQLite3 databases.";

/// An open SQLite book.
#[derive(Debug)]
pub struct SqliteBackend {
    book: SqliteBook,
    /// Whether this session has made its backup.
    backed_up: bool,
}

/// This process as the lock row names it: the host name as `gethostname` gives it, and the process ID, as GnuCash
/// writes them (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:155-159`).
fn owner() -> LockOwner {
    let uname = rustix::system::uname();
    LockOwner::new(&uname.nodename().to_string_lossy(), i64::from(std::process::id()))
}

impl SqliteBackend {
    /// Read the book. For [`Access::ReadWrite`], read it inside the transaction that takes the lock, and take the lock
    /// only if `rule` finds no reason to be read-only.
    pub(super) fn open(path: &Path, access: Access, rule: ReadOnlyRule<'_>) -> Result<Opened, ServiceError> {
        let reasons = RefCell::new(Vec::new());
        let (book, sqlite, found) = match access {
            Access::ReadOnly => red_ale_sql::open_read_only(path),
            Access::ReadWrite => {
                let writable = |book: &Book, found: &Findings| {
                    let r = rule(book, &findings(found.clone()));
                    let ok = r.is_empty();
                    *reasons.borrow_mut() = r;
                    ok
                };
                red_ale_sql::open(path, &owner(), &writable)
            }
        }
        .map_err(|e| error(path, e))?;
        let read_only = match access {
            Access::ReadOnly => rule(&book, &findings(found)),
            Access::ReadWrite => reasons.into_inner(),
        };
        let backend = Self {
            book: sqlite,
            backed_up: false,
        };
        Ok(Opened {
            book,
            read_only,
            backend: Box::new(backend),
        })
    }

    /// Write a new book with GnuCash 5.17's schema and lock it: the empty Book, with the ISO-dates feature GnuCash's
    /// first load would add (`design/sql-fidelity.md` §7), and the template root its `books` row names, so the first
    /// scheduled transaction goes under that root.
    pub(super) fn create(path: &Path, _settings: &Settings, env: &mut dyn Env) -> Result<Opened, ServiceError> {
        let mut book = Book::new_empty(env);
        set_feature_used(&mut book, ISO_DATES_FEATURE);
        red_ale_core::ops::scheduled::template_root(&mut book, env);
        let sqlite = red_ale_sql::create(path, &book, &owner(), env).map_err(|e| error(path, e))?;
        let backend = Self {
            book: sqlite,
            backed_up: false,
        };
        Ok(Opened {
            book,
            read_only: Vec::new(),
            backend: Box::new(backend),
        })
    }

    fn error(&self, e: SqlError) -> ServiceError {
        error(self.book.path(), e)
    }
}

/// A failure, in the Session's terms.
fn error(path: &Path, e: SqlError) -> ServiceError {
    let path = path.to_owned();
    match e {
        SqlError::Io(source) => ServiceError::Io { path, source },
        SqlError::Busy => ServiceError::Busy { path },
        SqlError::Locked(holder) => ServiceError::SqlLocked {
            path,
            hostname: holder.hostname,
            pid: holder.pid,
        },
        SqlError::CannotWrite(reason) => ServiceError::CannotLock { lock_file: path, reason },
        SqlError::BookChanged(detail) => ServiceError::BookChanged { path, detail },
        SqlError::Verification(detail) => ServiceError::Verification { detail },
        SqlError::Unstorable(reason) => DomainError::invalid(reason).into(),
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

    fn holds_lock(&self) -> bool {
        self.book.holds_lock()
    }

    fn release_lock(&mut self) -> Result<(), ServiceError> {
        self.book.release_lock().map_err(|e| self.error(e))
    }

    /// Refuse what an SQLite book can't hold (`design/sql-fidelity.md` §8).
    fn check_storable(&self, before: &Book, after: &Book) -> Result<(), ServiceError> {
        SqliteBook::unstorable(before, after).map_or(Ok(()), |reason| Err(DomainError::invalid(reason).into()))
    }

    /// Add the ISO-dates feature when the book lacks it, as GnuCash's load does; the save writes it with the Book's
    /// slot rows (`design/sql-fidelity.md` §4, "The ISO-dates Book feature").
    fn prepare_save(&self, book: &Book) -> Option<Book> {
        let declared = red_ale_core::book_options::features(book)
            .iter()
            .any(|(name, _)| name == ISO_DATES_FEATURE);
        (!declared).then(|| {
            let mut prepared = book.clone();
            set_feature_used(&mut prepared, ISO_DATES_FEATURE);
            prepared
        })
    }

    /// Write the rows that changed in one checked transaction, after backing the book up if this is the session's
    /// first save that writes. A save with nothing to write writes nothing and makes no backup.
    fn save(&mut self, saved: &Book, book: &Book, options: &SaveOptions, env: &mut dyn Env) -> Result<Saved, ServiceError> {
        if options.compress.is_some() {
            return Err(DomainError::invalid("an SQLite book is not compressed; compressed applies to XML books only").into());
        }
        if !SqliteBook::has_changes(saved, book) {
            return Ok(Saved {
                backup: None,
                compressed: None,
            });
        }
        let backup = if self.backed_up {
            None
        } else {
            let sqlite = &self.book;
            Some(backup_with(sqlite.path(), |tmp| sqlite.backup_to(tmp).map_err(|e| error(tmp, e)))?)
        };
        self.backed_up = true;
        self.book.save(saved, book, env).map_err(|e| self.error(e))?;
        Ok(Saved { backup, compressed: None })
    }

    fn compressed(&self) -> Option<bool> {
        None
    }

    fn xml_residue(&self) -> Option<&XmlResidue> {
        None
    }

    fn versions(&self) -> Option<&[(String, i64)]> {
        Some(&self.book.residue().versions)
    }
}
