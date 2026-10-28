//! Session behaviour on copies of the fixtures in a temporary directory.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use red_ale_core::{DomainError, FixedEnv, Time64};
use red_ale_service::lock::lock_path;
use red_ale_service::{ReadOnlyReason, ServiceError, Session, Settings};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR")))
}

fn copy_to(dir: &Path, name: &str) -> PathBuf {
    let dest = dir.join("book.gnucash");
    std::fs::copy(fixture(name), &dest).unwrap();
    dest
}

/// The rich fixture with extra book slots inserted after `book:id`.
fn with_book_slots(dir: &Path, slots: &str) -> PathBuf {
    let text = std::fs::read_to_string(fixture("synthetic/rich.gnucash")).unwrap();
    let id_end = text.find("</book:id>\n").unwrap() + "</book:id>\n".len();
    let patched = format!("{}<book:slots>\n{slots}</book:slots>\n{}", &text[..id_end], &text[id_end..]);
    let dest = dir.join("book.gnucash");
    std::fs::write(&dest, patched).unwrap();
    dest
}

fn slot(key: &str, kind: &str, value: &str) -> String {
    format!("  <slot>\n    <slot:key>{key}</slot:key>\n    <slot:value type=\"{kind}\">{value}</slot:value>\n  </slot>\n")
}

fn frame(key: &str, inner: &str) -> String {
    format!("  <slot>\n    <slot:key>{key}</slot:key>\n    <slot:value type=\"frame\">\n{inner}    </slot:value>\n  </slot>\n")
}

#[test]
fn open_locks_and_close_releases() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "synthetic/rich.gnucash");
    let session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert!(!session.is_read_only());
    assert!(lock_path(&path).exists());
    assert!(matches!(
        Session::open(path.to_str().unwrap(), false, Settings::default()),
        Err(ServiceError::Locked { .. })
    ));
    let reader = Session::open(path.to_str().unwrap(), true, Settings::default()).unwrap();
    assert_eq!(reader.read_only_reasons(), [ReadOnlyReason::Requested]);
    session.close(false).unwrap();
    assert!(!lock_path(&path).exists());
    drop(reader);
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_read_only_sqlite_session_never_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "sqlite/synthetic/rich.gnucash");
    let before = std::fs::read(&path).unwrap();
    let mut reader = Session::open(&format!("sqlite3://{}", path.display()), true, Settings::default()).unwrap();
    assert_eq!(reader.read_only_reasons(), [ReadOnlyReason::Requested]);
    assert!(matches!(reader.save(None), Err(ServiceError::ReadOnly { .. })));
    reader.close(true).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(listing(dir.path()), ["book.gnucash"]);
}

fn gnclock(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    conn.query_row("SELECT count(*) FROM gnclock", [], |r| r.get(0)).unwrap()
}

fn describe_first(session: &mut Session, text: &str) {
    session
        .mutate(|book, _| {
            let t = book.transactions.values_mut().next().unwrap();
            Arc::make_mut(t).description = Some(text.into());
            Ok(())
        })
        .unwrap();
}

#[test]
fn an_sqlite_session_journals_backs_up_once_and_saves() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "sqlite/synthetic/rich.gnucash");
    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert_eq!(session.format(), red_ale_service::backend::Format::Sqlite);
    assert!(!session.is_read_only());
    assert_eq!(gnclock(&path), 1);
    let report = session.save(None).unwrap();
    assert_eq!(report.backup, None, "a save with nothing to write makes no backup");
    describe_first(&mut session, "First");
    let journal = session.journal_path().unwrap().to_owned();
    assert!(
        journal.display().to_string().starts_with(&format!("{}.", path.display())),
        "{}",
        journal.display()
    );
    assert!(std::fs::read_to_string(&journal).unwrap().contains("First"));
    let backup = session.save(None).unwrap().backup.unwrap();
    assert!(
        backup.display().to_string().ends_with(".gnucash") && backup != path,
        "{}",
        backup.display()
    );
    describe_first(&mut session, "Second");
    assert_eq!(session.save(None).unwrap().backup, None, "one backup per session");
    session.close(false).unwrap();
    assert_eq!(gnclock(&path), 0);
    assert_eq!(gnclock(&backup), 0, "the backup holds no lock row");

    let read = Session::open(path.to_str().unwrap(), true, Settings::default()).unwrap();
    assert_eq!(read.book().transactions[0].description.as_deref(), Some("Second"));
    let old = Session::open(backup.to_str().unwrap(), true, Settings::default()).unwrap();
    assert_ne!(old.book().transactions[0].description.as_deref(), Some("First"));
    let names = listing(dir.path());
    assert!(
        names.iter().any(|n| Path::new(n).extension().is_some_and(|e| e == "log")),
        "{names:?}"
    );
}

