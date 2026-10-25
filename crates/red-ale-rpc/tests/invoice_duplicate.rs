//! Duplicating business documents through the dispatcher, on a copy of `synthetic/business-editable.gnucash`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

/// The posted customer invoice 000001, with two lines and a billing term.
const POSTED_INVOICE: &str = "aa210b7131de44c48b8513cd27c00e95";
/// The posted expense voucher 000001.
const VOUCHER: &str = "645f06f442be44ecaaee148b5233ffdf";

fn open() -> (tempfile::TempDir, Dispatcher) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(format!("{}/../../fixtures/synthetic/business-editable.gnucash", env!("CARGO_MANIFEST_DIR")), &path).unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(1_719_835_200), 1 << 100))));
    d.call("book.open", json!({ "path": path.display().to_string() })).unwrap();
    (dir, d)
}

fn call(d: &mut Dispatcher, method: &str, params: Value) -> Value {
    let shown = params.to_string();
    d.call(method, params).unwrap_or_else(|e| panic!("{method} {shown}: {e} {:?}", e.data))
}

#[test]
fn a_posted_invoice_duplicates_into_an_unposted_one() {
    let (_dir, mut d) = open();
    call(&mut d, "invoice.update", json!({ "invoice": POSTED_INVOICE, "docLink": "inv/1.pdf" }));
    let old = call(&mut d, "invoice.get", json!({ "invoice": POSTED_INVOICE }));
    let new = call(&mut d, "invoice.duplicate", json!({ "invoice": POSTED_INVOICE, "date": "2024-08-15" }));
    assert_ne!(new["guid"], old["guid"]);
    assert_eq!(new["id"], "000003", "the next gncInvoice number");
    assert_eq!((new["posted"].as_bool(), &new["postedTransaction"], new["active"].as_bool()), (Some(false), &Value::Null, Some(true)));
    assert_eq!(new["dateOpened"], "2024-08-15T10:59:00Z");
    ["owner", "currency", "terms", "billingId", "notes", "docLink", "chargeAmount"].iter().for_each(|f| assert_eq!(new[f], old[f], "{f}"));
    let (lines, old_lines) = (new["entries"].as_array().unwrap(), old["entries"].as_array().unwrap());
    assert_eq!(lines.len(), old_lines.len());
    lines.iter().zip(old_lines).for_each(|(n, o)| {
        assert_eq!((&n["invoice"], &n["date"]), (&new["guid"], &json!("2024-08-15T10:59:00Z")));
        ["description", "quantity", "invoicePrice", "invoiceAccount", "invoiceTaxTable", "discount", "dateEntered"]
            .iter()
            .for_each(|f| assert_eq!(n[f], o[f], "{f}"));
    });
    let named = call(&mut d, "invoice.duplicate", json!({ "invoice": POSTED_INVOICE, "id": "COPY-1" }));
    assert_eq!(named["id"], "COPY-1");
    assert!(named["dateOpened"].as_str().unwrap().ends_with("T10:59:00Z"), "today at neutral time");
    let e = d.call("invoice.duplicate", json!({ "invoice": "00000000000000000000000000000001" })).unwrap_err();
    assert_eq!(e.code, codes::NOT_FOUND);
}

#[test]
fn voucher_lines_come_back_as_cash() {
    let (_dir, mut d) = open();
    let new = call(&mut d, "invoice.duplicate", json!({ "invoice": VOUCHER, "date": "2024-08-15" }));
    assert_eq!((new["type"].as_str(), new["id"].as_str()), (Some("expenseVoucher"), Some("000002")));
    let lines = new["entries"].as_array().unwrap();
    assert!(!lines.is_empty());
    assert!(lines.iter().all(|l| l["payment"] == "cash" && l["bill"] == new["guid"] && l["order"].is_null()));
}
