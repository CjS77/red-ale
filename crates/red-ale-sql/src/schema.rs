//! GnuCash 5.17's schema, and the check of a book's schema and `versions` rows against it
//! (`design/sql-fidelity.md` §1, "What opens read-only", and §7).

use rusqlite::Connection;

use crate::cell::{Cell, Cells, Row, Table};
use crate::error::SqlError;

/// The DDL GnuCash 5.17 writes, in its order, as the oracle wrote it (`design/notes/sql-backend.md` §2.2).
pub const DDL: [&str; 27] = [
    "CREATE TABLE gnclock ( Hostname varchar(255), PID int )",
    "CREATE TABLE versions(table_name text(50) PRIMARY KEY NOT NULL, table_version integer NOT NULL)",
    "CREATE TABLE books(guid text(32) PRIMARY KEY NOT NULL, root_account_guid text(32) NOT NULL, root_template_guid text(32) NOT NULL)",
    "CREATE TABLE commodities(guid text(32) PRIMARY KEY NOT NULL, namespace text(2048) NOT NULL, mnemonic text(2048) NOT NULL, \
     fullname text(2048), cusip text(2048), fraction integer NOT NULL, quote_flag integer NOT NULL, quote_source text(2048), \
     quote_tz text(2048))",
    "CREATE TABLE accounts(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, account_type text(2048) NOT NULL, \
     commodity_guid text(32), commodity_scu integer NOT NULL, non_std_scu integer NOT NULL, parent_guid text(32), code text(2048), \
     description text(2048), hidden integer, placeholder integer)",
    "CREATE TABLE budgets(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, description text(2048), num_periods integer \
     NOT NULL)",
    "CREATE TABLE budget_amounts(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, budget_guid text(32) NOT NULL, account_guid text(32) \
     NOT NULL, period_num integer NOT NULL, amount_num bigint NOT NULL, amount_denom bigint NOT NULL)",
    "CREATE TABLE prices(guid text(32) PRIMARY KEY NOT NULL, commodity_guid text(32) NOT NULL, currency_guid text(32) NOT NULL, \
     date text(19) NOT NULL, source text(2048), type text(2048), value_num bigint NOT NULL, value_denom bigint NOT NULL)",
    "CREATE TABLE transactions(guid text(32) PRIMARY KEY NOT NULL, currency_guid text(32) NOT NULL, num text(2048) NOT NULL, \
     post_date text(19), enter_date text(19), description text(2048))",
    "CREATE INDEX tx_post_date_index ON transactions(post_date)",
    "CREATE TABLE splits(guid text(32) PRIMARY KEY NOT NULL, tx_guid text(32) NOT NULL, account_guid text(32) NOT NULL, memo text(2048) \
     NOT NULL, action text(2048) NOT NULL, reconcile_state text(1) NOT NULL, reconcile_date text(19), value_num bigint NOT NULL, \
     value_denom bigint NOT NULL, quantity_num bigint NOT NULL, quantity_denom bigint NOT NULL, lot_guid text(32))",
    "CREATE INDEX splits_tx_guid_index ON splits(tx_guid)",
    "CREATE INDEX splits_account_guid_index ON splits(account_guid)",
    "CREATE TABLE slots(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, obj_guid text(32) NOT NULL, name text(4096) NOT NULL, slot_type \
     integer NOT NULL, int64_val bigint, string_val text(4096), double_val float8, timespec_val text(19), guid_val text(32), \
     numeric_val_num bigint, numeric_val_denom bigint, gdate_val text(8))",
    "CREATE INDEX slots_guid_index ON slots(obj_guid)",
    "CREATE TABLE recurrences(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, obj_guid text(32) NOT NULL, recurrence_mult integer NOT \
     NULL, recurrence_period_type text(2048) NOT NULL, recurrence_period_start text(8) NOT NULL, recurrence_weekend_adjust text(2048) \
     NOT NULL)",
    "CREATE TABLE schedxactions(guid text(32) PRIMARY KEY NOT NULL, name text(2048), enabled integer NOT NULL, start_date text(8), \
     end_date text(8), last_occur text(8), num_occur integer NOT NULL, rem_occur integer NOT NULL, auto_create integer NOT NULL, \
     auto_notify integer NOT NULL, adv_creation integer NOT NULL, adv_notify integer NOT NULL, instance_count integer NOT NULL, \
     template_act_guid text(32) NOT NULL)",
    "CREATE TABLE lots(guid text(32) PRIMARY KEY NOT NULL, account_guid text(32), is_closed integer NOT NULL)",
    "CREATE TABLE billterms(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, description text(2048) NOT NULL, refcount \
     integer NOT NULL, invisible integer NOT NULL, parent text(32), type text(2048) NOT NULL, duedays integer, discountdays integer, \
     discount_num bigint, discount_denom bigint, cutoff integer)",
    "CREATE TABLE customers(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, id text(2048) NOT NULL, notes text(2048) NOT \
     NULL, active integer NOT NULL, discount_num bigint NOT NULL, discount_denom bigint NOT NULL, credit_num bigint NOT NULL, \
     credit_denom bigint NOT NULL, currency text(32) NOT NULL, tax_override integer NOT NULL, addr_name text(1024), addr_addr1 \
     text(1024), addr_addr2 text(1024), addr_addr3 text(1024), addr_addr4 text(1024), addr_phone text(128), addr_fax text(128), \
     addr_email text(256), shipaddr_name text(1024), shipaddr_addr1 text(1024), shipaddr_addr2 text(1024), shipaddr_addr3 text(1024), \
     shipaddr_addr4 text(1024), shipaddr_phone text(128), shipaddr_fax text(128), shipaddr_email text(256), terms text(32), \
     tax_included integer, taxtable text(32))",
    "CREATE TABLE employees(guid text(32) PRIMARY KEY NOT NULL, username text(2048) NOT NULL, id text(2048) NOT NULL, language \
     text(2048) NOT NULL, acl text(2048) NOT NULL, active integer NOT NULL, currency text(32) NOT NULL, ccard_guid text(32), \
     workday_num bigint NOT NULL, workday_denom bigint NOT NULL, rate_num bigint NOT NULL, rate_denom bigint NOT NULL, addr_name \
     text(1024), addr_addr1 text(1024), addr_addr2 text(1024), addr_addr3 text(1024), addr_addr4 text(1024), addr_phone text(128), \
     addr_fax text(128), addr_email text(256))",
    "CREATE TABLE entries(guid text(32) PRIMARY KEY NOT NULL, date text(19) NOT NULL, date_entered text(19), description text(2048), \
     action text(2048), notes text(2048), quantity_num bigint, quantity_denom bigint, i_acct text(32), i_price_num bigint, \
     i_price_denom bigint, i_discount_num bigint, i_discount_denom bigint, invoice text(32), i_disc_type text(2048), i_disc_how \
     text(2048), i_taxable integer, i_taxincluded integer, i_taxtable text(32), b_acct text(32), b_price_num bigint, b_price_denom \
     bigint, bill text(32), b_taxable integer, b_taxincluded integer, b_taxtable text(32), b_paytype integer, billable integer, \
     billto_type integer, billto_guid text(32), order_guid text(32))",
    "CREATE TABLE invoices(guid text(32) PRIMARY KEY NOT NULL, id text(2048) NOT NULL, date_opened text(19), date_posted text(19), \
     notes text(2048) NOT NULL, active integer NOT NULL, currency text(32) NOT NULL, owner_type integer, owner_guid text(32), terms \
     text(32), billing_id text(2048), post_txn text(32), post_lot text(32), post_acc text(32), billto_type integer, billto_guid \
     text(32), charge_amt_num bigint, charge_amt_denom bigint)",
    "CREATE TABLE jobs(guid text(32) PRIMARY KEY NOT NULL, id text(2048) NOT NULL, name text(2048) NOT NULL, reference text(2048) NOT \
     NULL, active integer NOT NULL, owner_type integer, owner_guid text(32))",
    "CREATE TABLE orders(guid text(32) PRIMARY KEY NOT NULL, id text(2048) NOT NULL, notes text(2048) NOT NULL, reference text(2048) \
     NOT NULL, active integer NOT NULL, date_opened text(19) NOT NULL, date_closed text(19) NOT NULL, owner_type integer NOT NULL, \
     owner_guid text(32) NOT NULL)",
    "CREATE TABLE taxtables(guid text(32) PRIMARY KEY NOT NULL, name text(50) NOT NULL, refcount bigint NOT NULL, invisible integer NOT \
     NULL, parent text(32))",
    "CREATE TABLE taxtable_entries(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, taxtable text(32) NOT NULL, account text(32) NOT \
     NULL, amount_num bigint NOT NULL, amount_denom bigint NOT NULL, type integer NOT NULL)",
];

