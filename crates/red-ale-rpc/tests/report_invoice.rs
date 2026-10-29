//! The Printable Invoice (`standard/invoice.rhai` on `lib/business`) through `report.run` and `report.export`: every
//! assertion of 5.17's `test-invoice.scm` (TI) for the Printable variant, then hand-worked cases for what the test
//! does not reach (`design/notes/report-invoice.md` §1.8). INV stands for
//! `5.17:gnucash/report/reports/standard/invoice.scm`, BC for `5.17:bindings/guile/business-core.scm`.
//!
//! TI runs on `create-test-invoice-data`'s eight documents, which `fixtures/reports/transaction.gnucash` holds with
//! GUIDs `3000…00` to `3000…07`. That book is XML only: GnuCash 5.17 cannot save its open order to SQLite
//! (`design/milestone5.md`, "Testing"), so the SQLite runs are on `synthetic/business.gnucash`, whose copy GnuCash
//! wrote. TI reads HTML cells through `sxml->table-row-col` (`5.17:gnucash/report/test/test-report-extras.scm:96-109`):
//! the first table inside the `div` of a class, a row's `<td>` texts, empty cells giving no text. Here the same
//! positions of the result: the table block of that class, a row's cells that are not `null` and not empty, each its
//! display string or text. "In the body" is any text or display string of the result.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod report_runs;

use report_runs::{Fixture, described_options};
use serde_json::{Map, Value, json};

const REPORT: &str = "Printable Invoice";

/// inv-*k* of `create-test-invoice-data` in `reports/transaction.gnucash`.
fn inv(k: u32) -> String {
    format!("3{:031}", k - 1)
}

/// The Display Columns options in INV's order (:409-443).
const COLUMNS: [&str; 9] = [
    "Date",
    "Description",
    "Action",
    "Quantity",
    "Price",
    "Discount",
    "Taxable",
    "Tax Amount",
    "Total",
];

/// The Display booleans TI sets (TI:79-83).
const DISPLAY: [&str; 9] = [
    "Due Date",
    "Use Detailed Tax Summary",
    "Subtotal",
    "References",
    "Billing Terms",
    "Billing ID",
    "Invoice Notes",
    "Payments",
    "Job Details",
];

/// `default-testing-options` (TI:68-83): the document, and every Display Columns and Display boolean TI names set to
/// `setting`.
fn testing_options(invoice: &str, setting: bool) -> Value {
    let all = |names: &[&str]| names.iter().map(|n| ((*n).to_owned(), json!(setting))).collect::<Map<_, _>>();
    json!({ "General": { "Invoice Number": invoice }, "Display Columns": all(&COLUMNS), "Display": all(&DISPLAY) })
}

/// `options` with `section`/`name` set to `value`.
fn with(mut options: Value, section: &str, name: &str, value: Value) -> Value {
    options[section][name] = value;
    options
}

/// Every block of a result, those inside groups included, in order.
fn all_blocks(result: &Value) -> Vec<&Value> {
    fn walk<'a>(blocks: &'a [Value], out: &mut Vec<&'a Value>) {
        blocks.iter().for_each(|b| {
            out.push(b);
            if let Some(inner) = b["blocks"].as_array() {
                walk(inner, out);
            }
        });
    }
    let mut out = Vec::new();
    result["sections"]
        .as_array()
        .unwrap()
        .iter()
        .for_each(|s| walk(s["blocks"].as_array().unwrap(), &mut out));
    out
}

/// Whether a block or cell has the class `name`.
fn has_class(v: &Value, name: &str) -> bool {
    v["class"].as_str().is_some_and(|c| c.split(' ').any(|n| n == name))
}

/// The table block of class `name`.
fn table<'a>(result: &'a Value, name: &str) -> &'a Value {
    let found: Vec<&Value> = all_blocks(result)
        .into_iter()
        .filter(|b| b["kind"] == "table" && has_class(b, name))
        .collect();
    assert_eq!(found.len(), 1, "one table of class {name}: {result:#}");
    found[0]
}

/// A cell's text as a page shows it: its display string, else its text.
fn shown(cell: &Value) -> Option<String> {
    cell["display"].as_str().or_else(|| cell["text"].as_str()).map(str::to_owned)
}

/// The texts of a row's cells, as TI's text nodes: `null` and empty cells left out.
fn row_texts(row: &Value) -> Vec<String> {
    row["cells"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(shown)
        .filter(|t| !t.is_empty())
        .collect()
}

/// Row *k* (from 1) of a table, as TI's `sxml-get-row-col … k #f`.
fn row(table: &Value, k: usize) -> Vec<String> {
    row_texts(&table["rows"][k - 1])
}

/// Column `col` of every row with text, as TI's `#f col`: 1 the first text, -1 the last.
fn column(table: &Value, col: i32) -> Vec<String> {
    table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(row_texts)
        .filter(|texts| !texts.is_empty())
        .map(|texts| {
            if col < 0 {
                texts[texts.len() - 1].clone()
            } else {
                texts[0].clone()
            }
        })
        .collect()
}

/// Every text and display string of a result, as TI's `(// body // *text*)`.
fn body(result: &Value) -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => m.iter().for_each(|(k, v)| match (k.as_str(), v) {
                ("text" | "display", Value::String(s)) => out.push(s.clone()),
                ("options" | "log", _) => {}
                _ => walk(v, out),
            }),
            Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&result["sections"], &mut out);
    out
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

