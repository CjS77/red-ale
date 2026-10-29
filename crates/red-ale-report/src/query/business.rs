//! The business handles (`design/reports.md` §5, "Invoice, Entry, Owner, Order, BillTerm", v0.6): the reads the
//! Printable Invoice makes of a document (`design/notes/report-invoice.md` §1.3, §4), each a thin wrapper over a
//! `red_ale_core` business read. Properties take `InvoiceDetail`'s and `EntryInfo`'s names.

use std::rc::Rc;

use red_ale_core::business::{Address, AmountType, BillTerm, DiscountHow, Entry, Invoice, Order, OwnerKind, OwnerRef};
use red_ale_core::ops::business::calc::{self, InvoiceType};
use red_ale_core::ops::business::contacts::{end_owner, owner_name};
use red_ale_core::ops::business::documents::{document_quantity, entries_of, entry_on_credit_note};
use red_ale_core::ops::business::posting::date_due;
use red_ale_core::{Guid, Numeric};
use rhai::{Array, Dynamic, Engine, Map};

use super::book::numeric;
use super::{BookHandle, Fallible, LotHandle, Snapshot, TransactionHandle, script_error};

/// A business document: an invoice, bill, expense voucher or credit note (Rhai type `Invoice`).
#[derive(Debug, Clone)]
pub struct InvoiceHandle {
    snap: Rc<Snapshot>,
    guid: Guid,
}

/// A line of a document (Rhai type `Entry`).
#[derive(Debug, Clone)]
pub struct EntryHandle {
    snap: Rc<Snapshot>,
    guid: Guid,
}

/// A customer, vendor, employee or job (Rhai type `Owner`).
#[derive(Debug, Clone)]
pub struct OwnerHandle {
    snap: Rc<Snapshot>,
    owner: OwnerRef,
}

/// An order (Rhai type `Order`).
#[derive(Debug, Clone)]
pub struct OrderHandle {
    snap: Rc<Snapshot>,
    guid: Guid,
}

/// A billing term (Rhai type `BillTerm`).
#[derive(Debug, Clone)]
pub struct BillTermHandle {
    snap: Rc<Snapshot>,
    guid: Guid,
}

fn get_invoice(snap: &Snapshot, guid: Guid) -> Fallible<&Invoice> {
    snap.book()
        .business
        .invoices
        .get(&guid)
        .map(AsRef::as_ref)
        .ok_or_else(|| script_error(format!("no invoice {guid}")))
}

fn kind_of(snap: &Snapshot, inv: &Invoice) -> Fallible<InvoiceType> {
    calc::invoice_type(snap.book(), inv).map_err(|e| script_error(e.to_string()))
}

fn optional<T: Clone + 'static>(value: Option<T>) -> Dynamic {
    value.map_or(Dynamic::UNIT, Dynamic::from)
}

impl InvoiceHandle {
    fn get(&self) -> Fallible<&Invoice> {
        get_invoice(&self.snap, self.guid)
    }

    fn owner(&self, owner: OwnerRef) -> OwnerHandle {
        OwnerHandle {
            snap: Rc::clone(&self.snap),
            owner,
        }
    }
}

impl EntryHandle {
    fn get(&self) -> Fallible<&Entry> {
        self.snap
            .book()
            .business
            .entries
            .get(&self.guid)
            .map(AsRef::as_ref)
            .ok_or_else(|| script_error(format!("no entry {}", self.guid)))
    }

    /// `gncEntryGetDocValue` and `-DocTaxValue` with `round` true (`5.17:libgnucash/engine/gncEntry.c:1535-1546`): the
    /// value or tax of the side of the entry's document, rounded, negated on a credit note.
    fn document_amount(&self, tax: bool) -> Fallible<Numeric> {
        let (book, e) = (self.snap.book(), self.get()?);
        let doc = e
            .invoice
            .or(e.bill)
            .ok_or_else(|| script_error(format!("entry {} is on no document", e.guid)))?;
        let kind = kind_of(&self.snap, get_invoice(&self.snap, doc)?)?;
        let a = calc::entry_amounts(book, e, kind.is_customer(), calc::entry_denom(book, e)).map_err(|e| script_error(e.to_string()))?;
        let n = if tax { a.tax_rounded } else { a.value_rounded };
        Ok(if kind.credit_note { n.neg() } else { n })
    }
}

