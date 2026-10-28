//! The Backend seam: one open book file, whatever its format, behind one trait, so the Session, the journal, the
//! read-only rule and every use case are the same for every format (`design/sql-fidelity.md` §2).
//!
//! Each Backend keeps its own Residue and lock; only it reads them.

use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use red_ale_core::{Book, Env};
use red_ale_xml::XmlResidue;

use crate::error::ServiceError;
use crate::session::{ReadOnlyReason, Settings};

mod sql;
mod xml;

pub use sql::SqliteBackend;
pub use xml::XmlBackend;

/// The format of a book file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// GnuCash's XML, gzipped or plain (`red-ale-xml`).
    Xml,
    /// An SQLite database (`red-ale-sql`), read-only for now.
    Sqlite,
}

impl Format {
    /// The format of the book `spec` names, at `path` ([`crate::session::resolve_path`]). The file's first bytes say,
    /// as GnuCash decides for a path or `file://` (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:1029-1068`). An
    /// `xml://` or `sqlite3://` URI chooses one backend, whose type check must still accept the file
    /// (`5.17:libgnucash/engine/qofsession.cpp:181-187`), so a scheme the content contradicts is an error.
    pub fn of(spec: &str, path: &Path) -> Result<Self, ServiceError> {
        let found = Self::sniff(path)?;
        match Self::of_scheme(spec) {
            Some(declared) if declared != found => Err(ServiceError::WrongFormat {
                path: path.to_owned(),
                declared,
                found,
            }),
            _ => Ok(found),
        }
    }

    /// The format a URI scheme names: `xml://` or `sqlite3://`. `file://` and plain paths name none.
    pub fn of_scheme(spec: &str) -> Option<Self> {
        let lower = spec.to_ascii_lowercase();
        if lower.starts_with("xml://") {
            Some(Self::Xml)
        } else if lower.starts_with("sqlite3://") {
            Some(Self::Sqlite)
        } else {
            None
        }
    }

    /// The format the file's first bytes say.
    fn sniff(path: &Path) -> Result<Self, ServiceError> {
        const MAGIC: &[u8] = b"SQLite format 3\0";
        let mut start = Vec::with_capacity(MAGIC.len());
        File::open(path)
            .and_then(|f| f.take(MAGIC.len() as u64).read_to_end(&mut start))
            .map_err(|source| ServiceError::Io {
                path: path.to_owned(),
                source,
            })?;
        Ok(if start == MAGIC { Self::Sqlite } else { Self::Xml })
    }

    /// The format's name, for messages.
    pub fn name(self) -> &'static str {
        match self {
            Self::Xml => "XML",
            Self::Sqlite => "SQLite",
        }
    }
}

/// Whether a session may write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Take no lock and never write.
    ReadOnly,
    /// Take the lock, unless the read-only rule finds a reason not to write.
    ReadWrite,
}

/// The Session's read-only rule: from the Book just read and what its Backend found, every reason the session must
/// be read-only. A Backend calls it while it can still decide not to hold the lock.
pub type ReadOnlyRule<'a> = &'a dyn Fn(&Book, &[ReadOnlyReason]) -> Vec<ReadOnlyReason>;

/// A book a Backend has opened.
#[derive(Debug)]
pub struct Opened {
    /// The Book as read.
    pub book: Book,
    /// Why the session is read-only; empty when it may write, in which case the Backend holds the lock.
    pub read_only: Vec<ReadOnlyReason>,
    /// The open file with its Residue and lock.
    pub backend: Box<dyn Backend>,
}

/// Open an existing book of `format`, taking its lock only if `access` is [`Access::ReadWrite`] and `rule` finds no
/// reason to be read-only.
pub fn open(path: &Path, format: Format, access: Access, rule: ReadOnlyRule<'_>) -> Result<Opened, ServiceError> {
    match format {
        Format::Xml => XmlBackend::open(path, access, rule),
        Format::Sqlite => SqliteBackend::open(path, access, rule),
    }
}

/// Create a new, empty book and lock it for writing. The file may not be written yet: the Session saves the new
/// Book at once, which writes it. SQLite books cannot be created yet.
pub fn create(path: &Path, format: Format, settings: &Settings, env: &mut dyn Env) -> Result<Opened, ServiceError> {
    match format {
        Format::Xml => XmlBackend::create(path, settings, env),
        Format::Sqlite => Err(ServiceError::SqliteCreate { path: path.to_owned() }),
    }
}

/// What a save may change about the file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SaveOptions {
    /// XML only: write gzipped (`true`) or plain; `None` keeps the file's current form.
    pub compress: Option<bool>,
}

/// What a save did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    /// The backup made before writing, if any.
    pub backup: Option<PathBuf>,
    /// XML only: whether the file was written gzipped.
    pub compressed: Option<bool>,
}

/// One open book file: its Residue, its lock and how to save it. Dropping it releases its lock.
pub trait Backend: fmt::Debug + Send {
    /// The file's format.
    fn format(&self) -> Format;

    /// The book file. Its journal and backups are named from it.
    fn path(&self) -> &Path;

    /// True while this Backend holds the book's lock.
    fn holds_lock(&self) -> bool;

    /// Release the lock now, reporting any failure. Releases only this Backend's own lock; does nothing if none is
    /// held.
    fn release_lock(&mut self) -> Result<(), ServiceError>;

    /// Refuse a Mutation whose result this format cannot hold without loss. Called for every Mutation, before the
    /// Session replaces its Book with `after`.
    fn check_storable(&self, before: &Book, after: &Book) -> Result<(), ServiceError>;

    /// The Book as this format requires it to be saved, if that differs from `book`. Called before a save of a dirty
    /// Book; the Session keeps the result once the save succeeds.
    fn prepare_save(&self, book: &Book) -> Option<Book>;

    /// Save `book`, given `saved`, the Book as read or last saved. Writes nothing unless what it wrote reads back as
    /// `book`.
    fn save(&mut self, saved: &Book, book: &Book, options: &SaveOptions, env: &mut dyn Env) -> Result<Saved, ServiceError>;

    /// XML only: whether the file is gzipped.
    fn compressed(&self) -> Option<bool>;

    /// XML only: the Residue, which Export Accounts writes from.
    fn xml_residue(&self) -> Option<&XmlResidue>;
}
