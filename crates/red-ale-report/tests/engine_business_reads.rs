//! The business reads of the query API (`design/reports.md` §5, v0.6) on `reports/transaction.gnucash`, which holds
//! `create-test-invoice-data`'s eight documents (`5.17:bindings/guile/test/test-engine-extras.scm:487-849`): inv-1
//! to inv-8 have GUIDs `3000…00` to `3000…07`. Expected values are read off the fixture's XML and, for inv-8's
//! figures, GnuCash 5.17's own reads of the same book (`design/notes/report-invoice.md` §1.8).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{GUID, Host, fixture, registry};
use red_ale_core::{Book, SlotValue};
use serde_json::json;

/// The log lines of `body`, run as a template's `render` on `book`.
fn log_of(book: Book, body: &str) -> Vec<String> {
    let script = format!(
        "fn templates() {{ [#{{ guid: \"{GUID}\", name: \"Reads\" }}] }}\nfn options(p, o, b) {{}}\n\
         fn render(params, book, opts, ctx) {{\n{body}\n#{{ title: \"reads\", sections: [] }}\n}}"
    );
    let reg = registry(&[("reads.rhai", &script)]);
    let result = Host::new(book).run(&reg, GUID, &json!({})).unwrap();
    result.log.into_iter().map(|l| l.text).collect()
}

fn transaction_book() -> Book {
    fixture("transaction.gnucash")
}

/// inv-8 (TEE:610-847): a customer invoice of eight entries in one order, with the frozen copy of the billing term,
/// posted on 1980-09-08 10:59 UTC and due at midnight before it (its `trans-date-due` slot), and paid in full: its lot
/// holds the payment and the posting, in file order. Its
/// totals are GnuCash's (subtotal 16368.17, tax 1111.01 to VAT-on-Sales, total 17479.18).
#[test]
fn a_customer_invoice_and_its_totals() {
    let body = r#"
        let inv = book.invoice("30000000000000000000000000000007");
        print(`${inv.id} ${inv.type} ${inv.isCustomerDocument} ${inv.isCreditNote} ${inv.posted}`);
        print(`${inv.datePosted} ${inv.dateDue} ${inv.currency.id}`);
        print(`${inv.terms.name} ${inv.terms.description} ${inv.terms.guid}`);
        print(`${inv.owner.kind} ${inv.owner.name} ${inv.owner.id} ${inv.owner.owner}`);
        let t = inv.totals();
        print(`${t.subtotal.to_decimal(2)} ${t.tax.to_decimal(2)} ${t.total.to_decimal(2)} ${t.taxes.map(|x| `${x.account.name}=${x.value.to_decimal(2)}`)}`);
        print(`${inv.entries().len()} ${inv.entries().map(|e| e.documentValue.to_decimal(2))}`);
        print(`${inv.entries().map(|e| e.order.reference)}`);
        print(`${inv.postedLot.splits().map(|s| s.amount.to_decimal(2))} ${inv.postedTransaction.guid}`);
    "#;
    let lines = log_of(transaction_book(), body);
    assert_eq!(
        lines,
        [
            "0008 invoice true false true",
            "337258740 337219200 CURRENCY::USD",
            "billterm-name billterm-desc 60000000000000000000000000000001",
            "customer cust-1-name cust-1-id ",
            "16368.17 1111.01 17479.18 [\"VAT-on-Sales=1111.01\"]",
            "8 [\"2133.25\", \"2061.96\", \"2133.25\", \"2061.96\", \"2133.25\", \"2133.25\", \"1851.95\", \"1859.30\"]",
            "[\"order-ref\", \"order-ref\", \"order-ref\", \"order-ref\", \"order-ref\", \"order-ref\", \"order-ref\", \"order-ref\"]",
            "[\"-17479.18\", \"17479.18\"] c0000000000000000000000000000042",
        ]
    );
}

