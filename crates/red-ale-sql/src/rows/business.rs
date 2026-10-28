//! Row decoders for the business tables. Each field reads as the XML reader gives it for what GnuCash's XML writer
//! writes from the same engine value (`5.17:libgnucash/backend/xml/gnc-*-xml-v2.cpp`, through `red-ale-xml`'s
//! business module): a field the writer leaves out when it is zero, empty or beside the point reads as the reader's
//! default.
//!
//! Enumerations are stored as GnuCash's numbers or names: owner types are `GncOwnerType`
//! (`5.17:libgnucash/engine/gncOwner.h:45-53`), tax-included and amount types `GncTaxIncluded` and `GncAmountType`
//! (`5.17:libgnucash/engine/gncTaxTable.h:78-90`), payment types `GncEntryPaymentType`
//! (`5.17:libgnucash/engine/gncEntry.h:43-47`), bill term types `GncBillTermType` by name
//! (`5.17:libgnucash/engine/gncBillTerm.h:115-126`).

use red_ale_core::business::{
    Address, AmountType, BillTerm, BillTermKind, Customer, DiscountHow, Employee, Entry, Invoice, Job, Order, OwnerKind, OwnerRef,
    PaymentType, TaxIncluded, TaxTable, TaxTableEntry, Vendor,
};
use red_ale_core::{Frame, Guid};

use super::{Lookup, key};
use crate::cell::{Cells, Row};
use crate::tables::{ADDRESS, BILLTERMS, CUSTOMERS, EMPLOYEES, ENTRIES, INVOICES, JOBS, ORDERS, TAXTABLE_ENTRIES, TAXTABLES, VENDORS};

/// An address from its `<prefix>_*` columns; NULL reads as empty, as the XML reader gives a missing element. SQL books
/// have no place for an address's Slots.
fn address(cells: &mut Cells<'_>, prefix: &str) -> Address {
    let mut parts = ADDRESS
        .iter()
        .map(|part| cells.text(&format!("{prefix}_{part}")).unwrap_or_default());
    let mut next = || parts.next().unwrap_or_default();
    Address {
        name: next(),
        addr1: next(),
        addr2: next(),
        addr3: next(),
        addr4: next(),
        phone: next(),
        fax: next(),
        email: next(),
        slots: Frame::new(),
    }
}

/// An owner from `<prefix>_type` and `<prefix>_guid`. GnuCash's "none" and "undefined" types (0 and 1) are no owner.
fn owner(cells: &mut Cells<'_>, prefix: &str) -> Option<OwnerRef> {
    let type_col = format!("{prefix}_type");
    let number = cells.int(&type_col);
    let guid = cells.guid(&format!("{prefix}_guid"));
    let n = number.filter(|n| *n > 1)?;
    let Some(kind) = OwnerKind::from_owner_type_number(n) else {
        cells.refuse(&type_col, format!("{n} is not an owner type"));
        return None;
    };
    guid.map(|guid| OwnerRef { kind, guid })
}

/// An owner the object cannot do without.
fn req_owner(cells: &mut Cells<'_>, prefix: &str) -> Option<OwnerRef> {
    let found = owner(cells, prefix);
    if found.is_none() {
        cells.row_finding("left out: it has no owner");
    }
    found
}

fn tax_included(cells: &mut Cells<'_>, number: Option<i64>, column: &str) -> TaxIncluded {
    match number {
        None | Some(3) => TaxIncluded::UseGlobal,
        Some(1) => TaxIncluded::Yes,
        Some(2) => TaxIncluded::No,
        Some(n) => {
            cells.refuse(column, format!("{n} is not a tax-included type"));
            TaxIncluded::UseGlobal
        }
    }
}

fn word<T>(cells: &mut Cells<'_>, column: &str, default: T, parse: impl Fn(&str) -> Option<T>) -> T {
    match cells.text(column) {
        None => default,
        Some(text) => parse(&text).unwrap_or_else(|| {
            cells.refuse(column, format!("{text:?} is not a value GnuCash writes"));
            default
        }),
    }
}

/// A `billterms` row. The child is not stored: GnuCash sets it from each child's `parent`
/// (`5.17:libgnucash/backend/sql/gnc-bill-term-sql.cpp:154-175`), which the reader does once every row is read.
pub fn bill_term(row: &Row, findings: &mut Vec<String>) -> Option<BillTerm> {
    let cells = &mut Cells::new(&BILLTERMS, row, findings);
    let guid = key(cells, "guid")?;
    let type_name = cells.req_text("type");
    let due = cells.int("duedays").unwrap_or(0);
    let disc = cells.int("discountdays").unwrap_or(0);
    let discount = cells.numeric_or_zero("discount");
    let kind = match type_name.as_str() {
        "GNC_TERM_TYPE_DAYS" => Some(BillTermKind::Days {
            due_days: due,
            disc_days: disc,
            discount,
        }),
        "GNC_TERM_TYPE_PROXIMO" => Some(BillTermKind::Proximo {
            due_day: due,
            disc_day: disc,
            discount,
            cutoff_day: cells.int("cutoff").unwrap_or(0),
        }),
        "" => None,
        other => {
            cells.refuse("type", format!("{other:?} is not a bill term type"));
            None
        }
    };
    let parent = cells.guid("parent").filter(|p| *p != guid);
    Some(BillTerm {
        guid,
        name: cells.req_text("name"),
        description: cells.req_text("description"),
        refcount: cells.req_int("refcount"),
        invisible: cells.req_bool("invisible"),
        slots: Frame::new(),
        child: None,
        parent,
        kind,
    })
}

