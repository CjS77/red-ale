//! Business objects: owners, documents, entries, tax tables and billing terms.
//!
//! Field sets follow GnuCash 5.17's engine objects (`5.17:libgnucash/engine/gnc*.c`) and the XML format in
//! `design/notes/xml-format-business.md`. Optional text fields use the empty string for "not set", which is how
//! GnuCash stores them and decides whether to write them.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{CommodityRef, Frame, Guid, Numeric, Time64};

/// A postal address with contact details. `5.17:libgnucash/engine/gncAddress.c`.
#[derive(Debug, Clone, PartialEq, Default)]
#[allow(missing_docs)]
pub struct Address {
    pub name: String,
    pub addr1: String,
    pub addr2: String,
    pub addr3: String,
    pub addr4: String,
    pub phone: String,
    pub fax: String,
    pub email: String,
    /// Address slots (rare; set the "Extra data" Book feature).
    pub slots: Frame,
}

/// The kind of an owner, with the type names GnuCash writes in `owner:type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OwnerKind {
    /// `gncCustomer`.
    Customer,
    /// `gncJob`.
    Job,
    /// `gncVendor`.
    Vendor,
    /// `gncEmployee`.
    Employee,
}

impl OwnerKind {
    /// The QOF type name written in files.
    pub fn type_name(self) -> &'static str {
        match self {
            Self::Customer => "gncCustomer",
            Self::Job => "gncJob",
            Self::Vendor => "gncVendor",
            Self::Employee => "gncEmployee",
        }
    }

    /// Parse a QOF type name.
    pub fn from_type_name(s: &str) -> Option<Self> {
        Some(match s {
            "gncCustomer" => Self::Customer,
            "gncJob" => Self::Job,
            "gncVendor" => Self::Vendor,
            "gncEmployee" => Self::Employee,
            _ => return None,
        })
    }

    /// `GncOwnerType` as stored in the lot slot `gncOwner/owner-type` (`5.17:libgnucash/engine/gncOwner.h:45-53`).
    pub fn owner_type_number(self) -> i64 {
        match self {
            Self::Customer => 2,
            Self::Job => 3,
            Self::Vendor => 4,
            Self::Employee => 5,
        }
    }

    /// Parse the number stored in `gncOwner/owner-type`.
    pub fn from_owner_type_number(n: i64) -> Option<Self> {
        Some(match n {
            2 => Self::Customer,
            3 => Self::Job,
            4 => Self::Vendor,
            5 => Self::Employee,
            _ => return None,
        })
    }
}

/// A reference to an owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OwnerRef {
    /// Which kind of owner.
    pub kind: OwnerKind,
    /// Its GUID.
    pub guid: Guid,
}

/// Whether prices include tax (`GncTaxIncluded`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaxIncluded {
    /// `YES`.
    Yes,
    /// `NO`.
    No,
    /// `USEGLOBAL`: use the book's default.
    #[default]
    UseGlobal,
}

impl TaxIncluded {
    /// The word written in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "YES",
            Self::No => "NO",
            Self::UseGlobal => "USEGLOBAL",
        }
    }

    /// Parse the word written in files.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "YES" => Self::Yes,
            "NO" => Self::No,
            "USEGLOBAL" => Self::UseGlobal,
            _ => return None,
        })
    }
}

/// A percentage or a fixed amount (`GncAmountType`): tax table lines and entry discounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AmountType {
    /// `VALUE`: a fixed amount.
    Value,
    /// `PERCENT`: a percentage.
    #[default]
    Percent,
}

impl AmountType {
    /// The word written in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Value => "VALUE",
            Self::Percent => "PERCENT",
        }
    }

    /// Parse the word written in files.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "VALUE" => Self::Value,
            "PERCENT" => Self::Percent,
            _ => return None,
        })
    }
}

/// When a discount applies relative to tax (`GncDiscountHow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiscountHow {
    /// `PRETAX`: discount, then tax on the discounted amount.
    #[default]
    PreTax,
    /// `SAMETIME`: tax and discount both on the undiscounted amount.
    SameTime,
    /// `POSTTAX`: tax first, then discount on the taxed amount.
    PostTax,
}

impl DiscountHow {
    /// The word written in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreTax => "PRETAX",
            Self::SameTime => "SAMETIME",
            Self::PostTax => "POSTTAX",
        }
    }

    /// Parse the word written in files.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "PRETAX" => Self::PreTax,
            "SAMETIME" => Self::SameTime,
            "POSTTAX" => Self::PostTax,
            _ => return None,
        })
    }
}