/// The cells of every row of a table, `null` as `-`, each its display string or text.
fn grid(table: &Value) -> Vec<Vec<String>> {
    let cell = |c: &Value| if c.is_null() { "-".to_owned() } else { shown(c).unwrap() };
    let rows = table["rows"].as_array().unwrap();
    rows.iter()
        .map(|r| r["cells"].as_array().unwrap().iter().map(cell).collect())
        .collect()
}

/// The column headings of a table.
fn headings(table: &Value) -> Vec<String> {
    let columns = table["columns"].as_array().unwrap();
    columns.iter().map(|c| c["label"].as_str().unwrap().to_owned()).collect()
}

/// `synthetic/business.gnucash`: a customer invoice taxed at GST+Levy and paid in part, a bill, an expense voucher
/// and an unposted credit note for a job, in euros; as XML and as GnuCash's SQLite copy.
fn business_books() -> [(&'static str, Fixture); 2] {
    [
        ("xml", Fixture::open_fixture("synthetic/business.gnucash", "us", true)),
        ("sqlite", Fixture::open_fixture("sqlite/synthetic/business.gnucash", "us", true)),
    ]
}

const ACME_INVOICE: &str = "35ad2d2ebe3847ea9a415e7cc53cc09f";
const VOUCHER: &str = "6193d3b14af34c25b389fc13ac14d277";
const JOB_CREDIT_NOTE: &str = "f6772a8b0ea543679c940e23904f9ed9";
const BILL: &str = "fc2cd829ac2442269c964a2ee27244cc";

fn document(guid: &str) -> Value {
    json!({ "General": { "Invoice Number": guid } })
}

// ---- test-invoice.scm, the Printable variant ----

/// TI:1-19: every option `set-option!` names exists, or TI would record `wrong-option` (TI:35-38).
#[test]
fn every_option_test_invoice_sets_exists() {
    let options = described_options(REPORT);
    let names: Vec<String> = std::iter::once("General/Invoice Number".to_owned())
        .chain(COLUMNS.iter().map(|c| format!("Display Columns/{c} ")))
        .chain(DISPLAY.iter().map(|d| format!("Display/{d} ")))
        .collect();
    names.iter().for_each(|n| {
        assert!(options.iter().any(|o| o.starts_with(n.as_str())), "no option {n}: {options:#?}");
    });
    assert!(
        options.iter().any(|o| o == "General/Invoice Number \"x\" \"invoice\" null"),
        "{options:#?}"
    );
}

/// inv-1 (TI:93-113): a customer invoice of one entry with a billing ID and notes.
#[test]
fn inv_1_simple_entry() {
    let mut f = Fixture::open("transaction", "us", true);
    let r = f.run(REPORT, &testing_options(&inv(1), true));
    let entries = table(&r, "entries-table");
    assert_eq!(column(entries, -1), strings(&["$6.00", "$6.00", "$6.00", "$6.00"]));
    assert_eq!(
        row(entries, 1)[1..],
        strings(&["entry-1-desc", "entry-1-action", "2.00", "$3.00", "0.00 %", "T", "$0.00", "$6.00"])
    );
    assert_eq!(row(table(&r, "client-table"), 1), strings(&["cust-1-name"]));
    let texts = body(&r);
    assert!(texts.contains(&"inv-1-billing-id".to_owned()));
    assert!(texts.contains(&"inv-1-notes".to_owned()));
}

/// inv-1 with every option off (TI:115-124): no columns, so the entry row shows nothing.
#[test]
fn inv_1_sparse_options() {
    let mut f = Fixture::open("transaction", "us", true);
    let r = f.run(REPORT, &testing_options(&inv(1), false));
    let entries = table(&r, "entries-table");
    assert_eq!(column(entries, 1), strings(&["Tax", "Total Price", "Amount Due"]));
    assert_eq!(column(entries, -1), strings(&["$0.00", "$6.00", "$6.00"]));
}

/// inv-2 to inv-7 (TI:126-235): a job's invoice, a bill, an expense voucher and the three credit notes. A bill's and a
/// voucher's discount cell is empty (INV:640-646), so it drops out; a credit note prints positive figures.
#[test]
fn inv_2_to_7_simple_entries() {
    let cases: [(u32, &[&str], &str, &[&str]); 6] = [
        (
            2,
            &[
                "entry-inv-2-desc",
                "entry-inv-2-action",
                "2.00",
                "$3.00",
                "0.00 %",
                "T",
                "$0.00",
                "$6.00",
            ],
            "cust-1-name",
            &["inv-2-notes", "job-1-id", "job-1-name"],
        ),
        (
            3,
            &["entry-inv-3-desc", "entry-inv-3-action", "2.00", "$3.00", "T", "$0.00", "$6.00"],
            "vend-1-name",
            &["inv-3-notes"],
        ),
        (
            4,
            &["entry-inv-4-desc", "entry-inv-4-action", "2.00", "$3.00", "T", "$0.00", "$6.00"],
            "emp-1-name",
            &["inv-4-notes"],
        ),
        (
            5,
            &["entry-5-desc", "entry-5-action", "2.00", "$3.00", "0.00 %", "T", "$0.00", "$6.00"],
            "cust-1-name",
            &[],
        ),
        (
            6,
            &["entry-inv-6-desc", "entry-inv-6-action", "2.00", "$3.00", "T", "$0.00", "$6.00"],
            "vend-1-name",
            &["inv-3-notes"],
        ),
        (
            7,
            &["entry-inv-7-desc", "entry-inv-7-action", "2.00", "$3.00", "T", "$0.00", "$6.00"],
            "emp-1-name",
            &["inv-4-notes"],
        ),
    ];
    let mut f = Fixture::open("transaction", "us", true);
    cases.into_iter().for_each(|(k, details, client, in_body)| {
        let r = f.run(REPORT, &testing_options(&inv(k), true));
        let entries = table(&r, "entries-table");
        assert_eq!(column(entries, -1), strings(&["$6.00", "$6.00", "$6.00", "$6.00"]), "inv-{k}");
        assert_eq!(row(entries, 1)[1..], strings(details), "inv-{k}");
        assert_eq!(row(table(&r, "client-table"), 1), strings(&[client]), "inv-{k}");
        let texts = body(&r);
        in_body
            .iter()
            .for_each(|t| assert!(texts.contains(&(*t).to_owned()), "inv-{k}: {t}"));
    });
}

/// inv-8 (TI:237-261): eight entries combining the tax and discount settings, an order, a billing term and a payment
/// in full. The amounts are GnuCash's (TI:255-259); the last assertion, that the document is paid, reads the Book.
#[test]
fn inv_8_combinations_of_entry_options() {
    let mut f = Fixture::open("transaction", "us", true);
    let r = f.run(REPORT, &testing_options(&inv(8), true));
    let texts = body(&r);
    assert!(texts.contains(&"billterm-desc".to_owned()));
    assert!(texts.contains(&"REF order-ref".to_owned()));
    let details = table(&r, "invoice-details-table");
    assert_eq!(row(details, 1)[..1], strings(&["Date:"]));
    assert_eq!(row(details, 2)[..1], strings(&["Due Date:"]));
    assert_eq!(
        column(table(&r, "entries-table"), -1),
        strings(&[
            "$2,133.25",
            "$2,061.96",
            "$2,133.25",
            "$2,061.96",
            "$2,133.25",
            "$2,133.25",
            "$1,851.95",
            "$1,859.30",
            "$16,368.17",
            "$1,111.01",
            "$17,479.18",
            "-$17,479.18",
            "$0.00"
        ])
    );
    let html = f.export(REPORT, &testing_options(&inv(8), true));
    ["$16,368.17", "-$17,479.18", "REF order-ref", "billterm-desc", "VAT-on-Sales"]
        .iter()
        .for_each(|t| assert!(html.contains(t), "{t} in the page"));
    let lot = f.call("invoice.get", &json!({ "invoice": inv(8) }))["postedLot"].clone();
    assert_eq!(f.call("lot.get", &json!({ "lot": lot }))["closed"], true, "gncInvoiceIsPaid");
}

// ---- Hand-worked cases ----

/// `rows` as owned strings.
fn rows(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter().map(|r| strings(r)).collect()
}

/// The text blocks of a result outside groups, as (class, text).
fn notes(result: &Value) -> Vec<(String, String)> {
    let blocks = result["sections"][0]["blocks"].as_array().unwrap();
    blocks
        .iter()
        .filter(|b| b["kind"] == "text")
        .map(|b| (b["class"].as_str().unwrap_or("").to_owned(), b["text"].as_str().unwrap().to_owned()))
        .collect()
}

/// No document, a GUID that names none (a deleted document) and an entry's GUID all give the warning of INV:760-766,
/// `gnc:html-make-generic-warning` with the report's "Invoice" and a colon as its title (`5.17:gnucash/report/html-utilities.scm:350-357`),
/// under no title and with no CSS. The option keeps the GUID it was given.
#[test]
fn no_document_gives_the_warning() {
    let mut f = Fixture::open("transaction", "us", true);
    let warning = json!([{ "kind": "warning", "title": "Invoice:",
                           "text": "No valid invoice selected. Click on the Options button and select the invoice to use." }]);
    let nothing = "ffffffffffffffffffffffffffffffff";
    [json!({}), document(nothing), document(&format!("4{:031}", 0))]
        .iter()
        .for_each(|options| {
            let r = f.run(REPORT, options);
            assert_eq!((&r["title"], &r["sections"][0]["blocks"]), (&json!(""), &warning), "{options}");
            assert!(r.get("css").is_none(), "{options}");
        });
    assert_eq!(f.run(REPORT, &document(nothing))["options"]["General"]["Invoice Number"], nothing);
    let html = f.export(REPORT, &json!({}));
    assert!(html.contains("<h3>Invoice:</h3><p>No valid invoice selected."));
}

/// Acme's invoice in the business book with the Printable Invoice's defaults (INV:409-516). The details (:230-298):
/// the posted and due dates, the billing ID and the empty row after it (:270), the terms' description. The client
/// (:111-161): the name, then the address's name and lines that are not empty (BC:69-82). No company detail is set, so
/// the company table has no rows (:163-220). The entries (:612-668): 10 hours at €120.50 taxed by GST+Levy, €1,205.00;
/// travel at €50.00 less a €5.00 discount, untaxed, €45.00. Net €1,250.00; tax 10 % of €1,205.00 plus the €2.50 levy,
/// €123.00; total €1,373.00; the payment of €500.00 printed as its lot split, negative (:580-606); due €873.00.
/// With the detailed tax summary each tax account is a row of its own (:705-712), in `gncInvoiceGetTotalTaxList`'s order:
/// the table sorts Levy before Tax (`5.17:libgnucash/engine/gncTaxTable.c:594-609`), and each of the three lists built
/// from it by prepending (`gncTaxTable.c:942-967`) reverses it, so Tax comes first. With the Total column off,
/// `display-subtotal` prints the payment positive (:570-578) and the table has one column less.
#[test]
fn a_customer_invoice_taxed_twice_and_paid_in_part() {
    for (format, mut f) in business_books() {
        let r = f.run(REPORT, &document(ACME_INVOICE));
        assert_eq!(
            all_blocks(&r)[0],
            &json!({ "kind": "text", "text": "Invoice #000001", "role": "heading", "class": "invoice-title" }),
            "{format}"
        );
        assert_eq!(
            grid(table(&r, "invoice-details-table")),
            rows(&[
                &["Date:", "03/05/2024"],
                &["Due Date:", "04/03/2024"],
                &["Reference:", "CUST-REF-9"],
                &["-", "-"],
                &["Terms:", "Net 30, 2% in 10"]
            ]),
            "{format}"
        );
        assert_eq!(
            grid(table(&r, "client-table")),
            rows(&[&["Acme & Sons <Ltd>"], &["Acme Billing\n1 Main St\nSuite 2"]]),
            "{format}"
        );
        assert_eq!(table(&r, "company-table")["rows"], json!([]), "{format}");
        let entries = table(&r, "entries-table");
        assert_eq!(
            headings(entries),
            [
                "Date",
                "Description",
                "Action",
                "Quantity",
                "Unit Price",
                "Discount",
                "Taxable",
                "Total",
                ""
            ],
            "{format}"
        );
        assert_eq!(
            grid(entries),
            rows(&[
                &[
                    "03/02/2024",
                    "Consulting",
                    "Hours",
                    "10.00",
                    "€120.50",
                    "0.00 %",
                    "T",
                    "€1,205.00",
                    "-"
                ],
                &["03/03/2024", "Travel", "", "1.00", "€50.00", "€5.00", "", "€45.00", "-"],
                &["Net Price", "€1,250.00"],
                &["Tax", "€123.00"],
                &["Total Price", "€1,373.00"],
                &["03/20/2024", "Payment, thank you!", "-€500.00", "-"],
                &["Amount Due", "€873.00"],
            ]),
            "{format}"
        );
        let detailed = with(
            with(document(ACME_INVOICE), "Display", "Use Detailed Tax Summary", json!(true)),
            "Display Columns",
            "Total",
            json!(false),
        );
        let r = f.run(REPORT, &detailed);
        let entries = table(&r, "entries-table");
        assert_eq!(headings(entries).len(), 8, "{format}");
        assert_eq!(
            grid(entries)[2..],
            rows(&[
                &["Net Price", "€1,250.00"],
                &["Tax", "€120.50"],
                &["Levy", "€2.50"],
                &["Total Price", "€1,373.00"],
                &["03/20/2024", "Payment, thank you!", "€500.00", "-"],
                &["Amount Due", "€873.00"],
            ]),
            "{format}"
        );
    }
}

/// The unposted credit note for Acme's job: "Credit Note" for every credit note (INV:772-787), "Invoice in progress…"
/// over both detail columns (:253-258), the job's number and name (:282-296), and the client through the job
/// (BC:63-67, :69-82). Its one line is stored as a quantity of 1, which a credit note prints negated
/// (`gncEntryGetDocQuantity`, :629), so its figures print negative; unposted, it has no payments.
#[test]
fn an_unposted_credit_note_for_a_job() {
    for (format, mut f) in business_books() {
        let r = f.run(REPORT, &with(document(JOB_CREDIT_NOTE), "Display", "Job Details", json!(true)));
        assert_eq!(all_blocks(&r)[0]["text"], "Credit Note #000002", "{format}");
        assert_eq!(
            grid(table(&r, "invoice-details-table")),
            rows(&[
                &["Invoice in progress…"],
                &["Job number:", "JOB-1"],
                &["Job name:", "Website rebuild"]
            ]),
            "{format}"
        );
        assert_eq!(
            grid(table(&r, "client-table")),
            rows(&[&["Acme & Sons <Ltd>"], &["Acme Billing\n1 Main St\nSuite 2"]]),
            "{format}"
        );
        assert_eq!(
            grid(table(&r, "entries-table")),
            rows(&[
                &["03/06/2024", "Refund", "", "-1.00", "€10.00", "0.00 %", "T", "-€10.00", "-"],
                &["Net Price", "-€10.00"],
                &["Tax", "€0.00"],
                &["Total Price", "-€10.00"],
                &["Amount Due", "-€10.00"],
            ]),
            "{format}"
        );
    }
}

/// A bill and an expense voucher: the bill price, an empty discount cell (:640-646), "T" for a taxable line with no
/// tax table (:650-654), and the vendor's or employee's name and address; an employee's name is its address's name
/// (`5.17:libgnucash/engine/gncEmployee.c:512-517`). The bill's €59.97 includes GST+Levy: (€59.97 - €2.50) / 1.1 =
/// €52.25 net, €7.72 tax.
#[test]
fn a_bill_and_an_expense_voucher() {
    for (format, mut f) in business_books() {
        let r = f.run(REPORT, &document(BILL));
        assert_eq!(all_blocks(&r)[0]["text"], "Bill #000001", "{format}");
        assert_eq!(
            grid(table(&r, "client-table")),
            rows(&[&["Paper Supplies"], &["Paper Supplies Accounts"]]),
            "{format}"
        );
        assert_eq!(
            grid(table(&r, "entries-table")),
            rows(&[
                &["03/02/2024", "Printer paper", "", "3.00", "€19.99", "", "T", "€52.25", "-"],
                &["Net Price", "€52.25"],
                &["Tax", "€7.72"],
                &["Total Price", "€59.97"],
                &["Amount Due", "€59.97"],
            ]),
            "{format}"
        );
        let r = f.run(REPORT, &document(VOUCHER));
        assert_eq!(all_blocks(&r)[0]["text"], "Expense Voucher #000001", "{format}");
        assert_eq!(grid(table(&r, "client-table")), rows(&[&["Jane Doe"], &["Jane Doe"]]), "{format}");
        assert_eq!(
            grid(table(&r, "entries-table"))[0],
            strings(&["03/04/2024", "Taxi", "", "1.00", "€32.75", "", "T", "€32.75", "-"]),
            "{format}"
        );
    }
}

/// A bill paid in part: its payment's lot split is +€20.00, which a bill prints negated (`gncInvoiceAmountPositive`
/// is false, INV:568, :583-586), and the amount due is the total plus the payments (:738-739). With the Total column
/// off, the payment prints positive (:570-578).
#[test]
fn a_bill_paid_in_part() {
    let mut f = Fixture::open_fixture("synthetic/business.gnucash", "us", false);
    f.call(
        "invoice.pay",
        &json!({ "invoice": BILL, "transferAccount": "Assets:Bank", "amount": "20", "date": "2024-03-10" }),
    );
    let r = f.run(REPORT, &document(BILL));
    assert_eq!(
        grid(table(&r, "entries-table"))[4..],
        rows(&[&["03/10/2024", "Payment, thank you!", "-€20.00", "-"], &["Amount Due", "€39.97"]])
    );
    let r = f.run(REPORT, &with(document(BILL), "Display Columns", "Total", json!(false)));
    assert_eq!(
        grid(table(&r, "entries-table"))[4..],
        rows(&[&["03/10/2024", "Payment, thank you!", "€20.00", "-"], &["Amount Due", "€39.97"]])
    );
}

/// The entries table has 1 + max(3, *n*) columns for *n* shown (INV:670-678): the shown ones headed
/// (`make-heading-list`, :87-105), then unlabelled ones; an entry row fills the shown columns (:612-668) and is
/// padded. With three shown there is one extra column; with none, four unlabelled ones and empty entry rows.
#[test]
fn each_display_column() {
    let mut f = Fixture::open("transaction", "us", true);
    let labels = [
        "Date",
        "Description",
        "Action",
        "Quantity",
        "Unit Price",
        "Discount",
        "Taxable",
        "Tax Amount",
        "Total",
    ];
    let entry = [
        "03/15/2024",
        "entry-1-desc",
        "entry-1-action",
        "2.00",
        "$3.00",
        "0.00 %",
        "T",
        "$0.00",
        "$6.00",
    ];
    let all = table(&f.run(REPORT, &testing_options(&inv(1), true)), "entries-table").clone();
    assert_eq!(headings(&all), labels.iter().chain(&[""]).copied().collect::<Vec<_>>());
    assert_eq!(grid(&all)[0], entry.iter().chain(&["-"]).copied().collect::<Vec<_>>());
    COLUMNS.iter().enumerate().for_each(|(i, column)| {
        let options = with(testing_options(&inv(1), true), "Display Columns", column, json!(false));
        let entries = table(&f.run(REPORT, &options), "entries-table").clone();
        let but = |items: &[&'static str]| -> Vec<&str> { items.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, s)| *s).collect() };
        assert_eq!(headings(&entries), [but(&labels), vec![""]].concat(), "{column}");
        assert_eq!(grid(&entries)[0], [but(&entry), vec!["-"]].concat(), "{column}");
    });
    let mut only = |shown: &[&str]| {
        let mut options = testing_options(&inv(1), false);
        shown.iter().for_each(|c| options["Display Columns"][*c] = json!(true));
        table(&f.run(REPORT, &options), "entries-table").clone()
    };
    let three = only(&["Quantity", "Price", "Total"]);
    assert_eq!(headings(&three), ["Quantity", "Unit Price", "Total", ""]);
    assert_eq!(grid(&three)[0], ["2.00", "$3.00", "$6.00", "-"]);
    let none = only(&[]);
    assert_eq!(headings(&none), ["", "", "", ""]);
    assert_eq!(grid(&none)[0], ["-", "-", "-", "-"]);
    assert_eq!(grid(&none)[1], ["Tax", "$0.00"]);
}

/// "Minimum # of entries" pads with empty rows, continuing the alternation, while the minimum less the rows is
/// positive (INV:692-699): 2.5 with one entry gives two rows; 0 gives none.
#[test]
fn minimum_entries_pad_to_the_ceiling() {
    let mut f = Fixture::open("transaction", "us", true);
    let padded = with(document(&inv(1)), "Display", "Minimum # of entries", json!("2.5"));
    let entries = table(&f.run(REPORT, &padded), "entries-table").clone();
    let alternate: Vec<&Value> = entries["rows"].as_array().unwrap()[..4].iter().map(|r| &r["alternate"]).collect();
    assert_eq!(alternate, [&json!(false), &json!(true), &json!(false), &Value::Null]);
    assert_eq!(grid(&entries)[1], vec!["-"; 9]);
    assert_eq!(grid(&entries)[3][0], "Net Price");
    let none = with(document(&inv(1)), "Display", "Minimum # of entries", json!(0));
    assert_eq!(grid(table(&f.run(REPORT, &none), "entries-table"))[1][0], "Net Price");
}

/// Every Layout choice (`layout-key-list`, INV:314-338) in the three header rows, each right-hand one in
/// `div.div-align-right` (:804-820): the picture as a `file:///` image (:300-308), the company table, today's date
/// (:310-312), empty space, the details and the client.
#[test]
fn every_layout_choice() {
    let mut f = Fixture::open("transaction", "us", true);
    let layout = json!({ "Row 1 Left": "picture", "Row 1 Right": "company", "Row 2 Left": "today", "Row 2 Right": "none",
                         "Row 3 Left": "invoice", "Row 3 Right": "client" });
    let mut options = document(&inv(8));
    options["Layout"] = layout;
    let r = f.run(REPORT, &options);
    let groups: Vec<Vec<(String, String)>> = all_blocks(&r)
        .into_iter()
        .filter(|b| b["kind"] == "group")
        .map(|g| {
            let pair = |b: &Value| (b["kind"].as_str().unwrap().to_owned(), b["class"].as_str().unwrap_or("").to_owned());
            g["blocks"].as_array().unwrap().iter().map(pair).collect()
        })
        .collect();
    let expected = [
        [("image", "picture"), ("table", "div-align-right company-table")],
        [("text", "invoice-print-date"), ("text", "div-align-right")],
        [("table", "invoice-details-table"), ("table", "div-align-right client-table")],
    ];
    let expected: Vec<Vec<(String, String)>> = expected
        .iter()
        .map(|g| g.iter().map(|(k, c)| ((*k).to_owned(), (*c).to_owned())).collect())
        .collect();
    assert_eq!(groups, expected);
    let texts = body(&r);
    assert!(texts.contains(&"03/15/2024".to_owned()), "today, the run's now");
    let image = all_blocks(&r).into_iter().find(|b| b["kind"] == "image").unwrap().clone();
    assert_eq!(image["src"], "file:///", "an empty Picture Location, as make-file-url gives it");
}

/// The Picture Location only names the picture: `make-file-url` puts `file:///` before it unless it starts so
/// (`5.17:gnucash/report/report-utilities.scm:1028-1031`), and a file that does not exist changes nothing but the
/// `src`. No run opens the file: the location here is a FIFO with no writer, which any open for reading would wait
/// on for ever.
#[test]
fn a_picture_location_is_never_read() {
    let mut f = Fixture::open("transaction", "us", true);
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("logo.png");
    let made = std::process::Command::new("mkfifo").arg(&fifo).status().unwrap();
    assert!(made.success());
    let picture = |location: &str| {
        let mut options = with(document(&inv(1)), "Layout", "Row 1 Left", json!("picture"));
        options["Layout"]["Picture Location"] = json!(location);
        options
    };
    let src = |r: &Value| all_blocks(r).into_iter().find(|b| b["kind"] == "image").unwrap()["src"].clone();
    let fifo_path = fifo.display().to_string();
    let cases = [
        ("/nowhere/logo.png", "file:////nowhere/logo.png"),
        ("file:///nowhere/logo.png", "file:///nowhere/logo.png"),
        ("https://example.com/logo.png", "file:///https://example.com/logo.png"),
        (fifo_path.as_str(), &format!("file:///{fifo_path}")),
    ];
    let results: Vec<Value> = cases
        .iter()
        .map(|(location, expected)| {
            let r = f.run(REPORT, &picture(location));
            assert_eq!(src(&r), *expected, "{location}");
            let html = f.export(REPORT, &picture(location));
            assert!(html.contains("<img src=\"file:///"), "{location}");
            r
        })
        .collect();
    let without_src = |r: &Value| {
        let mut r = r.clone();
        r.as_object_mut().unwrap().remove("options");
        r.to_string().replace(&src(&r).to_string(), "\"SRC\"")
    };
    results.iter().for_each(|r| assert_eq!(without_src(r), without_src(&results[0])));
}

/// The company table lists the Book's Business options that are set, in INV's order (:163-220), each in a cell of
/// its class; the footers name the company and its contact (:837-859) after the document's notes (:829-835), and the
/// Extra Notes close the page (:861-866). The Custom Title replaces "Invoice" (:788) and the owner's ID is shown
/// (:149-157).
#[test]
fn company_details_footers_and_a_custom_title() {
    let mut f = Fixture::open("transaction", "us", false);
    f.call(
        "book.setOptions",
        &json!({ "companyName": "Acme Ltd", "companyAddress": "1 Main St\nSpringfield", "companyContactPerson": "Pat",
                 "companyPhoneNumber": "555-0100", "companyFaxNumber": "555-0101", "companyEmailAddress": "ap@acme.example",
                 "companyWebsiteUrl": "https://acme.example", "companyId": "ID-7", "taxNumber": "TAX-9" }),
    );
    let mut options = document(&inv(1));
    options["General"]["Custom Title"] = json!("Statement");
    options["Display"] = json!({ "Payable to": true, "Company contact": true, "Invoice owner ID": true, "Invoice Notes": true,
                                 "Payable to string": "Pay" });
    let r = f.run(REPORT, &options);
    let company = table(&r, "company-table");
    let cells: Vec<(String, String)> = company["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["cells"][0]["class"].as_str().unwrap().to_owned(),
                shown(&row["cells"][0]).unwrap(),
            )
        })
        .collect();
    let expected = [
        ("company-name", "Acme Ltd"),
        ("company-address", "1 Main St\nSpringfield"),
        ("company-phone", "555-0100"),
        ("company-fax", "555-0101"),
        ("company-email", "ap@acme.example"),
        ("company-url", "https://acme.example"),
        ("company-tax-id", "ID-7"),
        ("company-tax-nr", "TAX-9"),
    ];
    let expected: Vec<(String, String)> = expected
        .iter()
        .map(|(c, t)| (format!("maybe-align-right {c}"), (*t).to_owned()))
        .collect();
    assert_eq!(cells, expected);
    assert_eq!(grid(table(&r, "client-table")), rows(&[&["cust-1-name"], &[""], &["cust-1-id"]]));
    let owned =
        |pairs: &[(&str, &str)]| -> Vec<(String, String)> { pairs.iter().map(|(c, t)| ((*c).to_owned(), (*t).to_owned())).collect() };
    assert_eq!(
        notes(&r),
        owned(&[
            ("invoice-title", "Statement #0001"),
            ("invoice-notes", "inv-1-notes"),
            ("invoice-footer-payable-to", "Pay: Acme Ltd"),
            ("invoice-footer-company-contact", "Please direct all enquiries to: Pat"),
            ("invoice-notes", "Thank you for your patronage!"),
        ])
    );
}

