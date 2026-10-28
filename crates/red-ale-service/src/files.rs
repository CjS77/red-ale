//! Saving a book file safely: backups, the replace step, and retention of old backups and journals.
//!
//! Follows GnuCash 5.17's XML backend (`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:357-832`, described in
//! `design/notes/storage-and-safety.md` §4-7) so the files beside a book look the same, with two deliberate
//! improvements: the replace step is an atomic `rename` after `fsync`, so the book name never disappears, and two
//! saves within one second wait for the next second instead of failing.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::ServiceError;

/// How long old backups and journals are kept. GnuCash's prefs `retain-type-*` and `retain-days`
/// (`5.17:gnucash/gschemas/org.gnucash.GnuCash.gschema.xml.in:73-92`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retention {
    /// Delete backups and old journals on every save.
    Never,
    /// Delete those at least this many whole days old (by modification time).
    Days(u32),
    /// Keep everything.
    Forever,
}

impl Default for Retention {
    /// GnuCash's default: 30 days.
    fn default() -> Self {
        Self::Days(30)
    }
}

/// `gnc_date_timestamp()`: the system clock in the local time zone as `YYYYMMDDhhmmss`
/// (`5.17:libgnucash/engine/gnc-datetime.cpp:638-644`).
pub fn local_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    crate::zone::local_zone()
        .to_local(red_ale_core::Time64(now))
        .map_or_else(|| "19700101000000".to_owned(), red_ale_core::time::LocalTime::compact)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> ServiceError + '_ {
    move |source| ServiceError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Link `orig` as `backup`, copying instead where hard links are unsupported (`link_or_make_backup`,
/// `5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:591-632`).
fn link_or_copy(orig: &Path, backup: &Path) -> io::Result<()> {
    match fs::hard_link(orig, backup) {
        Ok(()) => Ok(()),
        Err(e) if matches!(e.raw_os_error(), Some(1 | 38 | 95)) || e.kind() == io::ErrorKind::Unsupported => {
            let mut out = OpenOptions::new().write(true).create_new(true).mode(0o600).open(backup)?;
            io::copy(&mut File::open(orig)?, &mut out)?;
            out.sync_all()
        }
        Err(e) => Err(e),
    }
}

/// Keep the current book as `<book>.<timestamp>.gnucash`. Returns the backup's path, or `None` for a new book.
pub fn backup(book: &Path) -> Result<Option<PathBuf>, ServiceError> {
    if !book.exists() {
        return Ok(None);
    }
    let deadline = SystemTime::now() + Duration::from_millis(1500);
    loop {
        let target = with_suffix(book, &format!(".{}.gnucash", local_timestamp()));
        match link_or_copy(book, &target) {
            Ok(()) => return Ok(Some(target)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && SystemTime::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(ServiceError::Backup { path: target, source: e }),
        }
    }
}

/// Make a backup of `book` with `copy`, which writes a consistent copy of it to a new file: into a temporary file
/// beside the book, given the book's mode and group, then linked to `<book>.<timestamp>.gnucash` without replacing an
/// existing file, waiting for the next second as [`backup`] does. For SQLite books, whose copy must come from the open
/// connection (`design/sql-fidelity.md` §6).
pub fn backup_with(book: &Path, copy: impl FnOnce(&Path) -> Result<(), ServiceError>) -> Result<PathBuf, ServiceError> {
    let tmp = temp_path(book);
    let made = copy(&tmp).and_then(|()| keep_owner(&tmp, fs::metadata(book).ok().as_ref())).and_then(|()| link_backup(book, &tmp));
    let _ = fs::remove_file(&tmp);
    made
}

/// Link `tmp` to a new backup name of `book`.
fn link_backup(book: &Path, tmp: &Path) -> Result<PathBuf, ServiceError> {
    let deadline = SystemTime::now() + Duration::from_millis(1500);
    loop {
        let target = with_suffix(book, &format!(".{}.gnucash", local_timestamp()));
        match fs::hard_link(tmp, &target) {
            Ok(()) => return Ok(target),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && SystemTime::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(ServiceError::Backup { path: target, source: e }),
        }
    }
}

fn temp_path(book: &Path) -> PathBuf {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let seed = u64::from(nanos) ^ u64::from(std::process::id()).rotate_left(20);
    let tail: String = (0..6)
        .map(|i| {
            let idx = usize::try_from((seed >> (i * 6)) % CHARS.len() as u64).unwrap_or(0);
            char::from(CHARS[idx])
        })
        .collect();
    with_suffix(book, &format!(".tmp-{tail}"))
}

/// Replace `book` with a new file: `write` fills a temporary file beside it, which is synced to disk, then `check`
/// reads it back. Only if both succeed is the old file backed up, the old file's mode and group copied, and the
/// temporary file atomically renamed into place; otherwise the temporary file is removed and the book is untouched.
/// Returns the backup's path (`None` for a new book) and what `check` returned.
pub fn replace_book_file<T>(
    book: &Path,
    write: impl FnOnce(&mut File) -> Result<(), ServiceError>,
    check: impl FnOnce(&Path) -> Result<T, ServiceError>,
) -> Result<(Option<PathBuf>, T), ServiceError> {
    let old_meta = fs::metadata(book).ok();
    let tmp = temp_path(book);
    let result = write_temp(&tmp, write)
        .and_then(|()| check(&tmp))
        .and_then(|checked| Ok((backup(book)?, checked)))
        .and_then(|done| {
            keep_owner(&tmp, old_meta.as_ref())?;
            fs::rename(&tmp, book).map_err(io_err(book))?;
            Ok(done)
        });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    let done = result?;
    if let Some(dir) = book.parent().filter(|d| !d.as_os_str().is_empty()) {
        // Persist the rename. Directories cannot be fsynced on every platform; a failure here is not fatal.
        let _ = File::open(dir).and_then(|d| d.sync_all());
    }
    Ok(done)
}

fn write_temp(tmp: &Path, write: impl FnOnce(&mut File) -> Result<(), ServiceError>) -> Result<(), ServiceError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o666)
        .open(tmp)
        .map_err(io_err(tmp))?;
    write(&mut file)?;
    file.sync_all().map_err(io_err(tmp))
}

