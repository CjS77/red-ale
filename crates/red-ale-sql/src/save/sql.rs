//! Statements: SQL text for a table's rows, cells bound as parameters, and the checked execution every write goes
//! through. Values are always bound, never spliced into SQL text.

use rusqlite::types::{ToSqlOutput, ValueRef};
use rusqlite::{Connection, ToSql, params_from_iter};

use crate::cell::{Cell, Table};
use crate::error::SqlError;

impl ToSql for Cell {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(match self {
            Self::Null => ValueRef::Null,
            Self::Integer(i) => ValueRef::Integer(*i),
            Self::Real(r) => ValueRef::Real(*r),
            Self::Text(t) => ValueRef::Text(t),
            Self::Blob(b) => ValueRef::Blob(b),
        }))
    }
}

/// The cell SQLite stored.
pub(crate) fn cell(v: ValueRef<'_>) -> Cell {
    match v {
        ValueRef::Null => Cell::Null,
        ValueRef::Integer(i) => Cell::Integer(i),
        ValueRef::Real(r) => Cell::Real(r),
        ValueRef::Text(t) => Cell::Text(t.to_vec()),
        ValueRef::Blob(b) => Cell::Blob(b.to_vec()),
    }
}

/// The columns a row is written with: every column of the table but an `AUTOINCREMENT` `id`, which SQLite assigns.
pub(crate) fn written_columns(table: &Table) -> &'static [&'static str] {
    let skip = usize::from(table.columns.first() == Some(&"id"));
    table.columns.get(skip..).unwrap_or_default()
}

fn quoted(columns: &[&str]) -> String {
    columns.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ")
}

/// `INSERT` listing every written column.
pub(crate) fn insert(table: &Table) -> String {
    let columns = written_columns(table);
    let marks = (1..=columns.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
    format!("INSERT INTO \"{}\" ({}) VALUES ({marks})", table.name, quoted(columns))
}

/// `UPDATE … WHERE guid = ?` setting every column, the primary key included, as GnuCash's does
/// (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:932-964`). The key is the last parameter.
pub(crate) fn update(table: &Table) -> String {
    let columns = written_columns(table);
    let sets = columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("\"{c}\" = ?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    format!("UPDATE \"{}\" SET {sets} WHERE \"guid\" = ?{}", table.name, columns.len() + 1)
}

/// `SELECT` of every written column of the rows whose `column` is a value, in `rowid` order.
pub(crate) fn select(table: &Table, column: &str) -> String {
    format!(
        "SELECT {} FROM \"{}\" WHERE \"{column}\" = ?1 ORDER BY rowid",
        quoted(written_columns(table)),
        table.name
    )
}

/// What a test can make a save do wrong, to show a failed save writes nothing (`design/sql-fidelity.md` §9, "Save
/// atomicity").
#[doc(hidden)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Faults {
    /// Fail the statement with this number, counting from 1.
    pub fail_statement: Option<usize>,
    /// After writing, change one written cell behind the save's back, so the read-back finds it.
    pub corrupt: bool,
}

/// Runs a save's statements, counting them for [`Faults`].
pub(crate) struct Statements<'c> {
    pub(crate) conn: &'c Connection,
    faults: Faults,
    count: usize,
}

impl<'c> Statements<'c> {
    pub(crate) fn new(conn: &'c Connection, faults: Faults) -> Self {
        Self { conn, faults, count: 0 }
    }

    /// Run a statement, returning how many rows it changed.
    pub(crate) fn run(&mut self, sql: &str, params: &[Cell]) -> Result<usize, SqlError> {
        self.count += 1;
        if self.faults.fail_statement == Some(self.count) {
            return Err(SqlError::NotABook(format!("test fault at statement {}", self.count)));
        }
        let mut stmt = self.conn.prepare(sql).map_err(SqlError::from_write)?;
        stmt.execute(params_from_iter(params)).map_err(SqlError::from_write)
    }

    /// Run a statement that must change exactly `rows` rows.
    pub(crate) fn expect(&mut self, sql: &str, params: &[Cell], rows: usize) -> Result<(), SqlError> {
        let changed = self.run(sql, params)?;
        if changed == rows {
            Ok(())
        } else {
            Err(SqlError::Verification(format!(
                "{sql:?} changed {changed} rows where {rows} were expected; nothing was written"
            )))
        }
    }

    /// Every row a `SELECT` gives, as cells.
    pub(crate) fn rows(&self, sql: &str, params: &[Cell]) -> Result<Vec<Vec<Cell>>, SqlError> {
        let mut stmt = self.conn.prepare(sql).map_err(SqlError::from_sqlite)?;
        let n = stmt.column_count();
        let rows = stmt.query_map(params_from_iter(params), |r| {
            (0..n).map(|i| r.get_ref(i).map(cell)).collect::<Result<Vec<_>, _>>()
        })?;
        rows.collect::<Result<_, _>>().map_err(SqlError::from_sqlite)
    }

    /// True when the test asked for a corrupted write.
    pub(crate) fn corrupt(&self) -> bool {
        self.faults.corrupt
    }
}
