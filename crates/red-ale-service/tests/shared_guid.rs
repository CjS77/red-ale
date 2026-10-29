//! A transaction and a template transaction sharing a GUID (`design/file-fidelity.md`, `design/sql-fidelity.md`):
//! GnuCash keeps both in one collection, where one would hide the other, so no Mutation may make one and no save
//! writes one. Both refusals are `invalid` on the wire, as `DomainError::SharedGuid`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use red_ale_core::{DomainError, Guid};
use red_ale_service::{ServiceError, Session, Settings};

/// The template transaction of `synthetic/rich.gnucash`, XML and SQLite.
const TEMPLATE: &str = "68bf5e7740392eb15a079ee99f18a357";

/// The first transaction of `synthetic/rich.gnucash`.
const FIRST: &str = "f485efcd743943ddb6eede20c1d22c05";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR")))
}

fn copy_to(dir: &Path, name: &str) -> PathBuf {
    let dest = dir.join("book.gnucash");
    std::fs::copy(fixture(name), &dest).unwrap();
    dest
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

fn is_shared_guid(err: &ServiceError) -> bool {
    matches!(err, ServiceError::Domain(DomainError::SharedGuid { guid }) if guid.to_string() == TEMPLATE)
}

/// Add a copy of the first transaction, with new split GUIDs, under the template transaction's GUID.
fn add_copy_as_template_guid(session: &mut Session) -> Result<(), ServiceError> {
    let template = Guid::parse(TEMPLATE).unwrap();
    session.mutate(|book, env| {
        let mut copy = (**book.transactions.values().next().unwrap()).clone();
        copy.guid = template;
        copy.splits.iter_mut().for_each(|s| s.guid = env.new_guid());
        book.transactions.insert(template, Arc::new(copy));
        Ok(())
    })
}

fn a_mutation_making_a_shared_guid_is_refused(fixture_name: &str) {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_to(dir.path(), fixture_name);
    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    let before = session.book().clone();
    let err = add_copy_as_template_guid(&mut session).unwrap_err();
    assert!(is_shared_guid(&err), "{err}");
    assert_eq!(*session.book(), before);
    assert!(!session.is_dirty());
    assert_eq!(session.revision(), 0);
    assert!(session.journal_path().is_none());
}

#[test]
fn a_mutation_giving_a_transaction_a_template_transaction_s_guid_is_refused_on_xml() {
    a_mutation_making_a_shared_guid_is_refused("synthetic/rich.gnucash");
}

#[test]
fn a_mutation_giving_a_transaction_a_template_transaction_s_guid_is_refused_on_sqlite() {
    a_mutation_making_a_shared_guid_is_refused("sqlite/synthetic/rich.gnucash");
}

/// An XML file can hold the two under one GUID, as red-ale's testkit once wrote one; it opens, but red-ale won't
/// save it, writes nothing and makes no backup, and the session goes on. A Mutation removing one of the two is
/// allowed, and the book then saves.
#[test]
fn an_xml_book_read_with_a_shared_guid_opens_but_does_not_save_until_it_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let text = std::fs::read_to_string(fixture("synthetic/rich.gnucash")).unwrap();
    let patched = text.replacen(
        &format!("<trn:id type=\"guid\">{FIRST}</trn:id>"),
        &format!("<trn:id type=\"guid\">{TEMPLATE}</trn:id>"),
        1,
    );
    assert_ne!(patched, text);
    let path = dir.path().join("book.gnucash");
    std::fs::write(&path, &patched).unwrap();
    let mut session = Session::open(path.to_str().unwrap(), false, Settings::default()).unwrap();
    assert!(!session.is_read_only());
    let template = Guid::parse(TEMPLATE).unwrap();
    assert!(session.book().shares_transaction_guid(template));

    let err = session.save(None).unwrap_err();
    assert!(is_shared_guid(&err), "{err}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), patched, "nothing written");
    assert_eq!(listing(dir.path()), ["book.gnucash", "book.gnucash.LCK"], "no backup");

    session
        .mutate(|book, _| {
            book.transactions.shift_remove(&template);
            Ok(())
        })
        .unwrap();
    session.save(None).unwrap();
    session.close(false).unwrap();
    let saved = Session::open(path.to_str().unwrap(), true, Settings::default()).unwrap();
    assert!(!saved.book().shares_transaction_guid(template));
}