/// How an employee paid an expense (`GncEntryPaymentType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaymentType {
    /// `CASH`: the employee paid and is owed.
    #[default]
    Cash,
    /// `CARD`: paid with the company card.
    Card,
}

impl PaymentType {
    /// The word written in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "CASH",
            Self::Card => "CARD",
        }
    }

    /// Parse the word written in files.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "CASH" => Self::Cash,
            "CARD" => Self::Card,
            _ => return None,
        })
    }
}

/// A customer. `5.17:libgnucash/engine/gncCustomer.c`.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub struct Customer {
    pub guid: Guid,
    pub name: String,
    /// The customer number; customers with an empty ID are not saved.
    pub id: String,
    pub addr: Address,
    pub ship_addr: Address,
    pub notes: String,
    pub terms: Option<Guid>,
    pub tax_included: TaxIncluded,
    pub active: bool,
    /// Discount percentage.
    pub discount: Numeric,
    /// Credit limit.
    pub credit: Numeric,
    pub currency: Option<CommodityRef>,
    /// Use `tax_table` instead of the entries' own.
    pub use_tax_table: bool,
    pub tax_table: Option<Guid>,
    pub slots: Frame,
}

/// A vendor. `5.17:libgnucash/engine/gncVendor.c`.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub struct Vendor {
    pub guid: Guid,
    pub name: String,
    pub id: String,
    pub addr: Address,
    pub notes: String,
    pub terms: Option<Guid>,
    pub tax_included: TaxIncluded,
    pub active: bool,
    pub currency: Option<CommodityRef>,
    pub use_tax_table: bool,
    pub tax_table: Option<Guid>,
    pub slots: Frame,
}

/// An employee. The display name is the address name. `5.17:libgnucash/engine/gncEmployee.c`.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub struct Employee {
    pub guid: Guid,
    pub username: String,
    pub id: String,
    pub addr: Address,
    pub language: String,
    pub acl: String,
    pub active: bool,
    /// Hours per working day.
    pub workday: Numeric,
    /// Hourly rate.
    pub rate: Numeric,
    pub currency: Option<CommodityRef>,
    /// The company credit-card account for card-paid expenses.
    pub ccard: Option<Guid>,
    pub slots: Frame,
}

/// A job. `5.17:libgnucash/engine/gncJob.c`. The rate lives in the `job-rate` slot.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub struct Job {
    pub guid: Guid,
    pub id: String,
    pub name: String,
    pub reference: String,
    /// A customer or vendor.
    pub owner: OwnerRef,
    pub active: bool,
    pub slots: Frame,
}

/// An order. `5.17:libgnucash/engine/gncOrder.c`.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub struct Order {
    pub guid: Guid,
    pub id: String,
    pub owner: OwnerRef,
    pub opened: Time64,
    pub closed: Option<Time64>,
    pub notes: String,
    pub reference: String,
    pub active: bool,
    pub slots: Frame,
}

/// One line of a tax table.
#[derive(Debug, Clone, PartialEq)]
pub struct TaxTableEntry {
    /// The account the tax is posted to.
    pub account: Option<Guid>,
    /// The percentage or amount.
    pub amount: Numeric,
    /// Percentage or fixed value.
    pub kind: AmountType,
}

/// A tax table. Posting a document freezes a copy (a *child*) of each table it uses. `5.17:libgnucash/engine/gncTaxTable.c`.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub struct TaxTable {
    pub guid: Guid,
    pub name: String,
    /// Number of owners and entries referring to the table.
    pub refcount: i64,
    pub invisible: bool,
    pub child: Option<Guid>,
    pub parent: Option<Guid>,
    pub entries: Vec<TaxTableEntry>,
    pub slots: Frame,
}

/// The due-date rule of a billing term.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BillTermKind {
    /// Due a number of days after posting.
    Days {
        /// Days until due.
        due_days: i64,
        /// Days within which the discount applies.
        disc_days: i64,
        /// Early-payment discount percentage.
        discount: Numeric,
    },
    /// Due on a day of the next month (or the one after, past the cutoff day).
    Proximo {
        /// Day of month the document is due.
        due_day: i64,
        /// Day of month the discount ends.
        disc_day: i64,
        /// Early-payment discount percentage.
        discount: Numeric,
        /// Documents posted after this day of month roll to the following month.
        cutoff_day: i64,
    },
}

/// A billing term. `5.17:libgnucash/engine/gncBillTerm.c`.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub struct BillTerm {
    pub guid: Guid,
    pub name: String,
    pub description: String,
    pub refcount: i64,
    pub invisible: bool,
    pub slots: Frame,
    pub child: Option<Guid>,
    pub parent: Option<Guid>,
    /// `None` when the type was never set.
    pub kind: Option<BillTermKind>,
}