/// inv-1's single entry (TEE:638-648): 2 at $3, taxable with no table, no discount; inv-2 belongs to the job, whose
/// owner is cust-1; inv-3 is a bill and inv-4 an expense voucher, whose employee's name is its address name; inv-5,
/// -6 and -7 are credit notes, their quantities stored negated and printed positive.
#[test]
fn every_kind_of_document_and_its_entries() {
    let body = r#"
        for n in 0..7 {
            let inv = book.invoice(`3000000000000000000000000000000${n}`);
            let e = inv.entries()[0];
            print(`${inv.id} ${inv.type} ${inv.isCustomerDocument} ${inv.owner.kind}:${inv.owner.name} ${e.quantity} ${e.documentQuantity} ${e.documentValue} ${e.documentTaxValue}`);
        }
        let e = book.invoice("30000000000000000000000000000000").entries()[0];
        print(`${e.description} ${e.action} ${e.invoicePrice} ${e.discount} ${e.discountType} ${e.discountHow} ${e.invoiceTaxable} ${e.invoiceTaxTable} ${e.order}`);
        let job = book.invoice("30000000000000000000000000000001").owner;
        print(`${job.id} ${job.name} ${job.address} ${job.owner.name} ${job.owner.address.name == ""}`);
        print(`${book.invoice("30000000000000000000000000000003").owner.address.name}`);
    "#;
    let lines = log_of(transaction_book(), body);
    assert_eq!(
        lines,
        [
            "0001 invoice true customer:cust-1-name 2 2 6 0",
            "0002 invoice true job:job-1-name 2 2 6 0",
            "0003 bill false vendor:vend-1-name 2 2 6 0",
            "0004 expenseVoucher false employee:emp-1-name 2 2 6 0",
            "0005 creditNote true customer:cust-1-name -2 2 6 0",
            "0006 creditNote false vendor:vend-1-name -2 2 6 0",
            "0007 creditNote false employee:emp-1-name -2 2 6 0",
            "entry-1-desc entry-1-action 3 0 percent preTax true  ",
            "job-1-id job-1-name  cust-1-name true",
            "emp-1-name",
        ]
    );
}

/// A GUID that names no document, and text that is no GUID, read as `()`.
#[test]
fn an_invoice_that_does_not_exist_is_unit() {
    let body = r#"
        print(`${book.invoice("0123456789abcdef0123456789abcdef") == ()} ${book.invoice("x") == ()}`);
        let posted = book.invoice("30000000000000000000000000000000");
        print(`${posted.dateDue > 0}`);
    "#;
    assert_eq!(log_of(transaction_book(), body), ["true true", "true"]);
}

/// `book.option(path)` reads `options/<path>`: strings, integers, doubles as exact Rationals, GUIDs as text, and
/// `()` for frames and absent options. `ctx.print_time` formats a time under any strftime format.
#[test]
fn book_options_and_print_time() {
    let mut book = transaction_book();
    let slots = [
        ("options/Business/Company Name", SlotValue::String("Acme".into())),
        ("options/Business/Default Invoice Report Timeout", SlotValue::Double("2.5".into())),
        ("options/Business/Big", SlotValue::Double("1e+20".into())),
        ("options/Business/Small", SlotValue::Double("1.5e-3".into())),
        ("options/Business/Count", SlotValue::Integer(7)),
        ("options/Business/Fancy Date Format/custom", SlotValue::String("%d %B %Y".into())),
    ];
    slots.into_iter().for_each(|(p, v)| book.slots.set(p, v));
    let body = r#"
        print(`${book.option("Business/Company Name")} ${book.option("Business/Default Invoice Report Timeout")}`);
        print(`${book.option("Business/Big")} ${book.option("Business/Small")} ${book.option("Business/Count")}`);
        print(`${book.option("Business/Fancy Date Format")} ${book.option("Business/Company ID")}`);
        let fmt = book.option("Business/Fancy Date Format/custom");
        print(ctx.print_time(337258740, fmt));
        print(ctx.print_time(337258740, "%Y-%m-%d %H"));
    "#;
    assert_eq!(
        log_of(book, body),
        [
            "Acme 5/2",
            "100000000000000000000 3/2000 7",
            " ",
            "08 September 1980",
            "1980-09-08 %H"
        ]
    );
}
