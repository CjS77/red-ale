//! Errors from sessions and use cases.

use std::path::PathBuf;

use red_ale_core::ops::scheduled::ScheduledError;
use thiserror::Error;

use crate::proposals::ProposalKind;

/// Why a session operation failed.
#[derive(Debug, Error)]
pub enum ServiceError {
    /// Another program holds the book's lock.
    #[error("the book is locked by another program ({lock_file}); close it there first")]
    Locked {
        /// The lock file.
        lock_file: PathBuf,
    },
    /// The lock file could not be created.
    #[error("cannot create the lock file {lock_file}: {reason}")]
    CannotLock {
        /// The lock file.
        lock_file: PathBuf,
        /// The operating system's reason.
        reason: String,
    },
    /// A file operation failed.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The cause.
        source: std::io::Error,
    },
    /// The backup could not be made, so the book was not saved.
    #[error("cannot make the backup {path}: {source}")]
    Backup {
        /// The backup that could not be made.
        path: PathBuf,
        /// The cause.
        source: std::io::Error,
    },
    /// The path names an SQL book; v0.1 handles XML books only.
    #[error("{path} is an SQL book; red-ale v0.1 opens XML books only (SQLite arrives in v0.3)")]
    SqlBook {
        /// The path or URI.
        path: String,
    },
    /// The URI scheme is not supported.
    #[error("unsupported URI {uri}; use a path or an xml:// URI")]
    UnsupportedUri {
        /// The URI.
        uri: String,
    },
    /// The file could not be read or written as an XML book.
    #[error("{path}: {source}")]
    Xml {
        /// The book.
        path: PathBuf,
        /// The cause.
        source: red_ale_xml::XmlError,
    },
    /// The session is read-only.
    #[error("the book is open read-only: {reason}")]
    ReadOnly {
        /// Why.
        reason: String,
    },
    /// The file to create already exists.
    #[error("{path} already exists")]
    Exists {
        /// The file.
        path: PathBuf,
    },
    /// The written book did not read back identically, so it was not saved.
    #[error("save refused: {detail}; the book on disk is unchanged")]
    Verification {
        /// What differed.
        detail: String,
    },
    /// A domain rule refused the operation.
    #[error(transparent)]
    Domain(#[from] red_ale_core::DomainError),
    /// The journal could not be read.
    #[error("{path}: {source}")]
    Journal {
        /// The journal.
        path: PathBuf,
        /// The cause.
        source: crate::journal::JournalError,
    },
    /// The journal is the one this session is writing.
    #[error("{path} is the journal this session is writing; replay another")]
    CurrentJournal {
        /// The journal.
        path: PathBuf,
    },
    /// No book is open.
    #[error("no book is open")]
    NoSession,
    /// A book is already open.
    #[error("a book is already open: {path}")]
    AlreadyOpen {
        /// The open book.
        path: PathBuf,
    },
    /// An import file can't be read as its format at all (`design/api/import.md`, "Errors").
    #[error("import file {file}{}: {message}", .line.map(|l| format!(", line {l}")).unwrap_or_default())]
    ImportFile {
        /// The file's index among the import files.
        file: usize,
        /// The line, from 1, when one line is the cause.
        line: Option<usize>,
        /// Why, as a code: `encoding`, `tokenize` or `columns`.
        reason: &'static str,
        /// What is wrong, in GnuCash's words where it has them.
        message: String,
    },
    /// No proposal of this kind is held with this ID: unknown, applied, discarded, pushed out by newer ones, or made
    /// before the book was opened.
    #[error("no {kind} {id} is held; {}", kind.again())]
    ProposalNotFound {
        /// The kind of proposal asked for.
        kind: ProposalKind,
        /// The ID asked for.
        id: String,
    },
    /// The Book has changed since the proposal was made.
    #[error("the book has changed since {kind} {id} was made (revision {proposal_revision}, now {book_revision}); {}", kind.again())]
    StaleProposal {
        /// The kind of proposal.
        kind: ProposalKind,
        /// The proposal's ID.
        id: String,
        /// The Book revision it was made at.
        proposal_revision: u64,
        /// The Book revision now.
        book_revision: u64,
    },
    /// A scheduled transaction's instance can't be proposed or applied (`design/api/scheduled-transactions.md`).
    #[error(transparent)]
    Scheduled(#[from] ScheduledError),
}
