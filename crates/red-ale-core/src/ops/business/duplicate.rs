//! Duplicating a business document: GnuCash's Duplicate on the invoice page and in the Find results.
//!
//! The copy is `gncInvoiceCopy` (`5.17:libgnucash/engine/gncInvoice.c:335-407`) with `gncEntryCopy` for each line
//! (`5.17:libgnucash/engine/gncEntry.c:859-908`), then the resets of `gnc_ui_invoice_duplicate`
//! (`5.17:gnucash/gnome/dialog-invoice.c:3181-3238`): active, a new number, and a new date for the document and its
//! lines. See `design/api/business-documents.md`, "Duplicating a document".

use std::sync::Arc;

use crate::business::{Entry, Invoice};
use crate::ops::business::contacts::next_id;
use crate::ops::business::documents::{CREDIT_NOTE, DocumentKind, document_kind, entries_of};
use crate::ops::business::terms::{retarget_bill_term, retarget_tax_table};
use crate::ops::doc_links::{DOC_LINK, doc_link};
use crate::{Book, DomainError, DomainResult, Env, Frame, Guid, SlotValue, Time64};

/// What changes on the copy besides what GnuCash resets unconditionally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    /// The document to copy.
    pub source: Guid,
    /// The new document's number; empty takes the next number from its kind's counter.
    pub id: String,
    /// The opened date of the copy and the date of each of its lines: the neutral time of the chosen day.
    pub date: Time64,
}

/// Copy a document and its lines into a new unposted document. Returns the new document's GUID.
pub fn duplicate_invoice(book: &mut Book, env: &mut dyn Env, d: &Duplicate) -> DomainResult<Guid> {
    let source = book.business.invoices.get(&d.source).cloned().ok_or_else(|| DomainError::not_found("invoice", &d.source))?;
    let kind = document_kind(book, &source).ok_or_else(|| DomainError::invalid("the document has no valid owner"))?;
    let lines: Vec<Entry> =
        entries_of(book, d.source).into_iter().filter(|e| e.invoice == Some(d.source) || e.bill == Some(d.source)).cloned().collect();
    let id = if d.id.is_empty() { next_id(book, kind.counter()) } else { d.id.clone() };
    let copy = copy_invoice(&source, env.new_guid(), id, d.date);
    retarget_bill_term(book, None, copy.terms);
    let guid = copy.guid;
    book.business.invoices.insert(guid, Arc::new(copy));
    lines.iter().for_each(|line| {
        let e = copy_line(line, env.new_guid(), guid, kind, d.date);
        retarget_tax_table(book, None, e.i_tax_table);
        retarget_tax_table(book, None, e.b_tax_table);
        book.business.entries.insert(e.guid, Arc::new(e));
    });
    Ok(guid)
}

/// The document fields `gncInvoiceCopy` copies, unposted, with `gnc_ui_invoice_duplicate`'s resets. Of the slots only
/// an integer `credit-note` and the document link come across.
fn copy_invoice(from: &Invoice, guid: Guid, id: String, date: Time64) -> Invoice {
    let mut slots = Frame::new();
    if let Some(cn) = from.slots.get_key(CREDIT_NOTE).filter(|v| v.as_int().is_some()) {
        slots.insert_key(CREDIT_NOTE, cn.clone());
    }
    if let Some(link) = doc_link(&from.slots) {
        slots.insert_key(DOC_LINK, SlotValue::String(link.to_owned()));
    }
    Invoice {
        guid,
        id,
        opened: date,
        posted: None,
        active: true,
        post_txn: None,
        post_lot: None,
        post_acc: None,
        slots,
        ..from.clone()
    }
}

