//! What makes an SQLite book read-only, and what is refused outright (`design/sql-fidelity.md` §1). Every test works
//! on a copy in a temporary directory; the changes are made with a separate read-write connection, as another program
//! would make them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use red_ale_sql::{Findings, SqlError, open_read_only};
use rusqlite::Connection;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

fn copy(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(fixture(name), &path).unwrap();
    (dir, path)
}

/// A copy of the rich fixture with `sql` applied by another connection.
fn rich_with(sql: &str) -> (tempfile::TempDir, PathBuf) {
    let (dir, path) = copy("sqlite/synthetic/rich.gnucash");
    Connection::open(&path).unwrap().execute_batch(sql).unwrap();
    (dir, path)
}

fn findings(path: &Path) -> Findings {
    let before = std::fs::read(path).unwrap();
    let (_, _, findings) = open_read_only(path).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), before, "a read-only open changed the file");
    findings
}

fn unrecognised(sql: &str) -> Vec<String> {
    let (_dir, path) = rich_with(sql);
    let f = findings(&path);
    assert!(f.schema_versions.is_empty(), "{f:?}");
    f.unrecognised
}

#[test]
fn an_older_schema_version_names_its_table() {
    // Written by GnuCash 3.0 with splits at version 4 (`design/notes/sql-backend.md` §2.3).
    let (_dir, path) = copy("piecash/simple_sample.sqlite.gnucash");
    let f = findings(&path);
    assert_eq!(f.schema_versions, ["splits is version 4, GnuCash 5.17's is 5"]);
    assert!(f.unrecognised.is_empty(), "{f:?}");
}

#[test]
fn versions_changed_down_and_up_are_named() {
    let cases = [
        ("UPDATE versions SET table_version = 19921 WHERE table_name = 'Gnucash-Resave'", "Gnucash-Resave is version 19921"),
        ("UPDATE versions SET table_version = 19919 WHERE table_name = 'Gnucash'", "Gnucash is 19919"),
        ("UPDATE versions SET table_version = 5000018 WHERE table_name = 'Gnucash'", "Gnucash is 5000018"),
        ("UPDATE versions SET table_version = 1 WHERE table_name = 'prices'", "prices is version 1"),
        ("UPDATE versions SET table_version = 6 WHERE table_name = 'splits'", "splits is version 6"),
    ];
    cases.iter().for_each(|(sql, named)| {
        let (_dir, path) = rich_with(sql);
        let f = findings(&path);
        assert!(f.schema_versions.len() == 1 && f.schema_versions[0].starts_with(named), "{sql}: {f:?}");
    });
}

#[test]
fn unknown_tables_columns_and_indexes_are_named() {
    assert_eq!(unrecognised("CREATE TABLE zz_custom (x)"), ["table zz_custom is not in GnuCash 5.17's schema"]);
    assert_eq!(
        unrecognised("ALTER TABLE transactions ADD COLUMN zz_extra text"),
        ["table transactions has column zz_extra that GnuCash 5.17 does not"]
    );
    assert_eq!(
        unrecognised("CREATE INDEX zz_idx ON accounts(name)"),
        ["index zz_idx is not in GnuCash 5.17's schema"]
    );
    assert_eq!(
        unrecognised("CREATE TRIGGER zz_t AFTER INSERT ON splits BEGIN SELECT 1; END"),
        ["trigger zz_t is not in GnuCash 5.17's schema"]
    );
    assert_eq!(
        unrecognised("CREATE TABLE accounts_back AS SELECT * FROM accounts"),
        ["table accounts_back looks like one left by a failed GnuCash safe-save, which GnuCash would recover on open"]
    );
    assert_eq!(unrecognised("DROP TABLE jobs"), ["GnuCash 5.17's table jobs is missing"]);
    assert_eq!(unrecognised("DROP TABLE gnclock"), ["GnuCash 5.17's table gnclock is missing"]);
    assert_eq!(unrecognised("DROP INDEX slots_guid_index"), ["GnuCash 5.17's index slots_guid_index is missing"]);
}

