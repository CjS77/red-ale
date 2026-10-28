//! Row encoders for the business tables, the inverse of [`crate::rows::business`]. Enumerations are written as GnuCash
//! stores them: owner types as `GncOwnerType` numbers (`5.17:libgnucash/engine/gncOwner.h:45-53`), tax-included and
//! amount types as `GncTaxIncluded` and `GncAmountType` numbers (`5.17:libgnucash/engine/gncTaxTable.h:78-90`), except
//! a vendor's tax-included setting, which is its name; payment types as `GncEntryPaymentType`
//! (`5.17:libgnucash/engine/gncEntry.h:43-47`); bill term types by name (`5.17:libgnucash/engine/gncBillTerm.h:115-126`).

use red_ale_core::business::{
    Address, AmountType, BillTerm, BillTermKind, Customer, Employee, Entry, Invoice, Job, Order, OwnerRef, PaymentType, TaxIncluded,
    TaxTable, TaxTableEntry, Vendor,
};
use red_ale_core::{Guid, Numeric};

use super::{Refs, boolean, guid, int, numeric, opt_guid, opt_time, text, time};
use crate::cell::Cell;

/// An address's eight columns (`5.17:libgnucash/backend/sql/gnc-address-sql.cpp:51-68`).
fn address(a: &Address) -> [Cell; 8] {
    [&a.name, &a.addr1, &a.addr2, &a.addr3, &a.addr4, &a.phone, &a.fax, &a.email].map(|s| text(s))
}

/// An owner's type and GUID columns; both NULL for no owner.
fn owner(o: Option<OwnerRef>) -> [Cell; 2] {
    o.map_or([Cell::Null, Cell::Null], |o| [int(o.kind.owner_type_number()), guid(o.guid)])
}

/// `GncTaxIncluded` as a number.
fn tax_included(t: TaxIncluded) -> Cell {
    int(match t {
        TaxIncluded::Yes => 1,
        TaxIncluded::No => 2,
        TaxIncluded::UseGlobal => 3,
    })
}

/// A `billterms` row. The child has no column: GnuCash sets it from each child's `parent`.
pub fn bill_term(t: &BillTerm) -> Vec<Cell> {
    let (kind, due, disc, discount, cutoff) = match t.kind {
        Some(BillTermKind::Days {
            due_days,
            disc_days,
            discount,
        }) => ("GNC_TERM_TYPE_DAYS", due_days, disc_days, discount, 0),
        Some(BillTermKind::Proximo {
            due_day,
            disc_day,
            discount,
            cutoff_day,
        }) => ("GNC_TERM_TYPE_PROXIMO", due_day, disc_day, discount, cutoff_day),
        None => ("", 0, 0, Numeric::ZERO, 0),
    };
    let [discount_num, discount_denom] = numeric(discount);
    vec![
        guid(t.guid),
        text(&t.name),
        text(&t.description),
        int(t.refcount),
        boolean(t.invisible),
        opt_guid(t.parent),
        text(kind),
        int(due),
        int(disc),
        discount_num,
        discount_denom,
        int(cutoff),
    ]
}

/// A `customers` row.
pub fn customer(c: &Customer, refs: Refs<'_>) -> Vec<Cell> {
    let mut cells = vec![guid(c.guid), text(&c.name), text(&c.id), text(&c.notes), boolean(c.active)];
    cells.extend(numeric(c.discount));
    cells.extend(numeric(c.credit));
    cells.push(refs.commodity(c.currency.as_ref()));
    cells.push(boolean(c.use_tax_table));
    cells.extend(address(&c.addr));
    cells.extend(address(&c.ship_addr));
    cells.extend([opt_guid(c.terms), tax_included(c.tax_included), opt_guid(c.tax_table)]);
    cells
}

/// An `employees` row.
pub fn employee(e: &Employee, refs: Refs<'_>) -> Vec<Cell> {
    let mut cells = vec![
        guid(e.guid),
        text(&e.username),
        text(&e.id),
        text(&e.language),
        text(&e.acl),
        boolean(e.active),
        refs.commodity(e.currency.as_ref()),
        opt_guid(e.ccard),
    ];
    cells.extend(numeric(e.workday));
    cells.extend(numeric(e.rate));
    cells.extend(address(&e.addr));
    cells
}