fn has_iso_dates(path: &Path) -> bool {
    let session = Session::open(path.to_str().unwrap(), true, Settings::default()).unwrap();
    red_ale_core::book_options::features(session.book())
        .iter()
        .any(|(name, _)| name.starts_with("ISO-8601 formatted date strings"))
}

/// The ISO-dates Book feature is set by the first save that writes, never on open (`design/sql-fidelity.md` §4).
#[test]
fn the_first_save_sets_the_iso_dates_feature() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "sqlite/synthetic/rich.gnucash");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DELETE FROM slots WHERE name = 'features' OR name LIKE 'features/%'")
        .unwrap();
    drop(conn);
    assert!(!has_iso_dates(&path));
    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    session.save(None).unwrap();
    session.close(false).unwrap();
    assert!(!has_iso_dates(&path), "an open and a save with nothing to write set nothing");

    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    describe_first(&mut session, "First");
    session.save(None).unwrap();
    session.close(false).unwrap();
    assert!(has_iso_dates(&path));
}

/// An SQLite session's journal replays into the next session, as an XML book's does (`design/sql-fidelity.md` §6).
#[test]
fn an_sqlite_book_s_journal_replays() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "sqlite/gnucash/sample1.gnucash");
    let mut lost = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    describe_first(&mut lost, "Journaled");
    let journal = lost.journal_path().unwrap().to_owned();
    drop(lost);

    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert_ne!(session.book().transactions[0].description.as_deref(), Some("Journaled"));
    let report = session.replay_journal(&journal, false).unwrap();
    assert_eq!(report.updated.len(), 1, "{report:?}");
    session.save(None).unwrap();
    session.close(false).unwrap();
    let read = Session::open(path.to_str().unwrap(), true, Settings::default()).unwrap();
    assert_eq!(read.book().transactions[0].description.as_deref(), Some("Journaled"));
}

/// Retention treats an SQLite book's backups as an XML book's: by name and age (`design/sql-fidelity.md` §6).
#[test]
fn retention_removes_an_sqlite_book_s_backups() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "sqlite/synthetic/rich.gnucash");
    let settings = Settings {
        retention: red_ale_service::files::Retention::Never,
        ..Settings::default()
    };
    let mut session = Session::open(path.to_str().unwrap(), false, settings).unwrap();
    describe_first(&mut session, "First");
    let report = session.save(None).unwrap();
    let backup = report.backup.clone().unwrap();
    assert!(report.removed.contains(&backup), "{report:?}");
    assert!(!backup.exists());
    session.close(false).unwrap();
}

#[test]
fn dropping_an_sqlite_session_releases_its_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "sqlite/synthetic/rich.gnucash");
    let session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    let err = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap_err();
    assert!(
        matches!(&err, ServiceError::SqlLocked { pid, .. } if *pid == i64::from(std::process::id())),
        "{err}"
    );
    drop(session);
    assert_eq!(gnclock(&path), 0);
}

