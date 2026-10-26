//! Invoices, bills, expense vouchers, their lines and orders made through the API read back the same in GnuCash
//! 5.17, GnuCash numbers documents as red-ale does, and GnuCash computes totals from the lines red-ale wrote.
//!
//! Orders are not wrapped by GnuCash's Python bindings: the oracle checks only that GnuCash reads them all back
//! (`orderCount`); their fields are covered by the API tests and the XML round trip.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, open, oracle, oracle_check};
use serde_json::{Value, json};

const BOOK: &str = "synthetic/business-editable.gnucash";

fn guid_of(d: &mut red_ale_rpc::Dispatcher, method: &str, list: &str, id: &str) -> Value {
    call(d, method, &json!({ "id": id }))[list][0]["guid"].clone()
}

/// Run a GnuCash script on the book and return its output lines.
fn gnucash(book: &TempBook, body: &str) -> Vec<String> {
    let code = format!(
        r#"
import sys
from gnucash import Session, SessionOpenMode, Query
from gnucash.gnucash_business import Customer, Vendor, Employee, Invoice
def find(book, kind, cls, id):
    q = Query()
    q.search_for(kind)
    q.set_book(book)
    return [x for x in (cls(instance=i) for i in q.run()) if x.GetID() == id][0]
def book_invoices(book):
    q = Query()
    q.search_for("gncInvoice")
    q.set_book(book)
    return [Invoice(instance=i) for i in q.run()]
def frac(n):
    num, den = (n.num(), n.denom()) if callable(n.num) else (n.num, n.denom)
    return f"{{num}}/{{den}}"
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_NORMAL_OPEN)
try:
    book = s.book
{body}
finally:
    s.end()
"#
    );
    let copy = book.dir.path().join("oracle-copy.gnucash");
    std::fs::copy(&book.path, &copy).unwrap();
    oracle::run_code(&code, &[copy.to_str().unwrap()]).unwrap().lines().map(str::to_owned).collect()
}

/// A fraction GnuCash printed, in lowest terms.
fn value(frac: &str) -> (i64, i64) {
    let (n, d) = frac.split_once('/').unwrap();
    let (n, d): (i64, i64) = (n.parse().unwrap(), d.parse().unwrap());
    let g = (1..=d).rev().find(|g| n % g == 0 && d % g == 0).unwrap_or(1);
    (n / g, d / g)
}

#[test]
fn documents_and_lines_read_back_in_gnucash_with_its_totals() {
    let (book, mut d) = open(BOOK);
    let acme = guid_of(&mut d, "customer.list", "customers", "000001");
    let paper = guid_of(&mut d, "vendor.list", "vendors", "000001");
    let jane = guid_of(&mut d, "employee.list", "employees", "000001");
    let job = guid_of(&mut d, "job.list", "jobs", "JOB-1");

    let inv = call(&mut d, "invoice.create", &json!({ "owner": acme, "dateOpened": "2024-06-03", "billingId": "PO-12", "notes": "June" }));
    // Owner defaults: 5 % discount before tax, the customer's GST+Levy (10 % + 2.50), tax excluded.
    call(&mut d, "invoice.addEntry", &json!({ "invoice": inv["guid"], "date": "2024-06-04", "description": "Design", "action": "Hours", "quantity": "4", "price": "120", "account": "Income" }));
    call(
        &mut d,
        "invoice.addEntry",
        &json!({ "invoice": inv["guid"], "description": "Fee", "quantity": "1", "price": "10", "account": "Income", "taxable": false, "taxTable": null,
                 "discount": "0", "discountType": "value", "discountHow": "sameTime", "notes": "flat" }),
    );
    let bill = call(&mut d, "invoice.create", &json!({ "owner": paper, "billTo": acme, "terms": null }));
    call(
        &mut d,
        "invoice.addEntry",
        &json!({ "invoice": bill["guid"], "description": "Paper", "quantity": "3", "price": "19.99", "account": "Expenses", "billable": true, "billTo": job, "taxTable": "GST+Levy" }),
    );
    let voucher = call(&mut d, "invoice.create", &json!({ "owner": jane, "chargeAmount": "5" }));
    call(&mut d, "invoice.addEntry", &json!({ "invoice": voucher["guid"], "description": "Taxi", "quantity": "1", "price": "25", "account": "Expenses", "payment": "card" }));
    let job_inv = call(&mut d, "invoice.create", &json!({ "owner": job }));
    call(&mut d, "invoice.addEntry", &json!({ "invoice": job_inv["guid"], "description": "Build", "quantity": "2", "account": "Income" }));
    let gone = call(&mut d, "invoice.create", &json!({ "owner": acme }));
    call(&mut d, "invoice.addEntry", &json!({ "invoice": gone["guid"], "description": "Gone", "account": "Income" }));
    call(&mut d, "invoice.delete", &json!({ "invoice": gone["guid"] }));
    let posted = call(&mut d, "invoice.list", &json!({ "type": "invoice", "posted": true }))["invoices"][0]["guid"].clone();
    call(&mut d, "invoice.update", &json!({ "invoice": posted, "active": false }));
    let order = call(&mut d, "order.create", &json!({ "owner": acme, "reference": "R-1" }));
    call(&mut d, "order.addEntry", &json!({ "order": order["guid"], "description": "Ordered", "quantity": "10", "price": "3", "account": "Income" }));
    call(&mut d, "invoice.addEntry", &json!({ "invoice": job_inv["guid"], "order": order["guid"], "description": "Shipped", "account": "Income" }));
    call(&mut d, "order.close", &json!({ "order": order["guid"] }));
    let ids: Vec<String> = [&inv, &bill, &voucher, &job_inv].iter().map(|x| x["id"].as_str().unwrap().to_owned()).collect();
    assert_eq!(ids, ["000003", "000002", "000002", "000004"]);

    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));

    let out = gnucash(
        &book,
        r#"
    inv = find(book, "gncInvoice", Invoice, "000003")
    print(frac(inv.GetTotal()))
    print(book.InvoiceNextID(find(book, "gncCustomer", Customer, "000001")))
    print(book.BillNextID(find(book, "gncVendor", Vendor, "000001")))
    print(book.BillNextID(find(book, "gncEmployee", Employee, "000001")))
"#,
    );
    // 4 x 120 less 5 % = 456, tax 45.60 + 2.50; plus the untaxed 10.
    assert_eq!(value(&out[0]), (5141, 10), "514.10");
    // The deleted invoice used 000005: deleting a document does not give its number back.
    assert_eq!(out[1..], ["000006", "000003", "000003"]);
}