/// A `customers` row.
pub fn customer(row: &Row, lookup: &Lookup, findings: &mut Vec<String>) -> Option<Customer> {
    let cells = &mut Cells::new(&CUSTOMERS, row, findings);
    let guid = key(cells, "guid")?;
    let included = cells.int("tax_included");
    Some(Customer {
        guid,
        name: cells.req_text("name"),
        id: cells.req_text("id"),
        addr: address(cells, "addr"),
        ship_addr: address(cells, "shipaddr"),
        notes: cells.req_text("notes"),
        terms: cells.guid("terms"),
        tax_included: tax_included(cells, included, "tax_included"),
        active: cells.req_bool("active"),
        discount: cells.req_numeric("discount"),
        credit: cells.req_numeric("credit"),
        currency: lookup.commodity(cells, "currency"),
        use_tax_table: cells.req_bool("tax_override"),
        tax_table: cells.guid("taxtable"),
        slots: Frame::new(),
    })
}

/// An `employees` row.
pub fn employee(row: &Row, lookup: &Lookup, findings: &mut Vec<String>) -> Option<Employee> {
    let cells = &mut Cells::new(&EMPLOYEES, row, findings);
    let guid = key(cells, "guid")?;
    Some(Employee {
        guid,
        username: cells.req_text("username"),
        id: cells.req_text("id"),
        addr: address(cells, "addr"),
        language: cells.req_text("language"),
        acl: cells.req_text("acl"),
        active: cells.req_bool("active"),
        workday: cells.req_numeric("workday"),
        rate: cells.req_numeric("rate"),
        currency: lookup.commodity(cells, "currency"),
        ccard: cells.guid("ccard_guid"),
        slots: Frame::new(),
    })
}

fn payment(cells: &mut Cells<'_>) -> PaymentType {
    match cells.int("b_paytype") {
        None | Some(1) => PaymentType::Cash,
        Some(2) => PaymentType::Card,
        Some(n) => {
            cells.refuse("b_paytype", format!("{n} is not a payment type"));
            PaymentType::Cash
        }
    }
}

/// An `entries` row. The XML writer writes the invoice-side flags only for an entry on an invoice, and the bill-side
/// ones only for an entry on a bill (`5.17:libgnucash/backend/xml/gnc-entry-xml-v2.cpp:146-200`); without one they
/// read as the engine's defaults.
pub fn entry(row: &Row, findings: &mut Vec<String>) -> Option<Entry> {
    let cells = &mut Cells::new(&ENTRIES, row, findings);
    let guid = key(cells, "guid")?;
    let (date, entered) = (cells.time("date"), cells.time("date_entered"));
    let mut e = Entry::new(guid, date, entered);
    e.description = cells.text("description").unwrap_or_default();
    e.action = cells.text("action").unwrap_or_default();
    e.notes = cells.text("notes").unwrap_or_default();
    e.quantity = cells.numeric_or_zero("quantity");
    e.i_account = cells.guid("i_acct");
    e.i_price = cells.numeric_or_zero("i_price");
    e.i_discount = cells.numeric_or_zero("i_discount");
    e.invoice = cells.guid("invoice");
    let disc_type = word(cells, "i_disc_type", AmountType::Percent, AmountType::parse);
    let disc_how = word(cells, "i_disc_how", DiscountHow::PreTax, DiscountHow::parse);
    let (i_taxable, i_included) = (cells.bool("i_taxable"), cells.bool("i_taxincluded"));
    if e.invoice.is_some() {
        e.i_disc_type = disc_type;
        e.i_disc_how = disc_how;
        e.i_taxable = i_taxable.unwrap_or(true);
        e.i_tax_included = i_included.unwrap_or(false);
    }
    e.i_tax_table = cells.guid("i_taxtable");
    e.b_account = cells.guid("b_acct");
    e.b_price = cells.numeric_or_zero("b_price");
    e.bill = cells.guid("bill");
    let (b_taxable, b_included, pay) = (cells.bool("b_taxable"), cells.bool("b_taxincluded"), payment(cells));
    let (billable, bill_to) = (cells.bool("billable"), owner(cells, "billto"));
    if e.bill.is_some() {
        e.b_taxable = b_taxable.unwrap_or(true);
        e.b_tax_included = b_included.unwrap_or(false);
        e.b_payment = pay;
        e.billable = billable.unwrap_or(false);
        e.bill_to = bill_to;
    }
    e.b_tax_table = cells.guid("b_taxtable");
    e.order = cells.guid("order_guid");
    Some(e)
}