impl BookHandle {
    fn invoice(&self, guid: Guid) -> InvoiceHandle {
        InvoiceHandle {
            snap: Rc::clone(&self.0),
            guid,
        }
    }
}

pub(super) fn register(engine: &mut Engine) {
    engine.register_fn("invoice", |b: &mut BookHandle, guid: &str| -> Dynamic {
        let found = Guid::parse(guid.trim())
            .ok()
            .filter(|g| b.0.book().business.invoices.contains_key(g));
        found.map_or(Dynamic::UNIT, |g| Dynamic::from(b.invoice(g)))
    });
    invoice(engine);
    invoice_relations(engine);
    entry(engine);
    entry_amounts(engine);
    owner(engine);
    order_and_terms(engine);
}

fn invoice(engine: &mut Engine) {
    engine.register_type_with_name::<InvoiceHandle>("Invoice");
    engine.register_fn("==", |a: &mut InvoiceHandle, b: InvoiceHandle| a.guid == b.guid);
    engine.register_fn("!=", |a: &mut InvoiceHandle, b: InvoiceHandle| a.guid != b.guid);
    engine.register_fn("to_string", |i: &mut InvoiceHandle| format!("Invoice({})", i.guid));
    let field = |f: fn(&Invoice) -> Dynamic| move |i: &mut InvoiceHandle| -> Fallible<Dynamic> { Ok(f(i.get()?)) };
    engine.register_get("guid", |i: &mut InvoiceHandle| i.guid.to_string());
    engine.register_get("id", field(|i| i.id.clone().into()));
    engine.register_get("billingId", field(|i| i.billing_id.clone().into()));
    engine.register_get("notes", field(|i| i.notes.clone().into()));
    engine.register_get("posted", field(|i| i.is_posted().into()));
    engine.register_get("isCreditNote", field(|i| i.is_credit_note().into()));
    engine.register_get("datePosted", field(|i| optional(i.posted.filter(|_| i.is_posted()).map(|t| t.0))));
    // `gncInvoiceGetType`'s kind under `InvoiceDetail`'s names; a credit note of any kind is `creditNote`.
    engine.register_get("type", |i: &mut InvoiceHandle| -> Fallible<String> {
        let kind = kind_of(&i.snap, i.get()?)?;
        Ok(match (kind.credit_note, kind.end_owner.kind) {
            (true, _) => "creditNote",
            (false, OwnerKind::Vendor) => "bill",
            (false, OwnerKind::Employee) => "expenseVoucher",
            (false, OwnerKind::Customer | OwnerKind::Job) => "invoice",
        }
        .to_owned())
    });
    // GnuCash's `cust-doc?`: the end owner is a customer (`5.17:gnucash/report/reports/standard/invoice.scm:769-782`).
    engine.register_get("isCustomerDocument", |i: &mut InvoiceHandle| -> Fallible<bool> {
        Ok(kind_of(&i.snap, i.get()?)?.is_customer())
    });
    engine.register_get("dateDue", |i: &mut InvoiceHandle| -> Fallible<Dynamic> {
        Ok(optional(date_due(i.snap.book(), i.get()?).map(|t| t.0)))
    });
}