/// A line as `gncEntryCopy` copies it onto the new document (invoice side for customer documents, bill side
/// otherwise, by the owner, `5.17:libgnucash/engine/gncInvoice.c:380-394`). Its order, slots and payment type are not
/// copied: payment keeps `gncEntryCreate`'s default, cash. Its date becomes the copy's date
/// (`set_gncEntry_date`, `5.17:gnucash/gnome/dialog-invoice.c:350-361`); its date entered is kept.
fn copy_line(from: &Entry, guid: Guid, doc: Guid, kind: DocumentKind, date: Time64) -> Entry {
    let fresh = Entry::new(guid, date, from.entered);
    let (invoice, bill) = if kind.is_customer() { (Some(doc), None) } else { (None, Some(doc)) };
    Entry { guid, date, invoice, bill, order: None, b_payment: fresh.b_payment, slots: fresh.slots, ..from.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::{Address, OwnerKind, OwnerRef, PaymentType, TaxIncluded, Vendor};
    use crate::ops::business::contacts::add_vendor;
    use crate::ops::business::documents::{add_entry, add_invoice};
    use crate::{Account, AccountType, CommodityRef, FixedEnv, Numeric};

    fn vendor_bill() -> (Book, FixedEnv, Guid) {
        let mut env = FixedEnv::new(Time64(1_700_000_000), 3);
        let mut book = Book::new_empty(&mut env);
        let eur = CommodityRef::currency("EUR");
        let expense = Account::new(env.new_guid(), "Paper", AccountType::Expense, Some(eur.clone()), Some(book.root));
        let expense_guid = expense.guid;
        book.accounts.insert(expense.guid, Arc::new(expense));
        let v = Vendor {
            guid: env.new_guid(),
            name: "Paper Co".into(),
            id: String::new(),
            addr: Address::default(),
            notes: String::new(),
            terms: None,
            tax_included: TaxIncluded::No,
            active: true,
            currency: Some(eur.clone()),
            use_tax_table: false,
            tax_table: None,
            slots: Frame::new(),
        };
        let vendor = add_vendor(&mut book, v).unwrap();
        let mut slots = Frame::new();
        slots.set(CREDIT_NOTE, SlotValue::Integer(0));
        slots.set(DOC_LINK, SlotValue::String("bills/7.pdf".into()));
        slots.set("other", SlotValue::String("not copied".into()));
        let bill = Invoice {
            guid: env.new_guid(),
            id: String::new(),
            owner: OwnerRef { kind: OwnerKind::Vendor, guid: vendor },
            opened: Time64(1_600_000_000),
            posted: None,
            terms: None,
            billing_id: "V-7".into(),
            notes: "monthly".into(),
            active: false,
            post_txn: None,
            post_lot: None,
            post_acc: None,
            currency: Some(eur),
            bill_to: None,
            charge_amount: Numeric::ZERO,
            slots,
        };
        let bill = add_invoice(&mut book, bill).unwrap();
        let mut line = Entry::new(env.new_guid(), Time64(1_600_000_000), Time64(1_600_000_100));
        (line.bill, line.b_account, line.quantity, line.b_price) = (Some(bill), Some(expense_guid), Numeric::from_int(3), Numeric::new(250, 100));
        line.description = "A4".into();
        line.b_payment = PaymentType::Card;
        add_entry(&mut book, line).unwrap();
        (book, env, bill)
    }

    #[test]
    fn copies_as_gnucash_does_and_resets_the_rest() {
        let (mut book, mut env, bill) = vendor_bill();
        let d = Duplicate { source: bill, id: String::new(), date: Time64(1_719_831_540) };
        let copy = duplicate_invoice(&mut book, &mut env, &d).unwrap();
        let (old, new) = (&book.business.invoices[&bill], &book.business.invoices[&copy]);
        assert_eq!((new.id.as_str(), old.id.as_str()), ("000002", "000001"));
        assert_eq!((new.billing_id.as_str(), new.notes.as_str(), new.active, new.opened), ("V-7", "monthly", true, Time64(1_719_831_540)));
        assert_eq!(doc_link(&new.slots), Some("bills/7.pdf"));
        assert_eq!(new.slots.get_key(CREDIT_NOTE), Some(&SlotValue::Integer(0)));
        assert!(new.slots.get_key("other").is_none());
        let lines = entries_of(&book, copy);
        assert_eq!(lines.len(), 1);
        let line = lines[0];
        assert_eq!((line.bill, line.invoice, line.date, line.entered), (Some(copy), None, Time64(1_719_831_540), Time64(1_600_000_100)));
        assert_eq!((line.description.as_str(), line.quantity, line.b_payment), ("A4", Numeric::from_int(3), PaymentType::Cash));
        let named = Duplicate { id: "X-1".into(), ..d };
        let again = duplicate_invoice(&mut book, &mut env, &named).unwrap();
        assert_eq!(book.business.invoices[&again].id, "X-1");
        assert!(duplicate_invoice(&mut book, &mut env, &Duplicate { source: Guid::from_bytes([9; 16]), ..named }).is_err());
    }
}