fn keep_owner(tmp: &Path, old: Option<&fs::Metadata>) -> Result<(), ServiceError> {
    if let Some(meta) = old {
        fs::set_permissions(tmp, fs::Permissions::from_mode(meta.mode() & 0o7777)).map_err(io_err(tmp))?;
        // GnuCash copies the group only and ignores failures (`gnc-xml-backend.cpp:413-457`).
        let _ = std::os::unix::fs::chown(tmp, None, Some(meta.gid()));
    }
    Ok(())
}

/// True for names GnuCash's cleanup considers: `<book>.<14 digits>.(gnucash|log|xac)`, suffix case-sensitive in the
/// pre-filter and case-insensitive in the regex (`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:728-789`).
fn is_dated_sibling(book_name: &str, candidate: &str) -> bool {
    let Some(rest) = candidate.strip_prefix(book_name) else {
        return false;
    };
    if ![".gnucash", ".log", ".xac"].iter().any(|s| candidate.ends_with(s)) {
        return false;
    }
    let Some(rest) = rest.strip_prefix('.') else { return false };
    let (digits, suffix) = rest.split_at(rest.len().min(14));
    digits.len() == 14
        && digits.bytes().all(|b| b.is_ascii_digit())
        && [".gnucash", ".log", ".xac"].contains(&suffix.to_ascii_lowercase().as_str())
}

/// Delete old backups and journals of `book` according to `retention`, never touching `keep` (the open journal).
/// Returns the files deleted.
pub fn remove_old_files(book: &Path, retention: Retention, keep: Option<&Path>) -> Vec<PathBuf> {
    let Some(book_name) = book.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    let dir = book.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let now = SystemTime::now();
    entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_str().is_some_and(|n| is_dated_sibling(book_name, n)))
        .map(|e| e.path())
        .filter(|p| keep != Some(p.as_path()))
        .filter(|p| match retention {
            Retention::Never => true,
            Retention::Forever => false,
            Retention::Days(days) => fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .is_some_and(|age| age.as_secs() / 86_400 >= u64::from(days)),
        })
        .filter(|p| fs::remove_file(p).is_ok())
        .collect()
}

