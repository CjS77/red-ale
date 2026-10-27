//! The `.LCK` lock GnuCash and red-ale use to keep one writer per XML book.
//!
//! `5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:634-673`: an empty file `<book>.LCK`, created with
//! `O_CREAT | O_EXCL` and mode 0600, held for the life of the session. Its existence is the lock; there is no
//! staleness check, so red-ale never breaks a lock on its own.

use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use crate::error::ServiceError;

/// A held lock. Dropping it removes the `.LCK` file, so the lock is released on every exit path, including errors
/// and panics that unwind.
#[derive(Debug)]
pub struct BookLock {
    path: PathBuf,
    released: bool,
}

/// The lock file's path for a book.
pub fn lock_path(book: &Path) -> PathBuf {
    let mut name = book.as_os_str().to_owned();
    name.push(".LCK");
    PathBuf::from(name)
}

impl BookLock {
    /// Take the lock on `book`, failing if GnuCash or another red-ale holds it.
    pub fn acquire(book: &Path) -> Result<Self, ServiceError> {
        let path = lock_path(book);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| lock_error(&path, &e))?;
        Ok(Self { path, released: false })
    }

    /// The lock file's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Release the lock now, reporting any failure.
    pub fn release(mut self) -> Result<(), ServiceError> {
        self.released = true;
        std::fs::remove_file(&self.path).map_err(|e| ServiceError::Io {
            path: self.path.clone(),
            source: e,
        })
    }
}

impl Drop for BookLock {
    fn drop(&mut self) {
        if !self.released {
            // Nothing useful can be done with a failure here; `release` reports it when called explicitly.
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn lock_error(path: &Path, e: &io::Error) -> ServiceError {
    match e.kind() {
        io::ErrorKind::AlreadyExists => ServiceError::Locked {
            lock_file: path.to_owned(),
        },
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem | io::ErrorKind::StorageFull => ServiceError::CannotLock {
            lock_file: path.to_owned(),
            reason: e.to_string(),
        },
        _ => ServiceError::CannotLock {
            lock_file: path.to_owned(),
            reason: e.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_lock_fails_and_drop_releases() {
        let dir = tempfile::tempdir().unwrap();
        let book = dir.path().join("book.gnucash");
        let lock = BookLock::acquire(&book).unwrap();
        let meta = std::fs::metadata(lock.path()).unwrap();
        assert_eq!(meta.len(), 0);
        assert!(matches!(BookLock::acquire(&book), Err(ServiceError::Locked { .. })));
        drop(lock);
        assert!(!lock_path(&book).exists());
        BookLock::acquire(&book).unwrap().release().unwrap();
    }
}
