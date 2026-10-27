//! Business import through `import.preview` and `import.apply`: invoices and bills (`csvDocuments`) and customers and
//! vendors (`csvOwners`), on hand-built files in GnuCash's layouts (`design/api/import.md`, "Business documents",
//! "Customers and vendors", "Tests"). GnuCash has no tests for these importers; each case cites the lines of
//! `5.17:gnucash/import-export/bi-import/dialog-bi-import.c` or `customer-import/dialog-customer-import.c` it follows.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

/// 2024-07-01 12:00 UTC.
const NOW: i64 = 1_719_835_200;

struct Book {
    _dir: tempfile::TempDir,
    d: Dispatcher,
}

impl Book {
    /// A USD book with A/R, A/P, income, expense and tax accounts, a customer `C1`, a vendor `V1` and a 20 % tax table.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut d = Dispatcher::new(Settings::default());
        d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(NOW), 1 << 100))));
        d.call(
            "book.create",
            json!({ "path": dir.path().join("book.gnucash").display().to_string() }),
        )
        .unwrap();
        let mut b = Self { _dir: dir, d };
        [
            ("Assets", "ASSET", None, "USD"),
            ("Receivable", "RECEIVABLE", Some("Assets"), "USD"),
            ("Bank", "BANK", Some("Assets"), "USD"),
            ("Liabilities", "LIABILITY", None, "USD"),
            ("Payable", "PAYABLE", Some("Liabilities"), "USD"),
            ("Payable EUR", "PAYABLE", Some("Liabilities"), "EUR"),
            ("Tax", "LIABILITY", Some("Liabilities"), "USD"),
            ("Expenses", "EXPENSE", None, "USD"),
            ("Office", "EXPENSE", Some("Expenses"), "USD"),
            ("Travel", "EXPENSE", Some("Expenses"), "USD"),
            ("Abroad", "EXPENSE", Some("Expenses"), "EUR"),
            ("Income", "INCOME", None, "USD"),
            ("Sales", "INCOME", Some("Income"), "USD"),
            ("Consulting", "INCOME", Some("Income"), "USD"),
        ]
        .into_iter()
        .for_each(|(name, ty, parent, commodity)| {
            let mut p = json!({ "name": name, "type": ty, "commodity": commodity });
            if let Some(parent) = parent {
                p["parent"] = json!(parent);
            }
            b.ok("account.create", p);
        });
        b.ok("customer.create", json!({ "name": "Acme Ltd", "currency": "USD", "id": "C1" }));
        b.ok("vendor.create", json!({ "name": "Ink Ltd", "currency": "USD", "id": "V1" }));
        b.ok(
            "taxTable.create",
            json!({ "name": "VAT", "entries": [{ "account": "Liabilities:Tax", "amount": "20", "type": "percent" }] }),
        );
        b
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
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

    fn documents(&mut self, text: &str, extra: Value) -> Value {
        let mut p = json!({ "format": "csvDocuments", "files": [{ "name": "docs.csv", "text": text }], "dateFormat": "iso" });
        extra.as_object().unwrap().iter().for_each(|(k, v)| p[k] = v.clone());
        self.ok("import.preview", p)
    }

    fn owners(&mut self, text: &str, extra: Value) -> Value {
        let mut p = json!({ "format": "csvOwners", "files": [{ "name": "owners.csv", "text": text }] });
        extra.as_object().unwrap().iter().for_each(|(k, v)| p[k] = v.clone());
        self.ok("import.preview", p)
    }

    fn apply(&mut self, p: &Value, extra: Value) -> Value {
        let mut params = json!({ "proposal": p["id"] });
        extra.as_object().unwrap().iter().for_each(|(k, v)| params[k] = v.clone());
        self.ok("import.apply", params)
    }

    fn invoice(&mut self, guid: &Value) -> Value {
        self.ok("invoice.get", json!({ "invoice": guid }))
    }

    fn invoices(&mut self) -> Vec<Value> {
        self.ok("invoice.list", json!({}))["invoices"].as_array().unwrap().clone()
    }
}

