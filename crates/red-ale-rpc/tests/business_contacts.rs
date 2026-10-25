//! Customers, vendors, employees, jobs, tax tables and billing terms through the dispatcher.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

struct Book {
    _dir: tempfile::TempDir,
    d: Dispatcher,
}

fn open(name: &str) -> Book {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("book.gnucash");
    std::fs::copy(format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR")), &dest).unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(1_719_835_200), 0x00ed_0000_0000_0000_0000_0000_0000_0000))));
    d.call("book.open", json!({ "path": dest.display().to_string() })).unwrap();
    Book { _dir: dir, d }
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
fn customers_are_created_numbered_listed_and_deactivated() {
    let mut b = open("synthetic/business-editable.gnucash");
    let c = b.call(
        "customer.create",
        json!({
            "name": "Widget Co", "currency": "EUR", "addr": { "name": "Accounts", "addr1": "1 High St", "email": "a@w.example" },
            "terms": "Net30", "taxTable": "GST+Levy", "taxTableOverride": true, "taxIncluded": "no", "discount": "2.5", "credit": "1000"
        }),
    );
    assert_eq!(c["id"], "000002", "the fixture's gncCustomer counter is 1");
    assert_eq!(c["addr"]["addr1"], "1 High St");
    assert_eq!(c["currency"], "CURRENCY::EUR");
    assert_eq!(amount(&c["discount"]), (5, 2));
    let guid = c["guid"].as_str().unwrap().to_owned();

    let table = b.call("taxTable.get", json!({ "taxTable": "GST+Levy" }));
    assert_eq!(table["refcount"], 2, "the fixture's customer and the new one");
    let term = b.call("billTerm.get", json!({ "billTerm": "Net30" }));
    assert_eq!(term["refcount"], 2);

    let given = b.call("customer.create", json!({ "name": "Given", "currency": "EUR", "id": "C-77" }));
    assert_eq!(given["id"], "C-77");

    let list = b.call("customer.list", json!({ "name": "widget" }));
    assert_eq!(list["customers"].as_array().unwrap().len(), 1);

    let updated = b.call("customer.update", json!({ "customer": guid, "active": false, "terms": null, "addr": { "phone": "555" } }));
    assert_eq!(updated["active"], false);
    assert_eq!(updated["terms"], Value::Null);
    assert_eq!(updated["addr"]["addr1"], "1 High St", "other address fields are kept");
    assert_eq!(updated["addr"]["phone"], "555");
    assert_eq!(b.call("billTerm.get", json!({ "billTerm": "Net30" }))["refcount"], 1);
    assert_eq!(b.call("customer.list", json!({ "activeOnly": true }))["customers"].as_array().unwrap().len(), 2);

    assert_eq!(b.err("customer.create", json!({ "currency": "EUR" })).code, codes::INVALID, "a name is required");
    assert_eq!(b.err("customer.create", json!({ "name": "X" })).code, codes::INVALID, "a currency is required");
    assert_eq!(b.err("customer.create", json!({ "name": "X", "currency": "EUR", "discount": "101" })).code, codes::INVALID);
    assert_eq!(b.err("customer.create", json!({ "name": "X", "currency": "EUR", "discount": 5 })).code, codes::INVALID_PARAMS);
    assert_eq!(b.err("customer.create", json!({ "name": "X", "currency": "EUR", "terms": "Nope" })).code, codes::NOT_FOUND);
    assert_eq!(b.err("customer.update", json!({ "customer": guid, "id": "" })).code, codes::INVALID);
    assert_eq!(b.err("customer.delete", json!({ "customer": guid })).code, codes::METHOD_NOT_FOUND, "contacts are deactivated, not deleted");
}

#[test]
fn vendors_employees_and_jobs() {
    let mut b = open("synthetic/business-editable.gnucash");
    let v = b.call("vendor.create", json!({ "name": "Ink Ltd", "currency": "EUR", "terms": "Prox15", "taxIncluded": "yes" }));
    assert_eq!(v["id"], "000002");
    assert_eq!(v["taxIncluded"], "yes");

    let e = b.call("employee.create", json!({ "name": "Sam Lee", "currency": "EUR", "creditCardAccount": "Liabilities:Card", "rate": "45.50" }));
    assert_eq!(e["name"], "Sam Lee");
    assert_eq!(e["addr"]["name"], "Sam Lee");
    assert_eq!(e["id"], "000001", "the fixture made its employee without the counter");
    assert_eq!(b.err("employee.create", json!({ "name": "X", "currency": "EUR", "creditCardAccount": "Assets:Bank" })).code, codes::INVALID);
    let e = b.call("employee.update", json!({ "employee": e["guid"], "creditCardAccount": null, "active": false }));
    assert_eq!(e["creditCardAccount"], Value::Null);

    let j = b.call("job.create", json!({ "name": "Catalogue", "owner": v["guid"], "rate": "75", "reference": "PO-1" }));
    assert_eq!(j["owner"], json!({ "kind": "vendor", "guid": v["guid"] }));
    assert_eq!(amount(&j["rate"]), (75, 1));
    assert_eq!(b.err("job.create", json!({ "name": "Bad", "owner": e["guid"] })).code, codes::INVALID);
    assert_eq!(b.err("job.create", json!({ "name": "Bad" })).code, codes::INVALID);
    let jobs = b.call("job.list", json!({ "owner": v["guid"] }));
    assert_eq!(jobs["jobs"].as_array().unwrap().len(), 1);
    let j = b.call("job.update", json!({ "job": j["guid"], "rate": "0" }));
    assert_eq!(amount(&j["rate"]), (0, 1));

    // The fixture's job has an invoice, so it cannot move to another owner.
    let fixture_job = b.call("job.list", json!({ "id": "JOB-1" }))["jobs"][0]["guid"].clone();
    assert_eq!(b.err("job.update", json!({ "job": fixture_job, "owner": v["guid"] })).code, codes::CONFLICT);
    let info = b.call("book.info", json!({}));
    assert!(info["features"].as_array().unwrap().iter().any(|f| f["name"] == "Extra data in addresses, jobs or invoice entries"));
}

#[test]
fn tax_tables_keep_their_family_and_refcounts() {
    let mut b = open("synthetic/business-editable.gnucash");
    let t = b.call(
        "taxTable.create",
        json!({ "name": "VAT", "entries": [
            { "account": "Liabilities:Tax", "amount": "20", "type": "percent" },
            { "account": "Liabilities:Levy", "amount": "1.50", "type": "value" }
        ]}),
    );
    assert_eq!(t["entries"][0]["accountName"], "Liabilities:Levy", "sorted by account full name");
    assert_eq!(t["refcount"], 0);
    assert_eq!(b.err("taxTable.create", json!({ "name": "VAT", "entries": [{ "account": "Liabilities:Tax", "amount": "1", "type": "percent" }] })).code, codes::CONFLICT);
    assert_eq!(b.err("taxTable.create", json!({ "name": "None", "entries": [] })).code, codes::INVALID);
    assert_eq!(b.err("taxTable.create", json!({ "name": "Big", "entries": [{ "account": "Liabilities:Tax", "amount": "120", "type": "percent" }] })).code, codes::INVALID);

    let all = b.call("taxTable.list", json!({ "includeFrozen": true }))["taxTables"].as_array().unwrap().len();
    let visible = b.call("taxTable.list", json!({}))["taxTables"].as_array().unwrap().len();
    assert_eq!(all, visible + 1, "the fixture's posted invoice froze one copy of GST+Levy");

    let gst = b.call("taxTable.get", json!({ "taxTable": "GST+Levy" }));
    let child = gst["child"].as_str().unwrap().to_owned();
    assert_eq!(b.err("taxTable.update", json!({ "taxTable": child, "name": "Changed" })).code, codes::CONFLICT);
    assert_eq!(b.err("taxTable.delete", json!({ "taxTable": "GST+Levy" })).code, codes::CONFLICT, "a customer uses it");
    let renamed = b.call("taxTable.update", json!({ "taxTable": "GST+Levy", "name": "GST and Levy" }));
    assert_eq!(renamed["child"], json!(child), "renaming keeps the frozen copy");
    let changed = b.call("taxTable.update", json!({ "taxTable": "GST and Levy", "entries": [{ "account": "Liabilities:Tax", "amount": "12", "type": "percent" }] }));
    assert_eq!(changed["child"], Value::Null, "new lines drop it");
    assert_eq!(b.call("taxTable.get", json!({ "taxTable": child }))["entries"].as_array().unwrap().len(), 2, "the frozen copy is untouched");

    b.call("taxTable.delete", json!({ "taxTable": "VAT" }));
    assert_eq!(b.err("taxTable.get", json!({ "taxTable": "VAT" })).code, codes::NOT_FOUND);
}

#[test]
fn billing_terms_are_created_changed_and_deleted_when_unused() {
    let mut b = open("synthetic/business-editable.gnucash");
    let t = b.call("billTerm.create", json!({ "name": "Net60", "description": "60 days", "type": "days", "dueDays": 60, "discountDays": 10, "discount": "1.5" }));
    assert_eq!(t["type"], "days");
    assert_eq!(amount(&t["discount"]), (3, 2));
    let p = b.call("billTerm.create", json!({ "name": "EOM", "type": "proximo", "dueDays": 31, "cutoff": -3 }));
    assert_eq!(p["cutoff"], -3);
    assert_eq!(b.err("billTerm.create", json!({ "name": "Net60", "type": "days" })).code, codes::CONFLICT);
    assert_eq!(b.err("billTerm.create", json!({ "name": "Bad", "type": "days", "dueDays": 5, "discountDays": 10 })).code, codes::INVALID);
    assert_eq!(b.err("billTerm.create", json!({ "name": "Bad", "type": "proximo", "dueDays": 0 })).code, codes::INVALID);
    let t = b.call("billTerm.update", json!({ "billTerm": "Net60", "dueDays": 45 }));
    assert_eq!((t["dueDays"].as_i64(), t["discountDays"].as_i64(), t["type"].as_str()), (Some(45), Some(10), Some("days")));
    let net30 = b.call("billTerm.get", json!({ "billTerm": "Net30" }));
    assert!(net30["child"].is_string(), "the fixture's posted invoice froze Net30");
    let changed = b.call("billTerm.update", json!({ "billTerm": "Net30", "description": "thirty" }));
    assert_eq!(changed["child"], Value::Null);
    assert_eq!(b.err("billTerm.delete", json!({ "billTerm": "Net30" })).code, codes::CONFLICT);
    b.call("billTerm.delete", json!({ "billTerm": "Net60" }));
    let names: Vec<String> = b.call("billTerm.list", json!({}))["billTerms"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect();
    assert_eq!(names, ["EOM", "Net30", "Prox15"]);
}

#[test]
fn ids_continue_from_the_book_counters() {
    let mut b = open("piecash/invoices.gnucash");
    let v = b.call("vendor.create", json!({ "name": "Next", "currency": "EUR" }));
    assert_eq!(v["id"], "000002");
    let j = b.call("job.create", json!({ "name": "Third", "owner": v["guid"] }));
    assert_eq!(j["id"], "000003");
}

#[test]
fn a_read_only_book_refuses_changes() {
    let mut b = open("synthetic/business.gnucash");
    assert_eq!(b.call("book.info", json!({}))["readOnly"], true, "it declares credit notes");
    assert_eq!(b.err("customer.create", json!({ "name": "X", "currency": "EUR" })).code, codes::READ_ONLY);
    assert_eq!(b.call("customer.list", json!({}))["customers"].as_array().unwrap().len(), 1);
}

#[test]
fn counters_are_listed_set_and_used_for_new_ids() {
    let mut b = open("synthetic/business-editable.gnucash");
    let list = b.call("counter.list", json!({}));
    let names: Vec<&str> = list["counters"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["gncCustomer", "gncEmployee", "gncInvoice", "gncBill", "gncExpVoucher", "gncJob", "gncOrder", "gncVendor"]);
    let invoice = &list["counters"][2];
    assert_eq!(invoice["last"], 2);
    assert_eq!(invoice["next"], "000003");
    assert_eq!(invoice["formatValid"], false);
    assert_eq!(invoice["normalizedFormat"], "%.6li");

    let set = b.call("counter.set", json!({ "name": "gncCustomer", "last": 99, "format": "CUST-%05lli" }));
    assert_eq!(set["next"], "CUST-00100");
    assert_eq!(set["normalizedFormat"], "CUST-%05li");
    assert_eq!(b.call("customer.create", json!({ "name": "New", "currency": "EUR" }))["id"], "CUST-00100");
    let after = b.call("counter.set", json!({ "name": "gncCustomer", "format": null }));
    assert_eq!((after["last"].clone(), after["format"].clone(), after["next"].clone()), (json!(100), Value::Null, json!("000101")));

    let bad = b.err("counter.set", json!({ "name": "gncCustomer", "format": "%d" }));
    assert_eq!(bad.code, codes::INVALID);
    assert!(bad.message.contains("doesn't contain requested format specifier"), "{}", bad.message);
    assert_eq!(b.err("counter.set", json!({ "name": "gncThing", "last": 1 })).code, codes::INVALID);
    assert_eq!(b.err("counter.set", json!({ "name": "gncBill", "last": 1_000_000_000 })).code, codes::INVALID);
}