/// Replace a small file of red-ale's own, such as the Configuration, with no backup: write a temporary file beside it,
/// sync it and rename it into place, so the file is whole before and after, never half written. An existing file
/// keeps its mode; a new one is readable by its owner only, since the Configuration can hold keys.
pub fn replace_private_file(path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
    let mode = fs::metadata(path).map_or(0o600, |m| m.mode() & 0o7777);
    let tmp = temp_path(path);
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&tmp)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
        .and_then(|()| fs::set_permissions(&tmp, fs::Permissions::from_mode(mode)))
        .and_then(|()| fs::rename(&tmp, path));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written.map_err(io_err(path))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        // Persist the rename where the platform allows; a failure here is not fatal.
        let _ = File::open(dir).and_then(|d| d.sync_all());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn write_book_file(book: &Path, bytes: &[u8]) -> Result<Option<PathBuf>, ServiceError> {
        replace_book_file(book, |f| f.write_all(bytes).map_err(io_err(book)), |_| Ok(())).map(|(backup, ())| backup)
    }

    fn temp_files(dir: &Path) -> Vec<fs::DirEntry> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect()
    }

    #[test]
    fn a_failed_check_leaves_the_book_alone() {
        let dir = tempfile::tempdir().unwrap();
        let book = dir.path().join("b.gnucash");
        write_book_file(&book, b"one").unwrap();
        let refused = replace_book_file(
            &book,
            |f| f.write_all(b"two").map_err(io_err(&book)),
            |tmp| {
                assert_eq!(fs::read(tmp).unwrap(), b"two", "the check reads what was written");
                Err::<(), _>(ServiceError::Verification { detail: "test".into() })
            },
        );
        assert!(matches!(refused, Err(ServiceError::Verification { .. })));
        assert_eq!(fs::read(&book).unwrap(), b"one");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no backup and no temporary file");
    }

    #[test]
    fn private_files_are_replaced_whole_and_keep_their_mode() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("red-ale.json");
        replace_private_file(&file, b"{}").unwrap();
        assert_eq!(fs::metadata(&file).unwrap().mode() & 0o777, 0o600, "a new file is the owner's only");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        replace_private_file(&file, b"{\"a\": 1}").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"{\"a\": 1}");
        assert_eq!(fs::metadata(&file).unwrap().mode() & 0o777, 0o640);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no temporary file is left");
    }

    #[test]
    fn save_backs_up_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let book = dir.path().join("b.gnucash");
        assert_eq!(write_book_file(&book, b"one").unwrap(), None);
        let backup = write_book_file(&book, b"two").unwrap().unwrap();
        assert_eq!(fs::read(&book).unwrap(), b"two");
        assert_eq!(fs::read(&backup).unwrap(), b"one");
        let third = write_book_file(&book, b"three").unwrap().unwrap();
        assert_ne!(backup, third, "a save in the same second waits for a fresh name");
        assert!(temp_files(dir.path()).is_empty());
    }

    #[test]
    fn retention_matches_gnucash_names() {
        assert!(is_dated_sibling("b.gnucash", "b.gnucash.20200101000000.gnucash"));
        assert!(is_dated_sibling("b.gnucash", "b.gnucash.20200101000000.log"));
        assert!(!is_dated_sibling("b.gnucash", "b.gnucash.20200101000000.LOG"));
        assert!(!is_dated_sibling("b.gnucash", "b.gnucash.2020010100000.gnucash"));
        assert!(!is_dated_sibling("b.gnucash", "b.gnucash.tmp-AbC123"));
        assert!(!is_dated_sibling("b.gnucash", "b.gnucash"));
        let dir = tempfile::tempdir().unwrap();
        let book = dir.path().join("b.gnucash");
        fs::write(&book, b"x").unwrap();
        let old = dir.path().join("b.gnucash.20200101000000.gnucash");
        fs::write(&old, b"x").unwrap();
        assert!(remove_old_files(&book, Retention::Days(30), None).is_empty());
        assert_eq!(remove_old_files(&book, Retention::Never, None), vec![old]);
    }
}
