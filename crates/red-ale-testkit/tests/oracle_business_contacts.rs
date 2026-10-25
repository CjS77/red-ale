//! Customers, vendors, employees, jobs, tax tables and billing terms made through the API read back the same in
//! GnuCash 5.17, and GnuCash numbers and taxes with them as red-ale does.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, dispatcher, open, oracle, oracle_check};
use serde_json::{Value, json};

const BOOK: &str = "synthetic/business-editable.gnucash";

fn first_guid(d: &mut red_ale_rpc::Dispatcher, method: &str, list: &str) -> Value {
    call(d, method, &json!({}))[list][0]["guid"].clone()
}

/// What GnuCash's own counters give next for customers and vendors (the bindings expose only those two of the
/// contact counters), on a copy of `book`.
fn gnucash_next_ids(book: &TempBook) -> Vec<String> {
    let copy = book.dir.path().join("copy.gnucash");
    std::fs::copy(&book.path, &copy).unwrap();
    let code = r#"
import sys
from gnucash import Session, SessionOpenMode
from gnucash import gnucash_core_c as c
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_NORMAL_OPEN)
try:
    print(c.gncCustomerNextID(s.book.get_instance()))
    print(c.gncVendorNextID(s.book.get_instance()))
finally:
    s.end()
"#;
    oracle::run_code(code, &[copy.to_str().unwrap()]).unwrap().lines().map(str::to_owned).collect()
}

#[test]
fn contacts_read_back_in_gnucash() {
    let (book, mut d) = open(BOOK);
    let c = call(
        &mut d,
        "customer.create",
        &json!({
            "name": "Widget Co", "currency": "EUR", "terms": "Prox15", "taxTable": "GST+Levy", "taxTableOverride": true,
            "taxIncluded": "yes", "discount": "2.5", "credit": "1500.00", "notes": "new",
            "addr": { "name": "Accounts", "addr1": "1 High St", "addr2": "Floor 2", "addr3": "Town", "addr4": "Shire", "phone": "1", "fax": "2", "email": "a@b" },
            "shipAddr": { "name": "Dock", "addr1": "9 Pier" }
        }),
    );
    call(&mut d, "customer.update", &json!({ "customer": c["guid"], "active": false }));
    let acme = first_guid(&mut d, "customer.list", "customers");
    call(&mut d, "customer.update", &json!({ "customer": acme, "terms": null, "taxTable": null }));
    let v = call(&mut d, "vendor.create", &json!({ "name": "Ink Ltd", "currency": "USD", "terms": "Net30", "addr": { "name": "Ink AP" } }));
    call(&mut d, "employee.create", &json!({ "name": "Sam Lee", "username": "sam", "currency": "EUR", "creditCardAccount": "Liabilities:Card", "workday": "7.5", "rate": "45.50", "language": "fr", "acl": "x" }));
    call(&mut d, "job.create", &json!({ "name": "Catalogue", "owner": v["guid"], "rate": "75", "reference": "PO-1" }));
    call(&mut d, "job.create", &json!({ "name": "Rebuild", "owner": c["guid"], "id": "J-9" }));
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}

