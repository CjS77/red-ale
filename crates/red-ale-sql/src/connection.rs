//! Opening the database file (`design/sql-fidelity.md` "Layers", item 1, and §1, "Refused outright").
//!
//! The file is opened with explicit flags and never created. Only per-connection state is set: the busy timeout,
//! `trusted_schema` off and defensive mode on, with no extension loading. Nothing stored in the file
//! (`journal_mode`, `page_size`, …) is ever changed.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::config::DbConfig;
use rusqlite::{Connection, OpenFlags};

use crate::error::SqlError;

/// How long to wait for another program's write to finish, as `sql-fidelity.md` §5 sets.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The first bytes of every SQLite database.
const MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// The length of SQLite's database header.
const HEADER: usize = 100;

/// Check the header: an SQLite database, not in write-ahead-log mode (header bytes 18 and 19 are 2 in WAL mode).
fn check_header(path: &Path) -> Result<(), SqlError> {
    let mut header = Vec::with_capacity(HEADER);
    File::open(path)?.take(HEADER as u64).read_to_end(&mut header)?;
    if header.len() < HEADER || !header.starts_with(MAGIC) {
        return Err(SqlError::NotADatabase(format!("{} has no SQLite header", path.display())));
    }
    if header[18] == 2 || header[19] == 2 {
        return Err(SqlError::WriteAheadLog);
    }
    Ok(())
}

/// The rollback journal SQLite keeps beside the book while it writes.
fn journal_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push("-journal");
    PathBuf::from(name)
}

/// Refuse a journal with content beside the book: SQLite rolls a hot journal back when the book is next read, which
/// writes the book. A journal another program is writing right now is refused too; it is gone once that write ends.
fn check_journal(path: &Path) -> Result<(), SqlError> {
    let journal = journal_path(path);
    match std::fs::metadata(&journal) {
        Ok(meta) if meta.len() > 0 => Err(SqlError::HotJournal(journal.display().to_string())),
        _ => Ok(()),
    }
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)?;
    Ok(())
}

/// Open an existing book for reading and writing, never creating it. SQLite rolls a hot journal back here, which
/// restores the last committed state, as any GnuCash open would. A file SQLite can only open read-only (no write
/// permission) is refused, since the lock could not be taken.
pub(crate) fn open_read_write(path: &Path) -> Result<Connection, SqlError> {
    check_header(path)?;
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags).map_err(SqlError::from_write)?;
    configure(&conn).map_err(SqlError::from_sqlite)?;
    if conn.is_readonly(rusqlite::MAIN_DB).map_err(SqlError::from_sqlite)? {
        return Err(SqlError::CannotWrite(format!("{} is not writable", path.display())));
    }
    Ok(conn)
}

/// Open the book read-only (`SQLITE_OPEN_READONLY`): nothing is written to the file or beside it.
pub(crate) fn open_read_only(path: &Path) -> Result<Connection, SqlError> {
    check_header(path)?;
    check_journal(path)?;
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags).map_err(SqlError::from_sqlite)?;
    configure(&conn).map_err(SqlError::from_sqlite)?;
    Ok(conn)
}