fn payment(p: PaymentType) -> Cell {
    int(match p {
        PaymentType::Cash => 1,
        PaymentType::Card => 2,
    })
}

/// An `entries` row.
pub fn entry(e: &Entry) -> Vec<Cell> {
    let mut cells = vec![
        guid(e.guid),
        time(e.date),
        time(e.entered),
        text(&e.description),
        text(&e.action),
        text(&e.notes),
    ];
    cells.extend(numeric(e.quantity));
    cells.push(opt_guid(e.i_account));
    cells.extend(numeric(e.i_price));
    cells.extend(numeric(e.i_discount));
    cells.extend([
        opt_guid(e.invoice),
        text(e.i_disc_type.as_str()),
        text(e.i_disc_how.as_str()),
        boolean(e.i_taxable),
        boolean(e.i_tax_included),
        opt_guid(e.i_tax_table),
        opt_guid(e.b_account),
    ]);
    cells.extend(numeric(e.b_price));
    cells.extend([
        opt_guid(e.bill),
        boolean(e.b_taxable),
        boolean(e.b_tax_included),
        opt_guid(e.b_tax_table),
        payment(e.b_payment),
        boolean(e.billable),
    ]);
    cells.extend(owner(e.bill_to));
    cells.push(opt_guid(e.order));
    cells
}

/// An `invoices` row. An unposted invoice's `INT64_MAX` posted date is outside GnuCash's range, so NULL.
pub fn invoice(i: &Invoice, refs: Refs<'_>) -> Vec<Cell> {
    let mut cells = vec![
        guid(i.guid),
        text(&i.id),
        time(i.opened),
        opt_time(i.posted),
        text(&i.notes),
        boolean(i.active),
        refs.commodity(i.currency.as_ref()),
    ];
    cells.extend(owner(Some(i.owner)));
    cells.extend([
        opt_guid(i.terms),
        text(&i.billing_id),
        opt_guid(i.post_txn),
        opt_guid(i.post_lot),
        opt_guid(i.post_acc),
    ]);
    cells.extend(owner(i.bill_to));
    cells.extend(numeric(i.charge_amount));
    cells
}

/// A `jobs` row.
pub fn job(j: &Job) -> Vec<Cell> {
    let mut cells = vec![guid(j.guid), text(&j.id), text(&j.name), text(&j.reference), boolean(j.active)];
    cells.extend(owner(Some(j.owner)));
    cells
}

/// An `orders` row. An open order's `INT64_MAX` closing date is NULL.
pub fn order(o: &Order) -> Vec<Cell> {
    let mut cells = vec![
        guid(o.guid),
        text(&o.id),
        text(&o.notes),
        text(&o.reference),
        boolean(o.active),
        time(o.opened),
        opt_time(o.closed),
    ];
    cells.extend(owner(Some(o.owner)));
    cells
}

/// A `taxtables` row.
pub fn tax_table(t: &TaxTable) -> Vec<Cell> {
    vec![
        guid(t.guid),
        text(&t.name),
        int(t.refcount),
        boolean(t.invisible),
        opt_guid(t.parent),
    ]
}

/// A `taxtable_entries` row of tax table `table`, without its `id`.
pub fn tax_table_entry(table: Guid, e: &TaxTableEntry) -> Vec<Cell> {
    let [num, denom] = numeric(e.amount);
    let kind = match e.kind {
        AmountType::Value => 1,
        AmountType::Percent => 2,
    };
    vec![guid(table), opt_guid(e.account), num, denom, int(kind)]
}

/// A `vendors` row.
pub fn vendor(v: &Vendor, refs: Refs<'_>) -> Vec<Cell> {
    let mut cells = vec![
        guid(v.guid),
        text(&v.name),
        text(&v.id),
        text(&v.notes),
        refs.commodity(v.currency.as_ref()),
        boolean(v.active),
        boolean(v.use_tax_table),
    ];
    cells.extend(address(&v.addr));
    cells.extend([opt_guid(v.terms), text(v.tax_included.as_str()), opt_guid(v.tax_table)]);
    cells
}
