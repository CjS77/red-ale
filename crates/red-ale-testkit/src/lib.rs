//! Test support: fixture paths, temporary copies of books, and the GnuCash 5.17 oracle.
//!
//! The oracle loads a book in GnuCash's own engine and reports what GnuCash sees as canonical JSON
//! (`oracle/dump.py`). [`model::dump`] builds the same JSON from a red-ale Book, and [`diff`] lists where the two
//! differ. Tests never touch real books: every book is a copy in a temporary directory.

#![forbid(unsafe_code)]
// Test support code may fail loudly: a broken fixture is a broken test.
#![allow(clippy::missing_panics_doc, clippy::expect_used, clippy::panic)]

pub mod memory;
pub mod model;
pub mod oracle;
pub mod quote_server;
pub mod synthetic;

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The repository's `fixtures/` directory.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// A fixture by path relative to `fixtures/`, such as `synthetic/rich.gnucash`.
pub fn fixture(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

/// Fixtures in GnuCash's version 1 XML format, which red-ale refuses with a clear message.
pub const VERSION_1_FIXTURES: [&str; 1] = ["gnucash/currency_tree_xml.gnucash"];

/// Every XML book fixture in the version 2 format, relative to `fixtures/`, sorted.
pub fn xml_fixtures() -> Vec<String> {
    let mut out: Vec<String> = ["gnucash", "piecash", "reports", "synthetic"]
        .iter()
        .flat_map(|dir| {
            std::fs::read_dir(fixtures_dir().join(dir))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "gnucash"))
                .filter(|p| !std::fs::read(p).is_ok_and(|b| b.starts_with(b"SQLite format 3\0")))
                .filter_map(|p| p.file_name().map(|n| format!("{dir}/{}", n.to_string_lossy())))
                .collect::<Vec<_>>()
        })
        .collect();
    out.retain(|f| !VERSION_1_FIXTURES.contains(&f.as_str()));
    out.sort();
    out
}

/// The books of GnuCash 5.17's report tests, rebuilt by `scripts/report-fixtures/`, relative to `fixtures/`,
/// sorted.
pub fn report_fixtures() -> Vec<String> {
    xml_fixtures().into_iter().filter(|f| f.starts_with("reports/")).collect()
}

/// A book copied into its own temporary directory, removed when dropped.
#[derive(Debug)]
pub struct TempBook {
    /// The directory; backups and journals land here too.
    pub dir: tempfile::TempDir,
    /// The book's path.
    pub path: PathBuf,
}

impl TempBook {
    /// Copy a fixture.
    pub fn copy(name: &str) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("book.gnucash");
        std::fs::copy(fixture(name), &path).unwrap_or_else(|e| panic!("copying fixture {name}: {e}"));
        Self { dir, path }
    }

    /// A path in a fresh temporary directory where a new book can be created.
    pub fn new_path() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("book.gnucash");
        Self { dir, path }
    }

    /// The path as a string, for RPC params.
    pub fn path_str(&self) -> String {
        self.path.display().to_string()
    }
}

/// Where two JSON values differ: one line per difference, as `path: expected != actual`. `ignore` lists paths to
/// skip, where `*` matches any array index or object key (`business.invoices.*.total`).
pub fn diff(expected: &Value, actual: &Value, ignore: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    walk("", expected, actual, ignore, &mut out);
    out
}

fn ignored(path: &str, ignore: &[&str]) -> bool {
    let parts: Vec<&str> = path.split('.').collect();
    ignore.iter().any(|pattern| {
        let pat: Vec<&str> = pattern.split('.').collect();
        pat.len() == parts.len() && pat.iter().zip(&parts).all(|(p, q)| *p == "*" || p == q)
    })
}

fn walk(path: &str, expected: &Value, actual: &Value, ignore: &[&str], out: &mut Vec<String>) {
    if ignored(path, ignore) {
        return;
    }
    let join = |k: &str| if path.is_empty() { k.to_owned() } else { format!("{path}.{k}") };
    match (expected, actual) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().for_each(|k| walk(&join(k), a.get(k).unwrap_or(&Value::Null), b.get(k).unwrap_or(&Value::Null), ignore, out));
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            a.iter().zip(b).enumerate().for_each(|(i, (x, y))| walk(&join(&i.to_string()), x, y, ignore, out));
        }
        (a, b) if a != b => out.push(format!("{path}: GnuCash {a} != red-ale {b}")),
        _ => {}
    }
}


