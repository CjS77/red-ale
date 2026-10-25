//! Business documents: invoices, bills and expense vouchers (GnuCash's `GncInvoice`), their lines (entries) and
//! orders.
//!
//! The document type is never stored: it follows from the owner, through a job to the job's owner
//! (`gncInvoiceGetType`, `5.17:libgnucash/engine/gncInvoice.c:1065-1087`). A posted document is frozen: only its
//! active flag may change here. Posting, unposting, totals and payments are separate operations.

use std::sync::Arc;

use crate::business::{Entry, Invoice, Order, OwnerKind, OwnerRef, TaxIncluded};
use crate::ops::business::contacts::{check_currency, end_owner, job_rate, next_id, note_extra_data, owner_currency, owner_exists};
use crate::ops::business::terms::{check_bill_term_ref, check_tax_table_ref, retarget_bill_term, retarget_tax_table};
use crate::{AccountType, Book, DomainError, DomainResult, Guid, SlotValue, Time64};

/// Customer invoices are numbered from this counter (`gncInvoiceNextID`, `5.17:libgnucash/engine/gncInvoice.c:2256-2275`).
pub const COUNTER_INVOICE: &str = "gncInvoice";
/// Vendor bills are numbered from this counter.
pub const COUNTER_BILL: &str = "gncBill";
/// Expense vouchers are numbered from this counter.
pub const COUNTER_EXPENSE_VOUCHER: &str = "gncExpVoucher";
/// Orders are numbered from this counter (`5.17:libgnucash/engine/gncOrder.c:589-592`).
pub const COUNTER_ORDER: &str = "gncOrder";
/// The invoice slot flagging a credit note (`5.17:libgnucash/engine/gncInvoice.c:86`).
pub const CREDIT_NOTE: &str = "credit-note";
/// The book option naming the default tax table for customer documents, as the options dialog writes it
/// (`5.17:libgnucash/engine/gnc-optiondb.cpp:1338-1345`, under `options/`).
pub const OPT_DEFAULT_CUSTOMER_TAX_TABLE: &str = "options/Business/Default Customer Tax Table";
/// The same for vendor documents.
pub const OPT_DEFAULT_VENDOR_TAX_TABLE: &str = "options/Business/Default Vendor Tax Table";

/// What kind of document an invoice is, from its end owner. Credit notes are not modelled in v0.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentType {
    /// A customer invoice (`GNC_INVOICE_CUST_INVOICE`).
    Invoice,
    /// A vendor bill (`GNC_INVOICE_VEND_INVOICE`).
    Bill,
    /// An employee expense voucher (`GNC_INVOICE_EMPL_INVOICE`).
    ExpenseVoucher,
}

impl DocumentType {
    /// The type of a document whose end owner is of this kind.
    pub fn of_end_owner(kind: OwnerKind) -> Option<Self> {
        match kind {
            OwnerKind::Customer => Some(Self::Invoice),
            OwnerKind::Vendor => Some(Self::Bill),
            OwnerKind::Employee => Some(Self::ExpenseVoucher),
            OwnerKind::Job => None,
        }
    }

    /// The counter its numbers come from.
    pub fn counter(self) -> &'static str {
        match self {
            Self::Invoice => COUNTER_INVOICE,
            Self::Bill => COUNTER_BILL,
            Self::ExpenseVoucher => COUNTER_EXPENSE_VOUCHER,
        }
    }

    /// True for customer documents, whose lines use the entry's invoice side (`i-` fields); bills and vouchers use
    /// the bill side (`b-` fields).
    pub fn is_customer(self) -> bool {
        self == Self::Invoice
    }

    /// `gncInvoiceGetTypeString`, untranslated (`5.17:libgnucash/engine/gncInvoice.c:1089-1108`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invoice => "Invoice",
            Self::Bill => "Bill",
            Self::ExpenseVoucher => "Expense",
        }
    }
}

