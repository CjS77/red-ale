//! Business import red-ale applied, checked against GnuCash 5.17 (`design/api/import.md`, "Tests"): GnuCash opens
//! the book with no warnings and reads what red-ale reads (`oracle_check`, posted documents' lots and transactions
//! included); GnuCash's business scrub finds nothing to fix; and a document the import posts is the document
//! GnuCash's own `gncInvoicePostToAccount` posts with the same date, due date, memo and accumulate setting.
//!
//! GnuCash's importers can't run here (they build only with the GUI), so the comparison posts with the engine
//! function they call (`5.17:gnucash/import-export/bi-import/dialog-bi-import.c:879-886`).

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use red_ale_rpc::Dispatcher;
use red_ale_testkit::{TempBook, call, create, diff, oracle, oracle_check};
use serde_json::{Value, json};

fn script(name: &str) -> PathBuf {
    oracle::scripts_dir().join(name)
}

fn run(name: &str, args: &[&str]) -> String {
    oracle::run_script(&script(name), args).unwrap_or_else(|e| panic!("oracle: {e}"))
}

fn view(path: &Path) -> Value {
    serde_json::from_str(&run("business_posting.py", &["view", &path.display().to_string()])).unwrap()
}

/// Accounts, a tax table and the Owners the documents name, the Owners imported from CSV.
fn setup(d: &mut Dispatcher) {
    [
        ("Assets", "ASSET", None),
        ("Receivable", "RECEIVABLE", Some("Assets")),
        ("Liabilities", "LIABILITY", None),
        ("Payable", "PAYABLE", Some("Liabilities")),
        ("Tax", "LIABILITY", Some("Liabilities")),
        ("Expenses", "EXPENSE", None),
        ("Office", "EXPENSE", Some("Expenses")),
        ("Travel", "EXPENSE", Some("Expenses")),
        ("Income", "INCOME", None),
        ("Sales", "INCOME", Some("Income")),
        ("Services", "INCOME", Some("Income")),
    ]
    .into_iter()
    .for_each(|(name, ty, parent)| {
        let mut p = json!({ "name": name, "type": ty, "commodity": "USD" });
        if let Some(parent) = parent {
            p["parent"] = json!(parent);
        }
        call(d, "account.create", &p);
    });
    call(d, "taxTable.create", &json!({ "name": "VAT", "entries": [{ "account": "Liabilities:Tax", "amount": "20", "type": "percent" }] }));
    call(d, "config.set", &json!({ "changes": { "defaultCurrency": "USD" } }));
    let customers = "C-ACME;Acme Ltd;Wile E;1 Road;Town;;;555;;wile@acme;Good payer;Dock;2 Pier;;;;556;;dock@acme\n\
                     C-GLOBEX;Globex;Hank;;;;;;;;;;;;;;;;\n";
    let p = call(d, "import.preview", &json!({ "format": "csvOwners", "files": [{ "text": customers }] }));
    call(d, "import.apply", &json!({ "proposal": p["id"] }));
    let vendors = "\"V-INK\",\"Ink, Ltd\",\"Ann\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\"\n\
                   V-PAPER,Paper Co,Bob,9 Mill Lane,,,,,,,,,,,,,,,\n";
    let p = call(d, "import.preview", &json!({
        "format": "csvOwners", "ownerKind": "vendor", "separator": "commaQuoted", "files": [{ "text": vendors }]
    }));
    call(d, "import.apply", &json!({ "proposal": p["id"] }));
}

/// A document row: its fields by name, the rest empty.
fn row(fields: &[(&str, &str)]) -> String {
    const NAMES: [&str; 22] = [
        "id", "date_opened", "owner_id", "billing_id", "notes", "date", "desc", "action", "account", "quantity", "price", "disc_type",
        "disc_how", "discount", "taxable", "taxincluded", "tax_table", "date_posted", "due_date", "account_posted", "memo_posted",
        "accu_splits",
    ];
    let cell = |n: &str| fields.iter().find(|(k, _)| *k == n).map_or("", |(_, v)| v);
    NAMES.iter().map(|n| cell(n)).collect::<Vec<_>>().join(";") + "\n"
}

const POSTING: [&str; 5] = ["date_posted", "due_date", "account_posted", "memo_posted", "accu_splits"];