/// The instant [`dispatcher`]'s clock reports: 2024-07-01 12:00:00 UTC.
pub const TEST_NOW: red_ale_core::Time64 = red_ale_core::Time64(1_719_835_200);

/// A dispatcher whose sessions use a fixed clock ([`TEST_NOW`]) and sequential GUIDs, so results are reproducible.
/// Each session counts from its own seed, so a book reopened in the same test never gets a GUID it already holds.
pub fn dispatcher() -> red_ale_rpc::Dispatcher {
    dispatcher_in(red_ale_core::TimeZone::utc())
}

/// [`dispatcher`] in another local time zone.
pub fn dispatcher_in(zone: red_ale_core::TimeZone) -> red_ale_rpc::Dispatcher {
    let mut d = red_ale_rpc::Dispatcher::new(red_ale_service::Settings::default());
    let sessions = std::sync::atomic::AtomicU32::new(0);
    d.set_env_factory(Box::new(move || {
        let session = sessions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let seed = (0x00ed_u128 << 112) | (u128::from(session) << 80);
        Box::new(red_ale_core::FixedEnv::new(TEST_NOW, seed).with_zone(zone.clone()))
    }));
    d
}

/// A copy of a fixture opened for writing in a deterministic [`dispatcher`].
pub fn open(name: &str) -> (TempBook, red_ale_rpc::Dispatcher) {
    let book = TempBook::copy(name);
    let mut d = dispatcher();
    call(&mut d, "book.open", &serde_json::json!({ "path": book.path_str() }));
    (book, d)
}

/// A new, empty book created in a deterministic [`dispatcher`].
pub fn create() -> (TempBook, red_ale_rpc::Dispatcher) {
    let book = TempBook::new_path();
    let mut d = dispatcher();
    call(&mut d, "book.create", &serde_json::json!({ "path": book.path_str() }));
    (book, d)
}

/// Call a method that must succeed.
pub fn call(d: &mut red_ale_rpc::Dispatcher, method: &str, params: &Value) -> Value {
    d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {}", e.data.clone().unwrap_or_default()))
}

/// Call a method that must fail; returns the error.
pub fn call_err(d: &mut red_ale_rpc::Dispatcher, method: &str, params: &Value) -> red_ale_rpc::RpcError {
    match d.call(method, params.clone()) {
        Ok(v) => panic!("{method} {params} should fail but returned {v}"),
        Err(e) => e,
    }
}

/// Save and close the book in `d`, then compare GnuCash's reading of the file with red-ale's. Returns the
/// differences, ignoring [`model::NOT_YET_COMPUTED`] and `ignore`.
pub fn oracle_check(d: &mut red_ale_rpc::Dispatcher, book: &TempBook, ignore: &[&str]) -> Vec<String> {
    call(d, "book.close", &serde_json::json!({ "save": true }));
    let gnucash = oracle::dump(&book.path).unwrap_or_else(|e| panic!("oracle: {e}"));
    let bytes = std::fs::read(&book.path).expect("read book");
    let (ours, _) = red_ale_xml::read_book(&bytes).expect("red-ale reads its own book");
    let skip: Vec<&str> = model::NOT_YET_COMPUTED.iter().copied().chain(ignore.iter().copied()).collect();
    diff(&gnucash, &model::dump(&ours), &skip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn diff_reports_paths_and_honours_ignores() {
        let a = json!({ "x": [1, { "t": 2, "u": 3 }], "y": "a" });
        let b = json!({ "x": [1, { "t": 5, "u": 4 }], "y": "a" });
        assert_eq!(diff(&a, &b, &["x.*.u"]), vec!["x.1.t: GnuCash 2 != red-ale 5".to_owned()]);
        assert!(diff(&a, &a, &[]).is_empty());
    }

    #[test]
    fn finds_xml_fixtures_only() {
        let all = xml_fixtures();
        assert!(all.contains(&"synthetic/rich.gnucash".to_owned()));
        assert!(!all.iter().any(|f| f.contains("sqlite")));
    }
}