#[test]
fn a_new_sqlite_book_is_created_whole_and_locked() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.gnucash");
    let session = Session::create(&format!("sqlite3://{}", path.display()), Settings::default()).unwrap();
    assert_eq!(session.format(), red_ale_service::backend::Format::Sqlite);
    assert_eq!(gnclock(&path), 1);
    session.close(false).unwrap();
    assert_eq!(gnclock(&path), 0);
    assert_eq!(listing(dir.path()), ["new.gnucash"]);
    let reopened = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert!(!reopened.is_read_only(), "{:?}", reopened.read_only_reasons());
}

#[test]
fn an_sqlite_book_with_another_schema_version_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "piecash/simple_sample.sqlite.gnucash");
    let session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    let reasons = session.read_only_reasons();
    let versions = reasons.iter().find_map(|r| match r {
        ReadOnlyReason::SchemaVersions(list) => Some(list),
        _ => None,
    });
    assert!(
        versions.unwrap().iter().any(|v| v.starts_with("splits is version 4")),
        "{reasons:?}"
    );
    assert!(reasons.iter().any(|r| r.code() == "schema-version"));
    drop(session);
    assert_eq!(gnclock(&path), 0, "a book that opens read-only takes no lock");
}

#[test]
fn a_scheme_the_file_contradicts_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let xml = copy_to(dir.path(), "synthetic/rich.gnucash");
    let err = Session::open(&format!("sqlite3://{}", xml.display()), true, Settings::default()).unwrap_err();
    assert!(matches!(err, ServiceError::WrongFormat { .. }), "{err}");
    let other = tempfile::tempdir().unwrap();
    let sqlite = copy_to(other.path(), "sqlite/synthetic/rich.gnucash");
    let err = Session::open(&format!("xml://{}", sqlite.display()), true, Settings::default()).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("{} is an SQLite book, not an XML book as its URI says", sqlite.display())
    );
    assert!(!lock_path(&xml).exists() && !lock_path(&sqlite).exists());
}

#[test]
fn database_server_books_are_refused_before_anything_happens() {
    let err = Session::open("postgres://user@host/db", false, Settings::default()).unwrap_err();
    assert!(matches!(err, ServiceError::SqlBook { .. }), "{err}");
    assert!(err.to_string().contains("v0.7"), "{err}");
    let err = Session::open("mysql://user@host/db", false, Settings::default()).unwrap_err();
    assert!(matches!(err, ServiceError::SqlBook { .. }), "{err}");
}

#[test]
fn files_that_are_not_sqlite_books_are_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    let mut bytes = b"SQLite format 3\0".to_vec();
    bytes.resize(4096, 0xa5);
    std::fs::write(&path, &bytes).unwrap();
    let err = Session::open(path.to_str().unwrap(), true, Settings::default()).unwrap_err();
    assert!(matches!(err, ServiceError::Sql { .. }), "{err}");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn unsupported_feature_opens_read_only_without_a_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = with_book_slots(
        dir.path(),
        &frame(
            "features",
            &format!("  {}", slot("Some Future Feature", "string", "A feature from a later GnuCash")),
        ),
    );
    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert!(matches!(session.read_only_reasons(), [ReadOnlyReason::UnsupportedFeatures(_)]));
    assert!(!lock_path(&path).exists());
    assert!(matches!(session.mutate(|_, _| Ok(())), Err(ServiceError::ReadOnly { .. })));
    assert!(matches!(session.save(None), Err(ServiceError::ReadOnly { .. })));
}

#[test]
fn unknown_features_open_read_only_but_number_field_source_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let features = slot("Number Field Source", "string", "User specifies select transaction number source")
        + &slot("Some Future Feature", "string", "");
    let path = with_book_slots(dir.path(), &frame("features", &features));
    let session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    let [ReadOnlyReason::UnsupportedFeatures(names)] = session.read_only_reasons() else {
        panic!("{:?}", session.read_only_reasons())
    };
    assert_eq!(names, &["Some Future Feature"]);
    assert!(!lock_path(&path).exists());
}

