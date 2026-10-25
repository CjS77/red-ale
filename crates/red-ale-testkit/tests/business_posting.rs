//! Business totals, due dates, posting, unposting, payments and due reminders through the dispatcher.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::{Book, Guid, Numeric, SlotValue};
use red_ale_rpc::codes;
use red_ale_testkit::{call, call_err, open};
use serde_json::{Value, json};

/// Invoice 000001 of `synthetic/business-editable.gnucash`: customer Acme, posted, part paid.
const CUSTOMER_INVOICE: &str = "aa210b7131de44c48b8513cd27c00e95";
/// Bill 000001 (vendor Paper Supplies), posted.
const BILL: &str = "95e2e7416db94eb3a7de7433f3e7875f";
/// Expense voucher 000001 (employee Jane Doe), posted, paid by card.
const VOUCHER: &str = "645f06f442be44ecaaee148b5233ffdf";
/// Invoice 000002, owned by job JOB-1, not posted.
const JOB_INVOICE: &str = "71803cac5d8e4dceabfd0d56ec5e7521";
/// Billing term Net30.
const NET30: &str = "3416d941790749378beeb338b8357a72";

fn amount(v: &Value) -> String {
    format!("{}/{}", v["num"].as_str().unwrap(), v["denom"].as_str().unwrap())
}

