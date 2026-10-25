//! Errors from sessions and use cases.

use std::path::PathBuf;

use thiserror::Error;

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
}
