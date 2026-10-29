//! The business book options against GnuCash 5.17 (`oracle/business_options.py`): GnuCash reads the Business tab's
//! strings, the Tax Number and the Default Invoice Report red-ale writes, on XML and SQLite, and writes the same slots
//! for the same values (`qof_book_set_string_option`, `qof_book_set_default_invoice_report`,
//! `5.17:libgnucash/engine/qofbook.cpp:1043-1087`, `:1194-1205`).

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, dispatcher, oracle};
use serde_json::{Value, json};

const PRINTABLE: &str = "5123a759ceb9483abf2182d01c140e8d";

fn script(name: &str, args: &[&str]) -> String {
    oracle::check_version().unwrap();
    oracle::run_script(&oracle::scripts_dir().join(name), args).unwrap_or_else(|e| panic!("oracle: {e}"))
}

/// Every business option set, with text that needs escaping in XML and SQL.
fn options() -> Value {
    json!({
        "companyName": "Acme & Sons <Ltd>", "companyAddress": "1 High St\nSpringfield\n'Quoted' \"too\"",
        "companyContactPerson": "Zoë Smith", "companyPhoneNumber": "555-0100", "companyFaxNumber": "555-0101",
        "companyEmailAddress": "acme@example.com", "companyWebsiteUrl": "https://example.com/a?b=c&d",
        "companyId": "Tax-ID: 00-000000", "taxNumber": "GB 123 456",
        "defaultInvoiceReport": { "guid": PRINTABLE, "name": "Printable Invoice" }
    })
}

/// The slot paths GnuCash reads, with the wire name of each.
const PATHS: [(&str, &str); 9] = [
    ("options/Business/Company Name", "companyName"),
    ("options/Business/Company Address", "companyAddress"),
    ("options/Business/Company Contact Person", "companyContactPerson"),
    ("options/Business/Company Phone Number", "companyPhoneNumber"),
    ("options/Business/Company Fax Number", "companyFaxNumber"),
    ("options/Business/Company Email Address", "companyEmailAddress"),
    ("options/Business/Company Website URL", "companyWebsiteUrl"),
    ("options/Business/Company ID", "companyId"),
    ("options/Tax/Tax Number", "taxNumber"),
];

fn set_and_save(book: &TempBook, params: &Value) {
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    call(&mut d, "book.setOptions", params);
    call(&mut d, "book.close", &json!({ "save": true }));
}

fn gnucash_reads(book: &TempBook) -> Value {
    serde_json::from_str(&script("business_options.py", &["read", &book.path_str()])).unwrap()
}

fn gnucash_reads_what_red_ale_writes(fixture: &str) {
    let book = TempBook::copy(fixture);
    set_and_save(&book, &options());
    let read = gnucash_reads(&book);
    let expected = options();
    PATHS
        .iter()
        .for_each(|(path, wire)| assert_eq!(read["strings"][path], expected[wire], "{fixture}: {path}"));
    assert_eq!(read["defaultInvoiceReport"], expected["defaultInvoiceReport"], "{fixture}");
    assert_eq!(
        read["unknownFeatures"],
        Value::Null,
        "{fixture}: GnuCash would warn about the book's features"
    );
}

#[test]
fn gnucash_reads_the_business_options_red_ale_writes_on_xml() {
    gnucash_reads_what_red_ale_writes("synthetic/rich.gnucash");
}

#[test]
fn gnucash_reads_the_business_options_red_ale_writes_on_sqlite() {
    gnucash_reads_what_red_ale_writes("sqlite/synthetic/rich.gnucash");
}

#[test]
fn red_ale_writes_the_slots_gnucash_writes() {
    let (gnucash, ours) = (TempBook::copy("synthetic/rich.gnucash"), TempBook::copy("synthetic/rich.gnucash"));
    let expected = options();
    let mut by_path: serde_json::Map<String, Value> = PATHS.iter().map(|(p, w)| ((*p).to_owned(), expected[w].clone())).collect();
    by_path.insert("defaultInvoiceReport".into(), json!([PRINTABLE, "Printable Invoice"]));
    script(
        "business_options.py",
        &["set", &gnucash.path_str(), &Value::Object(by_path).to_string()],
    );
    set_and_save(&ours, &expected);
    let slots = |b: &TempBook| -> Value { serde_json::from_str(&script("book_options.py", &["slots", &b.path_str()])).unwrap() };
    let theirs = slots(&gnucash);
    assert_eq!(
        theirs["options"]["Business"]["Default Invoice Report"],
        json!(["string", format!("{PRINTABLE}/Printable Invoice")])
    );
    assert_eq!(slots(&ours), theirs);
}

#[test]
fn cleared_options_read_as_unset_in_gnucash() {
    let book = TempBook::copy("synthetic/rich.gnucash");
    set_and_save(&book, &options());
    let cleared: serde_json::Map<String, Value> = PATHS.iter().map(|(_, w)| ((*w).to_owned(), json!(""))).collect();
    let mut params = Value::Object(cleared);
    params["defaultInvoiceReport"] = Value::Null;
    set_and_save(&book, &params);
    let read = gnucash_reads(&book);
    PATHS.iter().for_each(|(path, _)| {
        let value = &read["strings"][path];
        assert!(value == "" || value.is_null(), "{path}: {value}");
    });
    assert_eq!(read["defaultInvoiceReport"], json!({ "guid": null, "name": null }));
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    let ours = call(&mut d, "book.options", &json!({}));
    PATHS.iter().for_each(|(_, wire)| assert_eq!(ours[wire], "", "{wire}"));
    assert_eq!(ours["defaultInvoiceReport"], Value::Null);
}