/// An `invoices` row. GnuCash writes NULL for the `INT64_MAX` posted date of an unposted invoice, so NULL is "not
/// posted".
pub fn invoice(row: &Row, lookup: &Lookup, findings: &mut Vec<String>) -> Option<Invoice> {
    let cells = &mut Cells::new(&INVOICES, row, findings);
    let guid = key(cells, "guid")?;
    let owner = req_owner(cells, "owner")?;
    Some(Invoice {
        guid,
        id: cells.req_text("id"),
        owner,
        opened: cells.time("date_opened"),
        posted: cells.opt_time("date_posted"),
        terms: cells.guid("terms"),
        billing_id: cells.text("billing_id").unwrap_or_default(),
        notes: cells.req_text("notes"),
        active: cells.req_bool("active"),
        post_txn: cells.guid("post_txn"),
        post_lot: cells.guid("post_lot"),
        post_acc: cells.guid("post_acc"),
        currency: lookup.commodity(cells, "currency"),
        bill_to: self::owner(cells, "billto"),
        charge_amount: cells.numeric_or_zero("charge_amt"),
        slots: Frame::new(),
    })
}

/// A `jobs` row.
pub fn job(row: &Row, findings: &mut Vec<String>) -> Option<Job> {
    let cells = &mut Cells::new(&JOBS, row, findings);
    let guid = key(cells, "guid")?;
    let owner = req_owner(cells, "owner")?;
    Some(Job {
        guid,
        id: cells.req_text("id"),
        name: cells.req_text("name"),
        reference: cells.req_text("reference"),
        owner,
        active: cells.req_bool("active"),
        slots: Frame::new(),
    })
}

/// An `orders` row. An open order's `INT64_MAX` closing date is written as NULL.
pub fn order(row: &Row, findings: &mut Vec<String>) -> Option<Order> {
    let cells = &mut Cells::new(&ORDERS, row, findings);
    let guid = key(cells, "guid")?;
    let owner = req_owner(cells, "owner")?;
    Some(Order {
        guid,
        id: cells.req_text("id"),
        owner,
        opened: cells.time("date_opened"),
        closed: cells.opt_time("date_closed"),
        notes: cells.req_text("notes"),
        reference: cells.req_text("reference"),
        active: cells.req_bool("active"),
        slots: Frame::new(),
    })
}

/// A `taxtables` row, with no entries yet. As for bill terms, the child comes from the children's `parent`.
pub fn tax_table(row: &Row, findings: &mut Vec<String>) -> Option<TaxTable> {
    let cells = &mut Cells::new(&TAXTABLES, row, findings);
    let guid = key(cells, "guid")?;
    Some(TaxTable {
        guid,
        name: cells.req_text("name"),
        refcount: cells.req_int("refcount"),
        invisible: cells.req_bool("invisible"),
        child: None,
        parent: cells.guid("parent").filter(|p| *p != guid),
        entries: Vec::new(),
        slots: Frame::new(),
    })
}

/// A `taxtable_entries` row: its tax table's GUID and the entry.
pub fn tax_table_entry(row: &Row, findings: &mut Vec<String>) -> Option<(Guid, TaxTableEntry)> {
    let cells = &mut Cells::new(&TAXTABLE_ENTRIES, row, findings);
    let table = cells.guid("taxtable");
    let account = cells.guid("account");
    let amount = cells.req_numeric("amount");
    let kind = match cells.req_int("type") {
        1 => Some(AmountType::Value),
        2 => Some(AmountType::Percent),
        n => {
            cells.refuse("type", format!("{n} is not an amount type"));
            None
        }
    };
    let (Some(table), Some(kind)) = (table, kind) else {
        cells.row_finding("left out: an entry without its tax table or type");
        return None;
    };
    Some((table, TaxTableEntry { account, amount, kind }))
}

/// A `vendors` row. Its tax-included setting is stored by name (`tax_inc`).
pub fn vendor(row: &Row, lookup: &Lookup, findings: &mut Vec<String>) -> Option<Vendor> {
    let cells = &mut Cells::new(&VENDORS, row, findings);
    let guid = key(cells, "guid")?;
    Some(Vendor {
        guid,
        name: cells.req_text("name"),
        id: cells.req_text("id"),
        addr: address(cells, "addr"),
        notes: cells.req_text("notes"),
        terms: cells.guid("terms"),
        tax_included: word(cells, "tax_inc", TaxIncluded::UseGlobal, TaxIncluded::parse),
        active: cells.req_bool("active"),
        currency: lookup.commodity(cells, "currency"),
        use_tax_table: cells.req_bool("tax_override"),
        tax_table: cells.guid("tax_table"),
        slots: Frame::new(),
    })
}