/// The last DDL statement, `vendors`, kept apart so [`DDL`] stays a readable size; [`ddl`] gives them all.
const VENDORS: &str = "CREATE TABLE vendors(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, id text(2048) NOT NULL, notes \
     text(2048) NOT NULL, currency text(32) NOT NULL, active integer NOT NULL, tax_override integer NOT NULL, addr_name text(1024), \
     addr_addr1 text(1024), addr_addr2 text(1024), addr_addr3 text(1024), addr_addr4 text(1024), addr_phone text(128), addr_fax \
     text(128), addr_email text(256), terms text(32), tax_inc text(2048), tax_table text(32))";

/// Every statement of GnuCash 5.17's DDL, in its order.
pub fn ddl() -> impl Iterator<Item = &'static str> {
    DDL.into_iter().chain([VENDORS])
}

/// The `Gnucash` and `Gnucash-Resave` rows' names.
const GNUCASH: &str = "Gnucash";
const RESAVE: &str = "Gnucash-Resave";
/// `GNUCASH_RESAVE_VERSION` (`5.17:CMakeLists.txt:37`).
pub const RESAVE_VERSION: i64 = 19_920;
/// 5.17's `gnc_prefs_get_long_version()`: major × 1,000,000 + minor (`5.17:libgnucash/core-utils/gnc-prefs.cpp:122-125`).
pub const GNUCASH_VERSION: i64 = 5_000_017;