#[test]
fn a_column_of_another_declared_type_is_named() {
    let sql = "CREATE TABLE lots_new(guid text(32) PRIMARY KEY NOT NULL, account_guid text(32), is_closed text NOT NULL); \
               INSERT INTO lots_new SELECT * FROM lots; DROP TABLE lots; ALTER TABLE lots_new RENAME TO lots;";
    assert_eq!(
        unrecognised(sql),
        [
            "table lots, column is_closed: not declared as GnuCash 5.17 declares it",
            "lots row 1, column is_closed: text where GnuCash writes an integer",
        ]
    );
}

#[test]
fn budget_amounts_must_agree_with_the_budget_s_slots() {
    let budget = "budget_amounts: the rows of budget 0123456789abcdef0123456789abcdef disagree with its amount Slots";
    assert_eq!(unrecognised("UPDATE budget_amounts SET amount_num = 101 WHERE id = 1"), [budget]);
    assert_eq!(unrecognised("UPDATE budget_amounts SET amount_num = 200, amount_denom = 2 WHERE id = 1"), [budget]);
    assert_eq!(unrecognised("DELETE FROM budget_amounts WHERE id = 2"), [budget]);
    let orphan = "INSERT INTO budget_amounts(budget_guid, account_guid, period_num, amount_num, amount_denom) \
                  VALUES ('00000000000000000000000000000009', '1feca2abf92347288c268ebf93753ec4', 0, 1, 1)";
    assert!(unrecognised(orphan).is_empty(), "an orphan row is not a finding");
}

#[test]
fn hidden_and_placeholder_columns_must_agree_with_the_slots() {
    assert_eq!(
        unrecognised("UPDATE accounts SET hidden = 1 WHERE name = 'Cash'"),
        ["accounts: 9ea336dec8b64310a48810eee3325a51's hidden column disagrees with its hidden Slot"]
    );
    assert_eq!(
        unrecognised("UPDATE accounts SET placeholder = NULL WHERE name = 'Assets'"),
        ["accounts: d4e3331aca124864ba674c50ae7ddcf1's placeholder column disagrees with its placeholder Slot"]
    );
}

#[test]
fn refused_cells_name_table_row_and_column() {
    assert_eq!(
        unrecognised("UPDATE transactions SET post_date = '2024-06-01 10:59:00 +0200' WHERE rowid = 1"),
        ["transactions row 1, column post_date: \"2024-06-01 10:59:00 +0200\" is not a timestamp in either form GnuCash writes"]
    );
    assert_eq!(
        unrecognised("UPDATE splits SET value_num = 1.5 WHERE rowid = 1"),
        ["splits row 1, column value_num: real where GnuCash writes an integer"]
    );
    assert_eq!(
        unrecognised("UPDATE slots SET double_val = 'text' WHERE slot_type = 2 AND name = 'aa-double'"),
        [
            "slots row 4, column double_val: text where GnuCash writes a real",
            "slots row 4: left out: slot \"aa-double\" has no owner or no value of its type 2",
        ]
    );
    assert_eq!(
        unrecognised("UPDATE accounts SET parent_guid = 'D4E3331ACA124864BA674C50AE7DDCF1' WHERE name = 'Cash'"),
        [
            "accounts row 4, column parent_guid: \"D4E3331ACA124864BA674C50AE7DDCF1\" is not a GUID as GnuCash writes one",
            "accounts: 9ea336dec8b64310a48810eee3325a51 is reachable from neither root",
            "splits: 3c890c1461e14b6a938a843767a85cd5 refers to account 9ea336dec8b64310a48810eee3325a51, which has no row in either tree",
        ]
    );
}

#[test]
fn compact_timestamps_read_as_iso_ones() {
    let (_dir, path) = rich_with("UPDATE transactions SET post_date = '20240601105900' WHERE rowid = 1");
    let (book, _, f) = open_read_only(&path).unwrap();
    assert!(f.is_empty(), "{f:?}");
    let (iso, _, _) = open_read_only(&fixture("sqlite/synthetic/rich.gnucash")).unwrap();
    assert_eq!(book.transactions[0].date_posted, iso.transactions[0].date_posted);
}