/// One line of an invoice, bill, voucher or order. `5.17:libgnucash/engine/gncEntry.c`.
///
/// The `i_` fields apply when the entry is on a customer invoice, the `b_` fields when it is on a bill or voucher.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs, clippy::struct_excessive_bools)] // Mirrors GnuCash's entry fields one for one.
pub struct Entry {
    pub guid: Guid,
    pub date: Time64,
    pub entered: Time64,
    pub description: String,
    pub action: String,
    pub notes: String,
    pub quantity: Numeric,
    pub i_account: Option<Guid>,
    pub i_price: Numeric,
    pub i_discount: Numeric,
    pub invoice: Option<Guid>,
    pub i_disc_type: AmountType,
    pub i_disc_how: DiscountHow,
    pub i_taxable: bool,
    pub i_tax_included: bool,
    pub i_tax_table: Option<Guid>,
    pub b_account: Option<Guid>,
    pub b_price: Numeric,
    pub bill: Option<Guid>,
    pub billable: bool,
    pub bill_to: Option<OwnerRef>,
    pub b_taxable: bool,
    pub b_tax_included: bool,
    pub b_payment: PaymentType,
    pub b_tax_table: Option<Guid>,
    pub order: Option<Guid>,
    pub slots: Frame,
}

impl Entry {
    /// A blank entry with GnuCash's defaults (`5.17:libgnucash/engine/gncEntry.c:413-444`).
    pub fn new(guid: Guid, date: Time64, entered: Time64) -> Self {
        Self {
            guid,
            date,
            entered,
            description: String::new(),
            action: String::new(),
            notes: String::new(),
            quantity: Numeric::ZERO,
            i_account: None,
            i_price: Numeric::ZERO,
            i_discount: Numeric::ZERO,
            invoice: None,
            i_disc_type: AmountType::Percent,
            i_disc_how: DiscountHow::PreTax,
            i_taxable: true,
            i_tax_included: false,
            i_tax_table: None,
            b_account: None,
            b_price: Numeric::ZERO,
            bill: None,
            billable: false,
            bill_to: None,
            b_taxable: true,
            b_tax_included: false,
            b_payment: PaymentType::Cash,
            b_tax_table: None,
            order: None,
            slots: Frame::new(),
        }
    }
}

/// An invoice, bill or expense voucher (GnuCash's `GncInvoice`). `5.17:libgnucash/engine/gncInvoice.c`.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub struct Invoice {
    pub guid: Guid,
    pub id: String,
    pub owner: OwnerRef,
    pub opened: Time64,
    pub posted: Option<Time64>,
    pub terms: Option<Guid>,
    pub billing_id: String,
    pub notes: String,
    pub active: bool,
    pub post_txn: Option<Guid>,
    pub post_lot: Option<Guid>,
    pub post_acc: Option<Guid>,
    pub currency: Option<CommodityRef>,
    pub bill_to: Option<OwnerRef>,
    /// The amount to charge back (vouchers).
    pub charge_amount: Numeric,
    pub slots: Frame,
}

impl Invoice {
    /// True when the document is a credit note (`credit-note` slot is a non-zero integer).
    pub fn is_credit_note(&self) -> bool {
        self.slots.get("credit-note").and_then(crate::SlotValue::as_int).is_some_and(|v| v != 0)
    }

    /// True once posted.
    pub fn is_posted(&self) -> bool {
        self.post_txn.is_some()
    }
}

/// The business objects of a Book, each kept in GUID order: the order GnuCash writes them
/// (`qof_object_foreach_sorted`, `5.17:libgnucash/engine/qofobject.cpp:223-237`).
#[derive(Debug, Clone, PartialEq, Default)]
#[allow(missing_docs)]
pub struct Business {
    pub bill_terms: BTreeMap<Guid, Arc<BillTerm>>,
    pub customers: BTreeMap<Guid, Arc<Customer>>,
    pub employees: BTreeMap<Guid, Arc<Employee>>,
    pub entries: BTreeMap<Guid, Arc<Entry>>,
    pub invoices: BTreeMap<Guid, Arc<Invoice>>,
    pub jobs: BTreeMap<Guid, Arc<Job>>,
    pub orders: BTreeMap<Guid, Arc<Order>>,
    pub tax_tables: BTreeMap<Guid, Arc<TaxTable>>,
    pub vendors: BTreeMap<Guid, Arc<Vendor>>,
}
