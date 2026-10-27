//! The XML Backend: `red-ale-xml` behind [`Backend`], with GnuCash's `.LCK` lock and safe replace.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use red_ale_core::{Book, Env};
use red_ale_xml::{ReadBack, XmlResidue, read_back, read_book_from, write_book_to};

use super::{Access, Backend, Format, Opened, ReadOnlyRule, SaveOptions, Saved};
use crate::error::ServiceError;
use crate::files::replace_book_file;
use crate::lock::BookLock;
use crate::session::{ReadOnlyReason, Settings};

/// How much of a book file is read or written at a time.
const IO_BUFFER: usize = 64 * 1024;

/// An open XML book: its path, its Residue and, while the session may write, its lock.
#[derive(Debug)]
pub struct XmlBackend {
    path: PathBuf,
    residue: XmlResidue,
    lock: Option<BookLock>,
}

impl XmlBackend {
    /// Lock (for [`Access::ReadWrite`]), read, and release the lock again if `rule` finds a reason to be read-only.
    pub(super) fn open(path: &Path, access: Access, rule: ReadOnlyRule<'_>) -> Result<Opened, ServiceError> {
        let lock = match access {
            Access::ReadWrite => Some(BookLock::acquire(path)?),
            Access::ReadOnly => None,
        };
        // Read under the lock, so no other writer can change the file while it is read.
        let (book, residue) = read(path)?;
        let read_only = rule(&book, &findings(&residue));
        let lock = match lock {
            Some(held) if !read_only.is_empty() => {
                held.release()?;
                None
            }
            other => other,
        };
        let backend = Self {
            path: path.to_owned(),
            residue,
            lock,
        };
        Ok(Opened {
            book,
            read_only,
            backend: Box::new(backend),
        })
    }

    /// Lock `path` and make an empty Book with a new book's Residue. Nothing is written until the first save.
    pub(super) fn create(path: &Path, settings: &Settings, env: &mut dyn Env) -> Result<Opened, ServiceError> {
        let lock = BookLock::acquire(path)?;
        let book = Book::new_empty(env);
        let residue = XmlResidue::new_book(&book, settings.compress_new_books);
        let backend = Self {
            path: path.to_owned(),
            residue,
            lock: Some(lock),
        };
        Ok(Opened {
            book,
            read_only: Vec::new(),
            backend: Box::new(backend),
        })
    }

    fn xml_error(&self, source: red_ale_xml::XmlError) -> ServiceError {
        ServiceError::Xml {
            path: self.path.clone(),
            source,
        }
    }
}

fn read(path: &Path) -> Result<(Book, XmlResidue), ServiceError> {
    let file = File::open(path).map_err(|e| ServiceError::Io {
        path: path.to_owned(),
        source: e,
    })?;
    read_book_from(BufReader::with_capacity(IO_BUFFER, file)).map_err(|e| ServiceError::Xml {
        path: path.to_owned(),
        source: e,
    })
}

/// What the read found that makes the book read-only.
fn findings(residue: &XmlResidue) -> Vec<ReadOnlyReason> {
    if residue.notices.unrecognised.is_empty() {
        Vec::new()
    } else {
        vec![ReadOnlyReason::UnrecognisedContent(residue.notices.unrecognised.clone())]
    }
}

impl Backend for XmlBackend {
    fn format(&self) -> Format {
        Format::Xml
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn holds_lock(&self) -> bool {
        self.lock.is_some()
    }

    fn release_lock(&mut self) -> Result<(), ServiceError> {
        self.lock.take().map_or(Ok(()), BookLock::release)
    }

    /// Every Book can be written as XML.
    fn check_storable(&self, _before: &Book, _after: &Book) -> Result<(), ServiceError> {
        Ok(())
    }

    fn prepare_save(&self, _book: &Book) -> Option<Book> {
        None
    }

    /// Write a temporary file, read it back and compare it, entity by entity, with `book`, then back up and rename.
    /// The Residue's own snapshot is the Book last saved, so `saved` is not needed.
    fn save(&mut self, _saved: &Book, book: &Book, options: &SaveOptions, _env: &mut dyn Env) -> Result<Saved, ServiceError> {
        let compressed = options.compress.unwrap_or(self.residue.compressed);
        let write = |file: &mut File| {
            let out = write_book_to(book, &self.residue, compressed, BufWriter::with_capacity(IO_BUFFER, file))
                .map_err(|e| self.xml_error(e))?;
            out.into_inner().map(drop).map_err(|e| self.xml_error(e.into_error().into()))
        };
        let check = |tmp: &Path| {
            let file = File::open(tmp).map_err(|source| ServiceError::Io {
                path: tmp.to_owned(),
                source,
            })?;
            match read_back(BufReader::with_capacity(IO_BUFFER, file), book).map_err(|e| self.xml_error(e))? {
                ReadBack::Same(residue) => Ok(*residue),
                ReadBack::Differs(detail) => Err(ServiceError::Verification { detail }),
            }
        };
        // The Residue read back has this Book as its snapshot, so it compares against this one's entities.
        let (backup, mut residue) = replace_book_file(&self.path, write, check)?;
        residue.notices = self.residue.notices.clone();
        self.residue = residue;
        Ok(Saved {
            backup,
            compressed: Some(compressed),
        })
    }

    fn compressed(&self) -> Option<bool> {
        Some(self.residue.compressed)
    }

    fn xml_residue(&self) -> Option<&XmlResidue> {
        Some(&self.residue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::lock_path;
    use crate::session::Session;

    fn new_book(dir: &Path) -> PathBuf {
        let path = dir.join("b.gnucash");
        Session::create(path.to_str().unwrap(), Settings::default()).unwrap().close(false).unwrap();
        path
    }

    #[test]
    fn the_lock_is_kept_only_when_the_rule_finds_no_reason() {
        let dir = tempfile::tempdir().unwrap();
        let path = new_book(dir.path());
        let mut opened = XmlBackend::open(&path, Access::ReadWrite, &|_, _| Vec::new()).unwrap();
        assert!(opened.read_only.is_empty());
        assert!(opened.backend.holds_lock() && lock_path(&path).exists());
        opened.backend.release_lock().unwrap();
        assert!(!opened.backend.holds_lock() && !lock_path(&path).exists());
        opened.backend.release_lock().unwrap();

        let opened = XmlBackend::open(&path, Access::ReadWrite, &|_, _| vec![ReadOnlyReason::Requested]).unwrap();
        assert_eq!(opened.read_only, vec![ReadOnlyReason::Requested]);
        assert!(!opened.backend.holds_lock() && !lock_path(&path).exists());
    }
}
