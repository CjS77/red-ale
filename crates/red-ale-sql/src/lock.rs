//! GnuCash's SQL lock: a row in `gnclock (Hostname varchar(255), PID int)`, where any row at all means locked
//! (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:101-227`; `design/sql-fidelity.md` §5).
//!
//! red-ale checks for a row and inserts its own in the transaction that reads the book, never creates the table, and
//! deletes only its own row, where GnuCash deletes every row once it finds its own.

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::SqlError;

/// The longest host name GnuCash writes: `gethostname` into a zeroed 256-byte buffer (`GNC_HOST_NAME_MAX`,
/// `5.17:libgnucash/backend/dbi/gnc-backend-dbi.hpp:41`; `gnc-dbisqlconnection.cpp:129`).
const HOST_NAME_MAX: usize = 255;

/// Who holds a lock: the `gnclock` row. The service fills it from the host name and process ID, so this crate makes
/// no system calls of its own and tests can play GnuCash's part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockOwner {
    /// The host name, as `gethostname` gives it.
    pub hostname: String,
    /// The process ID.
    pub pid: i64,
}

impl LockOwner {
    /// The owner with its host name cut to GnuCash's 255 bytes, at a character boundary.
    pub fn new(hostname: &str, pid: i64) -> Self {
        let end = (0..=hostname.len().min(HOST_NAME_MAX))
            .rev()
            .find(|i| hostname.is_char_boundary(*i))
            .unwrap_or(0);
        Self {
            hostname: hostname[..end].to_owned(),
            pid,
        }
    }
}

/// The lock row this session inserted, by `rowid`, so releasing it can never delete another program's row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeldLock {
    pub(crate) owner: LockOwner,
    pub(crate) rowid: i64,
}

/// True when the book has a `gnclock` table. A book without one opens read-only (§1): red-ale never creates it.
fn has_table(conn: &Connection) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'gnclock'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
}

/// Refuse a book whose `gnclock` holds a row, naming the first holder.
pub(crate) fn check_free(conn: &Connection) -> Result<(), SqlError> {
    if !has_table(conn)? {
        return Ok(());
    }
    let holder = conn
        .query_row("SELECT Hostname, PID FROM gnclock ORDER BY rowid LIMIT 1", [], |r| {
            Ok(LockOwner {
                hostname: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                pid: r.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            })
        })
        .optional()?;
    holder.map_or(Ok(()), |holder| Err(SqlError::Locked(holder)))
}

/// Insert `owner`'s row, inside the caller's transaction. Binding the host name, where GnuCash splices it into the SQL
/// unescaped, gives the same row.
pub(crate) fn insert(conn: &Connection, owner: &LockOwner) -> Result<HeldLock, SqlError> {
    conn.execute(
        "INSERT INTO gnclock (Hostname, PID) VALUES (?1, ?2)",
        params![owner.hostname, owner.pid],
    )
    .map_err(SqlError::from_write)?;
    Ok(HeldLock {
        owner: owner.clone(),
        rowid: conn.last_insert_rowid(),
    })
}

/// True when `gnclock` holds exactly this session's row: anything else means another program broke the lock (GnuCash's
/// "Open Anyway" deletes every row) or wrote one of its own.
pub(crate) fn is_held(conn: &Connection, held: &HeldLock) -> Result<bool, SqlError> {
    let rows: Vec<(i64, Option<String>, Option<i64>)> = conn
        .prepare("SELECT rowid, Hostname, PID FROM gnclock ORDER BY rowid")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    Ok(rows == [(held.rowid, Some(held.owner.hostname.clone()), Some(held.owner.pid))])
}

/// Delete this session's row, and only it, in one transaction. A row already gone (the lock was broken) deletes
/// nothing, and that is not an error.
pub(crate) fn release(conn: &mut Connection, held: &HeldLock) -> Result<(), SqlError> {
    let tx = conn.transaction().map_err(SqlError::from_sqlite)?;
    let deleted = tx
        .execute(
            "DELETE FROM gnclock WHERE rowid = ?1 AND Hostname = ?2 AND PID = ?3",
            params![held.rowid, held.owner.hostname, held.owner.pid],
        )
        .map_err(SqlError::from_write)?;
    if deleted > 1 {
        return Err(SqlError::NotABook(format!(
            "releasing the lock would delete {deleted} gnclock rows"
        )));
    }
    tx.commit().map_err(SqlError::from_write)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_names_are_cut_to_255_bytes_at_a_character_boundary() {
        assert_eq!(LockOwner::new("beast", 7).hostname, "beast");
        let long = "é".repeat(200);
        let cut = LockOwner::new(&long, 7).hostname;
        assert_eq!(cut.len(), 254);
        assert!(long.starts_with(&cut));
    }
}