/// The reads of a document that lead to other entities.
fn invoice_relations(engine: &mut Engine) {
    engine.register_get("owner", |i: &mut InvoiceHandle| -> Fallible<OwnerHandle> {
        Ok(i.owner(i.get()?.owner))
    });
    engine.register_get("currency", |i: &mut InvoiceHandle| -> Fallible<Dynamic> {
        let book = BookHandle(Rc::clone(&i.snap));
        Ok(optional(i.get()?.currency.as_ref().map(|c| book.commodity(c))))
    });
    engine.register_get("terms", |i: &mut InvoiceHandle| -> Fallible<Dynamic> {
        let terms = i.get()?.terms.filter(|g| i.snap.book().business.bill_terms.contains_key(g));
        Ok(optional(terms.map(|guid| BillTermHandle {
            snap: Rc::clone(&i.snap),
            guid,
        })))
    });
    engine.register_get("postedLot", |i: &mut InvoiceHandle| -> Fallible<Dynamic> {
        let lot = i.get()?.post_lot.filter(|g| i.snap.book().lot(*g).is_some());
        Ok(optional(lot.map(|guid| LotHandle {
            snap: Rc::clone(&i.snap),
            guid,
        })))
    });
    engine.register_get("postedTransaction", |i: &mut InvoiceHandle| -> Fallible<Dynamic> {
        let txn = i.get()?.post_txn.filter(|g| i.snap.book().transaction(*g).is_some());
        Ok(optional(txn.map(|guid| TransactionHandle {
            snap: Rc::clone(&i.snap),
            guid,
        })))
    });
    engine.register_fn("entries", |i: &mut InvoiceHandle| -> Array {
        let snap = &i.snap;
        entries_of(snap.book(), i.guid)
            .into_iter()
            .map(|e| {
                Dynamic::from(EntryHandle {
                    snap: Rc::clone(snap),
                    guid: e.guid,
                })
            })
            .collect()
    });
    engine.register_fn("totals", totals);
}

/// `gncInvoiceGetTotalSubtotal`, `-TotalTax`, `-Total` and `-TotalTaxList` (`5.17:libgnucash/engine/gncInvoice.c:917-1038`),
/// signed as printed on the document; `taxes` in GnuCash's list order, each `#{ account, value }`.
fn totals(i: &mut InvoiceHandle) -> Fallible<Map> {
    let t = calc::document_totals(i.snap.book(), i.get()?).map_err(|e| script_error(e.to_string()))?;
    let book = BookHandle(Rc::clone(&i.snap));
    let taxes: Array = t
        .taxes
        .iter()
        .map(|(account, value)| -> Fallible<Dynamic> {
            let pair: Map = [
                ("account".into(), Dynamic::from(book.account(*account))),
                ("value".into(), Dynamic::from(numeric(*value)?)),
            ]
            .into_iter()
            .collect();
            Ok(pair.into())
        })
        .collect::<Fallible<_>>()?;
    Ok([
        ("subtotal".into(), Dynamic::from(numeric(t.subtotal)?)),
        ("tax".into(), Dynamic::from(numeric(t.tax)?)),
        ("total".into(), Dynamic::from(numeric(t.total)?)),
        ("taxes".into(), taxes.into()),
    ]
    .into_iter()
    .collect())
}

fn entry(engine: &mut Engine) {
    engine.register_type_with_name::<EntryHandle>("Entry");
    engine.register_fn("==", |a: &mut EntryHandle, b: EntryHandle| a.guid == b.guid);
    engine.register_fn("!=", |a: &mut EntryHandle, b: EntryHandle| a.guid != b.guid);
    engine.register_fn("to_string", |e: &mut EntryHandle| format!("Entry({})", e.guid));
    let field = |f: fn(&Entry) -> Fallible<Dynamic>| move |e: &mut EntryHandle| -> Fallible<Dynamic> { f(e.get()?) };
    engine.register_get("guid", |e: &mut EntryHandle| e.guid.to_string());
    engine.register_get("date", field(|e| Ok(Dynamic::from(e.date.0))));
    engine.register_get("description", field(|e| Ok(e.description.clone().into())));
    engine.register_get("action", field(|e| Ok(e.action.clone().into())));
    engine.register_get("notes", field(|e| Ok(e.notes.clone().into())));
    engine.register_get("quantity", field(|e| number(e.quantity)));
    engine.register_get("invoicePrice", field(|e| number(e.i_price)));
    engine.register_get("billPrice", field(|e| number(e.b_price)));
    engine.register_get("discount", field(|e| number(e.i_discount)));
    engine.register_get("discountType", field(|e| Ok(discount_type(e.i_disc_type).into())));
    engine.register_get("discountHow", field(|e| Ok(discount_how(e.i_disc_how).into())));
    engine.register_get("invoiceTaxable", field(|e| Ok(e.i_taxable.into())));
    engine.register_get("billTaxable", field(|e| Ok(e.b_taxable.into())));
    engine.register_get("invoiceTaxTable", field(|e| tax_table(e.i_tax_table)));
    engine.register_get("billTaxTable", field(|e| tax_table(e.b_tax_table)));
    engine.register_get("order", |e: &mut EntryHandle| -> Fallible<Dynamic> {
        let order = e.get()?.order.filter(|g| e.snap.book().business.orders.contains_key(g));
        Ok(optional(order.map(|guid| OrderHandle {
            snap: Rc::clone(&e.snap),
            guid,
        })))
    });
}