#[test]
fn a_new_book_holds_documents_gnucash_can_read() {
    let (book, mut d) = open("piecash/invoices.gnucash");
    let mickey = guid_of(&mut d, "customer.list", "customers", "123456");
    let looney = guid_of(&mut d, "vendor.list", "vendors", "000001");
    let inv = call(&mut d, "invoice.create", &json!({ "owner": mickey }));
    call(&mut d, "invoice.addEntry", &json!({ "invoice": inv["guid"], "description": "Hat", "quantity": "2", "price": "35.99", "account": "Income:Other Income" }));
    let bill = call(&mut d, "invoice.create", &json!({ "owner": looney }));
    assert_eq!(bill["currency"], "CURRENCY::USD", "the vendor's currency");
    call(&mut d, "invoice.addEntry", &json!({ "invoice": bill["guid"], "description": "Ink", "quantity": "1", "price": "12", "account": "Expenses:Supplies" }));
    let order = call(&mut d, "order.create", &json!({ "owner": looney }));
    call(&mut d, "order.addEntry", &json!({ "order": order["guid"], "description": "Pens", "quantity": "5" }));
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let out = gnucash(&book, "    print(frac(find(book, \"gncInvoice\", Invoice, \"000002\").GetTotal()))\n");
    assert_eq!(value(&out[0]), (3599, 50), "71.98");
}

/// Credit notes red-ale makes in a book that had none: GnuCash reads them back as credit notes with the totals of the
/// lines red-ale wrote, shown positive as GnuCash shows them, and the book declares the `Credit Notes` feature
/// (`5.17:libgnucash/engine/gnc-features.h:54`) so GnuCash opens it with no unknown-feature warning.
#[test]
fn credit_notes_made_by_red_ale_read_back_in_gnucash() {
    let (book, mut d) = open("piecash/invoices.gnucash");
    let mickey = guid_of(&mut d, "customer.list", "customers", "123456");
    let looney = guid_of(&mut d, "vendor.list", "vendors", "000001");
    call(&mut d, "account.create", &json!({ "name": "Receivable", "type": "RECEIVABLE", "parent": "Assets", "commodity": "CURRENCY::EUR" }));
    let cn = call(&mut d, "invoice.create", &json!({ "owner": mickey, "creditNote": true }));
    call(&mut d, "invoice.addEntry", &json!({ "invoice": cn["guid"], "description": "Returned hat", "quantity": "1", "price": "35.99", "account": "Income:Other Income" }));
    call(&mut d, "invoice.post", &json!({ "invoice": cn["guid"], "account": "Assets:Receivable", "postDate": "2024-03-10" }));
    let vcn = call(&mut d, "invoice.create", &json!({ "owner": looney, "creditNote": true }));
    call(&mut d, "invoice.addEntry", &json!({ "invoice": vcn["guid"], "description": "Ink refund", "quantity": "1", "price": "12", "account": "Expenses:Supplies" }));
    // Invoices and bills have separate counters, so a document is found by its ID and its owner's.
    let docs = [(cn["id"].as_str().unwrap().to_owned(), "123456"), (vcn["id"].as_str().unwrap().to_owned(), "000001")];
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let body = docs
        .iter()
        .map(|(id, owner)| format!("    i = [x for x in book_invoices(book) if (x.GetID(), x.GetOwner().GetID()) == (\"{id}\", \"{owner}\")][0]\n    print(i.GetIsCreditNote(), i.GetTypeString(), frac(i.GetTotal()), i.IsPosted(), sep=\"|\")\n"))
        .collect::<Vec<_>>()
        .concat();
    let out = gnucash(&book, &body);
    let read: Vec<(&str, &str, (i64, i64), &str)> = out
        .iter()
        .map(|l| {
            let f: Vec<&str> = l.split('|').collect();
            (f[0], f[1], value(f[2]), f[3])
        })
        .collect();
    assert_eq!(read, [("True", "Credit Note", (3599, 100), "True"), ("True", "Credit Note", (12, 1), "False")], "{out:?}");
    let path = book.path_str();
    let script = |name: &str, args: &[&str]| -> Value {
        serde_json::from_str(&oracle::run_script(&oracle::scripts_dir().join(name), args).unwrap_or_else(|e| panic!("{e}"))).unwrap()
    };
    assert_eq!(script("opened.py", &[&path]), json!({ "unknownFeatures": null, "logged": [] }));
    let features = script("book_options.py", &["slots", &path])["features"].clone();
    assert!(features.get("Credit Notes").is_some(), "the book declares the Credit Notes feature: {features}");
}