/// The Layout › CSS option is the page's own CSS (INV:795), `base-css` by default (:36-45). CSS the CSS template
/// refuses (`design/reports.md` §8) is left out of the result with a `debug` log entry, and the page has none.
#[test]
fn the_css_option_and_refused_css() {
    let mut f = Fixture::open("transaction", "us", true);
    let r = f.run(REPORT, &document(&inv(1)));
    assert!(
        r["css"]
            .as_str()
            .unwrap()
            .starts_with("/* advanced users only */\n.div-align-right { float: right; }\n")
    );
    let html = f.export(REPORT, &document(&inv(1)));
    assert!(html.contains("\n.entries-table > table { width: 100% }\n"));
    assert!(html.contains("<div class=\"table-wrap entries-table\">"));
    let refused = with(
        document(&inv(1)),
        "Layout",
        "CSS",
        json!("p { background: URL(https://example.com/x.png) }"),
    );
    let r = f.run(REPORT, &refused);
    assert!(r.get("css").is_none());
    let entry = r["log"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        (entry["level"].as_str(), entry["text"].as_str()),
        (Some("debug"), Some("the report's CSS was left out: CSS may not contain \"url(\""))
    );
    let html = f.export(REPORT, &refused);
    assert!(!html.contains("example.com") && !html.contains("advanced users"));
}

