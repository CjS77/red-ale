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

#[test]
fn sql_books_are_refused_before_anything_happens() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), "piecash/simple_sample.sqlite.gnucash");
    let err = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap_err();
    assert!(matches!(err, ServiceError::SqlBook { .. }), "{err}");
    assert!(!lock_path(&path).exists());
    let err = Session::open(&format!("sqlite3://{}", path.display()), false, Settings::default()).unwrap_err();
    assert!(matches!(err, ServiceError::SqlBook { .. }));
    let err = Session::open("postgres://user@host/db", false, Settings::default()).unwrap_err();
    assert!(matches!(err, ServiceError::SqlBook { .. }));
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