#[test]
fn slot_rows_that_are_not_a_tree_are_named() {
    assert_eq!(
        unrecognised("UPDATE slots SET name = 'elsewhere/b' WHERE name = 'frame-nested/b'"),
        ["slots row 17: name \"elsewhere/b\" is not its parent's path and a key"]
    );
    assert_eq!(
        unrecognised("UPDATE slots SET slot_type = 7 WHERE name = 'color'"),
        ["slots row 7, column slot_type: 7 is not a slot type GnuCash 5.17 reads"]
    );
    let price_slot = "INSERT INTO slots(obj_guid, name, slot_type, string_val) VALUES ('8318eee98d264a3e85b01e608ef2026f', 'x', 4, 'y')";
    assert_eq!(unrecognised(price_slot), ["slots row 80: owned by 8318eee98d264a3e85b01e608ef2026f, which has no Slots in red-ale"]);
}

#[test]
fn orphan_rows_are_kept_in_the_residue_not_reported() {
    let sql = "INSERT INTO slots(obj_guid, name, slot_type, string_val) VALUES ('00000000000000000000000000000007', 'x', 4, 'y'); \
               INSERT INTO splits VALUES ('00000000000000000000000000000008', '00000000000000000000000000000006', \
               'c080059e1aec4c0b9ad4671e80965686', '', '', 'n', NULL, 1, 1, 1, 1, NULL);";
    let (_dir, path) = rich_with(sql);
    let (_, opened, f) = open_read_only(&path).unwrap();
    assert!(f.is_empty(), "{f:?}");
    let orphans = &opened.residue().orphans;
    assert_eq!(orphans.slots.iter().map(ToString::to_string).collect::<Vec<_>>(), ["00000000000000000000000000000007"]);
    assert_eq!(orphans.splits.iter().map(ToString::to_string).collect::<Vec<_>>(), ["00000000000000000000000000000006"]);
}

#[test]
fn a_wal_book_is_refused() {
    let (_dir, path) = copy("sqlite/synthetic/rich.gnucash");
    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    drop(conn);
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(open_read_only(&path), Err(SqlError::WriteAheadLog)));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn a_hot_journal_is_refused_and_left_alone() {
    // A crash mid-write: copy the book and its journal while another connection is writing, so the copy has a
    // journal no process holds a lock for.
    let (dir, path) = copy("sqlite/synthetic/rich.gnucash");
    let writer = Connection::open(&path).unwrap();
    writer.pragma_update(None, "cache_size", 1).unwrap();
    writer.execute_batch("BEGIN; UPDATE slots SET string_val = string_val || 'x'; UPDATE accounts SET name = name || 'x';").unwrap();
    let crashed = dir.path().join("crashed.gnucash");
    std::fs::copy(&path, &crashed).unwrap();
    std::fs::copy(dir.path().join("book.gnucash-journal"), dir.path().join("crashed.gnucash-journal")).unwrap();
    writer.execute_batch("ROLLBACK").unwrap();
    let journal = dir.path().join("crashed.gnucash-journal");
    let (book, journal_bytes) = (std::fs::read(&crashed).unwrap(), std::fs::read(&journal).unwrap());
    assert!(!journal_bytes.is_empty());
    assert!(matches!(open_read_only(&crashed), Err(SqlError::HotJournal(_))));
    assert_eq!(std::fs::read(&crashed).unwrap(), book);
    assert_eq!(std::fs::read(&journal).unwrap(), journal_bytes);
}

#[test]
fn files_that_are_not_books_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty.gnucash");
    std::fs::write(&empty, b"").unwrap();
    assert!(matches!(open_read_only(&empty), Err(SqlError::NotADatabase(_))));
    let xml = dir.path().join("xml.gnucash");
    std::fs::copy(fixture("synthetic/rich.gnucash"), &xml).unwrap();
    assert!(matches!(open_read_only(&xml), Err(SqlError::NotADatabase(_))));
    let mut garbage = b"SQLite format 3\0".to_vec();
    garbage.resize(4096, 0xa5);
    let bad = dir.path().join("bad.gnucash");
    std::fs::write(&bad, &garbage).unwrap();
    assert!(open_read_only(&bad).is_err());
    let (_d, no_book) = rich_with("DELETE FROM books");
    assert!(matches!(open_read_only(&no_book), Err(SqlError::NotABook(_))));
}