/// References list each order once, in the reverse of the order their first lines appear, skipping lines with no
/// order (`get-orders`, INV:113-118, :151-157). Quantities print exactly (`xaccPrintAmount` with
/// `gnc-default-print-info #f`, :629-631): 1/8 as `0.125`, 1/3 as `1/3`, whole numbers with two decimals.
#[test]
fn references_and_exact_quantities() {
    let mut f = Fixture::open_fixture("synthetic/business.gnucash", "us", false);
    let acme = "b8e36e1fda8a4ffbb3ab1703daaa2460";
    let order =
        |f: &mut Fixture, reference: &str| f.call("order.create", &json!({ "owner": acme, "reference": reference }))["guid"].clone();
    let (a, b) = (order(&mut f, "REF-A"), order(&mut f, "REF-B"));
    let doc = f.call("invoice.create", &json!({ "owner": acme }))["guid"].clone();
    let lines = [
        ("2024-03-01", a.clone(), json!({ "num": "1", "denom": "8" })),
        ("2024-03-02", Value::Null, json!({ "num": "1", "denom": "3" })),
        ("2024-03-03", b, json!("2")),
        ("2024-03-04", a, json!("1")),
    ];
    lines.into_iter().enumerate().for_each(|(i, (date, order, quantity))| {
        let mut line = json!({ "invoice": doc, "date": date, "description": format!("Line {i}"), "quantity": quantity,
                               "price": "3", "discount": "0", "taxable": false, "taxTable": null, "account": "Income" });
        if !order.is_null() {
            line["order"] = order;
        }
        f.call("invoice.addEntry", &line);
    });
    let r = f.run(REPORT, &document(doc.as_str().unwrap()));
    assert_eq!(grid(table(&r, "client-table"))[2..], rows(&[&["REF REF-B"], &["REF REF-A"]]));
    let quantities: Vec<String> = grid(table(&r, "entries-table"))[..4].iter().map(|row| row[3].clone()).collect();
    assert_eq!(quantities, ["0.125", "1/3", "2.00", "1.00"]);
}