#[test]
fn totals_of_a_customer_invoice() {
    let (_book, mut d) = open("synthetic/business-editable.gnucash");
    let t = call(&mut d, "invoice.totals", &json!({ "invoice": CUSTOMER_INVOICE }));
    // Consulting 10 × 120.50 taxed GST+Levy (10 % + 2.50); travel 50 less 5 post-tax, untaxed.
    assert_eq!(amount(&t["subtotal"]), "125000/100");
    assert_eq!(amount(&t["tax"]), "12300/100");
    assert_eq!(amount(&t["total"]), "137300/100");
    let taxes: Vec<(String, String)> = t["taxes"].as_array().unwrap().iter().map(|x| (x["accountName"].as_str().unwrap().to_owned(), amount(&x["amount"]))).collect();
    assert_eq!(taxes, vec![("Liabilities:Tax".to_owned(), "12050/100".to_owned()), ("Liabilities:Levy".to_owned(), "250/100".to_owned())]);
    let entries = t["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(amount(&entries[1]["discount"]), "500/100");
}

#[test]
fn totals_of_bills_and_vouchers() {
    let (_book, mut d) = open("synthetic/business-editable.gnucash");
    // 3 × 19.99 including 10 % and 2.50: (59.97 − 2.50) / 1.1 = 52.245… → 52.25 value, 7.72 tax.
    let bill = call(&mut d, "invoice.totals", &json!({ "invoice": BILL }));
    assert_eq!(amount(&bill["subtotal"]), "5225/100");
    assert_eq!(amount(&bill["total"]), "5997/100");
    let voucher = call(&mut d, "invoice.totals", &json!({ "invoice": VOUCHER }));
    assert_eq!(amount(&voucher["total"]), "3275/100");
    let e = call_err(&mut d, "invoice.totals", &json!({ "invoice": "00000000000000000000000000000001" }));
    assert_eq!(e.code, codes::NOT_FOUND);
}

#[test]
fn due_dates_from_terms() {
    let (_book, mut d) = open("synthetic/business-editable.gnucash");
    let r = call(&mut d, "invoice.dueDate", &json!({ "term": NET30, "postDate": "2024-03-05" }));
    assert_eq!(r, json!({ "dueDate": "2024-04-04", "due": "2024-04-04T10:59:00Z" }));
    let r = call(&mut d, "invoice.dueDate", &json!({ "invoice": JOB_INVOICE, "postDate": "2024-03-05" }));
    assert_eq!(r["dueDate"], "2024-03-05", "no terms: due on the posting date");
    let e = call_err(&mut d, "invoice.dueDate", &json!({ "postDate": "2024-03-05" }));
    assert_eq!(e.code, codes::INVALID_PARAMS);
}

const CUSTOMER: &str = "5b22073fa4e34411b07c14a77147ce6b";

fn guid(s: &str) -> Guid {
    Guid::parse(s).unwrap()
}

fn id_of(v: &Value) -> Guid {
    guid(v.as_str().unwrap())
}

/// (account full name, value, memo, action) of each split, in order.
fn splits(book: &Book, txn: Guid) -> Vec<(String, String, String, String)> {
    book.transaction(txn)
        .unwrap()
        .splits
        .iter()
        .map(|s| {
            (book.full_name(s.account.unwrap()).unwrap(), s.value.to_string(), s.memo.clone().unwrap_or_default(), s.action.clone().unwrap_or_default())
        })
        .collect()
}

fn lot_balance(book: &Book, lot: Guid) -> Numeric {
    book.splits_in_lot(lot).fold(Numeric::ZERO, |a, (_, s)| a.add_exact(s.amount).unwrap())
}

#[test]
fn post_a_job_invoice() {
    let (_book, mut d) = open("synthetic/business-editable.gnucash");
    let r = call(&mut d, "invoice.post", &json!({ "invoice": JOB_INVOICE, "account": "Assets:Receivable", "postDate": "2024-03-10", "memo": "m" }));
    assert_eq!(r["due"], "2024-03-10T00:00:00Z", "no terms: the start of the posting date");
    let book = d.context().book().unwrap().clone();
    let txn = id_of(&r["transaction"]);
    let t = book.transaction(txn).unwrap();
    assert_eq!(t.num.as_deref(), Some("000002"));
    assert_eq!(t.description.as_deref(), Some("Acme & Sons <Ltd>"), "the job's owner names the transaction");
    assert_eq!(t.txn_type(), Some('I'));
    assert_eq!(t.slots.get_str("trans-read-only"), Some("Generated from an invoice. Try unposting the invoice."));
    assert_eq!(
        splits(&book, txn),
        vec![
            ("Assets:Receivable".to_owned(), "1000/100".to_owned(), "m".to_owned(), "Invoice".to_owned()),
            ("Income".to_owned(), "-1000/100".to_owned(), "m".to_owned(), "Invoice".to_owned()),
        ]
    );
    let lot = book.lot(id_of(&r["lot"])).unwrap();
    assert_eq!(lot.title(), Some("Invoice 000002"));
    let inv = &book.business.invoices[&guid(JOB_INVOICE)];
    assert_eq!((inv.post_txn, inv.post_lot), (Some(txn), Some(lot.guid)));
    assert_eq!(inv.posted.unwrap().to_file_string(), "2024-03-10 10:59:00 +0000");
    let e = call_err(&mut d, "invoice.post", &json!({ "invoice": JOB_INVOICE, "account": "Assets:Receivable", "postDate": "2024-03-10" }));
    assert_eq!(e.code, codes::CONFLICT);
}

#[test]
fn refused_posts_change_nothing() {
    let (_book, mut d) = open("synthetic/business-editable.gnucash");
    let before = d.context().book().unwrap().clone();
    let e = call_err(&mut d, "invoice.post", &json!({ "invoice": JOB_INVOICE, "account": "Liabilities:Payable", "postDate": "2024-03-10" }));
    assert_eq!(e.code, codes::INVALID, "a customer document posts to A/Receivable");
    let e = call_err(&mut d, "invoice.post", &json!({ "invoice": JOB_INVOICE, "account": "Assets:Bank", "postDate": "2024-03-10" }));
    assert_eq!(e.code, codes::INVALID);
    assert_eq!(*d.context().book().unwrap(), before);
}

#[test]
fn unpost_keeps_payments_and_repost_with_autopay_reapplies_them() {
    let (_book, mut d) = open("synthetic/business-editable.gnucash");
    let before = d.context().book().unwrap().clone();
    let inv = &before.business.invoices[&guid(CUSTOMER_INVOICE)];
    let (old_txn, lot) = (inv.post_txn.unwrap(), inv.post_lot.unwrap());
    call(&mut d, "invoice.unpost", &json!({ "invoice": CUSTOMER_INVOICE, "resetTaxTables": true }));
    let book = d.context().book().unwrap().clone();
    assert!(book.transaction(old_txn).is_none());
    let l = book.lot(lot).expect("the lot keeps the payment");
    assert_eq!(l.slots.get("gncOwner/owner-type").and_then(SlotValue::as_int), Some(2));
    assert!(l.slots.get("gncInvoice").is_some() && l.slots.get("gncInvoice/invoice-guid").is_none(), "an empty gncInvoice frame stays");
    assert_eq!(lot_balance(&book, lot), Numeric::new(-500, 1));
    let inv = &book.business.invoices[&guid(CUSTOMER_INVOICE)];
    assert!(inv.post_txn.is_none() && inv.posted.is_none());
    let table = book.business.entries.values().find(|e| e.invoice == Some(inv.guid) && e.i_tax_table.is_some()).unwrap().i_tax_table.unwrap();
    assert!(book.business.tax_tables[&table].parent.is_none(), "reset points entries back at the visible table");

    let r = call(&mut d, "invoice.post", &json!({ "invoice": CUSTOMER_INVOICE, "account": "Assets:Receivable", "postDate": "2024-03-05", "autoPay": true }));
    let book = d.context().book().unwrap().clone();
    assert_eq!(r["due"], "2024-04-04T10:59:00Z", "Net30 from the frozen terms");
    let new_lot = id_of(&r["lot"]);
    assert_eq!(lot_balance(&book, new_lot), Numeric::new(873, 1), "the pre-payment was applied");
    assert!(book.lot(lot).is_none(), "the emptied pre-payment lot is gone");
}

#[test]
fn payments_apply_oldest_first_and_keep_the_rest_as_a_prepayment() {
    let (_book, mut d) = open("synthetic/business-editable.gnucash");
    call(&mut d, "invoice.post", &json!({ "invoice": JOB_INVOICE, "account": "Assets:Receivable", "postDate": "2024-03-10" }));
    let r = call(
        &mut d,
        "payment.process",
        &json!({
            "owner": { "type": "customer", "id": CUSTOMER }, "account": "Assets:Receivable", "transferAccount": "Assets:Bank",
            "amount": "900", "date": "2024-03-25", "num": "CHK-9", "memo": "april", "autoPay": true,
        }),
    );
    let book = d.context().book().unwrap().clone();
    let txn = id_of(&r["transaction"]);
    let t = book.transaction(txn).unwrap();
    assert_eq!(t.txn_type(), Some('P'));
    assert_eq!(t.num.as_deref(), Some("CHK-9"));
    assert!(t.slots.get("date-posted").is_none(), "payments are not normalised");
    let values: Vec<String> = t.splits.iter().map(|s| s.value.to_string()).collect();
    assert_eq!(values, vec!["90000/100", "-87300/100", "-1000/100", "-1700/100"]);
    let pay_lot = id_of(&r["paymentLot"]);
    assert_eq!(lot_balance(&book, pay_lot), Numeric::new(-17, 1));
    for inv in [CUSTOMER_INVOICE, JOB_INVOICE] {
        let lot = book.business.invoices[&guid(inv)].post_lot.unwrap();
        assert!(lot_balance(&book, lot).is_zero(), "{inv} is paid");
    }
}

#[test]
fn paying_a_bill_and_listing_documents_due() {
    let (_book, mut d) = open("synthetic/business-editable.gnucash");
    let due = call(&mut d, "invoice.due", &json!({ "kind": "bills", "now": "2024-03-28T00:00:00Z" }));
    let types: Vec<&str> = due.as_array().unwrap().iter().map(|x| x["type"].as_str().unwrap()).collect();
    assert_eq!(types, vec!["Bill"], "the card-paid voucher is closed");
    assert_eq!(amount(&due[0]["amount"]), "-5997/100");
    assert!(call(&mut d, "invoice.due", &json!({ "kind": "bills", "now": "2024-03-20T00:00:00Z" })).as_array().unwrap().is_empty());
    let invoices = call(&mut d, "invoice.due", &json!({ "kind": "invoices", "now": "2024-03-28T00:00:00Z", "daysInAdvance": 7 }));
    assert_eq!(invoices[0]["company"], "Acme & Sons <Ltd>");
    assert_eq!(amount(&invoices[0]["amount"]), "87300/100");

    let r = call(&mut d, "invoice.pay", &json!({ "invoice": BILL, "transferAccount": "Assets:Bank", "amount": "59.97", "date": "2024-04-01" }));
    assert!(r["paymentLot"].is_null(), "fully applied");
    let book = d.context().book().unwrap().clone();
    let t = book.transaction(id_of(&r["transaction"])).unwrap();
    let values: Vec<String> = t.splits.iter().map(|s| s.value.to_string()).collect();
    assert_eq!(values, vec!["5997/100", "-5997/100"], "A/P debited, bank credited");
    assert!(call(&mut d, "invoice.due", &json!({ "kind": "bills", "now": "2024-03-28T00:00:00Z" })).as_array().unwrap().is_empty());
}

fn deposit(d: &mut red_ale_rpc::Dispatcher, num: &str, splits: &Value) -> Value {
    let t = json!({ "currency": "EUR", "description": "Deposit", "num": num, "datePosted": "2024-03-20", "notes": "kept", "splits": splits });
    call(d, "transaction.post", &t)["guid"].clone()
}

#[test]
fn open_items_list_documents_and_prepayments() {
    let (_book, mut d) = open("synthetic/business-editable.gnucash");
    let owner = json!({ "type": "customer", "id": CUSTOMER });
    call(&mut d, "invoice.post", &json!({ "invoice": JOB_INVOICE, "account": "Assets:Receivable", "postDate": "2024-03-10" }));
    call(
        &mut d,
        "payment.process",
        &json!({ "owner": owner, "account": "Assets:Receivable", "transferAccount": "Assets:Bank", "amount": "5", "date": "2024-03-01" }),
    );
    let items = call(&mut d, "payment.openItems", &json!({ "owner": owner, "account": "Assets:Receivable" }))["items"].clone();
    let types: Vec<&str> = items.as_array().unwrap().iter().map(|i| i["type"].as_str().unwrap()).collect();
    assert_eq!(types, ["Pre-Payment", "Invoice", "Invoice"], "by due date, the pre-payment by its date: {items}");
    assert!(items[0]["invoice"].is_null() && items[0]["id"].is_null());
    assert_eq!(amount(&items[0]["balance"]), "-500/100");
    let e = call_err(&mut d, "payment.openItems", &json!({ "owner": owner, "account": "Assets:Bank" }));
    assert_eq!(e.code, codes::INVALID);
}

#[test]
fn a_transaction_becomes_a_payment_with_the_dialogs_defaults() {
    let (_book, mut d) = open("synthetic/business-editable.gnucash");
    let txn = deposit(&mut d, "D1", &json!([{ "account": "Assets:Bank", "value": "873" }, { "account": "Income", "value": "-873" }]));
    let e = call_err(&mut d, "payment.fromTransaction", &json!({ "transaction": txn, "owner": { "type": "customer", "id": CUSTOMER } }));
    assert_eq!(e.code, codes::INVALID, "no A/R split: the account is needed");
    let e = call_err(&mut d, "payment.fromTransaction", &json!({ "transaction": txn, "account": "Assets:Receivable" }));
    assert_eq!(e.code, codes::INVALID, "no owner to find");
    let r = call(
        &mut d,
        "payment.fromTransaction",
        &json!({ "transaction": txn, "owner": { "type": "customer", "id": CUSTOMER }, "account": "Assets:Receivable", "applyTo": [CUSTOMER_INVOICE] }),
    );
    assert_eq!(r["transaction"], txn, "the same transaction");
    assert!(r["paymentLot"].is_null(), "fully applied");
    let book = d.context().book().unwrap().clone();
    let t = book.transaction(id_of(&txn)).unwrap();
    assert_eq!((t.txn_type(), t.num.as_deref(), t.description.as_deref()), (Some('P'), Some("D1"), Some("Acme & Sons <Ltd>")));
    assert_eq!(t.notes(), Some("kept"));
    assert_eq!(t.splits.iter().filter_map(|s| s.memo.as_deref()).collect::<Vec<_>>(), ["Deposit"], "the description is the memo");
    let lot = book.business.invoices[&guid(CUSTOMER_INVOICE)].post_lot.unwrap();
    assert!(lot_balance(&book, lot).is_zero());

    // Now a payment: its owner and account are found, and with nothing chosen it becomes a pre-payment.
    let r = call(&mut d, "payment.fromTransaction", &json!({ "transaction": txn, "applyTo": [] }));
    let book = d.context().book().unwrap().clone();
    assert_eq!(lot_balance(&book, id_of(&r["paymentLot"])), Numeric::new(-873, 1));
    assert_eq!(lot_balance(&book, lot), Numeric::new(873, 1));
    // The defaults choose nothing (the pre-payment would be empty without this transaction), so it stays one.
    let r2 = call(&mut d, "payment.fromTransaction", &json!({ "transaction": txn, "amount": "900" }));
    let book = d.context().book().unwrap().clone();
    assert_eq!(lot_balance(&book, id_of(&r2["paymentLot"])), Numeric::new(-900, 1));
    assert!(book.lot(id_of(&r["paymentLot"])).is_none(), "the old payment lot emptied and went");
}

#[test]
fn transactions_that_cannot_become_payments() {
    let (_book, mut d) = open("synthetic/business-editable.gnucash");
    let owner = json!({ "type": "customer", "id": CUSTOMER });
    let posted = d.context().book().unwrap().business.invoices[&guid(CUSTOMER_INVOICE)].post_txn.unwrap().to_string();
    let e = call_err(&mut d, "payment.fromTransaction", &json!({ "transaction": posted, "owner": owner }));
    assert_eq!(e.code, codes::INVALID, "a posted document's transaction");
    let two_banks = deposit(
        &mut d,
        "D2",
        &json!([{ "account": "Assets:Bank", "value": "10" }, { "account": "Assets:Bank", "value": "5" }, { "account": "Income", "value": "-15" }]),
    );
    let e = call_err(&mut d, "payment.fromTransaction", &json!({ "transaction": two_banks, "owner": owner, "account": "Assets:Receivable" }));
    assert_eq!(e.code, codes::INVALID, "two candidate transfer splits");
    let no_bank = deposit(&mut d, "D3", &json!([{ "account": "Expenses", "value": "10" }, { "account": "Income", "value": "-10" }]));
    let e = call_err(&mut d, "payment.fromTransaction", &json!({ "transaction": no_bank, "owner": owner, "account": "Assets:Receivable" }));
    assert_eq!(e.code, codes::INVALID, "no transfer split");
    let zero = deposit(&mut d, "D4", &json!([{ "account": "Assets:Bank", "value": "0" }, { "account": "Income", "value": "0" }]));
    let e = call_err(&mut d, "payment.fromTransaction", &json!({ "transaction": zero, "owner": owner, "account": "Assets:Receivable" }));
    assert_eq!(e.code, codes::INVALID, "a zero amount");
}
