//! Invoices, bills, expense vouchers, their lines and orders through the dispatcher.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

struct Book {
    dir: tempfile::TempDir,
    d: Dispatcher,
}

fn open(name: &str) -> Book {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("book.gnucash");
    std::fs::copy(format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR")), &dest).unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(1_719_835_200), 0x00ed_0000_0000_0000_0000_0000_0000_0000))));
    d.call("book.open", json!({ "path": dest.display().to_string() })).unwrap();
    Book { dir, d }
}

impl Book {
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        match self.d.call(method, params.clone()) {
            Ok(v) => panic!("{method} {params} should fail, got {v}"),
            Err(e) => e,
        }
    }

    fn guid_of(&mut self, method: &str, list: &str, id: &str) -> Value {
        self.call(method, json!({ "id": id }))[list][0]["guid"].clone()
    }
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

#[test]
fn invoices_take_their_number_currency_and_terms_from_the_owner() {
    let mut b = open("synthetic/business-editable.gnucash");
    let acme = b.guid_of("customer.list", "customers", "000001");
    let inv = b.call("invoice.create", json!({ "owner": acme, "dateOpened": "2024-06-03", "billingId": "PO-12", "notes": "June" }));
    assert_eq!(inv["type"], "invoice");
    assert_eq!(inv["id"], "000003", "the fixture used two gncInvoice numbers");
    assert_eq!(inv["currency"], "CURRENCY::EUR");
    let net30 = b.call("billTerm.get", json!({ "billTerm": "Net30" }));
    assert_eq!(inv["terms"], net30["guid"]);
    assert_eq!(net30["refcount"], 2, "the customer and the new invoice");
    assert_eq!(inv["posted"], false);
    assert!(inv["dateOpened"].as_str().unwrap().starts_with("2024-06-0"));

    let paper = b.guid_of("vendor.list", "vendors", "000001");
    let bill = b.call("invoice.create", json!({ "owner": paper, "billTo": acme, "terms": null }));
    assert_eq!((bill["type"].as_str(), bill["id"].as_str(), &bill["terms"]), (Some("bill"), Some("000002"), &Value::Null));
    assert_eq!(bill["billTo"]["kind"], "customer");

    let jane = b.guid_of("employee.list", "employees", "000001");
    let voucher = b.call("invoice.create", json!({ "owner": jane, "chargeAmount": "5" }));
    assert_eq!((voucher["type"].as_str(), voucher["id"].as_str()), (Some("expenseVoucher"), Some("000002")));
    assert_eq!(b.err("invoice.create", json!({ "owner": acme, "chargeAmount": "5" })).code, codes::INVALID);
    assert_eq!(b.err("invoice.create", json!({ "owner": acme, "billTo": acme })).code, codes::INVALID);
    assert_eq!(b.err("invoice.create", json!({})).code, codes::INVALID);

    let job = b.guid_of("job.list", "jobs", "JOB-1");
    let job_inv = b.call("invoice.create", json!({ "owner": job }));
    assert_eq!((job_inv["type"].as_str(), job_inv["id"].as_str()), (Some("invoice"), Some("000004")));
    assert_eq!(b.err("invoice.update", json!({ "invoice": job_inv["guid"], "owner": paper })).code, codes::INVALID, "the type cannot change");

    let listed = b.call("invoice.list", json!({ "owner": acme, "posted": false }));
    let ids: Vec<&str> = listed["invoices"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 3, "the fixture's job invoice, the new one and the new job invoice: {ids:?}");
}

#[test]
fn lines_use_the_document_side_and_owner_defaults() {
    let mut b = open("synthetic/business-editable.gnucash");
    let acme = b.guid_of("customer.list", "customers", "000001");
    let inv = b.call("invoice.create", json!({ "owner": acme }));
    let before = b.call("taxTable.get", json!({ "taxTable": "GST+Levy" }))["refcount"].as_i64().unwrap();
    let line = b.call(
        "invoice.addEntry",
        json!({ "invoice": inv["guid"], "date": "2024-06-05", "description": "Design", "action": "Hours", "quantity": "4", "account": "Income", "price": "120" }),
    );
    assert_eq!(line["invoice"], inv["guid"]);
    assert_eq!(amount(&line["discount"]), (5, 1), "the customer's discount");
    assert_eq!(line["invoiceTaxable"], true);
    assert_eq!(line["invoiceTaxIncluded"], false, "the customer's taxIncluded is no");
    assert!(line["invoiceTaxTable"].is_string(), "the customer overrides with its table");
    assert_eq!(line["dateEntered"], "2024-07-01T12:00:00Z");
    assert_eq!(b.call("taxTable.get", json!({ "taxTable": "GST+Levy" }))["refcount"].as_i64().unwrap(), before + 1);

    assert_eq!(b.err("invoice.addEntry", json!({ "invoice": inv["guid"], "account": "Expenses" })).code, codes::INVALID);
    assert_eq!(b.err("invoice.addEntry", json!({ "invoice": inv["guid"], "account": "Assets:Receivable" })).code, codes::INVALID);
    assert_eq!(b.err("invoice.addEntry", json!({ "invoice": inv["guid"], "billable": true })).code, codes::INVALID);
    assert_eq!(b.err("invoice.addEntry", json!({ "invoice": inv["guid"], "quantity": 1 })).code, codes::INVALID_PARAMS);

    let line = b.call(
        "invoice.updateEntry",
        json!({ "entry": line["guid"], "taxTable": null, "taxable": false, "discountType": "value", "discountHow": "postTax", "discount": "10" }),
    );
    assert_eq!((line["invoiceTaxTable"].clone(), line["discountType"].as_str(), line["discountHow"].as_str()), (Value::Null, Some("value"), Some("postTax")));
    assert_eq!(b.call("taxTable.get", json!({ "taxTable": "GST+Levy" }))["refcount"].as_i64().unwrap(), before);

    let job = b.guid_of("job.list", "jobs", "JOB-1");
    let job_inv = b.call("invoice.create", json!({ "owner": job }));
    let job_line = b.call("invoice.addEntry", json!({ "invoice": job_inv["guid"], "quantity": "2" }));
    assert_eq!(amount(&job_line["invoicePrice"]), (90, 1), "the job's rate");

    let paper = b.guid_of("vendor.list", "vendors", "000001");
    let bill = b.call("invoice.create", json!({ "owner": paper }));
    let bline = b.call(
        "invoice.addEntry",
        json!({ "invoice": bill["guid"], "account": "Expenses", "quantity": "3", "price": "19.99", "billable": true, "billTo": acme }),
    );
    assert_eq!((bline["bill"].clone(), bline["invoice"].clone()), (bill["guid"].clone(), Value::Null));
    assert_eq!(bline["billTaxIncluded"], true, "the vendor's taxIncluded is yes");
    assert_eq!(bline["billable"], true);
    assert_eq!(b.err("invoice.addEntry", json!({ "invoice": bill["guid"], "discount": "1" })).code, codes::INVALID);
    assert_eq!(b.err("invoice.addEntry", json!({ "invoice": bill["guid"], "account": "Income" })).code, codes::INVALID);
    assert_eq!(b.err("invoice.addEntry", json!({ "invoice": bill["guid"], "payment": "card" })).code, codes::INVALID);

    let detail = b.call("invoice.get", json!({ "invoice": inv["guid"] }));
    assert_eq!(detail["entryCount"], 1);
    assert_eq!(detail["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn expense_vouchers_record_card_payments() {
    let mut b = open("synthetic/business-editable.gnucash");
    let jane = b.guid_of("employee.list", "employees", "000001");
    let v = b.call("invoice.create", json!({ "owner": jane, "chargeAmount": "2.50" }));
    let taxi = b.call("invoice.addEntry", json!({ "invoice": v["guid"], "description": "Taxi", "quantity": "1", "price": "25", "account": "Expenses", "payment": "card" }));
    assert_eq!(taxi["payment"], "card");
    assert_eq!(taxi["billTaxTable"], Value::Null, "vouchers get no owner tax defaults");
    assert_eq!(b.err("invoice.addEntry", json!({ "invoice": v["guid"], "taxTable": "GST+Levy" })).code, codes::INVALID);
    let lunch = b.call("invoice.addEntry", json!({ "invoice": v["guid"], "description": "Lunch", "quantity": "1", "price": "15", "account": "Expenses" }));
    assert_eq!(lunch["payment"], "cash");
    let detail = b.call("invoice.get", json!({ "invoice": v["guid"] }));
    assert_eq!(amount(&detail["chargeAmount"]), (5, 2));
    assert_eq!(detail["entryCount"], 2);
}

#[test]
fn posted_documents_are_frozen_and_unposted_ones_can_be_deleted() {
    let mut b = open("synthetic/business-editable.gnucash");
    let posted = b.call("invoice.list", json!({ "type": "invoice", "posted": true }))["invoices"][0].clone();
    assert_eq!(posted["id"], "000001");
    assert_eq!(b.err("invoice.update", json!({ "invoice": posted["guid"], "notes": "x" })).code, codes::CONFLICT);
    assert_eq!(b.err("invoice.delete", json!({ "invoice": posted["guid"] })).code, codes::CONFLICT);
    assert_eq!(b.err("invoice.addEntry", json!({ "invoice": posted["guid"] })).code, codes::CONFLICT);
    let line = b.call("invoice.get", json!({ "invoice": posted["guid"] }))["entries"][0]["guid"].clone();
    assert_eq!(b.err("invoice.updateEntry", json!({ "entry": line, "quantity": "1" })).code, codes::CONFLICT);
    assert_eq!(b.err("invoice.removeEntry", json!({ "entry": line })).code, codes::CONFLICT);
    let off = b.call("invoice.update", json!({ "invoice": posted["guid"], "active": false }));
    assert_eq!(off["active"], false, "a posted document can still be deactivated");

    let acme = b.guid_of("customer.list", "customers", "000001");
    let inv = b.call("invoice.create", json!({ "owner": acme }));
    let terms_before = b.call("billTerm.get", json!({ "billTerm": "Net30" }))["refcount"].as_i64().unwrap();
    let table_before = b.call("taxTable.get", json!({ "taxTable": "GST+Levy" }))["refcount"].as_i64().unwrap();
    b.call("invoice.addEntry", json!({ "invoice": inv["guid"], "description": "A" }));
    b.call("invoice.addEntry", json!({ "invoice": inv["guid"], "description": "B" }));
    let entries_before = b.call("book.info", json!({}))["counts"].clone();
    b.call("invoice.delete", json!({ "invoice": inv["guid"] }));
    assert_eq!(b.err("invoice.get", json!({ "invoice": inv["guid"] })).code, codes::NOT_FOUND);
    assert_eq!(b.call("billTerm.get", json!({ "billTerm": "Net30" }))["refcount"].as_i64().unwrap(), terms_before - 1);
    assert_eq!(b.call("taxTable.get", json!({ "taxTable": "GST+Levy" }))["refcount"].as_i64().unwrap(), table_before, "both lines released the table");
    assert_eq!(b.call("book.info", json!({}))["counts"]["invoices"].as_i64().unwrap(), entries_before["invoices"].as_i64().unwrap() - 1);
}

#[test]
fn orders_hold_lines_and_close() {
    let mut b = open("synthetic/business-editable.gnucash");
    let acme = b.guid_of("customer.list", "customers", "000001");
    let o = b.call("order.create", json!({ "owner": acme, "reference": "REF-9", "notes": "rush" }));
    assert_eq!(o["id"], "000001");
    assert_eq!(o["closed"], false);
    assert_eq!(b.err("order.close", json!({ "order": o["guid"] })).code, codes::INVALID, "no lines yet");
    let line = b.call("order.addEntry", json!({ "order": o["guid"], "description": "Widgets", "quantity": "10", "price": "3" }));
    assert_eq!(line["order"], o["guid"]);
    let inv = b.call("invoice.create", json!({ "owner": acme }));
    let both = b.call("invoice.addEntry", json!({ "invoice": inv["guid"], "order": o["guid"], "description": "Shipped widgets" }));
    assert_eq!((both["invoice"].clone(), both["order"].clone()), (inv["guid"].clone(), o["guid"].clone()));
    b.call("invoice.delete", json!({ "invoice": inv["guid"] }));
    assert_eq!(b.call("order.get", json!({ "order": o["guid"] }))["entryCount"], 2, "order lines survive the invoice");

    let paper = b.guid_of("vendor.list", "vendors", "000001");
    assert_eq!(b.err("order.update", json!({ "order": o["guid"], "owner": paper })).code, codes::CONFLICT);
    let o = b.call("order.update", json!({ "order": o["guid"], "notes": "updated" }));
    assert_eq!(o["notes"], "updated");
    let closed = b.call("order.close", json!({ "order": o["guid"], "date": "2024-07-02T09:00:00Z" }));
    assert_eq!((closed["closed"].as_bool(), closed["dateClosed"].as_str()), (Some(true), Some("2024-07-02T09:00:00Z")));
    assert_eq!(b.err("order.addEntry", json!({ "order": o["guid"] })).code, codes::CONFLICT);
    assert_eq!(b.err("order.close", json!({ "order": o["guid"] })).code, codes::CONFLICT);
    assert_eq!(b.call("order.list", json!({ "closed": true }))["orders"].as_array().unwrap().len(), 1);
    let jane = b.guid_of("employee.list", "employees", "000001");
    assert_eq!(b.err("order.create", json!({ "owner": jane })).code, codes::INVALID);

    // A vendor's order still uses the invoice side, as GnuCash's order ledger does; a bill's line cannot join it.
    let vo = b.call("order.create", json!({ "owner": paper }));
    let line = b.call("order.addEntry", json!({ "order": vo["guid"], "quantity": "2", "price": "7", "discount": "1" }));
    assert_eq!((line["invoicePrice"]["num"].as_str(), line["billPrice"]["num"].as_str()), (Some("7"), Some("0")));
    assert_eq!(b.err("order.addEntry", json!({ "order": vo["guid"], "billable": true })).code, codes::INVALID);
    let bill = b.call("invoice.create", json!({ "owner": paper }));
    assert_eq!(b.err("invoice.addEntry", json!({ "invoice": bill["guid"], "order": vo["guid"] })).code, codes::INVALID);
}

#[test]
fn documents_survive_save_and_reopen() {
    let mut b = open("synthetic/business-editable.gnucash");
    let acme = b.guid_of("customer.list", "customers", "000001");
    let inv = b.call("invoice.create", json!({ "owner": acme, "notes": "kept" }));
    b.call("invoice.addEntry", json!({ "invoice": inv["guid"], "description": "Kept line", "quantity": "1", "price": "9.99", "account": "Income" }));
    let o = b.call("order.create", json!({ "owner": acme }));
    b.call("order.addEntry", json!({ "order": o["guid"], "description": "Order line" }));
    b.call("book.close", json!({ "save": true }));
    let path = b.dir.path().join("book.gnucash").display().to_string();
    b.call("book.open", json!({ "path": path }));
    let back = b.call("invoice.get", json!({ "invoice": inv["guid"] }));
    assert_eq!((back["notes"].as_str(), back["entries"][0]["description"].as_str()), (Some("kept"), Some("Kept line")));
    assert_eq!(b.call("order.get", json!({ "order": o["guid"] }))["entries"][0]["description"], "Order line");
}