/// The entry's figures as its document prints them.
fn entry_amounts(engine: &mut Engine) {
    // `gncEntryGetDocQuantity` (`5.17:libgnucash/engine/gncEntry.c:952-956`): negated on a credit note.
    engine.register_get("documentQuantity", |e: &mut EntryHandle| -> Fallible<rhai::Dynamic> {
        let entry = e.get()?;
        let q = document_quantity(entry.quantity, entry_on_credit_note(e.snap.book(), entry));
        numeric(q).map(Dynamic::from)
    });
    engine.register_get("documentValue", |e: &mut EntryHandle| -> Fallible<Dynamic> {
        numeric(e.document_amount(false)?).map(Dynamic::from)
    });
    engine.register_get("documentTaxValue", |e: &mut EntryHandle| -> Fallible<Dynamic> {
        numeric(e.document_amount(true)?).map(Dynamic::from)
    });
}

fn number(n: Numeric) -> Fallible<Dynamic> {
    numeric(n).map(Dynamic::from)
}

/// A tax table reference as its GUID's text, or `()`.
#[allow(clippy::unnecessary_wraps)]
fn tax_table(t: Option<Guid>) -> Fallible<Dynamic> {
    Ok(optional(t.map(|g| g.to_string())))
}

fn discount_type(t: AmountType) -> &'static str {
    match t {
        AmountType::Value => "value",
        AmountType::Percent => "percent",
    }
}

fn discount_how(h: DiscountHow) -> &'static str {
    match h {
        DiscountHow::PreTax => "preTax",
        DiscountHow::SameTime => "sameTime",
        DiscountHow::PostTax => "postTax",
    }
}

impl OwnerHandle {
    fn address(&self) -> Option<&Address> {
        let b = &self.snap.book().business;
        match self.owner.kind {
            OwnerKind::Customer => b.customers.get(&self.owner.guid).map(|c| &c.addr),
            OwnerKind::Vendor => b.vendors.get(&self.owner.guid).map(|v| &v.addr),
            OwnerKind::Employee => b.employees.get(&self.owner.guid).map(|e| &e.addr),
            OwnerKind::Job => None,
        }
    }

    fn id(&self) -> Option<String> {
        let b = &self.snap.book().business;
        let g = self.owner.guid;
        match self.owner.kind {
            OwnerKind::Customer => b.customers.get(&g).map(|c| c.id.clone()),
            OwnerKind::Vendor => b.vendors.get(&g).map(|v| v.id.clone()),
            OwnerKind::Employee => b.employees.get(&g).map(|e| e.id.clone()),
            OwnerKind::Job => b.jobs.get(&g).map(|j| j.id.clone()),
        }
    }
}

fn address_map(a: &Address) -> Map {
    [
        ("name", &a.name),
        ("addr1", &a.addr1),
        ("addr2", &a.addr2),
        ("addr3", &a.addr3),
        ("addr4", &a.addr4),
        ("phone", &a.phone),
        ("fax", &a.fax),
        ("email", &a.email),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.clone().into()))
    .collect()
}