/// The bills: two posted (accumulated and not), one left open.
fn bills() -> Vec<Vec<(&'static str, &'static str)>> {
    vec![
        vec![
            ("id", "B-100"),
            ("date_opened", "2024-03-01"),
            ("owner_id", "V-INK"),
            ("billing_id", "INK-1"),
            ("notes", "March"),
            ("date", "2024-03-02"),
            ("desc", "Toner"),
            ("action", "Material"),
            ("account", "Expenses:Office"),
            ("quantity", "2"),
            ("price", "45.50"),
            ("taxable", "yes"),
            ("tax_table", "VAT"),
            ("date_posted", "2024-03-05"),
            ("due_date", "2024-04-04"),
            ("account_posted", "Liabilities:Payable"),
            ("memo_posted", "imported"),
            ("accu_splits", "yes"),
        ],
        vec![("date", "2024-03-03"), ("desc", "Paper"), ("account", "Expenses:Office"), ("quantity", "10"), ("price", "1.25")],
        vec![("desc", "Ink"), ("account", "Expenses:Office"), ("quantity", "1"), ("price", "3.10")],
        vec![
            ("id", "B-101"),
            ("date_opened", "2024-03-06"),
            ("owner_id", "V-PAPER"),
            ("date", "2024-03-06"),
            ("desc", "Delivery"),
            ("account", "Expenses:Travel"),
            ("quantity", "1"),
            ("price", "12"),
            ("taxable", "y"),
            ("taxincluded", "y"),
            ("tax_table", "VAT"),
            ("date_posted", "2024-03-07"),
            ("due_date", "2024-03-21"),
            ("account_posted", "Liabilities:Payable"),
            ("memo_posted", "per entry"),
            ("accu_splits", "no"),
        ],
        vec![
            ("id", "B-101"),
            ("date", "2024-03-06"),
            ("desc", "Paper"),
            ("account", "Expenses:Office"),
            ("quantity", "3"),
            ("price", "2.5"),
        ],
        vec![
            ("id", "B-102"),
            ("date_opened", "2024-03-08"),
            ("owner_id", "V-INK"),
            ("date", "2024-03-08"),
            ("desc", "Open"),
            ("account", "Expenses:Office"),
            ("price", "7"),
        ],
    ]
}

/// The invoices: discounts of each kind and timing, tax included, a negative one, one left open.
fn invoices() -> Vec<Vec<(&'static str, &'static str)>> {
    vec![
        vec![
            ("id", "I-200"),
            ("date_opened", "2024-04-01"),
            ("owner_id", "C-ACME"),
            ("notes", "April"),
            ("date", "2024-04-01"),
            ("desc", "Widgets"),
            ("account", "Income:Sales"),
            ("quantity", "3"),
            ("price", "19.99"),
            ("disc_type", "%"),
            ("disc_how", "<"),
            ("discount", "10"),
            ("taxable", "t"),
            ("tax_table", "VAT"),
            ("date_posted", "2024-04-02"),
            ("due_date", "2024-05-02"),
            ("account_posted", "Assets:Receivable"),
            ("memo_posted", "april sales"),
            ("accu_splits", "1"),
        ],
        vec![
            ("date", "2024-04-01"),
            ("desc", "Setup"),
            ("account", "Income:Services"),
            ("quantity", "1"),
            ("price", "100"),
            ("disc_type", "$"),
            ("disc_how", ">"),
            ("discount", "15"),
            ("taxable", "t"),
            ("tax_table", "VAT"),
        ],
        vec![
            ("date", "2024-04-01"),
            ("desc", "Hours"),
            ("account", "Income:Services"),
            ("quantity", "2.5"),
            ("price", "60"),
            ("disc_type", "%"),
            ("disc_how", "="),
            ("discount", "5"),
            ("taxable", "t"),
            ("taxincluded", "x"),
            ("tax_table", "VAT"),
        ],
        vec![
            ("id", "I-201"),
            ("date_opened", "2024-04-05"),
            ("owner_id", "C-GLOBEX"),
            ("date", "2024-04-05"),
            ("desc", "Draft"),
            ("account", "Income:Sales"),
            ("price", "40"),
        ],
        vec![
            ("id", "I-202"),
            ("date_opened", "2024-04-06"),
            ("owner_id", "C-GLOBEX"),
            ("date", "2024-04-06"),
            ("desc", "Refund"),
            ("account", "Income:Sales"),
            ("quantity", "-1"),
            ("price", "25"),
            ("date_posted", "2024-04-06"),
            ("account_posted", "Assets:Receivable"),
        ],
    ]
}

fn csv(rows: &[Vec<(&str, &str)>], posting: bool) -> String {
    rows.iter().map(|r| row(&r.iter().copied().filter(|(k, _)| posting || !POSTING.contains(k)).collect::<Vec<_>>())).collect()
}

fn import(d: &mut Dispatcher, text: &str, kind: &str, update: bool) -> Value {
    let p = call(d, "import.preview", &json!({
        "format": "csvDocuments", "documentKind": kind, "dateFormat": "iso", "updateExisting": update, "files": [{ "text": text }]
    }));
    assert_eq!(p["errors"], json!([]), "{}", p["errors"]);
    call(d, "import.apply", &json!({ "proposal": p["id"] }));
    p
}

