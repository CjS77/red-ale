//! Creating a book, and backing one up (`design/sql-fidelity.md` §6, §7).
//!
//! A new book is written into a temporary file beside the target in one transaction, with GnuCash 5.17's DDL and
//! `versions` rows, the `books` row and its slot rows, the two roots and the lock row, then linked to its name without
//! replacing anything, so a half-made book is never seen. No `PRAGMA` is set: the bundled SQLite's defaults are what
//! GnuCash's books have.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use red_ale_core::{Account, AccountType, Book, Env, Guid};
use rusqlite::{Connection, params};

use crate::error::SqlError;
use crate::lock::{self, HeldLock, LockOwner};
use crate::residue::SqlResidue;
use crate::save::sql::{Statements, insert};
use crate::schema::{VERSIONS, ddl};
use crate::{SqliteBook, connection, encode, tables};

/// The name GnuCash gives the template root (`5.17:libgnucash/engine/SX-book.cpp:121`).
const TEMPLATE_ROOT: &str = "Template Root";

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// The schema, the rows of an empty book and the lock row, in one transaction.
fn fill(conn: &mut Connection, book: &Book, template_root: Guid, owner: &LockOwner, env: &mut dyn Env) -> Result<HeldLock, SqlError> {
    let tx = conn.transaction().map_err(SqlError::from_write)?;
    ddl()
        .try_for_each(|statement| tx.execute_batch(statement))
        .map_err(SqlError::from_write)?;
    VERSIONS
        .iter()
        .try_for_each(|(name, version)| tx.execute("INSERT INTO versions VALUES (?1, ?2)", params![name, version]).map(drop))
        .map_err(SqlError::from_write)?;
    let mut st = Statements::new(&tx, crate::Faults::default());
    st.expect(&insert(&tables::BOOKS), &encode::book(book.guid, book.root, template_root), 1)?;
    encode::slots::rows(book.guid, &book.slots, &mut || env.new_guid())?
        .iter()
        .try_for_each(|r| st.expect(&insert(&tables::SLOTS), r, 1))?;
    let no_commodities = indexmap::IndexMap::new();
    let refs = encode::Refs {
        commodities: &no_commodities,
    };
    let root = book
        .account(book.root)
        .ok_or_else(|| SqlError::NotABook("the new book has no root account".into()))?;
    let template = Account::new(template_root, TEMPLATE_ROOT, AccountType::Root, None, None);
    [root, &template]
        .iter()
        .try_for_each(|a| st.expect(&insert(&tables::ACCOUNTS), &encode::account(a, refs), 1))?;
    let held = lock::insert(&tx, owner)?;
    tx.commit().map_err(SqlError::from_write)?;
    Ok(held)
}

/// Write a new book at `path`, which must not exist, from `book`, an empty Book with only its root account and Slots,
/// and lock it for `owner`. Frame GUIDs and the template root's come from `env`.
pub fn create(path: &Path, book: &Book, owner: &LockOwner, env: &mut dyn Env) -> Result<SqliteBook, SqlError> {
    let tmp = with_suffix(path, &format!(".tmp-{}", env.new_guid()));
    OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    let template_root = book.templates.root.unwrap_or_else(|| env.new_guid());
    let made = (|| {
        let mut conn = connection::create(&tmp)?;
        let held = fill(&mut conn, book, template_root, owner, env)?;
        drop(conn);
        std::fs::File::open(&tmp)?.sync_all()?;
        std::fs::hard_link(&tmp, path)?;
        Ok::<_, SqlError>(held)
    })();
    let _ = std::fs::remove_file(&tmp);
    let held = made?;
    let conn = connection::open_read_write(path)?;
    let residue = SqlResidue {
        template_root,
        slot_rows: count_slots(&conn, book.guid)?,
        versions: VERSIONS.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect(),
        ..SqlResidue::default()
    };
    let data_version = connection::data_version(&conn)?;
    let opened = SqliteBook {
        path: path.to_owned(),
        residue,
        conn: Some(conn),
        lock: Some(held),
        data_version,
        faults: crate::Faults::default(),
    };
    Ok(opened)
}

/// How many slot rows the Book's Slots were written as.
fn count_slots(conn: &Connection, book: Guid) -> Result<std::collections::BTreeMap<Guid, usize>, SqlError> {
    let rows: i64 = conn.query_row("SELECT count(*) FROM slots", [], |r| r.get(0))?;
    let rows = usize::try_from(rows).unwrap_or(0);
    Ok(if rows == 0 {
        std::collections::BTreeMap::new()
    } else {
        [(book, rows)].into()
    })
}

impl SqliteBook {
    /// Copy the database to `dest`, a new file, with SQLite's online backup API in one step, so the copy is one
    /// consistent state; then delete this session's lock row from the copy, so it opens in GnuCash unlocked.
    pub fn backup_to(&self, dest: &Path) -> Result<(), SqlError> {
        let conn = self
            .conn
            .as_ref()
            .ok_or_else(|| SqlError::NotABook("the book is not open for writing".into()))?;
        let mut copy = connection::create(dest)?;
        {
            let backup = rusqlite::backup::Backup::new(conn, &mut copy)?;
            backup.step(-1)?;
        }
        if let Some(held) = &self.lock {
            copy.execute(
                "DELETE FROM gnclock WHERE rowid = ?1 AND Hostname = ?2 AND PID = ?3",
                params![held.rowid, held.owner.hostname, held.owner.pid],
            )
            .map_err(SqlError::from_write)?;
        }
        drop(copy);
        std::fs::File::open(dest)?.sync_all()?;
        Ok(())
    }
}
