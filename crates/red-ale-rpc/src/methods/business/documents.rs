//! Methods: invoices, bills and expense vouchers (one `invoice.*` family, the type following from the owner), their
//! lines, and orders. See `design/api/business-documents.md`.

// Optional fields that can be cleared are three-state on the wire: absent (keep), null (clear) or a value.
#![allow(clippy::option_option)]

use red_ale_core::business::{AmountType, DiscountHow, Entry, Invoice, Order, OwnerRef, PaymentType};
use red_ale_core::ops::business::contacts::{owner_currency, owner_terms};
use red_ale_core::ops::business::documents::{
    CREDIT_NOTE, DocumentKind, add_entry, add_invoice, add_order, close_order, default_document_entry, delete_invoice, document_kind,
    document_quantity, entries_of, entry_document_kind, entry_on_credit_note, owner_document_kind, remove_entry, update_entry, update_invoice,
    update_order,
};
use red_ale_core::{Book, Env, Frame, Guid, Numeric, Time64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::contacts::{OwnerInfo, local_noon, local_today, nullable, params_pair, resolve_owner};
use super::terms::{AmountTypeWire, BillTermRef, TaxTableRef};
use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, CommodityId, Day, Done, Id, Timestamp};

/// The document methods.
pub fn methods() -> Vec<Method> {
    vec![
        method("invoice.create", "Create an invoice, bill or expense voucher; the owner decides which.", Effect::Write, invoice_create),
        method("invoice.get", "A document with its lines.", Effect::Read, invoice_get),
        method("invoice.list", "Invoices, bills and vouchers, optionally filtered.", Effect::Read, invoice_list),
        method("invoice.update", "Change an unposted document, or (de)activate any document.", Effect::Write, invoice_update),
        method("invoice.delete", "Delete an unposted document and its lines.", Effect::Destructive, invoice_delete),
        method("invoice.addEntry", "Add a line to an unposted document (and optionally to an order).", Effect::Write, invoice_add_entry),
        method("invoice.updateEntry", "Change a line of an unposted document or open order.", Effect::Write, entry_update),
        method("invoice.removeEntry", "Delete a line from its documents and order.", Effect::Destructive, entry_remove),
        method("order.create", "Create an order for a customer, vendor or job.", Effect::Write, order_create),
        method("order.get", "An order with its lines.", Effect::Read, order_get),
        method("order.list", "Orders, optionally filtered.", Effect::Read, order_list),
        method("order.update", "Change an order.", Effect::Write, order_update),
        method("order.close", "Close an order that has lines.", Effect::Write, order_close),
        method("order.addEntry", "Add a line to an open order.", Effect::Write, order_add_entry),
        method("order.updateEntry", "Change a line of an open order (same as invoice.updateEntry).", Effect::Write, entry_update),
        method("order.removeEntry", "Delete a line (same as invoice.removeEntry).", Effect::Destructive, entry_remove),
    ]
}

/// The kind of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DocumentKindWire {
    /// A customer invoice.
    Invoice,
    /// A vendor bill.
    Bill,
    /// An employee expense voucher.
    ExpenseVoucher,
    /// A credit note of an invoice, bill or voucher; its owner says which.
    CreditNote,
}

fn type_wire(book: &Book, inv: &Invoice) -> Option<DocumentKindWire> {
    if inv.is_credit_note() {
        return Some(DocumentKindWire::CreditNote);
    }
    document_kind(book, inv).map(|t| match t {
        DocumentKind::Invoice => DocumentKindWire::Invoice,
        DocumentKind::Bill => DocumentKindWire::Bill,
        DocumentKind::ExpenseVoucher => DocumentKindWire::ExpenseVoucher,
    })
}

fn date_or_today(env: &mut dyn Env, date: Option<Day>) -> Result<Time64, RpcError> {
    Ok(local_noon(match date {
        Some(d) => d.0,
        None => local_today(env.now())?,
    }))
}

fn set<T>(target: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *target = v;
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Lines.

/// When a discount applies relative to tax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DiscountHowWire {
    /// Discount first, then tax on the discounted amount.
    PreTax,
    /// Tax and discount both on the undiscounted amount.
    SameTime,
    /// Tax first, then the discount on the taxed amount.
    PostTax,
}