/// The `versions` rows of a book GnuCash 5.17 created, in its order (`design/notes/sql-backend.md` §2.3).
pub const VERSIONS: [(&str, i64); 24] = [
    (GNUCASH, GNUCASH_VERSION),
    (RESAVE, RESAVE_VERSION),
    ("books", 1),
    ("commodities", 1),
    ("accounts", 1),
    ("budgets", 1),
    ("budget_amounts", 1),
    ("prices", 3),
    ("transactions", 4),
    ("splits", 5),
    ("slots", 4),
    ("recurrences", 2),
    ("schedxactions", 1),
    ("lots", 2),
    ("billterms", 2),
    ("customers", 2),
    ("employees", 2),
    ("entries", 4),
    ("invoices", 4),
    ("jobs", 1),
    ("orders", 1),
    ("taxtables", 2),
    ("taxtable_entries", 3),
    ("vendors", 1),
];

/// One schema object: its type, its name and the table it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Object {
    kind: String,
    name: String,
    table: String,
    sql: String,
}

fn objects(conn: &Connection) -> rusqlite::Result<Vec<Object>> {
    let mut stmt = conn.prepare(
        "SELECT type, name, tbl_name, coalesce(sql, '') FROM sqlite_master WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY rowid",
    )?;
    stmt.query_map([], |r| {
        Ok(Object {
            kind: r.get(0)?,
            name: r.get(1)?,
            table: r.get(2)?,
            sql: r.get(3)?,
        })
    })?
    .collect()
}

/// A table's columns as `PRAGMA table_xinfo` gives them: name, declared type (lower case), `NOT NULL`, default,
/// primary key position and hidden flag.
type Column = (String, String, i64, Option<String>, i64, i64);