/// A line of 22 fields from (field, value) pairs; the rest empty.
fn row(fields: &[(&str, &str)], sep: char) -> String {
    const NAMES: [&str; 22] = [
        "id",
        "date_opened",
        "owner_id",
        "billing_id",
        "notes",
        "date",
        "desc",
        "action",
        "account",
        "quantity",
        "price",
        "disc_type",
        "disc_how",
        "discount",
        "taxable",
        "taxincluded",
        "tax_table",
        "date_posted",
        "due_date",
        "account_posted",
        "memo_posted",
        "accu_splits",
    ];
    let cell = |n: &str| fields.iter().find(|(k, _)| *k == n).map_or("", |(_, v)| v);
    NAMES.iter().map(|n| cell(n)).collect::<Vec<_>>().join(&sep.to_string()) + "\n"
}

fn codes_of(warnings: &Value) -> Vec<String> {
    warnings
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap().to_owned())
        .collect()
}

fn amount(v: &Value) -> (i64, i64) {
    let n: i64 = v["num"].as_str().unwrap().parse().unwrap();
    let d: i64 = v["denom"].as_str().unwrap().parse().unwrap();
    let g = gcd(n.abs(), d);
    (n / g, d / g)
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

// ---------------------------------------------------------------------------------------------------------------
// Bills and invoices.

/// Two bills, the second continued by a row with an empty ID (`dialog-bi-import.c:510-524`); the first posted with
/// every posting column (`:843-886`).
fn two_bills() -> String {
    let first = [
        ("id", "B-1"),
        ("date_opened", "2024-03-01"),
        ("owner_id", "V1"),
        ("billing_id", "INK-77"),
        ("notes", "March ink"),
        ("date", "2024-03-02"),
        ("desc", "Toner"),
        ("action", "Material"),
        ("account", "Expenses:Office"),
        ("quantity", "2"),
        ("price", "45.50"),
        ("taxable", "yes"),
        ("taxincluded", "no"),
        ("tax_table", "VAT"),
        ("date_posted", "2024-03-05"),
        ("due_date", "2024-04-04"),
        ("account_posted", "Liabilities:Payable"),
        ("memo_posted", "posted by import"),
        ("accu_splits", "x"),
    ];
    let second = [
        ("id", "B-1"),
        ("date", "2024-03-03"),
        ("desc", "Paper"),
        ("account", "Expenses:Office"),
        ("quantity", "10"),
        ("price", "1.25"),
    ];
    let third = [
        ("id", "B-2"),
        ("date_opened", "2024-03-10"),
        ("owner_id", "V1"),
        ("desc", "Taxi"),
        ("account", "Expenses:Travel"),
        ("price", "30"),
    ];
    let fourth = [
        ("desc", "Train"),
        ("account", "Expenses:Travel"),
        ("quantity", "1"),
        ("price", "2*10.5"),
    ];
    [row(&first, ';'), row(&second, ';'), row(&third, ';'), row(&fourth, ';')].concat()
}

#[test]
fn bills_are_grouped_by_id_created_and_posted() {
    let mut b = Book::new();
    let p = b.documents(&two_bills(), json!({}));
    assert_eq!(p["format"], "csvDocuments");
    assert_eq!(p["errors"], json!([]));
    let docs = p["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0]["id"], "B-1");
    assert_eq!(docs[0]["type"], "bill");
    assert_eq!(docs[0]["action"], "add");
    assert_eq!(docs[0]["lines"], json!([1, 2]));
    assert_eq!(docs[0]["billingId"], "INK-77");
    assert_eq!(docs[0]["entries"][0]["taxable"], true);
    assert_eq!(docs[0]["post"]["willPost"], true);
    assert_eq!(
        docs[0]["post"]["accumulateSplits"], true,
        "`x` is true (dialog-bi-import-helper.c:44-45)"
    );
    assert_eq!(docs[0]["post"]["dueDate"], "2024-04-04");
    // 2 × 45.50 + 20 % tax, and 10 × 1.25 untaxed.
    assert_eq!(amount(&docs[0]["totals"]["total"]), (1217, 10));
    assert_eq!(amount(&docs[0]["totals"]["tax"]), (91, 5));
    assert_eq!(docs[1]["lines"], json!([3, 4]));
    assert_eq!(docs[1]["post"], Value::Null);
    assert_eq!(amount(&docs[1]["entries"][1]["price"]), (21, 1), "expressions are evaluated");
    assert_eq!(
        codes_of(&docs[1]["warnings"]),
        ["fixed", "fixed", "fixed"],
        "an empty quantity is 1, empty dates the opening date"
    );
    assert_eq!(docs[1]["entries"][0]["date"], "2024-03-10");
    assert_eq!(amount(&docs[1]["entries"][0]["quantity"]), (1, 1));
    assert_eq!(p["summary"]["add"], 2);

    let applied = b.apply(&p, json!({}));
    assert_eq!(applied["documents"]["created"].as_array().unwrap().len(), 2);
    assert_eq!(applied["documents"]["posted"].as_array().unwrap().len(), 1);
    let bill = b.invoice(&applied["documents"]["created"][0]["guid"]);
    assert_eq!(bill["posted"], true);
    assert_eq!(bill["entries"].as_array().unwrap().len(), 2);
    assert_eq!(bill["currency"], "CURRENCY::USD");
    let txn = b.ok("transaction.get", json!({ "guid": bill["postedTransaction"] }));
    assert_eq!(txn["num"], "B-1");
    let memos: Vec<&str> = txn["splits"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["memo"].as_str())
        .collect();
    assert!(memos.contains(&"posted by import"), "{memos:?}");
    assert_eq!(
        txn["splits"].as_array().unwrap().len(),
        3,
        "accumulated: one expense split, one tax split, one A/P split"
    );
    let open = b.invoice(&applied["documents"]["created"][1]["guid"]);
    assert_eq!(open["posted"], false);
}

/// Invoices take their notes, discounts and tax flags from the row (`dialog-bi-import.c:810-826`); quoted fields may
/// hold the separator, and `""` in notes and descriptions is a quote (`dialog-bi-import-gui.c:342`, `:957-974`).
#[test]
fn invoices_with_discounts_quotes_and_flags() {
    let mut b = Book::new();
    let base = [
        ("id", "I-1"),
        ("date_opened", "2024-05-01"),
        ("owner_id", "C1"),
        ("account", "Income:Sales"),
        ("date", "2024-05-02"),
    ];
    let line = |extra: &[(&str, &str)]| {
        let fields: Vec<(&str, &str)> = base.iter().copied().chain(extra.iter().copied()).collect();
        row(&fields, ',')
    };
    let text = [
        line(&[
            ("notes", "\"Order, \"\"rush\"\"\""),
            ("desc", "\"Widget, large\""),
            ("quantity", "3"),
            ("price", "10"),
            ("disc_type", "%"),
            ("discount", "10"),
            ("disc_how", "="),
        ]),
        line(&[
            ("desc", "Setup"),
            ("quantity", "1"),
            ("price", "50"),
            ("disc_type", "$"),
            ("discount", "5"),
            ("disc_how", ">"),
            ("taxable", "T"),
            ("tax_table", "VAT"),
        ]),
        line(&[
            ("desc", "Hours"),
            ("quantity", "\"1,000.5\""),
            ("price", "1"),
            ("taxable", "1"),
            ("taxincluded", "Yes"),
            ("tax_table", "VAT"),
        ]),
        line(&[
            ("desc", "Free"),
            ("quantity", "1"),
            ("price", "0"),
            ("taxable", "no"),
            ("taxincluded", "false"),
        ]),
    ]
    .concat();
    let p = b.documents(&text, json!({ "documentKind": "invoice", "separator": "commaQuoted" }));
    assert_eq!(p["errors"], json!([]), "{}", p["errors"]);
    let d = &p["documents"][0];
    assert_eq!(d["type"], "invoice");
    assert_eq!(d["notes"], "Order, \"rush\"");
    let e = d["entries"].as_array().unwrap();
    assert_eq!(e[0]["description"], "Widget, large");
    assert_eq!(e[0]["notes"], "Order, \"rush\"", "an invoice entry keeps its row's notes");
    assert_eq!(
        (e[0]["discountType"].as_str(), e[0]["discountHow"].as_str()),
        (Some("percent"), Some("sameTime"))
    );
    assert_eq!(
        (e[1]["discountType"].as_str(), e[1]["discountHow"].as_str()),
        (Some("value"), Some("postTax"))
    );
    assert_eq!(e[2]["discountHow"], "preTax");
    assert_eq!(amount(&e[2]["quantity"]), (2001, 2), "group separators in the integer part");
    let flags: Vec<(bool, bool)> = e
        .iter()
        .map(|e| (e["taxable"].as_bool().unwrap(), e["taxIncluded"].as_bool().unwrap()))
        .collect();
    assert_eq!(
        flags,
        [(false, false), (true, false), (true, true), (false, false)],
        "text2bool (dialog-bi-import-helper.c:34-49)"
    );
    let applied = b.apply(&p, json!({}));
    let inv = b.invoice(&applied["documents"]["created"][0]["guid"]);
    assert_eq!(inv["type"], "invoice");
    assert_eq!(inv["entries"].as_array().unwrap().len(), 4);
    let totals = b.ok("invoice.totals", json!({ "invoice": inv["guid"] }));
    assert_eq!(
        amount(&totals["total"]),
        amount(&p["documents"][0]["totals"]["total"]),
        "the proposal's totals are the document's"
    );
}

/// An error in any row drops every row of its document (`dialog-bi-import.c:525-554`); red-ale's own errors (a
/// number that doesn't parse, an unknown tax table, an account the entry ledger refuses) do the same.
#[test]
fn errors_drop_the_whole_document_and_need_skip_errors() {
    let mut b = Book::new();
    let good = [
        ("id", "OK"),
        ("date_opened", "2024-03-01"),
        ("owner_id", "V1"),
        ("account", "Expenses:Office"),
        ("quantity", "1"),
        ("price", "5"),
    ];
    let second = [("desc", "second row of OK"), ("account", "Expenses:Nope"), ("price", "1")];
    let cases: Vec<(&str, Vec<(&str, &str)>)> = vec![
        (
            "vendor V9 does not exist",
            vec![("id", "E1"), ("owner_id", "V9"), ("account", "Expenses:Office"), ("price", "1")],
        ),
        ("owner not set", vec![("id", "E2"), ("account", "Expenses:Office"), ("price", "1")]),
        (
            "price not set",
            vec![("id", "E3"), ("owner_id", "V1"), ("account", "Expenses:Office")],
        ),
        (
            "account Expenses:Nope does not exist",
            vec![("id", "E4"), ("owner_id", "V1"), ("account", "Expenses:Nope"), ("price", "1")],
        ),
        (
            "is not a valid posting date",
            vec![
                ("id", "E5"),
                ("owner_id", "V1"),
                ("account", "Expenses:Office"),
                ("price", "1"),
                ("date_posted", "2024-13-01"),
            ],
        ),
        (
            "is not of type Accounts Payable",
            vec![
                ("id", "E6"),
                ("owner_id", "V1"),
                ("account", "Expenses:Office"),
                ("price", "1"),
                ("date_posted", "2024-03-01"),
                ("account_posted", "Assets:Receivable"),
            ],
        ),
        (
            "is not a number",
            vec![("id", "E7"), ("owner_id", "V1"), ("account", "Expenses:Office"), ("price", "12abc")],
        ),
        (
            "tax table GST does not exist",
            vec![
                ("id", "E8"),
                ("owner_id", "V1"),
                ("account", "Expenses:Office"),
                ("price", "1"),
                ("tax_table", "GST"),
            ],
        ),
        (
            "a line cannot use a INCOME account",
            vec![("id", "E9"), ("owner_id", "V1"), ("account", "Income:Sales"), ("price", "1")],
        ),
    ];
    let text: String = cases
        .iter()
        .map(|(_, f)| row(f, ';'))
        .chain([row(&good, ';'), row(&second, ';')])
        .collect();
    let p = b.documents(&text, json!({}));
    assert_eq!(
        p["documents"],
        json!([]),
        "OK's second row names an unknown account, so OK goes too"
    );
    let messages: Vec<&str> = p["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["message"].as_str().unwrap())
        .collect();
    cases
        .iter()
        .for_each(|(m, _)| assert!(messages.iter().any(|x| x.contains(m)), "{m} not in {messages:?}"));
    assert!(messages.contains(&"Error(s) in invoice OK, all rows of this invoice ignored."));
    assert_eq!(b.err("import.apply", json!({ "proposal": p["id"] })).code, codes::INVALID);
    let applied = b.apply(&p, json!({ "skipErrors": true }));
    assert_eq!(applied["documents"]["created"], json!([]));
    assert!(b.invoices().is_empty());
}

/// Fixes (`dialog-bi-import.c:393-400`, `:440-452`, `:490-505`), each a warning: a bad due date becomes the posting
/// date, a bad opening date today, an empty quantity 1, a bad entry date the opening date.
#[test]
fn fixes_are_warnings() {
    let mut b = Book::new();
    let fields = [
        ("id", "F1"),
        ("date_opened", "someday"),
        ("owner_id", "V1"),
        ("date", "nope"),
        ("account", "Expenses:Office"),
        ("price", "4"),
        ("date_posted", "2024-06-03"),
        ("due_date", "later"),
        ("account_posted", "Liabilities:Payable"),
    ];
    let p = b.documents(&row(&fields, ';'), json!({}));
    let d = &p["documents"][0];
    assert_eq!(codes_of(&d["warnings"]), ["fixed", "fixed", "fixed", "fixed"], "{}", d["warnings"]);
    assert_eq!(d["dateOpened"], "2024-07-01", "today");
    assert_eq!(d["entries"][0]["date"], "2024-07-01", "the fixed opening date");
    assert_eq!(d["post"]["dueDate"], "2024-06-03");
    let applied = b.apply(&p, json!({}));
    assert_eq!(applied["documents"]["posted"].as_array().unwrap().len(), 1);
}

/// Importing the same file twice finds every document by ID (`dialog-bi-import.c:688-775`): without
/// `updateExisting` all are skipped; with it, unposted ones get the rows again, as GnuCash does, with a warning, and
/// posted ones are never touched.
#[test]
fn the_same_file_twice() {
    let mut b = Book::new();
    let p = b.documents(&two_bills(), json!({}));
    b.apply(&p, json!({}));
    let again = b.documents(&two_bills(), json!({}));
    assert_eq!(
        (
            again["summary"]["add"].as_u64(),
            again["summary"]["update"].as_u64(),
            again["summary"]["skip"].as_u64()
        ),
        (Some(0), Some(0), Some(2))
    );
    assert_eq!(codes_of(&again["documents"][0]["warnings"]), ["exists"]);
    let applied = b.apply(&again, json!({}));
    assert_eq!(applied["documents"]["created"], json!([]));
    assert_eq!(applied["documents"]["updated"], json!([]));
    assert_eq!(b.invoices().len(), 2);

    let update = b.documents(&two_bills(), json!({ "updateExisting": true }));
    let docs = update["documents"].as_array().unwrap();
    assert_eq!(docs[0]["action"], "skip");
    assert_eq!(codes_of(&docs[0]["warnings"]), ["posted"]);
    assert_eq!(docs[1]["action"], "update");
    assert!(docs[1]["existing"].is_string());
    let duplicates = codes_of(&docs[1]["warnings"]).iter().filter(|c| *c == "duplicate-entry").count();
    assert_eq!(duplicates, 2, "{}", docs[1]["warnings"]);
    let applied = b.apply(&update, json!({}));
    let bill = b.invoice(&applied["documents"]["updated"][0]["guid"]);
    assert_eq!(bill["entries"].as_array().unwrap().len(), 4);
}

/// Rows with an ID used earlier in the file, not next to it, are a second document with that ID: found, as GnuCash
/// finds the one it just made, and updated (`dialog-bi-import.c:688-775`).
#[test]
fn an_id_again_later_in_the_file_updates_what_the_file_created() {
    let mut b = Book::new();
    let a = |desc: &str| {
        row(
            &[
                ("id", "R-1"),
                ("date_opened", "2024-03-01"),
                ("owner_id", "V1"),
                ("desc", desc),
                ("account", "Expenses:Office"),
                ("price", "1"),
            ],
            ';',
        )
    };
    let other = row(
        &[
            ("id", "R-2"),
            ("date_opened", "2024-03-01"),
            ("owner_id", "V1"),
            ("account", "Expenses:Office"),
            ("price", "1"),
        ],
        ';',
    );
    let text = [a("first"), other, a("again")].concat();
    let p = b.documents(&text, json!({}));
    assert_eq!(p["documents"][2]["action"], "skip", "not updated without updateExisting");
    let p = b.documents(&text, json!({ "updateExisting": true }));
    let docs = p["documents"].as_array().unwrap();
    assert_eq!(
        (
            docs[2]["action"].as_str(),
            docs[2]["sameAs"].as_u64(),
            docs[2]["existing"].is_null()
        ),
        (Some("update"), Some(0), true)
    );
    let e = b.err(
        "import.apply",
        json!({ "proposal": p["id"], "changes": [{ "index": 0, "action": "skip" }] }),
    );
    assert_eq!(e.code, codes::INVALID, "skipping the document a later record updates");
    let e = b.err(
        "import.apply",
        json!({ "proposal": p["id"], "changes": [{ "index": 1, "action": "add" }] }),
    );
    assert_eq!(e.code, codes::INVALID, "records can only be skipped");
    let applied = b.apply(&p, json!({ "changes": [{ "index": 1, "action": "skip" }] }));
    assert_eq!(applied["documents"]["skipped"], json!([1]));
    let made = b.invoice(&applied["documents"]["created"][0]["guid"]);
    assert_eq!(made["entries"].as_array().unwrap().len(), 2);
}

/// GnuCash posts only when every account is in the document's currency (`dialog-bi-import.c:867-901`); red-ale's
/// posting rules (no placeholder A/P) leave the document unposted with the reason.
#[test]
fn documents_that_cannot_be_posted_say_why() {
    let mut b = Book::new();
    b.ok(
        "account.create",
        json!({ "name": "Closed", "type": "PAYABLE", "commodity": "USD", "parent": "Liabilities", "placeholder": true }),
    );
    let doc = |id: &str, account: &str, posted: &str| {
        let fields = [
            ("id", id),
            ("date_opened", "2024-03-01"),
            ("owner_id", "V1"),
            ("account", account),
            ("price", "1"),
            ("date_posted", "2024-03-02"),
            ("account_posted", posted),
        ];
        row(&fields, ';')
    };
    let text = [
        doc("X1", "Expenses:Abroad", "Liabilities:Payable"),
        doc("X2", "Expenses:Office", "Liabilities:Payable EUR"),
        doc("X3", "Expenses:Office", "Liabilities:Closed"),
    ]
    .concat();
    let p = b.documents(&text, json!({}));
    let docs = p["documents"].as_array().unwrap();
    let reasons: Vec<&str> = docs.iter().map(|d| d["post"]["reason"].as_str().unwrap()).collect();
    assert_eq!(reasons[0], "Invoice X1 NOT posted because it requires currency conversion.");
    assert_eq!(reasons[1], "Invoice X2 NOT posted because currencies don't match.");
    assert!(reasons[2].contains("placeholder"), "{}", reasons[2]);
    assert!(
        docs.iter()
            .all(|d| d["post"]["willPost"] == false && codes_of(&d["warnings"]).contains(&"not-posted".to_owned()))
    );
    let applied = b.apply(&p, json!({}));
    assert_eq!(applied["documents"]["created"].as_array().unwrap().len(), 3);
    assert_eq!(applied["documents"]["posted"], json!([]));
}

/// bi-import makes no credit notes: negative amounts give a document with a negative total, posted the ordinary way.
/// A credit note already holding the ID gets a `duplicate-id` warning.
#[test]
fn negative_documents_and_credit_note_ids() {
    let mut b = Book::new();
    let vendor = b.ok("vendor.list", json!({}))["vendors"][0]["guid"].clone();
    b.ok("invoice.create", json!({ "owner": vendor, "id": "N-1", "creditNote": true }));
    let fields = [
        ("id", "N-1"),
        ("date_opened", "2024-03-01"),
        ("owner_id", "V1"),
        ("account", "Expenses:Office"),
        ("quantity", "-2"),
        ("price", "10"),
        ("date_posted", "2024-03-02"),
        ("account_posted", "Liabilities:Payable"),
    ];
    let p = b.documents(&row(&fields, ';'), json!({}));
    let d = &p["documents"][0];
    assert_eq!(d["action"], "add");
    assert_eq!(
        codes_of(&d["warnings"]),
        ["fixed", "fixed", "duplicate-id"],
        "the empty due and entry dates are fixed"
    );
    assert_eq!(amount(&d["totals"]["total"]), (-20, 1));
    let applied = b.apply(&p, json!({}));
    let bill = b.invoice(&applied["documents"]["created"][0]["guid"]);
    assert_eq!(bill["type"], "bill");
    assert_eq!(bill["posted"], true);
}

/// Posting before the auto read-only threshold is flagged, as imported transactions are.
#[test]
fn posting_before_the_read_only_threshold_is_flagged() {
    let mut b = Book::new();
    b.ok("book.setOptions", json!({ "autoReadOnlyDays": 30 }));
    let fields = [
        ("id", "O-1"),
        ("date_opened", "2024-03-01"),
        ("owner_id", "V1"),
        ("account", "Expenses:Office"),
        ("price", "1"),
        ("date_posted", "2024-03-02"),
        ("account_posted", "Liabilities:Payable"),
    ];
    let p = b.documents(&row(&fields, ';'), json!({}));
    assert_eq!(p["thresholdDate"], "2024-06-01");
    assert_eq!(p["documents"][0]["readOnlyByDate"], true);
    assert_eq!(p["summary"]["readOnlyByDate"], 1);
    let applied = b.apply(&p, json!({}));
    assert_eq!(codes_of(&applied["warnings"]), ["read-only-by-date"]);
}

/// The separators, date formats and leaf account names GnuCash's dialog and preferences offer.
#[test]
fn separators_date_formats_and_leaf_names() {
    let mut b = Book::new();
    let fields = |date: &'static str, account: &'static str| {
        vec![
            ("id", "S-1"),
            ("date_opened", date),
            ("owner_id", "V1"),
            ("date", date),
            ("account", account),
            ("price", "2"),
        ]
    };
    [
        ("us", "03/15/2024", ',', "comma"),
        ("uk", "15/03/2024", ';', "semicolon"),
        ("ce", "15.03.2024", ';', "semicolon"),
        ("iso", "2024-03-15", ',', "commaQuoted"),
    ]
    .iter()
    .for_each(|(format, date, sep, separator)| {
        let p = b.documents(
            &row(&fields(date, "Office"), *sep),
            json!({ "dateFormat": format, "separator": separator, "accountNames": "leaf" }),
        );
        assert_eq!(p["errors"], json!([]), "{format}: {}", p["errors"]);
        assert_eq!(p["documents"][0]["dateOpened"], "2024-03-15", "{format}");
        assert_eq!(p["documents"][0]["entries"][0]["accountName"], "Expenses:Office");
    });
    let p = b.documents(&row(&fields("2024-03-15", "Office"), ';'), json!({}));
    assert!(
        p["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("account Office does not exist"),
        "full names by default"
    );
    let p = b.documents("garbage\n\n", json!({}));
    assert_eq!(
        p["ignoredLines"],
        json!([{ "file": 0, "line": 1, "reason": "noMatch" }, { "file": 0, "line": 2, "reason": "noMatch" }])
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Customers and vendors.

/// A line of 19 fields.
fn owner_row(cells: &[&str; 19], sep: char) -> String {
    cells.join(&sep.to_string()) + "\n"
}

/// Every column, the header row, the company fix-up (`dialog-customer-import.c:236-254`), updating by ID and the
/// Counter for an empty ID (`:321-352`), then the same file again.
#[test]
#[allow(clippy::too_many_lines)]
fn customers_are_created_and_updated_by_id_with_every_field() {
    let mut b = Book::new();
    b.ok("config.set", json!({ "changes": { "defaultCurrency": "EUR" } }));
    let header = [
        "id",
        "company",
        "name",
        "addr1",
        "addr2",
        "addr3",
        "addr4",
        "phone",
        "fax",
        "email",
        "notes",
        "shipname",
        "shipaddr1",
        "shipaddr2",
        "shipaddr3",
        "shipaddr4",
        "shipphone",
        "shipfax",
        "shipemail",
    ];
    let full = [
        "C9",
        "Globex",
        "Hank Scorpio",
        "1 Volcano Rd",
        "Cypress Creek",
        "",
        "USA",
        "555-1",
        "555-2",
        "hank@globex",
        "VIP",
        "Dock 4",
        "2 Pier St",
        "",
        "",
        "",
        "555-3",
        "",
        "dock@globex",
    ];
    let mut acme = [""; 19];
    (acme[0], acme[1], acme[2]) = ("C1", "Acme Ltd", "Wile E");
    let mut solo = [""; 19];
    solo[2] = "Solo Trader";
    let text = [
        owner_row(&header, ';'),
        owner_row(&full, ';'),
        owner_row(&acme, ';'),
        owner_row(&solo, ';'),
        owner_row(&[""; 19], ';'),
    ]
    .concat();
    let p = b.owners(&text, json!({}));
    let o = p["owners"].as_array().unwrap();
    assert_eq!(o.len(), 4);
    assert_eq!((o[0]["action"].as_str(), o[0]["alternative"].as_str()), (Some("skip"), Some("add")));
    assert_eq!(codes_of(&o[0]["warnings"]), ["header-row"]);
    assert_eq!(o[1]["action"], "add");
    assert_eq!(o[1]["currency"], "CURRENCY::EUR", "defaultCurrency");
    assert_eq!(o[1]["fields"]["shipping"]["email"], "dock@globex");
    assert_eq!(o[2]["action"], "update");
    assert_eq!(o[2]["changes"], json!(["address.name"]));
    assert_eq!(
        (o[3]["id"].as_str(), o[3]["idFromCounter"].as_bool()),
        (Some("000001"), Some(true)),
        "the customer counter's next number"
    );
    assert_eq!(o[3]["fields"]["company"], "Solo Trader");
    assert_eq!(codes_of(&o[3]["warnings"]), ["fixed"]);
    assert_eq!(p["errors"][0]["line"], 5, "neither company nor name");

    let applied = b.apply(&p, json!({ "skipErrors": true }));
    let owners = &applied["owners"];
    assert_eq!(owners["created"].as_array().unwrap().len(), 2);
    assert_eq!(owners["updated"].as_array().unwrap().len(), 1);
    assert_eq!(owners["skipped"], json!([0]));
    let c = b.ok("customer.get", json!({ "customer": owners["created"][0]["guid"] }));
    assert_eq!(c["name"], "Globex");
    assert_eq!(c["currency"], "CURRENCY::EUR");
    assert_eq!(c["notes"], "VIP");
    let counted = b.ok("customer.get", json!({ "customer": owners["created"][1]["guid"] }));
    assert_eq!(counted["id"], "000001");

    let again = b.owners(&text, json!({}));
    assert_eq!(
        again["summary"]["add"], 1,
        "only the row without an ID, which takes a new number as in GnuCash"
    );
    assert_eq!(again["owners"][3]["id"], "000002");
    assert_eq!(again["summary"]["update"], 0);
    assert!(
        again["owners"].as_array().unwrap()[1..3]
            .iter()
            .all(|o| o["action"] == "skip" && codes_of(&o["warnings"]) == ["unchanged"])
    );
    let applied = b.apply(&again, json!({ "skipErrors": true }));
    assert_eq!(applied["owners"]["updated"], json!([]), "the rows with IDs change nothing");
    assert_eq!(
        applied["owners"]["created"].as_array().unwrap().len(),
        1,
        "a row with no ID is a new customer each time, as in GnuCash (:321-338)"
    );
    assert_eq!(b.ok("customer.get", json!({ "customer": c["guid"] }))["notes"], "VIP");
}

/// A header row the caller chooses to import, as GnuCash would; an inactive Owner made active; vendors' shipping
/// columns ignored with a warning.
#[test]
fn vendors_header_rows_and_reactivation() {
    let mut b = Book::new();
    let v = b.ok("vendor.list", json!({}))["vendors"][0]["guid"].clone();
    b.ok("vendor.update", json!({ "vendor": v, "active": false }));
    let mut header = [""; 19];
    (header[0], header[1], header[2]) = ("ID", "Company", "Name");
    let mut ink = [""; 19];
    (ink[0], ink[1], ink[11]) = ("V1", "Ink Ltd", "Ship");
    let text = [owner_row(&header, ','), owner_row(&ink, ',')].concat();
    let p = b.owners(&text, json!({ "ownerKind": "vendor", "separator": "comma" }));
    let o = p["owners"].as_array().unwrap();
    assert_eq!(o[1]["action"], "update");
    assert_eq!(o[1]["reactivate"], true);
    assert_eq!(o[1]["changes"], json!(["active"]));
    assert_eq!(codes_of(&o[1]["warnings"]), ["ignored-columns"]);
    assert_eq!(o[1]["fields"]["shipping"], Value::Null);
    let applied = b.apply(&p, json!({ "changes": [{ "index": 0, "action": "add" }] }));
    assert_eq!(
        applied["owners"]["created"].as_array().unwrap().len(),
        1,
        "the header row, as GnuCash imports it"
    );
    let vendor = b.ok("vendor.get", json!({ "vendor": v }));
    assert_eq!(vendor["active"], true);
    let made = b.ok("vendor.get", json!({ "vendor": applied["owners"]["created"][0]["guid"] }));
    assert_eq!((made["id"].as_str(), made["name"].as_str()), (Some("ID"), Some("Company")));
}
