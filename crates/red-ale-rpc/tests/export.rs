//! The export methods (`design/api/export.md`) through the dispatcher, on copies of `synthetic/rich.gnucash`. The
//! expected files in `export/` were written by red-ale and checked by hand against
//! `5.17:gnucash/import-export/csv-exp/`; the byte-level rules are pinned line by line in `red-ale-export`'s tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

const NOW: Time64 = Time64(1_719_835_200);
const ACCOUNTS: [&str; 8] = [
    "Assets:Cash",
    "Assets:Brokerage:ACME",
    "Assets:Brokerage:Zeta",
    "Assets:Euro Bank",
    "Assets:NonStd",
    "Liabilities:Visa",
    "Expenses:Food",
    "Income:Salary",
];

struct Book {
    dir: tempfile::TempDir,
    path: PathBuf,
    d: Dispatcher,
}

fn open(read_only: bool) -> Book {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(
        format!("{}/../../fixtures/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR")),
        &path,
    )
    .unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(NOW, 0x00ed_0000_0000_0000_0000_0000_0000_0000))));
    d.call("book.open", json!({ "path": path.display().to_string(), "readOnly": read_only }))
        .unwrap();
    Book { dir, path, d }
}

impl Book {
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.d
            .call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        self.d
            .call(method, params.clone())
            .err()
            .unwrap_or_else(|| panic!("{method} {params} should fail"))
    }

    /// Every file beside the book, with its contents.
    fn files(&self) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(self.dir.path())
            .unwrap()
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap()))
            .collect()
    }
}

fn expected(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/export").join(name)).unwrap()
}

#[test]
fn the_account_tree() {
    let mut b = open(false);
    let out = b.call("export.csvTree", json!({}));
    assert_eq!(out["csv"].as_str().unwrap(), expected("rich-tree.csv"));
    assert_eq!(out["rows"], 20);
    let quoted = b.call("export.csvTree", json!({ "separator": ";", "quotes": true }));
    assert!(quoted["csv"].as_str().unwrap().starts_with("\"Type\";\"Full Account Name\";"));
}

#[test]
fn transactions_in_both_layouts() {
    let mut b = open(false);
    let complex = b.call("export.csvTransactions", json!({ "accounts": ACCOUNTS }));
    assert_eq!(complex["csv"].as_str().unwrap(), expected("rich-complex.csv"));
    assert_eq!((complex["transactions"].clone(), complex["lines"].clone()), (json!(6), json!(13)));
    let simple = b.call("export.csvTransactions", json!({ "accounts": ACCOUNTS, "layout": "simple" }));
    assert_eq!(simple["csv"].as_str().unwrap(), expected("rich-simple.csv"));
    assert_eq!((simple["transactions"].clone(), simple["lines"].clone()), (json!(6), json!(6)));
}

/// Accounts by GUID or full name, a date range, the date format setting and a one-call Locale.
#[test]
fn filters_and_formats() {
    let mut b = open(false);
    let food = b.call("account.get", json!({ "account": "Expenses:Food" }))["guid"].clone();
    let params =
        json!({ "accounts": [food], "from": "2024-06-02", "to": "2024-06-05", "layout": "simple", "locale": "de_DE", "separator": ";" });
    let out = b.call("export.csvTransactions", params);
    let lines: Vec<&str> = out["csv"].as_str().unwrap().split("\r\n").collect();
    assert_eq!(out["transactions"], 1, "{out}");
    assert!(lines[1].starts_with("02.06.2024;Expenses:Food;101;"), "{}", lines[1]);
    assert!(
        lines[1].ends_with(";n;12,34 $;12,34;12,34 $;12,34;1,00"),
        "numbers in de_DE: {}",
        lines[1]
    );
}

#[test]
fn requests_are_checked() {
    let mut b = open(false);
    assert_eq!(b.err("export.csvTransactions", json!({ "accounts": [] })).code, codes::INVALID);
    assert_eq!(
        b.err("export.csvTransactions", json!({ "accounts": ["Assets:Nowhere"] })).code,
        codes::NOT_FOUND
    );
    let backwards = json!({ "accounts": ["Assets:Cash"], "from": "2024-06-05", "to": "2024-06-01" });
    assert_eq!(b.err("export.csvTransactions", backwards).code, codes::INVALID);
    assert_eq!(
        b.err("export.csvTransactions", json!({ "accounts": ["Assets:Cash"], "layout": "wide" }))
            .code,
        codes::INVALID_PARAMS
    );
    assert_eq!(
        b.err("export.csvTransactions", json!({ "accounts": ["Assets:Cash"], "locale": "xx_YY" }))
            .code,
        codes::INVALID_PARAMS
    );
    assert_eq!(b.err("export.csvTree", json!({ "separator": "" })).code, codes::INVALID);
}