fn columns(conn: &Connection, table: &str) -> rusqlite::Result<Vec<Column>> {
    let mut stmt = conn.prepare("SELECT name, lower(type), \"notnull\", dflt_value, pk, hidden FROM pragma_table_xinfo(?1) ORDER BY cid")?;
    stmt.query_map([table], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
        .collect()
}

fn index_columns(conn: &Connection, index: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")?;
    stmt.query_map([index], |r| r.get::<_, Option<String>>(0).map(Option::unwrap_or_default))?
        .collect()
}

fn autoincrement(sql: &str) -> bool {
    sql.to_ascii_uppercase().contains("AUTOINCREMENT")
}

/// What differs between the schema of `conn` and GnuCash 5.17's, each a finding.
pub(crate) fn check(conn: &Connection) -> Result<Vec<String>, SqlError> {
    let expected_db = Connection::open_in_memory()?;
    ddl().try_for_each(|statement| expected_db.execute_batch(statement))?;
    let expected = objects(&expected_db)?;
    let actual = objects(conn)?;
    let mut findings: Vec<String> = actual
        .iter()
        .filter(|a| !expected.iter().any(|e| e.kind == a.kind && e.name == a.name))
        .map(unknown_object)
        .collect();
    findings.extend(
        expected
            .iter()
            .filter(|e| !actual.iter().any(|a| a.kind == e.kind && a.name == e.name))
            .map(|e| format!("GnuCash 5.17's {} {} is missing", e.kind, e.name)),
    );
    for e in &expected {
        let Some(a) = actual.iter().find(|a| a.kind == e.kind && a.name == e.name) else { continue };
        if let Some(difference) = compare(&expected_db, conn, e, a)? {
            findings.push(difference);
        }
    }
    Ok(findings)
}

fn unknown_object(o: &Object) -> String {
    let name = &o.name;
    let lower = name.to_ascii_lowercase();
    if o.kind == "table" && (lower.ends_with("back") || lower.contains("_merge")) {
        format!("table {name} looks like one left by a failed GnuCash safe-save, which GnuCash would recover on open")
    } else {
        format!("{} {name} is not in GnuCash 5.17's schema", o.kind)
    }
}

/// The first way an object of the book differs from GnuCash 5.17's.
fn compare(expected_db: &Connection, conn: &Connection, e: &Object, a: &Object) -> Result<Option<String>, SqlError> {
    let name = &e.name;
    if e.kind == "index" {
        let same = e.table == a.table && index_columns(expected_db, name)? == index_columns(conn, name)?;
        return Ok((!same).then(|| format!("index {name} is not GnuCash 5.17's")));
    }
    let (want, have) = (columns(expected_db, name)?, columns(conn, name)?);
    let column = |c: &Column| c.0.clone();
    if let Some(extra) = have.iter().find(|h| !want.iter().any(|w| w.0 == h.0)) {
        return Ok(Some(format!("table {name} has column {} that GnuCash 5.17 does not", column(extra))));
    }
    if let Some(missing) = want.iter().find(|w| !have.iter().any(|h| h.0 == w.0)) {
        return Ok(Some(format!("table {name} lacks GnuCash 5.17's column {}", column(missing))));
    }
    if want != have {
        let differs = want.iter().zip(&have).find(|(w, h)| w != h).map_or_else(String::new, |(w, _)| w.0.clone());
        return Ok(Some(format!("table {name}, column {differs}: not declared as GnuCash 5.17 declares it")));
    }
    Ok((autoincrement(&e.sql) != autoincrement(&a.sql)).then(|| format!("table {name}: AUTOINCREMENT differs from GnuCash 5.17's")))
}

/// The `versions` table's columns, in the order read.
pub const VERSIONS_TABLE: Table = Table {
    name: "versions",
    columns: &["table_name", "table_version"],
};

/// The `versions` rows that differ from GnuCash 5.17's, each naming the row (Decided item 4 of `design/milestone4.md`):
/// every table row must equal 5.17's and `Gnucash-Resave` must be 19920; `Gnucash` records the release that last
/// wrote the whole database, so it need only lie between 19920 and 5.17's (`design/sql-fidelity.md` §1, "Schema
/// versions"). Also returns the rows as read.
pub fn check_versions(rows: &[Row], findings: &mut Vec<String>) -> (Vec<String>, Vec<(String, i64)>) {
    let read: Vec<(String, i64)> = rows
        .iter()
        .map(|row| {
            let mut cells = Cells::new(&VERSIONS_TABLE, row, findings);
            (cells.req_text("table_name"), cells.req_int("table_version"))
        })
        .collect();
    let version_of = |name: &str| read.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
    let mut differ: Vec<String> = VERSIONS
        .iter()
        .filter_map(|(name, want)| {
            let have = version_of(name);
            let fine = match *name {
                GNUCASH => have.is_some_and(|v| (RESAVE_VERSION..=GNUCASH_VERSION).contains(&v)),
                _ => have == Some(*want),
            };
            (!fine).then(|| match have {
                Some(v) if *name == GNUCASH => format!("{name} is {v}, outside {RESAVE_VERSION} to {GNUCASH_VERSION}"),
                Some(v) => format!("{name} is version {v}, GnuCash 5.17's is {want}"),
                None => format!("{name} has no version row"),
            })
        })
        .collect();
    differ.extend(
        read.iter()
            .filter(|(name, _)| !VERSIONS.iter().any(|(n, _)| n == name))
            .map(|(name, v)| format!("{name} is version {v}, a table GnuCash 5.17 does not have")),
    );
    (differ, read)
}

/// A cell, for tests and the fuzzer: a `versions` row.
pub fn version_row(rowid: i64, name: &str, version: i64) -> Row {
    Row {
        rowid,
        cells: vec![Cell::Text(name.as_bytes().to_vec()), Cell::Integer(version)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(changes: &[(&str, i64)]) -> Vec<Row> {
        (1..)
            .zip(VERSIONS.iter().map(|(n, v)| changes.iter().find(|(c, _)| c == n).copied().unwrap_or((*n, *v))))
            .map(|(i, (n, v))| version_row(i, n, v))
            .collect()
    }

    fn differ(changes: &[(&str, i64)]) -> Vec<String> {
        check_versions(&rows(changes), &mut Vec::new()).0
    }

    #[test]
    fn gnucash_5_17_s_rows_pass() {
        assert!(differ(&[]).is_empty());
        assert!(differ(&[("Gnucash", 4_000_005)]).is_empty(), "an older release that last wrote the whole book");
        assert!(differ(&[("Gnucash", RESAVE_VERSION)]).is_empty());
    }

    #[test]
    fn every_other_row_is_named() {
        assert_eq!(differ(&[("splits", 4)]), ["splits is version 4, GnuCash 5.17's is 5"]);
        assert_eq!(differ(&[("splits", 6)]), ["splits is version 6, GnuCash 5.17's is 5"]);
        assert_eq!(differ(&[("Gnucash-Resave", 19_921)]), ["Gnucash-Resave is version 19921, GnuCash 5.17's is 19920"]);
        assert_eq!(differ(&[("Gnucash", 19_919)]), ["Gnucash is 19919, outside 19920 to 5000017"]);
        assert_eq!(differ(&[("Gnucash", 5_000_018)]), ["Gnucash is 5000018, outside 19920 to 5000017"]);
        let mut extra = rows(&[]);
        extra.push(version_row(99, "zz", 1));
        extra.remove(19);
        assert_eq!(
            check_versions(&extra, &mut Vec::new()).0,
            ["jobs has no version row", "zz is version 1, a table GnuCash 5.17 does not have"]
        );
    }

    #[test]
    fn the_ddl_builds_gnucash_s_schema() {
        let conn = Connection::open_in_memory().unwrap();
        ddl().for_each(|s| conn.execute_batch(s).unwrap());
        assert!(check(&conn).unwrap().is_empty());
        let tables = objects(&conn).unwrap().into_iter().filter(|o| o.kind == "table").count();
        assert_eq!(tables, VERSIONS.len());
    }
}