#[test]
fn ids_continue_gnucash_counters_and_honour_counter_formats() {
    let book = TempBook::copy(BOOK);
    let text = std::fs::read_to_string(&book.path).unwrap();
    let formats = "<slot>\n    <slot:key>counter_formats</slot:key>\n    <slot:value type=\"frame\">\n      <slot>\n        <slot:key>gncCustomer</slot:key>\n        <slot:value type=\"string\">C-%04li%%</slot:value>\n      </slot>\n      <slot>\n        <slot:key>gncVendor</slot:key>\n        <slot:value type=\"string\">%d</slot:value>\n      </slot>\n    </slot:value>\n  </slot>\n  <slot>\n    <slot:key>counters</slot:key>";
    std::fs::write(&book.path, text.replacen("<slot>\n    <slot:key>counters</slot:key>", formats, 1)).unwrap();
    let expected = gnucash_next_ids(&book);
    assert_eq!(expected, ["C-0002%", "000002"], "an invalid format falls back to the default");

    let mut rpc = dispatcher();
    call(&mut rpc, "book.open", &json!({ "path": book.path_str() }));
    let cust = call(&mut rpc, "customer.create", &json!({ "name": "A", "currency": "EUR" }));
    let vend = call(&mut rpc, "vendor.create", &json!({ "name": "B", "currency": "EUR" }));
    let emp = call(&mut rpc, "employee.create", &json!({ "name": "C", "currency": "EUR" }));
    let job = call(&mut rpc, "job.create", &json!({ "name": "D", "owner": cust["guid"] }));
    let ord = call(&mut rpc, "order.create", &json!({ "owner": cust["guid"] }));
    let ours: Vec<&str> = [&cust, &vend].iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(ours, expected);
    // Employees, jobs and orders use the same counter code; the fixture made its employee without the counter.
    let rest: Vec<&str> = [&emp, &job, &ord].iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(rest, ["000001", "000001", "000001"]);
    // And GnuCash, reading the counters red-ale saved, carries on from them.
    let diffs = oracle_check(&mut rpc, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    assert_eq!(gnucash_next_ids(&book), ["C-0003%", "000003"]);
}

/// For every counter, GnuCash's last number, the format it numbers with and the ID it gives next
/// (`qof_book_get_counter`, `qof_book_get_counter_format`, `qof_book_increment_and_format_counter`).
const GNUCASH_COUNTERS: &str = r#"
import ctypes, json, sys
from gnucash import Session, SessionOpenMode, _sw_core_utils
import gnc_engine as g
assert _sw_core_utils.gnc_version() == "5.17"
g.engine.qof_book_get_counter.restype = ctypes.c_int64
g.engine.qof_book_get_counter.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
g.engine.qof_book_get_counter_format.restype = ctypes.c_char_p
g.engine.qof_book_get_counter_format.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
g.engine.qof_book_increment_and_format_counter.restype = ctypes.c_char_p
g.engine.qof_book_increment_and_format_counter.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_NORMAL_OPEN)
try:
    book = s.book
    out = []
    for name in sys.argv[2:]:
        key = name.encode()
        out.append({"name": name, "last": g.engine.qof_book_get_counter(g.ptr(book), key),
                    "normalizedFormat": g.engine.qof_book_get_counter_format(g.ptr(book), key).decode(),
                    "next": g.engine.qof_book_increment_and_format_counter(g.ptr(book), key).decode()})
    print(json.dumps(out))
finally:
    s.end()
"#;

fn gnucash_counters(book: &TempBook, names: &[&str]) -> Value {
    let copy = book.dir.path().join("counters.gnucash");
    std::fs::copy(&book.path, &copy).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("counters.py");
    std::fs::write(&script, GNUCASH_COUNTERS).unwrap();
    std::fs::copy(oracle::scripts_dir().join("gnc_engine.py"), dir.path().join("gnc_engine.py")).unwrap();
    let mut args = vec![copy.to_str().unwrap()];
    args.extend_from_slice(names);
    serde_json::from_str(&oracle::run_script(&script, &args).unwrap()).unwrap()
}

#[test]
fn counters_and_formats_set_by_red_ale_number_the_same_in_gnucash() {
    let (book, mut d) = open(BOOK);
    let settings = [
        json!({ "name": "gncCustomer", "last": 41, "format": "C-%04li" }),
        json!({ "name": "gncEmployee", "last": 0 }),
        json!({ "name": "gncInvoice", "last": 999, "format": "INV %lli/2024%%" }),
        json!({ "name": "gncBill", "format": "%-8I64i|" }),
        json!({ "name": "gncExpVoucher", "last": 7, "format": "%+.3li" }),
        json!({ "name": "gncJob", "last": 999_999_999 }),
        json!({ "name": "gncOrder", "last": 12, "format": "%5.4li" }),
        json!({ "name": "gncVendor", "last": 3, "format": "%#'0 9li" }),
    ];
    settings.iter().for_each(|p| {
        call(&mut d, "counter.set", p);
    });
    let ours = call(&mut d, "counter.list", &json!({}))["counters"].clone();
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let names: Vec<&str> = ours.as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    let theirs = gnucash_counters(&book, &names);
    let ours: Vec<Value> = ours
        .as_array()
        .unwrap()
        .iter()
        .map(|c| json!({ "name": c["name"], "last": c["last"], "normalizedFormat": c["normalizedFormat"], "next": c["next"] }))
        .collect();
    assert_eq!(Value::Array(ours), theirs);
}