/// A run leaves the Book, its revision, its dirty flag, the file and its directory as they were, on an XML and a
/// SQLite book open for editing; two exports are the same bytes.
#[test]
fn a_run_changes_nothing() {
    let books = [
        ("xml", Fixture::open("transaction", "us", false), inv(8)),
        (
            "sqlite",
            Fixture::open_fixture("sqlite/synthetic/business.gnucash", "us", false),
            ACME_INVOICE.to_owned(),
        ),
    ];
    for (format, mut f, doc) in books {
        let bytes = std::fs::read(f.book_path()).unwrap();
        let before = f.d.context().book().unwrap().clone();
        let revision = f.d.context().session().unwrap().revision();
        let files = f.files();
        let options = with(document(&doc), "Layout", "Row 1 Left", json!("picture"));
        f.run(REPORT, &options);
        let (a, b) = (f.export(REPORT, &options), f.export(REPORT, &options));
        assert_eq!(a, b, "{format}");
        assert_eq!(f.d.context().book().unwrap(), &before, "{format}");
        assert_eq!(f.d.context().session().unwrap().revision(), revision, "{format}");
        assert_eq!(f.call("book.info", &json!({}))["dirty"], false, "{format}");
        assert_eq!(f.files(), files, "{format}: no journal or other file was written");
        assert_eq!(std::fs::read(f.book_path()).unwrap(), bytes, "{format}: the file is unchanged");
    }
}