impl From<DiscountHowWire> for DiscountHow {
    fn from(h: DiscountHowWire) -> Self {
        match h {
            DiscountHowWire::PreTax => Self::PreTax,
            DiscountHowWire::SameTime => Self::SameTime,
            DiscountHowWire::PostTax => Self::PostTax,
        }
    }
}

impl From<DiscountHow> for DiscountHowWire {
    fn from(h: DiscountHow) -> Self {
        match h {
            DiscountHow::PreTax => Self::PreTax,
            DiscountHow::SameTime => Self::SameTime,
            DiscountHow::PostTax => Self::PostTax,
        }
    }
}

/// How an employee paid an expense.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PaymentWire {
    /// The employee paid and is owed.
    Cash,
    /// The company credit card paid.
    Card,
}

impl From<PaymentWire> for PaymentType {
    fn from(p: PaymentWire) -> Self {
        match p {
            PaymentWire::Cash => Self::Cash,
            PaymentWire::Card => Self::Card,
        }
    }
}

impl From<PaymentType> for PaymentWire {
    fn from(p: PaymentType) -> Self {
        match p {
            PaymentType::Cash => Self::Cash,
            PaymentType::Card => Self::Card,
        }
    }
}

/// Feeds the line fields to a macro, so the add and update params share one list.
macro_rules! with_line_fields {
    ($mac:ident! { $($pre:tt)* }) => {
        $mac! { $($pre)* {
            /// The line's date; stored as noon local time, as GnuCash's register does. Defaults to today.
            date: Option<Day>,
            /// Description.
            description: Option<String>,
            /// Action: a free unit label such as "Hours".
            action: Option<String>,
            /// Notes.
            notes: Option<String>,
            /// Quantity, as printed on the document; stored negated on a credit note.
            quantity: Option<Amount>,
            /// The income account (invoices) or expense account (bills, vouchers); `null` for none.
            #[serde(deserialize_with = "nullable")]
            account: Option<Option<AccountRef>>,
            /// Unit price. A new line on a job's document defaults to the job's rate.
            price: Option<Amount>,
            /// Invoices only: the discount, a percentage or an amount. Defaults to the customer's discount.
            discount: Option<Amount>,
            /// Invoices only: whether the discount is a percentage or an amount.
            discount_type: Option<AmountTypeWire>,
            /// Invoices only: when the discount applies relative to tax.
            discount_how: Option<DiscountHowWire>,
            /// Invoices and bills: the line is taxed. Defaults to true when a tax table applies.
            taxable: Option<bool>,
            /// Invoices and bills: the price includes tax. Defaults from the customer's or vendor's setting.
            tax_included: Option<bool>,
            /// Invoices and bills: the tax table, by GUID or name; `null` for none. Defaults to the owner's table when
            /// it overrides, else the book's default.
            #[serde(deserialize_with = "nullable")]
            tax_table: Option<Option<TaxTableRef>>,
            /// Bills and vouchers: the cost is to be invoiced on to `billTo`.
            billable: Option<bool>,
            /// Bills and vouchers: the customer or job to invoice the cost to; `null` for none.
            #[serde(deserialize_with = "nullable")]
            bill_to: Option<Option<Id>>,
            /// Vouchers only: cash (the employee paid) or card (the company card paid).
            payment: Option<PaymentWire>,
        }}
    };
}