/// The document type for an owner, following a job to its owner.
pub fn document_type(book: &Book, owner: OwnerRef) -> Option<DocumentType> {
    end_owner(book, owner).and_then(|o| DocumentType::of_end_owner(o.kind))
}

/// The type of a stored invoice.
pub fn invoice_type(book: &Book, invoice: &Invoice) -> Option<DocumentType> {
    document_type(book, invoice.owner)
}

fn invoice(book: &Book, guid: Guid) -> DomainResult<&Invoice> {
    book.business.invoices.get(&guid).map(AsRef::as_ref).ok_or_else(|| DomainError::not_found("invoice", &guid))
}

fn order(book: &Book, guid: Guid) -> DomainResult<&Order> {
    book.business.orders.get(&guid).map(AsRef::as_ref).ok_or_else(|| DomainError::not_found("order", &guid))
}

fn refuse_posted(inv: &Invoice) -> DomainResult<()> {
    if inv.is_posted() {
        return Err(DomainError::conflict(format!("document {} is posted; unpost it first", inv.id)));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Invoices, bills and vouchers.

/// The invariants GnuCash needs to read the document back, and the GUI's rules: an owner whose end owner is a
/// customer, vendor or employee, an opened date (`design/notes/xml-format-business.md` §5.5), the owner's currency
/// (`5.17:gnucash/gnome/dialog-invoice.c:414`), a bill-to customer or job only on bills and vouchers, and a charge
/// amount only on vouchers.
fn check_invoice(book: &Book, inv: &Invoice, old: Option<&Invoice>) -> DomainResult<DocumentType> {
    if !owner_exists(book, inv.owner) {
        return Err(DomainError::not_found("owner", &inv.owner.guid));
    }
    let doc = document_type(book, inv.owner).ok_or_else(|| DomainError::invalid("a document's owner must be a customer, vendor, employee or job"))?;
    if inv.is_credit_note() {
        return Err(DomainError::invalid("credit notes are not supported in red-ale v0.1"));
    }
    if inv.opened == Time64(i64::MAX) {
        return Err(DomainError::invalid("a document needs an opened date"));
    }
    check_currency(book, inv.currency.as_ref())?;
    if inv.currency != owner_currency(book, inv.owner) {
        return Err(DomainError::invalid("a document's currency is its owner's currency"));
    }
    check_bill_term_ref(book, inv.terms, old.and_then(|o| o.terms))?;
    check_bill_to(book, inv.bill_to, doc)?;
    if !inv.charge_amount.is_zero() && doc != DocumentType::ExpenseVoucher {
        return Err(DomainError::invalid("only expense vouchers have an amount to charge"));
    }
    Ok(doc)
}

fn check_bill_to(book: &Book, bill_to: Option<OwnerRef>, doc: DocumentType) -> DomainResult<()> {
    let Some(b) = bill_to else { return Ok(()) };
    if doc.is_customer() {
        return Err(DomainError::invalid("only bills and expense vouchers have a bill-to customer"));
    }
    if !matches!(b.kind, OwnerKind::Customer | OwnerKind::Job) || !owner_exists(book, b) {
        return Err(DomainError::invalid("bill-to must be an existing customer or job"));
    }
    Ok(())
}

/// Add an unposted document. An empty ID takes the next number from the counter of its type. Returns its GUID.
pub fn add_invoice(book: &mut Book, mut inv: Invoice) -> DomainResult<Guid> {
    if book.business.invoices.contains_key(&inv.guid) {
        return Err(DomainError::conflict(format!("{} is already in use", inv.guid)));
    }
    if inv.is_posted() || inv.post_lot.is_some() || inv.post_acc.is_some() || inv.posted.is_some() {
        return Err(DomainError::invalid("a new document cannot be posted"));
    }
    let doc = check_invoice(book, &inv, None)?;
    if inv.id.is_empty() {
        inv.id = next_id(book, doc.counter());
    }
    retarget_bill_term(book, None, inv.terms);
    let guid = inv.guid;
    book.business.invoices.insert(guid, Arc::new(inv));
    Ok(guid)
}

/// Replace a document with its new state. A posted document may only be (de)activated; an unposted one may change
/// anything but its type and posting fields.
pub fn update_invoice(book: &mut Book, inv: Invoice) -> DomainResult<()> {
    let old = invoice(book, inv.guid)?.clone();
    if old.is_posted() {
        let only_active = Invoice { active: inv.active, ..old.clone() };
        if inv != only_active {
            return Err(DomainError::conflict(format!("document {} is posted; only its active flag can change", old.id)));
        }
    } else {
        if inv.id.is_empty() {
            return Err(DomainError::invalid("the ID cannot be empty"));
        }
        let fields = |i: &Invoice| (i.post_txn, i.post_lot, i.post_acc, i.posted);
        if fields(&inv) != fields(&old) {
            return Err(DomainError::invalid("posting fields change only by posting"));
        }
        let doc = check_invoice(book, &inv, Some(&old))?;
        if invoice_type(book, &old) != Some(doc) {
            return Err(DomainError::invalid(format!("the owner must stay a {} owner", doc_owner_word(invoice_type(book, &old)))));
        }
        retarget_bill_term(book, old.terms, inv.terms);
    }
    book.business.invoices.insert(inv.guid, Arc::new(inv));
    Ok(())
}

fn doc_owner_word(doc: Option<DocumentType>) -> &'static str {
    match doc {
        Some(DocumentType::Invoice) => "customer",
        Some(DocumentType::Bill) => "vendor",
        Some(DocumentType::ExpenseVoucher) => "employee",
        None => "valid",
    }
}

/// Delete an unposted document and its lines, as `gncInvoiceRemoveEntries` then `gncInvoiceDestroy` do: lines also
/// on an order (or on the other side's document) stay there; the others are destroyed, releasing their tax tables,
/// and the document releases its billing term (`5.17:libgnucash/engine/gncInvoice.c:409-440`, `:760-793`). GnuCash's
/// GUI only discards new documents; red-ale offers this for any unposted one.
pub fn delete_invoice(book: &mut Book, guid: Guid) -> DomainResult<()> {
    let inv = invoice(book, guid)?.clone();
    refuse_posted(&inv)?;
    if book.lots.values().any(|l| l.slots.get("gncInvoice/invoice-guid").and_then(SlotValue::as_guid) == Some(guid)) {
        return Err(DomainError::conflict("a lot still refers to this document"));
    }
    let lines: Vec<Guid> = book.business.entries.values().filter(|e| e.invoice == Some(guid) || e.bill == Some(guid)).map(|e| e.guid).collect();
    lines.into_iter().for_each(|g| detach_line(book, g, guid));
    retarget_bill_term(book, inv.terms, None);
    book.business.invoices.remove(&guid);
    Ok(())
}

/// Take a line off a document; destroy it if it is then on no document or order.
fn detach_line(book: &mut Book, line: Guid, doc: Guid) {
    let Some(e) = book.business.entries.get_mut(&line) else { return };
    let e = Arc::make_mut(e);
    if e.invoice == Some(doc) {
        e.invoice = None;
    }
    if e.bill == Some(doc) {
        e.bill = None;
    }
    if e.invoice.is_none() && e.bill.is_none() && e.order.is_none() {
        destroy_line(book, line);
    }
}

/// Destroy a line, releasing its tax tables (`gncEntryFree`, `5.17:libgnucash/engine/gncEntry.c:466-474`).
fn destroy_line(book: &mut Book, line: Guid) {
    if let Some(e) = book.business.entries.remove(&line) {
        retarget_tax_table(book, e.i_tax_table, None);
        retarget_tax_table(book, e.b_tax_table, None);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Lines (entries).

/// GnuCash's order of lines on a document: date, date entered, description, action, then GUID (`gncEntryCompare`,
/// `5.17:libgnucash/engine/gncEntry.c:1647-1665`).
pub fn line_order(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    (a.date, a.entered, &a.description, &a.action, a.guid).cmp(&(b.date, b.entered, &b.description, &b.action, b.guid))
}

/// The lines of a document or order, in GnuCash's order.
pub fn lines_of(book: &Book, doc: Guid) -> Vec<&Entry> {
    let mut lines: Vec<&Entry> =
        book.business.entries.values().map(AsRef::as_ref).filter(|e| e.invoice == Some(doc) || e.bill == Some(doc) || e.order == Some(doc)).collect();
    lines.sort_by(|a, b| line_order(a, b));
    lines
}

/// The document type of the side a line belongs to: its invoice, else its bill. Order lines use the invoice side
/// whoever owns the order: GnuCash's order ledger is a customer-document ledger
/// (`5.17:gnucash/register/ledger-core/gncEntryLedger.c:286-294`).
pub fn line_type(book: &Book, e: &Entry) -> Option<DocumentType> {
    let of_invoice = |g: Option<Guid>| g.and_then(|g| book.business.invoices.get(&g)).and_then(|i| invoice_type(book, i));
    of_invoice(e.invoice).or_else(|| of_invoice(e.bill)).or_else(|| e.order.map(|_| DocumentType::Invoice))
}

/// A line's links: its invoice must be a customer invoice and its bill a bill or voucher, both unposted, and its
/// order must be open.
fn check_line_links(book: &Book, e: &Entry) -> DomainResult<()> {
    if e.invoice.is_none() && e.bill.is_none() && e.order.is_none() {
        return Err(DomainError::invalid("a line must be on a document or an order"));
    }
    if let Some(g) = e.invoice {
        let inv = invoice(book, g)?;
        refuse_posted(inv)?;
        if invoice_type(book, inv) != Some(DocumentType::Invoice) {
            return Err(DomainError::invalid("only a customer invoice holds lines on the invoice side"));
        }
    }
    if let Some(g) = e.bill {
        let bill = invoice(book, g)?;
        refuse_posted(bill)?;
        if invoice_type(book, bill).is_none_or(DocumentType::is_customer) {
            return Err(DomainError::invalid("only a bill or expense voucher holds lines on the bill side"));
        }
    }
    if let Some(g) = e.order {
        let o = order(book, g)?;
        if o.closed.is_some() {
            return Err(DomainError::conflict(format!("order {} is closed", o.id)));
        }
    }
    Ok(())
}

/// Accounts a line may use: the entry ledger leaves out A/R, A/P, bank, cash, equity, trading and placeholder
/// accounts, and expense accounts on customer documents or income accounts on bills and vouchers
/// (`5.17:gnucash/register/ledger-core/gncEntryLedgerLoad.c:140-181`).
fn check_line_account(book: &Book, account: Option<Guid>, old: Option<Guid>, customer: bool) -> DomainResult<()> {
    let Some(g) = account.filter(|_| account != old) else { return Ok(()) };
    let a = book.account(g).filter(|_| g != book.root).ok_or_else(|| DomainError::not_found("account", &g))?;
    let excluded = [AccountType::Payable, AccountType::Receivable, AccountType::Cash, AccountType::Bank, AccountType::Equity, AccountType::Trading];
    let wrong_side = if customer { AccountType::Expense } else { AccountType::Income };
    if excluded.contains(&a.account_type) || a.account_type == wrong_side {
        return Err(DomainError::invalid(format!("a line cannot use a {} account", a.account_type.as_str())));
    }
    if a.is_placeholder() {
        return Err(DomainError::invalid("a line cannot use a placeholder account"));
    }
    Ok(())
}

fn check_line(book: &Book, e: &Entry, old: Option<&Entry>) -> DomainResult<()> {
    check_line_links(book, e)?;
    let customer = line_type(book, e).is_some_and(DocumentType::is_customer);
    check_line_account(book, e.i_account, old.and_then(|o| o.i_account), customer || e.invoice.is_some())?;
    check_line_account(book, e.b_account, old.and_then(|o| o.b_account), false)?;
    check_tax_table_ref(book, e.i_tax_table, old.and_then(|o| o.i_tax_table))?;
    check_tax_table_ref(book, e.b_tax_table, old.and_then(|o| o.b_tax_table))?;
    if let Some(b) = e.bill_to.filter(|b| old.and_then(|o| o.bill_to) != Some(*b))
        && (!matches!(b.kind, OwnerKind::Customer | OwnerKind::Job) || !owner_exists(book, b))
    {
        return Err(DomainError::invalid("bill-to must be an existing customer or job"));
    }
    Ok(())
}

/// Add a line to the documents and order its links name. Returns its GUID.
pub fn add_entry(book: &mut Book, e: Entry) -> DomainResult<Guid> {
    if book.business.entries.contains_key(&e.guid) {
        return Err(DomainError::conflict(format!("{} is already in use", e.guid)));
    }
    if e.order.is_some() && e.bill.is_some() {
        return Err(DomainError::invalid("order lines use the invoice side, so a new one cannot be on a bill or voucher"));
    }
    check_line(book, &e, None)?;
    retarget_tax_table(book, None, e.i_tax_table);
    retarget_tax_table(book, None, e.b_tax_table);
    note_extra_data(book, !e.slots.is_empty());
    let guid = e.guid;
    book.business.entries.insert(guid, Arc::new(e));
    Ok(guid)
}

/// Replace a line with its new state. Its links do not change.
pub fn update_entry(book: &mut Book, e: Entry) -> DomainResult<()> {
    let old = book.business.entries.get(&e.guid).cloned().ok_or_else(|| DomainError::not_found("entry", &e.guid))?;
    if (e.invoice, e.bill, e.order) != (old.invoice, old.bill, old.order) {
        return Err(DomainError::invalid("a line cannot move between documents"));
    }
    check_line(book, &e, Some(&old))?;
    retarget_tax_table(book, old.i_tax_table, e.i_tax_table);
    retarget_tax_table(book, old.b_tax_table, e.b_tax_table);
    note_extra_data(book, !e.slots.is_empty());
    book.business.entries.insert(e.guid, Arc::new(e));
    Ok(())
}

/// Delete a line from its order and documents, as the entry ledger does
/// (`5.17:gnucash/register/ledger-core/gncEntryLedger.c:837-880`). Its documents must be unposted and its order open.
pub fn remove_entry(book: &mut Book, guid: Guid) -> DomainResult<()> {
    let e = book.business.entries.get(&guid).cloned().ok_or_else(|| DomainError::not_found("entry", &guid))?;
    check_line_links(book, &e)?;
    destroy_line(book, guid);
    Ok(())
}

/// A new line with GnuCash's defaults for its document (`gncEntryCreate`, `5.17:libgnucash/engine/gncEntry.c:413-444`)
/// and the entry ledger's defaults from the owner (`5.17:gnucash/register/ledger-core/gncEntryLedgerLoad.c:375-466`):
/// a job's rate as the price; the customer's discount; tax included from the owner's setting (`USEGLOBAL` takes the
/// preference's shipped default, yes); the owner's tax table when it overrides, else the book's default; and taxable
/// when there is a table. Vouchers get the engine defaults only.
pub fn default_entry(book: &Book, guid: Guid, date: Time64, entered: Time64, owner: OwnerRef) -> Entry {
    let mut e = Entry::new(guid, date, entered);
    let Some(doc) = document_type(book, owner) else { return e };
    if doc == DocumentType::ExpenseVoucher {
        return e;
    }
    let price = (owner.kind == OwnerKind::Job).then(|| book.business.jobs.get(&owner.guid).map(|j| job_rate(j))).flatten().unwrap_or_default();
    let b = &book.business;
    let end = end_owner(book, owner).map(|o| o.guid).unwrap_or_default();
    let (included, override_table, discount, option) = match doc {
        DocumentType::Invoice => b.customers.get(&end).map_or((TaxIncluded::UseGlobal, None, None, OPT_DEFAULT_CUSTOMER_TAX_TABLE), |c| {
            (c.tax_included, c.use_tax_table.then_some(c.tax_table).flatten(), Some(c.discount), OPT_DEFAULT_CUSTOMER_TAX_TABLE)
        }),
        _ => b.vendors.get(&end).map_or((TaxIncluded::UseGlobal, None, None, OPT_DEFAULT_VENDOR_TAX_TABLE), |v| {
            (v.tax_included, v.use_tax_table.then_some(v.tax_table).flatten(), None, OPT_DEFAULT_VENDOR_TAX_TABLE)
        }),
    };
    let table = override_table.or_else(|| default_tax_table(book, option));
    let included = included != TaxIncluded::No;
    if doc.is_customer() {
        e.i_price = price;
        e.i_discount = discount.unwrap_or_default();
        (e.i_taxable, e.i_tax_table, e.i_tax_included) = (table.is_some(), table, included);
    } else {
        e.b_price = price;
        (e.b_taxable, e.b_tax_table, e.b_tax_included) = (table.is_some(), table, included);
    }
    e
}

/// The book's default tax table, if it names a visible one. `gncTaxTableGetDefault` reads the option without the
/// space ("TaxTable", `5.17:libgnucash/engine/gncTaxTable.c:680-699`), which the options dialog never writes; both
/// spellings are read, the dialog's first (`design/notes/xml-format-business.md` §10.2).
fn default_tax_table(book: &Book, option: &str) -> Option<Guid> {
    let legacy = option.replace("Tax Table", "TaxTable");
    [option, legacy.as_str()]
        .iter()
        .find_map(|p| book.slots.get(p).and_then(SlotValue::as_guid))
        .filter(|g| book.business.tax_tables.get(g).is_some_and(|t| t.parent.is_none() && !t.invisible))
}

// ---------------------------------------------------------------------------------------------------------------
// Orders.

/// An order needs an owner that is a customer, vendor or job and an opened date; GnuCash cannot read one back
/// without them (`design/notes/xml-format-business.md` §5.7).
fn check_order(book: &Book, o: &Order) -> DomainResult<()> {
    if !owner_exists(book, o.owner) {
        return Err(DomainError::not_found("owner", &o.owner.guid));
    }
    if !matches!(document_type(book, o.owner), Some(DocumentType::Invoice | DocumentType::Bill)) {
        return Err(DomainError::invalid("an order's owner must be a customer, vendor or job"));
    }
    if o.opened == Time64(i64::MAX) {
        return Err(DomainError::invalid("an order needs an opened date"));
    }
    Ok(())
}

/// Add an open order. An empty ID takes the next number from the `gncOrder` counter. Returns its GUID.
pub fn add_order(book: &mut Book, mut o: Order) -> DomainResult<Guid> {
    if book.business.orders.contains_key(&o.guid) {
        return Err(DomainError::conflict(format!("{} is already in use", o.guid)));
    }
    check_order(book, &o)?;
    o.closed = None;
    if o.id.is_empty() {
        o.id = next_id(book, COUNTER_ORDER);
    }
    let guid = o.guid;
    book.business.orders.insert(guid, Arc::new(o));
    Ok(guid)
}

/// Replace an order with its new state. The closed date changes only by closing; the owner only while the order has
/// no lines.
pub fn update_order(book: &mut Book, o: Order) -> DomainResult<()> {
    let old = order(book, o.guid)?.clone();
    if o.id.is_empty() {
        return Err(DomainError::invalid("the ID cannot be empty"));
    }
    if o.closed != old.closed {
        return Err(DomainError::invalid("an order's closed date changes only by closing it"));
    }
    check_order(book, &o)?;
    if o.owner != old.owner && book.business.entries.values().any(|e| e.order == Some(o.guid)) {
        return Err(DomainError::conflict("the order has lines, so its owner cannot change"));
    }
    book.business.orders.insert(o.guid, Arc::new(o));
    Ok(())
}

/// Close an open order at `when`. It needs at least one line (`5.17:gnucash/gnome/dialog-order.c:255-316`).
pub fn close_order(book: &mut Book, guid: Guid, when: Time64) -> DomainResult<()> {
    let mut o = order(book, guid)?.clone();
    if o.closed.is_some() {
        return Err(DomainError::conflict(format!("order {} is already closed", o.id)));
    }
    if !book.business.entries.values().any(|e| e.order == Some(guid)) {
        return Err(DomainError::invalid("the order must have at least one line"));
    }
    o.closed = Some(when);
    book.business.orders.insert(guid, Arc::new(o));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::{Address, AmountType, Customer, TaxTableEntry, Vendor};
    use crate::ops::business::contacts::{add_customer, add_vendor};
    use crate::ops::business::terms::add_tax_table;
    use crate::{Account, CommodityRef, Env, FixedEnv, Frame, Numeric};

    struct World {
        book: Book,
        env: FixedEnv,
        customer: Guid,
        vendor: Guid,
        income: Guid,
        expense: Guid,
        table: Guid,
    }

    fn account(book: &mut Book, env: &mut FixedEnv, name: &str, t: AccountType) -> Guid {
        let a = Account::new(env.new_guid(), name, t, Some(CommodityRef::currency("EUR")), Some(book.root));
        let g = a.guid;
        book.accounts.insert(g, Arc::new(a));
        g
    }

    fn world() -> World {
        let mut env = FixedEnv::new(Time64(1_700_000_000), 1);
        let mut book = Book::new_empty(&mut env);
        let income = account(&mut book, &mut env, "Income", AccountType::Income);
        let expense = account(&mut book, &mut env, "Expenses", AccountType::Expense);
        let tax = account(&mut book, &mut env, "Tax", AccountType::Liability);
        let line = TaxTableEntry { account: Some(tax), amount: Numeric::from_int(10), kind: AmountType::Percent };
        let table = add_tax_table(&mut book, &mut env, "VAT", vec![line]).unwrap();
        let eur = Some(CommodityRef::currency("EUR"));
        let c = Customer {
            guid: env.new_guid(),
            name: "Acme".into(),
            id: String::new(),
            addr: Address::default(),
            ship_addr: Address::default(),
            notes: String::new(),
            terms: None,
            tax_included: TaxIncluded::No,
            active: true,
            discount: Numeric::from_int(5),
            credit: Numeric::ZERO,
            currency: eur.clone(),
            use_tax_table: true,
            tax_table: Some(table),
            slots: Frame::new(),
        };
        let customer = add_customer(&mut book, c).unwrap();
        let v = Vendor {
            guid: env.new_guid(),
            name: "Paper".into(),
            id: String::new(),
            addr: Address::default(),
            notes: String::new(),
            terms: None,
            tax_included: TaxIncluded::UseGlobal,
            active: true,
            currency: eur,
            use_tax_table: false,
            tax_table: None,
            slots: Frame::new(),
        };
        let vendor = add_vendor(&mut book, v).unwrap();
        World { book, env, customer, vendor, income, expense, table }
    }

    fn new_invoice(w: &mut World, owner: OwnerRef) -> Invoice {
        Invoice {
            guid: w.env.new_guid(),
            id: String::new(),
            owner,
            opened: Time64(1_700_000_000),
            posted: None,
            terms: None,
            billing_id: String::new(),
            notes: String::new(),
            active: true,
            post_txn: None,
            post_lot: None,
            post_acc: None,
            currency: Some(CommodityRef::currency("EUR")),
            bill_to: None,
            charge_amount: Numeric::ZERO,
            slots: Frame::new(),
        }
    }

    #[test]
    fn documents_are_numbered_per_type() {
        let mut w = world();
        let cust = OwnerRef { kind: OwnerKind::Customer, guid: w.customer };
        let vend = OwnerRef { kind: OwnerKind::Vendor, guid: w.vendor };
        let inv = new_invoice(&mut w, cust);
        let bill = new_invoice(&mut w, vend);
        let inv = add_invoice(&mut w.book, inv).unwrap();
        let bill = add_invoice(&mut w.book, bill).unwrap();
        assert_eq!(w.book.business.invoices[&inv].id, "000001");
        assert_eq!(w.book.business.invoices[&bill].id, "000001");
        assert_eq!(w.book.slots.get("counters/gncBill"), Some(&SlotValue::Integer(1)));
        let mut wrong = new_invoice(&mut w, cust);
        wrong.currency = Some(CommodityRef::currency("USD"));
        assert!(add_invoice(&mut w.book, wrong).is_err());
        let mut cn = new_invoice(&mut w, cust);
        cn.slots.set(CREDIT_NOTE, SlotValue::Integer(1));
        assert!(add_invoice(&mut w.book, cn).is_err());
    }

    #[test]
    fn lines_take_owner_defaults_and_count_tax_tables() {
        let mut w = world();
        let c = OwnerRef { kind: OwnerKind::Customer, guid: w.customer };
        let inv = new_invoice(&mut w, c);
        let inv = add_invoice(&mut w.book, inv).unwrap();
        let refs_before = w.book.business.tax_tables[&w.table].refcount;
        let mut e = default_entry(&w.book, w.env.new_guid(), Time64(0), Time64(0), c);
        assert_eq!((e.i_tax_table, e.i_taxable, e.i_tax_included, e.i_discount), (Some(w.table), true, false, Numeric::from_int(5)));
        e.invoice = Some(inv);
        e.i_account = Some(w.expense);
        assert!(add_entry(&mut w.book, e.clone()).is_err(), "expense account on an invoice");
        e.i_account = Some(w.income);
        let g = add_entry(&mut w.book, e).unwrap();
        assert_eq!(w.book.business.tax_tables[&w.table].refcount, refs_before + 1);

        let mut posted = (*w.book.business.invoices[&inv]).clone();
        posted.post_txn = Some(Guid::from_bytes([7; 16]));
        w.book.business.invoices.insert(inv, Arc::new(posted));
        assert!(remove_entry(&mut w.book, g).is_err(), "posted");
        assert!(delete_invoice(&mut w.book, inv).is_err());
        let mut unposted = (*w.book.business.invoices[&inv]).clone();
        unposted.post_txn = None;
        w.book.business.invoices.insert(inv, Arc::new(unposted));

        delete_invoice(&mut w.book, inv).unwrap();
        assert!(w.book.business.entries.is_empty());
        assert_eq!(w.book.business.tax_tables[&w.table].refcount, refs_before);
    }

    #[test]
    fn order_lines_survive_invoice_deletion_and_orders_close() {
        let mut w = world();
        let cust = OwnerRef { kind: OwnerKind::Customer, guid: w.customer };
        let ord = Order {
            guid: w.env.new_guid(),
            id: String::new(),
            owner: cust,
            opened: Time64(0),
            closed: None,
            notes: String::new(),
            reference: String::new(),
            active: true,
            slots: Frame::new(),
        };
        let ord = add_order(&mut w.book, ord).unwrap();
        assert!(close_order(&mut w.book, ord, Time64(5)).is_err(), "no lines");
        let inv = new_invoice(&mut w, cust);
        let inv = add_invoice(&mut w.book, inv).unwrap();
        let mut line = Entry::new(w.env.new_guid(), Time64(0), Time64(0));
        (line.invoice, line.order) = (Some(inv), Some(ord));
        let line = add_entry(&mut w.book, line).unwrap();
        delete_invoice(&mut w.book, inv).unwrap();
        assert_eq!(w.book.business.entries[&line].invoice, None);
        close_order(&mut w.book, ord, Time64(5)).unwrap();
        assert_eq!(w.book.business.orders[&ord].closed, Some(Time64(5)));
        assert!(remove_entry(&mut w.book, line).is_err(), "closed order");
    }
}
