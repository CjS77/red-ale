//! Export accounts: a new book file holding the open book's account tree and commodities and nothing else
//! (`design/api/export.md`, "Export accounts").
//!
//! GnuCash's File ▸ Export ▸ Export Accounts (`gnc_file_do_export`, `5.17:gnucash/gnome-utils/gnc-file.c:1312-1443`)
//! adds `.gnucash` to the name, refuses the open book's own file, and writes in place with no temporary file. red-ale
//! keeps the first two and writes safely instead: it refuses a file that exists, takes the new file's `.LCK` lock,
//! writes a temporary file beside it, reads it back, and renames it into place. The open book is only read: it stays
//! unchanged and not dirty, and its journal is not touched.

use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use red_ale_core::DomainError;
use red_ale_xml::{ExportCounts, check_accounts_export, write_accounts_to};

use crate::ServiceError;
use crate::files::replace_book_file;
use crate::lock::BookLock;
use crate::session::{Session, resolve_path};

/// The extension GnuCash adds (`GNC_DATAFILE_EXT`, `gnc_uri_add_extension`, `5.17:libgnucash/engine/gnc-uri-utils.c:413-429`).
const EXTENSION: &str = ".gnucash";

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountsExport {
    /// The new file.
    pub path: PathBuf,
    /// The commodity and account elements it holds.
    pub counts: ExportCounts,
}

/// Write `session`'s account tree and commodities to a new book file at `spec`, a path or `xml://` URI. `.gnucash` is
/// added when the name lacks it. The open book's own file is `invalid`; any existing file is `Exists`.
pub fn export_accounts(session: &Session, spec: &str) -> Result<AccountsExport, ServiceError> {
    let path = with_extension(resolve_path(spec)?);
    if same_file(&path, session.path()) {
        return Err(DomainError::invalid(format!("{} is the open book; export to another file", path.display())).into());
    }
    refuse_existing(&path)?;
    let lock = BookLock::acquire(&path)?;
    // Checked again under the lock: GnuCash, or another red-ale, creates a book only while holding its lock.
    refuse_existing(&path)?;
    let counts = write(session, &path)?;
    lock.release()?;
    Ok(AccountsExport { path, counts })
}

fn with_extension(path: PathBuf) -> PathBuf {
    if path.as_os_str().to_string_lossy().ends_with(EXTENSION) {
        return path;
    }
    let mut name = OsString::from(path);
    name.push(EXTENSION);
    PathBuf::from(name)
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

fn refuse_existing(path: &Path) -> Result<(), ServiceError> {
    if path.symlink_metadata().is_ok() { Err(ServiceError::Exists { path: path.to_owned() }) } else { Ok(()) }
}

/// Write through a temporary file, read it back against the Book, and rename it into place.
fn write(session: &Session, path: &Path) -> Result<ExportCounts, ServiceError> {
    let xml = |source| ServiceError::Xml { path: path.to_owned(), source };
    let write = |file: &mut File| {
        let out = write_accounts_to(session.book(), session.residue(), BufWriter::new(file)).map_err(xml)?;
        out.into_inner().map(drop).map_err(|e| xml(e.into_error().into()))
    };
    let check = |tmp: &Path| {
        let file = File::open(tmp).map_err(|source| ServiceError::Io { path: tmp.to_owned(), source })?;
        check_accounts_export(BufReader::new(file), session.book()).map_err(xml)?.map_err(|detail| ServiceError::Verification { detail })
    };
    replace_book_file(path, write, check).map(|(_, counts)| counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_is_added_when_missing() {
        assert_eq!(with_extension(PathBuf::from("/x/chart")), PathBuf::from("/x/chart.gnucash"));
        assert_eq!(with_extension(PathBuf::from("/x/chart.gnucash")), PathBuf::from("/x/chart.gnucash"));
        assert_eq!(with_extension(PathBuf::from("/x/chart.GNUCASH")), PathBuf::from("/x/chart.GNUCASH.gnucash"));
    }
}