/// Declares a params struct of keys followed by the line fields, with `fields()` to take the fields out.
macro_rules! line_params {
    ($(#[$sm:meta])* $name:ident, { $($(#[$km:meta])* $k:ident: $kt:ty,)* } { $($(#[$m:meta])* $f:ident: $t:ty,)* }) => {
        $(#[$sm])*
        #[derive(Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        pub struct $name {
            $($(#[$km])* pub $k: $kt,)*
            $($(#[$m])* #[serde(default)] pub $f: $t,)*
        }

        impl $name {
            /// The line fields.
            pub fn fields(self) -> LineFields {
                LineFields { $($f: self.$f,)* }
            }
        }
    };
}

/// Declares the struct holding just the line fields.
macro_rules! line_fields {
    ($(#[$sm:meta])* $name:ident, { $($(#[$m:meta])* $f:ident: $t:ty,)* }) => {
        // The field attributes are serde's, so the struct derives Deserialize, though only the params are read.
        $(#[$sm])*
        #[derive(Debug, Default, Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        pub struct $name {
            $($(#[$m])* #[serde(default)] pub $f: $t,)*
        }
    };
}

with_line_fields!(line_fields! {
    /// The fields of a line, as given.
    LineFields,
});

with_line_fields!(line_params! {
    /// Params of `invoice.addEntry`.
    AddLineParams,
    {
        /// The unposted document.
        invoice: Id,
        /// Also put the line on this open order. Customer invoices only: order lines use the invoice side, as GnuCash's
        /// order ledger does.
        #[serde(default)]
        order: Option<Id>,
    }
});

with_line_fields!(line_params! {
    /// Params of `order.addEntry`.
    AddOrderLineParams,
    {
        /// The open order.
        order: Id,
    }
});

with_line_fields!(line_params! {
    /// Params of `invoice.updateEntry` and `order.updateEntry`: the fields to change.
    UpdateLineParams,
    {
        /// The line.
        entry: Id,
    }
});

impl LineFields {
    /// Refuse fields that do not exist on this kind of document, as GnuCash's entry ledger shows no column for them
    /// (`5.17:gnucash/register/ledger-core/gncEntryLedgerLayout.c:218-282`).
    fn check_side(&self, doc: DocumentKind) -> Result<(), RpcError> {
        let refuse =
            |present: bool, what: &str| if present { Err(RpcError::invalid(format!("{what} not on this kind of document"))) } else { Ok(()) };
        match doc {
            DocumentKind::Invoice => {
                refuse(self.billable.is_some() || self.bill_to.is_some(), "billable and billTo are")?;
                refuse(self.payment.is_some(), "payment is")
            }
            DocumentKind::Bill => {
                refuse(self.discount.is_some() || self.discount_type.is_some() || self.discount_how.is_some(), "discounts are")?;
                refuse(self.payment.is_some(), "payment is")
            }
            DocumentKind::ExpenseVoucher => {
                refuse(self.discount.is_some() || self.discount_type.is_some() || self.discount_how.is_some(), "discounts are")?;
                refuse(self.taxable.is_some() || self.tax_included.is_some() || self.tax_table.is_some(), "tax settings are")
            }
        }
    }

    fn apply(self, book: &Book, e: &mut Entry, doc: DocumentKind) -> Result<(), RpcError> {
        self.check_side(doc)?;
        if let Some(d) = self.date {
            e.date = local_noon(d.0);
        }
        set(&mut e.description, self.description);
        set(&mut e.action, self.action);
        set(&mut e.notes, self.notes);
        let credit_note = entry_on_credit_note(book, e);
        set(&mut e.quantity, self.quantity.map(|a| document_quantity(a.0, credit_note)));
        let account = self.account.map(|a| a.map(|a| a.resolve(book)).transpose()).transpose()?;
        let table = self.tax_table.map(|t| t.map(|t| t.resolve(book)).transpose()).transpose()?;
        if doc.is_customer() {
            set(&mut e.i_account, account);
            set(&mut e.i_price, self.price.map(|a| a.0));
            set(&mut e.i_discount, self.discount.map(|a| a.0));
            set(&mut e.i_disc_type, self.discount_type.map(AmountType::from));
            set(&mut e.i_disc_how, self.discount_how.map(DiscountHow::from));
            set(&mut e.i_taxable, self.taxable);
            set(&mut e.i_tax_included, self.tax_included);
            set(&mut e.i_tax_table, table);
        } else {
            set(&mut e.b_account, account);
            set(&mut e.b_price, self.price.map(|a| a.0));
            set(&mut e.b_taxable, self.taxable);
            set(&mut e.b_tax_included, self.tax_included);
            set(&mut e.b_tax_table, table);
            set(&mut e.billable, self.billable);
            if let Some(b) = self.bill_to {
                e.bill_to = b.map(|id| resolve_owner(book, id)).transpose()?;
            }
            set(&mut e.b_payment, self.payment.map(PaymentType::from));
        }
        Ok(())
    }
}

/// A line. Both sides are shown; the one its document uses is `invoice*` for customer invoices and `bill*` for
/// bills and vouchers.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs, clippy::struct_excessive_bools)] // GnuCash's entry flags, one for one.
pub struct EntryInfo {
    pub guid: Id,
    pub date: Timestamp,
    pub date_entered: Timestamp,
    pub description: String,
    pub action: String,
    pub notes: String,
    /// As stored: negated on a credit note.
    pub quantity: Amount,
    /// As printed on the document.
    pub document_quantity: Amount,
    /// The customer invoice it is on.
    pub invoice: Option<Id>,
    /// The bill or voucher it is on.
    pub bill: Option<Id>,
    /// The order it is on.
    pub order: Option<Id>,
    pub invoice_account: Option<Id>,
    pub invoice_price: Amount,
    pub discount: Amount,
    pub discount_type: AmountTypeWire,
    pub discount_how: DiscountHowWire,
    pub invoice_taxable: bool,
    pub invoice_tax_included: bool,
    pub invoice_tax_table: Option<Id>,
    pub bill_account: Option<Id>,
    pub bill_price: Amount,
    pub bill_taxable: bool,
    pub bill_tax_included: bool,
    pub bill_tax_table: Option<Id>,
    pub billable: bool,
    pub bill_to: Option<OwnerInfo>,
    pub payment: PaymentWire,
}

impl EntryInfo {
    fn of(book: &Book, e: &Entry) -> Self {
        Self {
            guid: e.guid.into(),
            date: e.date.into(),
            date_entered: e.entered.into(),
            description: e.description.clone(),
            action: e.action.clone(),
            notes: e.notes.clone(),
            quantity: e.quantity.into(),
            document_quantity: document_quantity(e.quantity, entry_on_credit_note(book, e)).into(),
            invoice: e.invoice.map(Into::into),
            bill: e.bill.map(Into::into),
            order: e.order.map(Into::into),
            invoice_account: e.i_account.map(Into::into),
            invoice_price: e.i_price.into(),
            discount: e.i_discount.into(),
            discount_type: e.i_disc_type.into(),
            discount_how: e.i_disc_how.into(),
            invoice_taxable: e.i_taxable,
            invoice_tax_included: e.i_tax_included,
            invoice_tax_table: e.i_tax_table.map(Into::into),
            bill_account: e.b_account.map(Into::into),
            bill_price: e.b_price.into(),
            bill_taxable: e.b_taxable,
            bill_tax_included: e.b_tax_included,
            bill_tax_table: e.b_tax_table.map(Into::into),
            billable: e.billable,
            bill_to: e.bill_to.map(Into::into),
            payment: e.b_payment.into(),
        }
    }
}

/// Names one line.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EntryKey {
    /// The line.
    pub entry: Id,
}

fn get_entry(book: &Book, guid: Guid) -> Result<EntryInfo, RpcError> {
    book.business.entries.get(&guid).map(|e| EntryInfo::of(book, e)).ok_or_else(|| RpcError::not_found("entry", guid))
}

/// Make a new line for a document or order, with GnuCash's defaults, then the caller's fields. A document's lines
/// take the entry ledger's owner defaults; an order's take the engine's only, as GnuCash's order ledger has no
/// document to take them from (`5.17:gnucash/register/ledger-core/gncEntryLedgerLoad.c:354-366`), and use the
/// invoice side.
fn new_line(book: &Book, env: &mut dyn Env, parent: Guid, fields: LineFields, link: impl FnOnce(&mut Entry)) -> Result<Entry, RpcError> {
    let date = date_or_today(env, None)?;
    let (guid, now) = (env.new_guid(), env.now());
    let (mut e, doc) = if let Some(inv) = book.business.invoices.get(&parent) {
        let doc = owner_document_kind(book, inv.owner).ok_or_else(|| RpcError::invalid("the document has no valid owner"))?;
        (default_document_entry(book, guid, date, now, inv), doc)
    } else if book.business.orders.contains_key(&parent) {
        (Entry::new(guid, date, now), DocumentKind::Invoice)
    } else {
        return Err(RpcError::not_found("document", parent));
    };
    link(&mut e);
    fields.apply(book, &mut e, doc)?;
    Ok(e)
}

fn invoice_add_entry(ctx: &mut Context, p: AddLineParams) -> Result<EntryInfo, RpcError> {
    let (doc, order) = (p.invoice.0, p.order.map(|o| o.0));
    let fields = p.fields();
    ctx.mutate_rpc(|book, env| {
        let inv = book.business.invoices.get(&doc).ok_or_else(|| RpcError::not_found("invoice", doc))?;
        let customer = document_kind(book, inv).is_some_and(DocumentKind::is_customer);
        let e = new_line(book, env, doc, fields, |e| {
            if customer {
                e.invoice = Some(doc);
            } else {
                e.bill = Some(doc);
            }
            e.order = order;
        })?;
        let guid = add_entry(book, e)?;
        get_entry(book, guid)
    })
}

fn order_add_entry(ctx: &mut Context, p: AddOrderLineParams) -> Result<EntryInfo, RpcError> {
    let order = p.order.0;
    let fields = p.fields();
    ctx.mutate_rpc(|book, env| {
        let e = new_line(book, env, order, fields, |e| e.order = Some(order))?;
        let guid = add_entry(book, e)?;
        get_entry(book, guid)
    })
}

fn entry_update(ctx: &mut Context, p: UpdateLineParams) -> Result<EntryInfo, RpcError> {
    let guid = p.entry.0;
    let fields = p.fields();
    ctx.mutate_rpc(|book, _| {
        let mut e = book.business.entries.get(&guid).map(|e| (**e).clone()).ok_or_else(|| RpcError::not_found("entry", guid))?;
        let doc = entry_document_kind(book, &e).ok_or_else(|| RpcError::invalid("the line is on no document or order"))?;
        fields.apply(book, &mut e, doc)?;
        update_entry(book, e)?;
        get_entry(book, guid)
    })
}

fn entry_remove(ctx: &mut Context, p: EntryKey) -> Result<Done, RpcError> {
    ctx.mutate(|book, _| remove_entry(book, p.entry.0)).map(|()| Done {})
}

// ---------------------------------------------------------------------------------------------------------------
// Invoices, bills and vouchers.

params_pair!(
    /// Params of `invoice.create`. `owner` is required.
    InvoiceFields,
    /// Params of `invoice.update`: the fields to change. Posted documents accept only `active`.
    InvoiceUpdate,
    /// The document.
    invoice: Id,
    {
        /// A customer (invoice), vendor (bill), employee (expense voucher) or job (its owner's type). On update it can
        /// change only to an owner of the same type; the currency follows the owner.
        owner: Option<Id>,
        /// Document number. On create, absent or empty takes the next number from the counter of the type
        /// (`gncInvoice`, `gncBill` or `gncExpVoucher`).
        id: Option<String>,
        /// Date opened; stored as noon local time. Defaults to today.
        date_opened: Option<Day>,
        /// Billing term, by GUID or name; `null` for none. On create, absent takes the owner's term.
        #[serde(deserialize_with = "nullable")]
        terms: Option<Option<BillTermRef>>,
        /// The customer's reference (purchase order number) or the vendor's invoice number.
        billing_id: Option<String>,
        /// Notes.
        notes: Option<String>,
        /// Bills and vouchers: the customer or job the costs are for; `null` for none.
        #[serde(deserialize_with = "nullable")]
        bill_to: Option<Option<Id>>,
        /// Vouchers: an extra amount to charge to the employee's credit card when posted.
        charge_amount: Option<Amount>,
        /// Active; `false` deactivates.
        active: Option<bool>,
        /// A credit note (default false). Fixed once the document exists: an update may only repeat it.
        credit_note: Option<bool>,
    }
);

impl InvoiceFields {
    fn apply(self, book: &Book, env: &mut dyn Env, inv: &mut Invoice) -> Result<(), RpcError> {
        if let Some(o) = self.owner {
            inv.owner = resolve_owner(book, o)?;
            inv.currency = owner_currency(book, inv.owner);
        }
        set(&mut inv.id, self.id);
        if let Some(d) = self.date_opened {
            inv.opened = date_or_today(env, Some(d))?;
        }
        if let Some(t) = self.terms {
            inv.terms = t.map(|t| t.resolve(book)).transpose()?;
        }
        set(&mut inv.billing_id, self.billing_id);
        set(&mut inv.notes, self.notes);
        if let Some(b) = self.bill_to {
            inv.bill_to = b.map(|id| resolve_owner(book, id)).transpose()?;
        }
        set(&mut inv.charge_amount, self.charge_amount.map(|a| a.0));
        set(&mut inv.active, self.active);
        if let Some(cn) = self.credit_note {
            inv.slots.set(CREDIT_NOTE, red_ale_core::SlotValue::Integer(i64::from(cn)));
        }
        Ok(())
    }
}

/// A document, without its lines.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct InvoiceInfo {
    pub guid: Id,
    pub id: String,
    /// From the owner (through a job to its owner) and the credit-note flag.
    #[serde(rename = "type")]
    pub kind: Option<DocumentKindWire>,
    pub owner: OwnerInfo,
    pub currency: Option<CommodityId>,
    pub date_opened: Timestamp,
    pub posted: bool,
    pub date_posted: Option<Timestamp>,
    pub terms: Option<Id>,
    pub billing_id: String,
    pub notes: String,
    pub active: bool,
    pub bill_to: Option<OwnerInfo>,
    pub charge_amount: Amount,
    pub posted_account: Option<Id>,
    pub posted_transaction: Option<Id>,
    pub posted_lot: Option<Id>,
    pub entry_count: usize,
}

/// A document with its lines, in GnuCash's order (date, date entered, description, action, GUID).
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceDetail {
    /// The document.
    #[serde(flatten)]
    pub invoice: InvoiceInfo,
    /// Its lines.
    pub entries: Vec<EntryInfo>,
}

fn invoice_info(book: &Book, inv: &Invoice) -> InvoiceInfo {
    InvoiceInfo {
        guid: inv.guid.into(),
        id: inv.id.clone(),
        kind: type_wire(book, inv),
        owner: inv.owner.into(),
        currency: inv.currency.clone().map(Into::into),
        date_opened: inv.opened.into(),
        posted: inv.is_posted(),
        date_posted: inv.posted.filter(|_| inv.is_posted()).map(Into::into),
        terms: inv.terms.map(Into::into),
        billing_id: inv.billing_id.clone(),
        notes: inv.notes.clone(),
        active: inv.active,
        bill_to: inv.bill_to.map(Into::into),
        charge_amount: inv.charge_amount.into(),
        posted_account: inv.post_acc.map(Into::into),
        posted_transaction: inv.post_txn.map(Into::into),
        posted_lot: inv.post_lot.map(Into::into),
        entry_count: book.business.entries.values().filter(|e| e.invoice == Some(inv.guid) || e.bill == Some(inv.guid)).count(),
    }
}

fn get_invoice(book: &Book, guid: Guid) -> Result<InvoiceDetail, RpcError> {
    let inv = book.business.invoices.get(&guid).ok_or_else(|| RpcError::not_found("invoice", guid))?;
    let entries = entries_of(book, guid).into_iter().filter(|e| e.invoice == Some(guid) || e.bill == Some(guid)).map(|e| EntryInfo::of(book, e)).collect();
    Ok(InvoiceDetail { invoice: invoice_info(book, inv), entries })
}

/// Names one document.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InvoiceKey {
    /// The document.
    pub invoice: Id,
}

/// Filters for `invoice.list`. All given filters must match.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InvoiceListParams {
    /// Only this type.
    #[serde(default, rename = "type")]
    pub kind: Option<DocumentKindWire>,
    /// Only this owner's documents; a customer or vendor also matches its jobs' documents.
    #[serde(default)]
    pub owner: Option<Id>,
    /// Only posted (`true`) or unposted (`false`) documents.
    #[serde(default)]
    pub posted: Option<bool>,
    /// Only active documents.
    #[serde(default)]
    pub active_only: bool,
    /// The exact document number.
    #[serde(default)]
    pub id: Option<String>,
}

/// Documents.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InvoiceList {
    /// In GUID order.
    pub invoices: Vec<InvoiceInfo>,
}

fn invoice_matches(book: &Book, p: &InvoiceListParams, inv: &Invoice) -> bool {
    let end = red_ale_core::ops::business::contacts::end_owner(book, inv.owner);
    p.kind.is_none_or(|k| type_wire(book, inv) == Some(k))
        && p.owner.is_none_or(|o| inv.owner.guid == o.0 || end.is_some_and(|e| e.guid == o.0))
        && p.posted.is_none_or(|posted| inv.is_posted() == posted)
        && (!p.active_only || inv.active)
        && p.id.as_ref().is_none_or(|id| *id == inv.id)
}

fn invoice_create(ctx: &mut Context, p: InvoiceFields) -> Result<InvoiceDetail, RpcError> {
    let owner = p.owner.ok_or_else(|| RpcError::invalid("a document needs an owner"))?;
    ctx.mutate_rpc(|book, env| {
        let owner = resolve_owner(book, owner)?;
        let mut inv = new_invoice(book, env, owner)?;
        p.apply(book, env, &mut inv)?;
        let guid = add_invoice(book, inv)?;
        get_invoice(book, guid)
    })
}

/// A new document with GnuCash's defaults (`gncInvoiceCreate`, `5.17:libgnucash/engine/gncInvoice.c:313-334`) and the
/// invoice dialog's: opened today, the owner's currency and terms, and the credit-note flag written as 0, as every
/// document the GUI saves has it (`5.17:gnucash/gnome/dialog-invoice.c:414`, `:433`, `:2981`).
fn new_invoice(book: &Book, env: &mut dyn Env, owner: OwnerRef) -> Result<Invoice, RpcError> {
    let mut slots = Frame::new();
    slots.set(CREDIT_NOTE, red_ale_core::SlotValue::Integer(0));
    Ok(Invoice {
        guid: env.new_guid(),
        id: String::new(),
        owner,
        opened: date_or_today(env, None)?,
        posted: None,
        terms: owner_terms(book, owner),
        billing_id: String::new(),
        notes: String::new(),
        active: true,
        post_txn: None,
        post_lot: None,
        post_acc: None,
        currency: owner_currency(book, owner),
        bill_to: None,
        charge_amount: Numeric::ZERO,
        slots,
    })
}

fn invoice_get(ctx: &mut Context, p: InvoiceKey) -> Result<InvoiceDetail, RpcError> {
    get_invoice(ctx.book()?, p.invoice.0)
}

fn invoice_list(ctx: &mut Context, p: InvoiceListParams) -> Result<InvoiceList, RpcError> {
    let book = ctx.book()?;
    let invoices = book.business.invoices.values().filter(|i| invoice_matches(book, &p, i)).map(|i| invoice_info(book, i)).collect();
    Ok(InvoiceList { invoices })
}

fn invoice_update(ctx: &mut Context, p: InvoiceUpdate) -> Result<InvoiceDetail, RpcError> {
    let (key, fields) = p.split();
    ctx.mutate_rpc(|book, env| {
        let mut inv = book.business.invoices.get(&key.0).map(|i| (**i).clone()).ok_or_else(|| RpcError::not_found("invoice", key.0))?;
        fields.apply(book, env, &mut inv)?;
        update_invoice(book, inv)?;
        get_invoice(book, key.0)
    })
}

fn invoice_delete(ctx: &mut Context, p: InvoiceKey) -> Result<Done, RpcError> {
    ctx.mutate(|book, _| delete_invoice(book, p.invoice.0)).map(|()| Done {})
}

// ---------------------------------------------------------------------------------------------------------------
// Orders.

params_pair!(
    /// Params of `order.create`. `owner` is required.
    OrderFields,
    /// Params of `order.update`: the fields to change.
    OrderUpdate,
    /// The order.
    order: Id,
    {
        /// A customer, vendor or job. It can change only while the order has no lines.
        owner: Option<Id>,
        /// Order number. On create, absent or empty takes the next number from the book's `gncOrder` counter.
        id: Option<String>,
        /// Date opened; stored as noon local time. Defaults to today.
        date_opened: Option<Day>,
        /// Notes.
        notes: Option<String>,
        /// Reference.
        reference: Option<String>,
        /// Active; `false` deactivates.
        active: Option<bool>,
    }
);

impl OrderFields {
    fn apply(self, book: &Book, env: &mut dyn Env, o: &mut Order) -> Result<(), RpcError> {
        if let Some(owner) = self.owner {
            o.owner = resolve_owner(book, owner)?;
        }
        set(&mut o.id, self.id);
        if let Some(d) = self.date_opened {
            o.opened = date_or_today(env, Some(d))?;
        }
        set(&mut o.notes, self.notes);
        set(&mut o.reference, self.reference);
        set(&mut o.active, self.active);
        Ok(())
    }
}

/// An order, without its lines.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct OrderInfo {
    pub guid: Id,
    pub id: String,
    pub owner: OwnerInfo,
    pub date_opened: Timestamp,
    pub closed: bool,
    pub date_closed: Option<Timestamp>,
    pub notes: String,
    pub reference: String,
    pub active: bool,
    pub entry_count: usize,
}

/// An order with its lines, in GnuCash's order.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderDetail {
    /// The order.
    #[serde(flatten)]
    pub order: OrderInfo,
    /// Its lines.
    pub entries: Vec<EntryInfo>,
}

fn order_info(book: &Book, o: &Order) -> OrderInfo {
    OrderInfo {
        guid: o.guid.into(),
        id: o.id.clone(),
        owner: o.owner.into(),
        date_opened: o.opened.into(),
        closed: o.closed.is_some(),
        date_closed: o.closed.map(Into::into),
        notes: o.notes.clone(),
        reference: o.reference.clone(),
        active: o.active,
        entry_count: book.business.entries.values().filter(|e| e.order == Some(o.guid)).count(),
    }
}

fn get_order(book: &Book, guid: Guid) -> Result<OrderDetail, RpcError> {
    let o = book.business.orders.get(&guid).ok_or_else(|| RpcError::not_found("order", guid))?;
    let entries = entries_of(book, guid).into_iter().filter(|e| e.order == Some(guid)).map(|e| EntryInfo::of(book, e)).collect();
    Ok(OrderDetail { order: order_info(book, o), entries })
}

/// Names one order.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OrderKey {
    /// The order.
    pub order: Id,
}