#[test]
fn num_field_source_and_red_line_books_open_for_editing() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = slot("Day Threshold for Read-Only Transactions (red line)", "double", "30")
        + &slot("Use Split Action Field for Number", "string", "t");
    let features = slot(
        "Number Field Source",
        "string",
        "User specifies source of 'num' field'; either transaction number or split action (requires at least GnuCash 2.5.0)",
    );
    let path = with_book_slots(
        dir.path(),
        &(frame("features", &features) + &frame("options", &frame("Accounts", &accounts))),
    );
    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert!(!session.is_read_only(), "{:?}", session.read_only_reasons());
    assert!(lock_path(&path).exists());
    session.mutate(|_, _| Ok(())).unwrap();
}

#[test]
fn credit_note_fixture_opens_for_editing() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "synthetic/business.gnucash");
    let session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert!(session.read_only_reasons().is_empty(), "{:?}", session.read_only_reasons());
    assert!(lock_path(&path).exists());
}

#[test]
fn requested_read_only_takes_no_lock_and_refuses_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "synthetic/rich.gnucash");
    let mut session = Session::open(path.to_str().unwrap(), true, Settings::default()).unwrap();
    assert_eq!(session.read_only_reasons(), [ReadOnlyReason::Requested]);
    assert!(!lock_path(&path).exists());
    assert!(matches!(session.mutate(|_, _| Ok(())), Err(ServiceError::ReadOnly { .. })));
}

#[test]
fn supported_features_stay_writable() {
    let dir = tempfile::tempdir().unwrap();
    let features = slot("Register sort and filter settings stored in .gcm file", "string", "x")
        + &slot(
            "Use a dedicated opening balance account identified by an 'equity-type' slot",
            "string",
            "y",
        );
    let path = with_book_slots(dir.path(), &frame("features", &features));
    let session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert!(!session.is_read_only(), "{:?}", session.read_only_reasons());
}

#[test]
fn red_line_and_trading_accounts_open_for_editing() {
    let dir = tempfile::tempdir().unwrap();
    let accounts =
        slot("Use Trading Accounts", "string", "t") + &slot("Day Threshold for Read-Only Transactions (red line)", "double", "30");
    let path = with_book_slots(dir.path(), &frame("options", &frame("Accounts", &accounts)));
    let session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert!(!session.is_read_only(), "{:?}", session.read_only_reasons());
}

#[test]
fn num_field_source_option_alone_opens_for_editing() {
    let dir = tempfile::tempdir().unwrap();
    let option = slot("Use Split Action Field for Number", "string", "t");
    let path = with_book_slots(dir.path(), &frame("options", &frame("Accounts", &option)));
    let session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert!(!session.is_read_only(), "{:?}", session.read_only_reasons());
}

#[test]
fn a_failed_open_leaves_no_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.gnucash");
    std::fs::write(&path, "<?xml version=\"1.0\"?>\n<gnc-v2>\n<gnc:book version=\"2.0.0\">\n").unwrap();
    assert!(matches!(
        Session::open(path.to_str().unwrap(), false, Settings::default()),
        Err(ServiceError::Xml { .. })
    ));
    assert!(!lock_path(&path).exists());
}

#[test]
fn unknown_elements_open_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let text = std::fs::read_to_string(fixture("synthetic/rich.gnucash")).unwrap().replacen(
        "<act:type>ASSET</act:type>",
        "<act:type>ASSET</act:type>\n  <act:frobnicate/>",
        1,
    );
    let path = dir.path().join("book.gnucash");
    std::fs::write(&path, text).unwrap();
    let session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert!(matches!(session.read_only_reasons(), [ReadOnlyReason::UnrecognisedContent(_)]));
}