#[test]
fn tax_tables_and_terms_read_back_and_gnucash_taxes_with_them() {
    let (book, mut d) = open(BOOK);
    call(
        &mut d,
        "taxTable.create",
        &json!({ "name": "VAT", "entries": [
            { "account": "Liabilities:Tax", "amount": "20", "type": "percent" },
            { "account": "Liabilities:Levy", "amount": "1.50", "type": "value" }
        ]}),
    );
    call(&mut d, "taxTable.create", &json!({ "name": "Gone", "entries": [{ "account": "Liabilities:Tax", "amount": "1", "type": "percent" }] }));
    call(&mut d, "taxTable.delete", &json!({ "taxTable": "Gone" }));
    call(&mut d, "taxTable.update", &json!({ "taxTable": "GST+Levy", "entries": [{ "account": "Liabilities:Tax", "amount": "12.5", "type": "percent" }] }));
    let p = call(&mut d, "billTerm.create", &json!({ "name": "EOM", "description": "end of month", "type": "proximo", "dueDays": 31, "discountDays": 5, "discount": "1", "cutoff": -3 }));
    call(&mut d, "billTerm.create", &json!({ "name": "Net60", "type": "days", "dueDays": 60 }));
    call(&mut d, "billTerm.update", &json!({ "billTerm": "Net60", "dueDays": 45, "discountDays": 10, "discount": "2" }));
    let v = call(&mut d, "vendor.create", &json!({ "name": "Ink", "currency": "EUR", "terms": p["guid"], "taxTable": "VAT" }));
    call(&mut d, "customer.create", &json!({ "name": "Net60 user", "currency": "EUR", "terms": "Net60", "taxTable": "VAT" }));
    call(&mut d, "vendor.update", &json!({ "vendor": v["guid"], "taxTable": null }));
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));

    // GnuCash taxes a line of 100 with the tables red-ale wrote: VAT 20 % + 1.50, GST+Levy now 12.5 %.
    let code = r#"
import sys
from gnucash import Session, SessionOpenMode, GncNumeric
from gnucash.gnucash_business import Entry, TaxTable
from gnucash import gnucash_core_c as c
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    for name in ("VAT", "GST+Levy"):
        t = TaxTable(instance=c.gncTaxTableLookupByName(s.book.get_instance(), name))
        e = Entry(s.book)
        e.SetQuantity(GncNumeric(1, 1))
        e.SetInvPrice(GncNumeric(100, 1))
        e.SetInvTaxable(True)
        e.SetInvTaxTable(t)
        v = e.GetDocTaxValue(False, True, False)
        num, den = (v.num(), v.denom()) if callable(v.num) else (v.num, v.denom)
        print(f"{name} {num}/{den}")
finally:
    s.end()
"#;
    let out = oracle::run_code(code, &[book.path.to_str().unwrap()]).unwrap();
    let taxes: Vec<(String, String)> = out.lines().map(|l| l.rsplit_once(' ').map(|(a, b)| (a.to_owned(), b.to_owned())).unwrap()).collect();
    let reduce = |f: &str| {
        let (n, dn) = f.split_once('/').unwrap();
        let (n, dn): (i64, i64) = (n.parse().unwrap(), dn.parse().unwrap());
        n * 1000 / dn
    };
    assert_eq!(taxes.iter().map(|(n, v)| (n.as_str(), reduce(v))).collect::<Vec<_>>(), [("VAT", 21_500), ("GST+Levy", 12_500)]);
}