/// GnuCash's business scrub on a copy of `book` changes nothing.
fn assert_scrub_clean(book: &Path) {
    let copy = TempBook::new_path();
    std::fs::copy(book, &copy.path).unwrap();
    run("business_payments.py", &["scrub", &copy.path_str()]);
    let differences = diff(&view(book), &view(&copy.path), &[]);
    assert!(differences.is_empty(), "GnuCash's business scrub changed red-ale's book:\n{}", differences.join("\n"));
}

#[test]
fn gnucash_reads_imported_documents_and_owners_and_its_scrub_finds_nothing() {
    oracle::check_version().unwrap();
    let (book, mut d) = create();
    setup(&mut d);
    let first = import(&mut d, &csv(&bills(), true), "bill", false);
    assert_eq!(first["summary"]["add"], 3);
    import(&mut d, &csv(&invoices(), true), "invoice", false);
    let again = import(&mut d, &csv(&invoices(), true), "invoice", false);
    let changed = (again["summary"]["add"].as_u64(), again["summary"]["update"].as_u64());
    assert_eq!(changed, (Some(0), Some(0)), "the same file again changes nothing");
    let appended = import(&mut d, &csv(&invoices(), true), "invoice", true);
    assert_eq!(appended["summary"]["update"], 1, "only the unposted invoice takes rows again");
    let acme = "C-ACME;Acme Ltd;Wile E;1 Road;Town;;;555;;wile@acme;Good payer;Dock;2 Pier;;;;556;;dock@acme\n";
    let owners = call(&mut d, "import.preview", &json!({ "format": "csvOwners", "files": [{ "text": acme }] }));
    assert_eq!(owners["summary"]["skip"], 1, "an unchanged Owner");
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let opened: Value = serde_json::from_str(&run("opened.py", &[&book.path_str()])).unwrap();
    assert_eq!(opened, json!({ "unknownFeatures": null, "logged": [] }));
    assert_scrub_clean(&book.path);
}

/// The post ops GnuCash runs on the documents imported unposted.
fn post_ops(rows: &[Vec<(&str, &str)>]) -> Vec<Value> {
    rows.iter()
        .filter(|r| r.iter().any(|(k, v)| *k == "date_posted" && !v.is_empty()))
        .map(|r| {
            let get = |k: &str| r.iter().find(|(f, _)| *f == k).map_or("", |(_, v)| *v);
            let due = if get("due_date").is_empty() { get("date_posted") } else { get("due_date") };
            let accumulate = matches!(get("accu_splits").chars().next(), Some('y' | 't' | '1' | 'x'));
            json!({
                "op": "post", "invoice": get("id"), "account": get("account_posted"), "postDate": get("date_posted"), "dueStart": due,
                "memo": get("memo_posted"), "accumulate": accumulate, "autopay": false
            })
        })
        .collect()
}

#[test]
fn documents_the_import_posts_are_what_gnucash_posts() {
    oracle::check_version().unwrap();
    let (base, mut d) = create();
    setup(&mut d);
    call(&mut d, "book.close", &json!({ "save": true }));
    let copy = || {
        let t = TempBook::new_path();
        std::fs::copy(&base.path, &t.path).unwrap();
        t
    };
    let ours = copy();
    // One dispatcher throughout: its sessions seed GUIDs apart, where a new one would reuse the GUIDs setup made.
    call(&mut d, "book.open", &json!({ "path": ours.path_str() }));
    import(&mut d, &csv(&bills(), true), "bill", false);
    import(&mut d, &csv(&invoices(), true), "invoice", false);
    let fidelity = oracle_check(&mut d, &ours, &[]);
    assert!(fidelity.is_empty(), "{}", fidelity.join("\n"));

    let unposted = copy();
    call(&mut d, "book.open", &json!({ "path": unposted.path_str() }));
    import(&mut d, &csv(&bills(), false), "bill", false);
    import(&mut d, &csv(&invoices(), false), "invoice", false);
    call(&mut d, "book.close", &json!({ "save": true }));
    // A directory of its own, so GnuCash's backup cannot clash with the one red-ale made this second.
    let theirs = TempBook::new_path();
    std::fs::copy(&unposted.path, &theirs.path).unwrap();
    let ops: Vec<Value> = post_ops(&bills()).into_iter().chain(post_ops(&invoices())).collect();
    let ops_file = theirs.dir.path().join("ops.json");
    std::fs::write(&ops_file, Value::Array(ops).to_string()).unwrap();
    run("business_posting.py", &["do", &theirs.path_str(), &ops_file.display().to_string()]);
    let differences = diff(&view(&theirs.path), &view(&ours.path), &[]);
    assert!(differences.is_empty(), "red-ale's import posted differently from GnuCash:\n{}", differences.join("\n"));
}