/// GnuCash 5.17 reads a legacy frequency spec and rewrites it as a schedule (`design/notes/xml-format-core.md` §8.5),
/// so red-ale keeps it as read and edits nothing.
#[test]
fn a_legacy_frequency_spec_opens_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let freqspec = concat!(
        "  <sx:freqspec>\n    <gnc:freqspec version=\"1.0.0\">\n      <fs:ui_type>monthly</fs:ui_type>\n",
        "    </gnc:freqspec>\n  </sx:freqspec>\n"
    );
    let text = std::fs::read_to_string(fixture("synthetic/rich.gnucash"))
        .unwrap()
        .replacen("<gnc:schedxaction version=\"2.0.0\">", "<gnc:schedxaction version=\"1.0.0\">", 1)
        .replacen("  <sx:schedule>", &format!("{freqspec}  <sx:schedule>"), 1);
    let path = dir.path().join("book.gnucash");
    std::fs::write(&path, &text).unwrap();
    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    let [ReadOnlyReason::UnrecognisedContent(reasons)] = session.read_only_reasons() else {
        panic!("{:?}", session.read_only_reasons())
    };
    assert_eq!(
        *reasons,
        ["scheduled transaction \"Monthly rent\" has a legacy frequency spec (sx:freqspec), which GnuCash 5.17 rewrites as a schedule"]
    );
    assert!(matches!(session.save(None), Err(ServiceError::ReadOnly { .. })));
    assert_eq!(
        red_ale_xml::write_book_text(session.book(), session.xml_residue().unwrap()),
        text,
        "kept as read"
    );
}

#[test]
fn failed_mutation_changes_nothing_and_writes_no_journal() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "synthetic/rich.gnucash");
    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    let before = session.book().clone();
    let result: Result<(), _> = session.mutate(|book, _| {
        book.accounts.clear();
        Err(DomainError::invalid("refused"))
    });
    assert!(result.is_err());
    assert_eq!(*session.book(), before);
    assert!(!session.is_dirty());
    assert!(session.journal_path().is_none());
}

#[test]
fn unbalanced_change_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "synthetic/rich.gnucash");
    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    let guid = *session.book().transactions.keys().next().unwrap();
    let err = session
        .mutate(|book, _| {
            let t = Arc::make_mut(book.transactions.get_mut(&guid).unwrap());
            t.splits[0].value = t.splits[0].value.add_fixed(red_ale_core::Numeric::new(1, 100)).unwrap();
            Ok(())
        })
        .unwrap_err();
    assert!(matches!(err, ServiceError::Domain(DomainError::Unbalanced { .. })), "{err}");
}

#[test]
fn edit_save_reopen_and_journal() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "synthetic/rich.gnucash");
    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    session.set_env(FixedEnv::new(Time64(1_700_000_000), 1));
    let guid = *session.book().transactions.keys().next().unwrap();
    session
        .mutate(|book, _| {
            Arc::make_mut(book.transactions.get_mut(&guid).unwrap()).description = Some("edited".into());
            Ok(())
        })
        .unwrap();
    let journal = session.journal_path().unwrap().to_owned();
    let report = session.save(None).unwrap();
    assert!(report.backup.is_some());
    let log = std::fs::read_to_string(&journal).unwrap();
    assert!(log.contains("\tedited\t"), "{log}");
    assert!(log.lines().any(|l| l.starts_with("B\t")) && log.lines().any(|l| l.starts_with("C\t")));
    session.close(false).unwrap();
    let reopened = Session::open(path.to_str().unwrap(), true, Settings::default()).unwrap();
    assert_eq!(reopened.book().transaction(guid).unwrap().description.as_deref(), Some("edited"));
}

#[test]
fn create_new_book() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.gnucash");
    let session = Session::create(path.to_str().unwrap(), Settings::default()).unwrap();
    assert!(path.exists());
    assert!(matches!(
        Session::create(path.to_str().unwrap(), Settings::default()),
        Err(ServiceError::Exists { .. } | ServiceError::Locked { .. })
    ));
    session.close(false).unwrap();
    let reopened = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert_eq!(reopened.book().accounts.len(), 1);
    assert_eq!(reopened.compressed(), Some(true));
}