/// Params of `order.close`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OrderCloseParams {
    /// The open order.
    pub order: Id,
    /// When it closed; defaults to now, as GnuCash's order dialog does.
    #[serde(default)]
    pub date: Option<Timestamp>,
}

/// Filters for `order.list`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OrderListParams {
    /// Only this owner's orders; a customer or vendor also matches its jobs' orders.
    #[serde(default)]
    pub owner: Option<Id>,
    /// Only open (`false`) or closed (`true`) orders.
    #[serde(default)]
    pub closed: Option<bool>,
    /// Only active orders.
    #[serde(default)]
    pub active_only: bool,
}

/// Orders.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OrderList {
    /// In GUID order.
    pub orders: Vec<OrderInfo>,
}

fn order_create(ctx: &mut Context, p: OrderFields) -> Result<OrderDetail, RpcError> {
    let owner = p.owner.ok_or_else(|| RpcError::invalid("an order needs an owner"))?;
    ctx.mutate_rpc(|book, env| {
        // gncOrderCreate (5.17:libgnucash/engine/gncOrder.c:281-299): empty strings, active, open.
        let owner = resolve_owner(book, owner)?;
        let mut o = Order {
            guid: env.new_guid(),
            id: String::new(),
            owner,
            opened: date_or_today(env, None)?,
            closed: None,
            notes: String::new(),
            reference: String::new(),
            active: true,
            slots: Frame::new(),
        };
        p.apply(book, env, &mut o)?;
        let guid = add_order(book, o)?;
        get_order(book, guid)
    })
}