/// Exports only read: on a book open for writing, nothing beside the book changes but the exported file, the book is
/// not dirty and its journal is not written; on a book open read-only, every export works.
#[test]
fn exports_only_read() {
    let mut b = open(false);
    let before = b.files();
    let journal = b.call("book.info", json!({}))["journal"].clone();
    b.call("export.csvTree", json!({}));
    b.call("export.csvTransactions", json!({ "accounts": ACCOUNTS }));
    let target = b.dir.path().join("chart.gnucash");
    b.call("export.accounts", json!({ "path": target.display().to_string() }));
    let info = b.call("book.info", json!({}));
    assert_eq!((info["dirty"].clone(), info["journal"].clone()), (json!(false), journal));
    let mut after = b.files();
    assert!(after.remove("chart.gnucash").is_some());
    assert_eq!(before, after, "nothing but the exported file appeared or changed");

    let mut r = open(true);
    r.call("export.csvTree", json!({}));
    r.call("export.csvTransactions", json!({ "accounts": ACCOUNTS }));
    let target = r.dir.path().join("chart");
    let written = r.call("export.accounts", json!({ "path": target.display().to_string() }));
    assert_eq!(written["path"], json!(format!("{}.gnucash", target.display())));
    assert_eq!(r.call("book.info", json!({}))["dirty"], false);
}

/// Export accounts writes an uncompressed GnuCash file with no book element, and leaves no lock or temporary file.
#[test]
fn export_accounts_writes_a_new_file() {
    let mut b = open(false);
    let target = b.dir.path().join("chart.gnucash");
    let written = b.call("export.accounts", json!({ "path": format!("xml://{}", target.display()) }));
    assert_eq!(
        written,
        json!({ "path": target.display().to_string(), "accounts": 21, "commodities": 5 })
    );
    let text = std::fs::read_to_string(&target).unwrap();
    assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\" ?>\n<gnc-v2"));
    assert!(text.contains("<gnc:count-data cd:type=\"account\">21</gnc:count-data>\n"));
    assert!(!text.contains("<gnc:book") && !text.contains("<gnc:transaction") && !text.contains("<act:lots>"));
    let names: Vec<String> = b.files().into_keys().collect();
    assert_eq!(
        names,
        ["book.gnucash", "book.gnucash.LCK", "chart.gnucash"],
        "the open book's lock, and no other"
    );
}

#[test]
fn export_accounts_never_overwrites() {
    let mut b = open(false);
    let own = b.path.display().to_string();
    assert_eq!(b.err("export.accounts", json!({ "path": own })).code, codes::INVALID);
    let existing = b.dir.path().join("taken.gnucash");
    std::fs::write(&existing, b"keep me").unwrap();
    let e = b.err("export.accounts", json!({ "path": existing.display().to_string() }));
    assert_eq!(e.code, codes::FILE_EXISTS);
    assert_eq!(std::fs::read(&existing).unwrap(), b"keep me");
    let without_extension = b.dir.path().join("taken");
    assert_eq!(
        b.err("export.accounts", json!({ "path": without_extension.display().to_string() }))
            .code,
        codes::FILE_EXISTS
    );
    let locked = b.dir.path().join("locked.gnucash");
    std::fs::write(b.dir.path().join("locked.gnucash.LCK"), b"").unwrap();
    assert_eq!(
        b.err("export.accounts", json!({ "path": locked.display().to_string() })).code,
        codes::BOOK_LOCKED
    );
    assert!(!locked.exists());
}

#[test]
fn exports_need_a_book() {
    let mut d = Dispatcher::new(Settings::default());
    ["export.csvTree", "export.accounts"].iter().for_each(|m| {
        let params = if *m == "export.accounts" {
            json!({ "path": "/nowhere/x.gnucash" })
        } else {
            json!({})
        };
        assert_eq!(d.call(m, params).unwrap_err().code, codes::NO_BOOK);
    });
}