fn owner(engine: &mut Engine) {
    engine.register_type_with_name::<OwnerHandle>("Owner");
    engine.register_fn("==", |a: &mut OwnerHandle, b: OwnerHandle| a.owner == b.owner);
    engine.register_fn("!=", |a: &mut OwnerHandle, b: OwnerHandle| a.owner != b.owner);
    engine.register_fn("to_string", |o: &mut OwnerHandle| format!("Owner({})", o.owner.guid));
    engine.register_get("guid", |o: &mut OwnerHandle| o.owner.guid.to_string());
    engine.register_get("kind", |o: &mut OwnerHandle| {
        match o.owner.kind {
            OwnerKind::Customer => "customer",
            OwnerKind::Job => "job",
            OwnerKind::Vendor => "vendor",
            OwnerKind::Employee => "employee",
        }
        .to_owned()
    });
    // `gncOwnerGetName`: an employee's is its address name, a job's its name.
    engine.register_get("name", |o: &mut OwnerHandle| -> Fallible<String> {
        owner_name(o.snap.book(), o.owner).ok_or_else(|| script_error(format!("no owner {}", o.owner.guid)))
    });
    engine.register_get("id", |o: &mut OwnerHandle| -> Fallible<String> {
        o.id().ok_or_else(|| script_error(format!("no owner {}", o.owner.guid)))
    });
    engine.register_get("address", |o: &mut OwnerHandle| optional(o.address().map(address_map)));
    // A job's customer or vendor (`gncOwnerGetEndOwner`), else `()`.
    engine.register_get("owner", |o: &mut OwnerHandle| -> Dynamic {
        let end = (o.owner.kind == OwnerKind::Job)
            .then(|| end_owner(o.snap.book(), o.owner))
            .flatten();
        optional(end.map(|owner| OwnerHandle {
            snap: Rc::clone(&o.snap),
            owner,
        }))
    });
}

fn order_and_terms(engine: &mut Engine) {
    engine.register_type_with_name::<OrderHandle>("Order");
    engine.register_fn("==", |a: &mut OrderHandle, b: OrderHandle| a.guid == b.guid);
    engine.register_fn("!=", |a: &mut OrderHandle, b: OrderHandle| a.guid != b.guid);
    engine.register_fn("to_string", |o: &mut OrderHandle| format!("Order({})", o.guid));
    let order = |o: &OrderHandle, f: fn(&Order) -> String| -> Fallible<String> {
        let found = o.snap.book().business.orders.get(&o.guid);
        found.map(|x| f(x)).ok_or_else(|| script_error(format!("no order {}", o.guid)))
    };
    engine.register_get("guid", |o: &mut OrderHandle| o.guid.to_string());
    engine.register_get("id", move |o: &mut OrderHandle| order(o, |x| x.id.clone()));
    engine.register_get("reference", move |o: &mut OrderHandle| order(o, |x| x.reference.clone()));

    engine.register_type_with_name::<BillTermHandle>("BillTerm");
    engine.register_fn("==", |a: &mut BillTermHandle, b: BillTermHandle| a.guid == b.guid);
    engine.register_fn("!=", |a: &mut BillTermHandle, b: BillTermHandle| a.guid != b.guid);
    engine.register_fn("to_string", |t: &mut BillTermHandle| format!("BillTerm({})", t.guid));
    let term = |t: &BillTermHandle, f: fn(&BillTerm) -> String| -> Fallible<String> {
        let found = t.snap.book().business.bill_terms.get(&t.guid);
        found
            .map(|x| f(x))
            .ok_or_else(|| script_error(format!("no billing term {}", t.guid)))
    };
    engine.register_get("guid", |t: &mut BillTermHandle| t.guid.to_string());
    engine.register_get("name", move |t: &mut BillTermHandle| term(t, |x| x.name.clone()));
    engine.register_get("description", move |t: &mut BillTermHandle| term(t, |x| x.description.clone()));
}