fn order_get(ctx: &mut Context, p: OrderKey) -> Result<OrderDetail, RpcError> {
    get_order(ctx.book()?, p.order.0)
}

fn order_list(ctx: &mut Context, p: OrderListParams) -> Result<OrderList, RpcError> {
    let book = ctx.book()?;
    let matches = |o: &Order| {
        let end = red_ale_core::ops::business::contacts::end_owner(book, o.owner);
        p.owner.is_none_or(|w| o.owner.guid == w.0 || end.is_some_and(|e| e.guid == w.0))
            && p.closed.is_none_or(|c| o.closed.is_some() == c)
            && (!p.active_only || o.active)
    };
    let orders = book.business.orders.values().filter(|o| matches(o)).map(|o| order_info(book, o)).collect();
    Ok(OrderList { orders })
}

fn order_update(ctx: &mut Context, p: OrderUpdate) -> Result<OrderDetail, RpcError> {
    let (key, fields) = p.split();
    ctx.mutate_rpc(|book, env| {
        let mut o = book.business.orders.get(&key.0).map(|o| (**o).clone()).ok_or_else(|| RpcError::not_found("order", key.0))?;
        fields.apply(book, env, &mut o)?;
        update_order(book, o)?;
        get_order(book, key.0)
    })
}

fn order_close(ctx: &mut Context, p: OrderCloseParams) -> Result<OrderDetail, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let when = p.date.map_or_else(|| env.now(), |t| t.0);
        close_order(book, p.order.0, when)?;
        get_order(book, p.order.0)
    })
}

