//! Methods: business contacts (customers, vendors, employees and jobs), and the wire types the business methods
//! share. See `design/api/business-contacts.md`.
//!
//! There are no delete methods: GnuCash never deletes a saved contact, it deactivates it (`active: false`).

// Optional fields that can be cleared are three-state on the wire: absent (keep), null (clear) or a value.
#![allow(clippy::option_option)]

use chrono::{Datelike, Local, NaiveDate, TimeZone};
use red_ale_core::business::{Address, Customer, Employee, Job, OwnerKind, OwnerRef, TaxIncluded, Vendor};
use red_ale_core::ops::business::contacts::{
    add_customer, add_employee, add_job, add_vendor, find_owner, job_rate, set_job_rate, update_customer, update_employee, update_job,
    update_vendor,
};
use red_ale_core::{Book, Date, Frame, Guid, Numeric, Time64};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use super::terms::{BillTermRef, TaxTableRef};
use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, CommodityId, Id};

/// The business contacts methods.
pub fn methods() -> Vec<Method> {
    vec![
        method("customer.create", "Create a customer; its ID comes from the book's counter unless given.", Effect::Write, customer_create),
        method("customer.get", "A customer.", Effect::Read, customer_get),
        method("customer.list", "Customers, optionally filtered by name, ID or active state.", Effect::Read, customer_list),
        method("customer.update", "Change a customer; `active: false` deactivates it.", Effect::Write, customer_update),
        method("vendor.create", "Create a vendor; its ID comes from the book's counter unless given.", Effect::Write, vendor_create),
        method("vendor.get", "A vendor.", Effect::Read, vendor_get),
        method("vendor.list", "Vendors, optionally filtered by name, ID or active state.", Effect::Read, vendor_list),
        method("vendor.update", "Change a vendor; `active: false` deactivates it.", Effect::Write, vendor_update),
        method("employee.create", "Create an employee; its ID comes from the book's counter unless given.", Effect::Write, employee_create),
        method("employee.get", "An employee.", Effect::Read, employee_get),
        method("employee.list", "Employees, optionally filtered by name, ID or active state.", Effect::Read, employee_list),
        method("employee.update", "Change an employee; `active: false` deactivates it.", Effect::Write, employee_update),
        method(
            "job.create",
            "Create a job for a customer or vendor; its ID comes from the book's counter unless given.",
            Effect::Write,
            job_create,
        ),
        method("job.get", "A job.", Effect::Read, job_get),
        method("job.list", "Jobs, optionally filtered by owner, name, ID or active state.", Effect::Read, job_list),
        method("job.update", "Change a job; `active: false` deactivates it.", Effect::Write, job_update),
    ]
}

// ---------------------------------------------------------------------------------------------------------------
// Shared wire helpers for the business methods.

/// Declares a create params struct and an update params struct with the same optional fields; the update struct
/// also names the entity it changes. `split` turns an update into the entity's key and the shared fields.
macro_rules! params_pair {
    (
        $(#[$cm:meta])* $create:ident,
        $(#[$um:meta])* $update:ident,
        $(#[$km:meta])* $key:ident: $kt:ty,
        { $($(#[$m:meta])* $f:ident: $t:ty,)* }
    ) => {
        $(#[$cm])*
        #[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        pub struct $create {
            $($(#[$m])* #[serde(default)] pub $f: $t,)*
        }

        $(#[$um])*
        #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        pub struct $update {
            $(#[$km])*
            pub $key: $kt,
            $($(#[$m])* #[serde(default)] pub $f: $t,)*
        }

        impl $update {
            /// The key and the fields to change.
            pub fn split(self) -> ($kt, $create) {
                (self.$key, $create { $($f: self.$f,)* })
            }
        }
    };
}
pub(crate) use params_pair;

/// Read a field that may be absent (no change), `null` (clear it) or a value.
pub(crate) fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// GnuCash stores a date chosen in its dialogs as noon local time (`time64CanonicalDayTime`,
/// `5.17:libgnucash/engine/gnc-date.cpp:404-410`); red-ale does the same in the server's time zone.
pub(crate) fn local_noon(date: Date) -> Time64 {
    NaiveDate::from_ymd_opt(date.year(), date.month(), date.day())
        .and_then(|d| d.and_hms_opt(12, 0, 0))
        .and_then(|dt| Local.from_local_datetime(&dt).earliest())
        .map_or_else(|| date.neutral_time(), |dt| Time64(dt.timestamp()))
}

/// Today's date in the server's time zone.
pub(crate) fn local_today(now: Time64) -> Result<Date, RpcError> {
    Local
        .timestamp_opt(now.0, 0)
        .single()
        .and_then(|dt| Date::from_ymd(dt.year(), dt.month(), dt.day()))
        .ok_or_else(|| RpcError::internal("the clock is out of range"))
}

/// What kind of owner a GUID names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum OwnerKindWire {
    /// A customer.
    Customer,
    /// A vendor.
    Vendor,
    /// An employee.
    Employee,
    /// A job (of a customer or vendor).
    Job,
}

impl From<OwnerKind> for OwnerKindWire {
    fn from(k: OwnerKind) -> Self {
        match k {
            OwnerKind::Customer => Self::Customer,
            OwnerKind::Vendor => Self::Vendor,
            OwnerKind::Employee => Self::Employee,
            OwnerKind::Job => Self::Job,
        }
    }
}

/// An owner: a customer, vendor, employee or job.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnerInfo {
    /// Its kind.
    pub kind: OwnerKindWire,
    /// Its GUID.
    pub guid: Id,
}

impl From<OwnerRef> for OwnerInfo {
    fn from(o: OwnerRef) -> Self {
        Self { kind: o.kind.into(), guid: o.guid.into() }
    }
}

/// The owner a GUID names, of whatever kind.
pub(crate) fn resolve_owner(book: &Book, id: Id) -> Result<OwnerRef, RpcError> {
    find_owner(book, id.0).ok_or_else(|| RpcError::not_found("owner", id.0))
}

/// Address fields to set; absent fields keep their value.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct AddressParams {
    /// The addressee.
    #[serde(default)]
    pub name: Option<String>,
    /// First address line.
    #[serde(default)]
    pub addr1: Option<String>,
    /// Second address line.
    #[serde(default)]
    pub addr2: Option<String>,
    /// Third address line.
    #[serde(default)]
    pub addr3: Option<String>,
    /// Fourth address line.
    #[serde(default)]
    pub addr4: Option<String>,
    /// Telephone.
    #[serde(default)]
    pub phone: Option<String>,
    /// Fax.
    #[serde(default)]
    pub fax: Option<String>,
    /// Email.
    #[serde(default)]
    pub email: Option<String>,
}

impl AddressParams {
    fn apply(self, a: &mut Address) {
        let set = |target: &mut String, v: Option<String>| {
            if let Some(v) = v {
                *target = v;
            }
        };
        set(&mut a.name, self.name);
        set(&mut a.addr1, self.addr1);
        set(&mut a.addr2, self.addr2);
        set(&mut a.addr3, self.addr3);
        set(&mut a.addr4, self.addr4);
        set(&mut a.phone, self.phone);
        set(&mut a.fax, self.fax);
        set(&mut a.email, self.email);
    }
}

/// An address. Empty strings are unset fields.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct AddressInfo {
    pub name: String,
    pub addr1: String,
    pub addr2: String,
    pub addr3: String,
    pub addr4: String,
    pub phone: String,
    pub fax: String,
    pub email: String,
}

impl From<&Address> for AddressInfo {
    fn from(a: &Address) -> Self {
        Self {
            name: a.name.clone(),
            addr1: a.addr1.clone(),
            addr2: a.addr2.clone(),
            addr3: a.addr3.clone(),
            addr4: a.addr4.clone(),
            phone: a.phone.clone(),
            fax: a.fax.clone(),
            email: a.email.clone(),
        }
    }
}

/// Whether prices include tax by default for the contact's documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TaxIncludedWire {
    /// `YES`.
    Yes,
    /// `NO`.
    No,
    /// `USEGLOBAL`: the user's preference.
    UseGlobal,
}

impl From<TaxIncludedWire> for TaxIncluded {
    fn from(t: TaxIncludedWire) -> Self {
        match t {
            TaxIncludedWire::Yes => Self::Yes,
            TaxIncludedWire::No => Self::No,
            TaxIncludedWire::UseGlobal => Self::UseGlobal,
        }
    }
}

impl From<TaxIncluded> for TaxIncludedWire {
    fn from(t: TaxIncluded) -> Self {
        match t {
            TaxIncluded::Yes => Self::Yes,
            TaxIncluded::No => Self::No,
            TaxIncluded::UseGlobal => Self::UseGlobal,
        }
    }
}

/// Filters for the contact lists. All given filters must match.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListParams {
    /// Only active contacts.
    #[serde(default)]
    pub active_only: bool,
    /// Case-insensitive part of the name.
    #[serde(default)]
    pub name: Option<String>,
    /// The exact ID. IDs are not unique in GnuCash, so several may match.
    #[serde(default)]
    pub id: Option<String>,
}

impl ListParams {
    fn matches(&self, name: &str, id: &str, active: bool) -> bool {
        (!self.active_only || active)
            && self.name.as_ref().is_none_or(|n| name.to_lowercase().contains(&n.to_lowercase()))
            && self.id.as_ref().is_none_or(|i| i == id)
    }
}

fn set<T>(target: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *target = v;
    }
}

fn resolve_term(book: &Book, r: Option<Option<BillTermRef>>, target: &mut Option<Guid>) -> Result<(), RpcError> {
    if let Some(r) = r {
        *target = r.map(|r| r.resolve(book)).transpose()?;
    }
    Ok(())
}

fn resolve_table(book: &Book, r: Option<Option<TaxTableRef>>, target: &mut Option<Guid>) -> Result<(), RpcError> {
    if let Some(r) = r {
        *target = r.map(|r| r.resolve(book)).transpose()?;
    }
    Ok(())
}

fn blank_address() -> Address {
    Address::default()
}

// ---------------------------------------------------------------------------------------------------------------
// Customers.

params_pair!(
    /// Params of `customer.create`. `name` and `currency` are required.
    CustomerFields,
    /// Params of `customer.update`: the fields to change.
    CustomerUpdate,
    /// The customer.
    customer: Id,
    {
        /// Company name.
        name: Option<String>,
        /// Customer number. On create, absent or empty takes the next number from the book's `gncCustomer` counter.
        id: Option<String>,
        /// The currency of the customer's documents.
        currency: Option<CommodityId>,
        /// Billing address.
        addr: Option<AddressParams>,
        /// Shipping address.
        ship_addr: Option<AddressParams>,
        /// Notes.
        notes: Option<String>,
        /// Default billing term for new invoices, by GUID or name; `null` for none.
        #[serde(deserialize_with = "nullable")]
        terms: Option<Option<BillTermRef>>,
        /// Tax table, by GUID or name; `null` for none.
        #[serde(deserialize_with = "nullable")]
        tax_table: Option<Option<TaxTableRef>>,
        /// New invoice lines use `taxTable` rather than the book's default.
        tax_table_override: Option<bool>,
        /// Whether prices include tax by default.
        tax_included: Option<TaxIncludedWire>,
        /// Default discount percentage for new invoice lines, 0 to 100.
        discount: Option<Amount>,
        /// Credit limit.
        credit: Option<Amount>,
        /// Active; `false` deactivates.
        active: Option<bool>,
    }
);

/// A customer.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct CustomerInfo {
    pub guid: Id,
    pub id: String,
    pub name: String,
    pub addr: AddressInfo,
    pub ship_addr: AddressInfo,
    pub notes: String,
    pub currency: Option<CommodityId>,
    pub terms: Option<Id>,
    pub tax_table: Option<Id>,
    pub tax_table_override: bool,
    pub tax_included: TaxIncludedWire,
    pub discount: Amount,
    pub credit: Amount,
    pub active: bool,
}

impl From<&Customer> for CustomerInfo {
    fn from(c: &Customer) -> Self {
        Self {
            guid: c.guid.into(),
            id: c.id.clone(),
            name: c.name.clone(),
            addr: (&c.addr).into(),
            ship_addr: (&c.ship_addr).into(),
            notes: c.notes.clone(),
            currency: c.currency.clone().map(Into::into),
            terms: c.terms.map(Into::into),
            tax_table: c.tax_table.map(Into::into),
            tax_table_override: c.use_tax_table,
            tax_included: c.tax_included.into(),
            discount: c.discount.into(),
            credit: c.credit.into(),
            active: c.active,
        }
    }
}

/// Customers.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CustomerList {
    /// In GUID order.
    pub customers: Vec<CustomerInfo>,
}

/// Names one customer.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CustomerKey {
    /// The customer.
    pub customer: Id,
}

impl CustomerFields {
    fn apply(self, book: &Book, c: &mut Customer) -> Result<(), RpcError> {
        set(&mut c.name, self.name);
        set(&mut c.id, self.id);
        if let Some(cur) = self.currency {
            c.currency = Some(cur.0);
        }
        if let Some(a) = self.addr {
            a.apply(&mut c.addr);
        }
        if let Some(a) = self.ship_addr {
            a.apply(&mut c.ship_addr);
        }
        set(&mut c.notes, self.notes);
        resolve_term(book, self.terms, &mut c.terms)?;
        resolve_table(book, self.tax_table, &mut c.tax_table)?;
        set(&mut c.use_tax_table, self.tax_table_override);
        set(&mut c.tax_included, self.tax_included.map(Into::into));
        set(&mut c.discount, self.discount.map(|a| a.0));
        set(&mut c.credit, self.credit.map(|a| a.0));
        set(&mut c.active, self.active);
        Ok(())
    }
}

/// A new customer with GnuCash's defaults (`gncCustomerCreate`, `5.17:libgnucash/engine/gncCustomer.c:303-331`).
fn new_customer(guid: Guid) -> Customer {
    Customer {
        guid,
        name: String::new(),
        id: String::new(),
        addr: blank_address(),
        ship_addr: blank_address(),
        notes: String::new(),
        terms: None,
        tax_included: TaxIncluded::UseGlobal,
        active: true,
        discount: Numeric::ZERO,
        credit: Numeric::ZERO,
        currency: None,
        use_tax_table: false,
        tax_table: None,
        slots: Frame::new(),
    }
}

fn get_customer(book: &Book, guid: Guid) -> Result<CustomerInfo, RpcError> {
    book.business.customers.get(&guid).map(|c| c.as_ref().into()).ok_or_else(|| RpcError::not_found("customer", guid))
}

fn customer_create(ctx: &mut Context, p: CustomerFields) -> Result<CustomerInfo, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let mut c = new_customer(env.new_guid());
        p.apply(book, &mut c)?;
        let guid = add_customer(book, c)?;
        get_customer(book, guid)
    })
}

fn customer_get(ctx: &mut Context, p: CustomerKey) -> Result<CustomerInfo, RpcError> {
    get_customer(ctx.book()?, p.customer.0)
}

fn customer_list(ctx: &mut Context, p: ListParams) -> Result<CustomerList, RpcError> {
    let book = ctx.book()?;
    let customers = book.business.customers.values().filter(|c| p.matches(&c.name, &c.id, c.active)).map(|c| c.as_ref().into()).collect();
    Ok(CustomerList { customers })
}

fn customer_update(ctx: &mut Context, p: CustomerUpdate) -> Result<CustomerInfo, RpcError> {
    let (key, fields) = p.split();
    ctx.mutate_rpc(|book, _| {
        let mut c = book.business.customers.get(&key.0).map(|c| (**c).clone()).ok_or_else(|| RpcError::not_found("customer", key.0))?;
        fields.apply(book, &mut c)?;
        update_customer(book, c)?;
        get_customer(book, key.0)
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Vendors.

params_pair!(
    /// Params of `vendor.create`. `name` and `currency` are required.
    VendorFields,
    /// Params of `vendor.update`: the fields to change.
    VendorUpdate,
    /// The vendor.
    vendor: Id,
    {
        /// Company name.
        name: Option<String>,
        /// Vendor number. On create, absent or empty takes the next number from the book's `gncVendor` counter.
        id: Option<String>,
        /// The currency of the vendor's documents.
        currency: Option<CommodityId>,
        /// Payment address.
        addr: Option<AddressParams>,
        /// Notes.
        notes: Option<String>,
        /// Default billing term for new bills, by GUID or name; `null` for none.
        #[serde(deserialize_with = "nullable")]
        terms: Option<Option<BillTermRef>>,
        /// Tax table, by GUID or name; `null` for none.
        #[serde(deserialize_with = "nullable")]
        tax_table: Option<Option<TaxTableRef>>,
        /// New bill lines use `taxTable` rather than the book's default.
        tax_table_override: Option<bool>,
        /// Whether prices include tax by default.
        tax_included: Option<TaxIncludedWire>,
        /// Active; `false` deactivates.
        active: Option<bool>,
    }
);

/// A vendor.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct VendorInfo {
    pub guid: Id,
    pub id: String,
    pub name: String,
    pub addr: AddressInfo,
    pub notes: String,
    pub currency: Option<CommodityId>,
    pub terms: Option<Id>,
    pub tax_table: Option<Id>,
    pub tax_table_override: bool,
    pub tax_included: TaxIncludedWire,
    pub active: bool,
}

impl From<&Vendor> for VendorInfo {
    fn from(v: &Vendor) -> Self {
        Self {
            guid: v.guid.into(),
            id: v.id.clone(),
            name: v.name.clone(),
            addr: (&v.addr).into(),
            notes: v.notes.clone(),
            currency: v.currency.clone().map(Into::into),
            terms: v.terms.map(Into::into),
            tax_table: v.tax_table.map(Into::into),
            tax_table_override: v.use_tax_table,
            tax_included: v.tax_included.into(),
            active: v.active,
        }
    }
}

/// Vendors.
#[derive(Debug, Serialize, JsonSchema)]
pub struct VendorList {
    /// In GUID order.
    pub vendors: Vec<VendorInfo>,
}

/// Names one vendor.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VendorKey {
    /// The vendor.
    pub vendor: Id,
}

impl VendorFields {
    fn apply(self, book: &Book, v: &mut Vendor) -> Result<(), RpcError> {
        set(&mut v.name, self.name);
        set(&mut v.id, self.id);
        if let Some(cur) = self.currency {
            v.currency = Some(cur.0);
        }
        if let Some(a) = self.addr {
            a.apply(&mut v.addr);
        }
        set(&mut v.notes, self.notes);
        resolve_term(book, self.terms, &mut v.terms)?;
        resolve_table(book, self.tax_table, &mut v.tax_table)?;
        set(&mut v.use_tax_table, self.tax_table_override);
        set(&mut v.tax_included, self.tax_included.map(Into::into));
        set(&mut v.active, self.active);
        Ok(())
    }
}

/// A new vendor with GnuCash's defaults (`gncVendorCreate`, `5.17:libgnucash/engine/gncVendor.c:453-476`).
fn new_vendor(guid: Guid) -> Vendor {
    Vendor {
        guid,
        name: String::new(),
        id: String::new(),
        addr: blank_address(),
        notes: String::new(),
        terms: None,
        tax_included: TaxIncluded::UseGlobal,
        active: true,
        currency: None,
        use_tax_table: false,
        tax_table: None,
        slots: Frame::new(),
    }
}

fn get_vendor(book: &Book, guid: Guid) -> Result<VendorInfo, RpcError> {
    book.business.vendors.get(&guid).map(|v| v.as_ref().into()).ok_or_else(|| RpcError::not_found("vendor", guid))
}

fn vendor_create(ctx: &mut Context, p: VendorFields) -> Result<VendorInfo, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let mut v = new_vendor(env.new_guid());
        p.apply(book, &mut v)?;
        let guid = add_vendor(book, v)?;
        get_vendor(book, guid)
    })
}

fn vendor_get(ctx: &mut Context, p: VendorKey) -> Result<VendorInfo, RpcError> {
    get_vendor(ctx.book()?, p.vendor.0)
}

fn vendor_list(ctx: &mut Context, p: ListParams) -> Result<VendorList, RpcError> {
    let book = ctx.book()?;
    let vendors = book.business.vendors.values().filter(|v| p.matches(&v.name, &v.id, v.active)).map(|v| v.as_ref().into()).collect();
    Ok(VendorList { vendors })
}

fn vendor_update(ctx: &mut Context, p: VendorUpdate) -> Result<VendorInfo, RpcError> {
    let (key, fields) = p.split();
    ctx.mutate_rpc(|book, _| {
        let mut v = book.business.vendors.get(&key.0).map(|v| (**v).clone()).ok_or_else(|| RpcError::not_found("vendor", key.0))?;
        fields.apply(book, &mut v)?;
        update_vendor(book, v)?;
        get_vendor(book, key.0)
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Employees.

params_pair!(
    /// Params of `employee.create`. `name` and `currency` are required.
    EmployeeFields,
    /// Params of `employee.update`: the fields to change.
    EmployeeUpdate,
    /// The employee.
    employee: Id,
    {
        /// The employee's name, which GnuCash keeps as the address name; it overrides `addr.name`.
        name: Option<String>,
        /// Employee number. On create, absent or empty takes the next number from the book's `gncEmployee` counter.
        id: Option<String>,
        /// User name.
        username: Option<String>,
        /// The currency of the employee's expense vouchers.
        currency: Option<CommodityId>,
        /// Payment address.
        addr: Option<AddressParams>,
        /// Language.
        language: Option<String>,
        /// Access control list (stored only).
        acl: Option<String>,
        /// Working hours per day.
        workday: Option<Amount>,
        /// Hourly rate.
        rate: Option<Amount>,
        /// The company credit card account (type CREDIT) card-paid expenses are charged to; `null` for none.
        #[serde(deserialize_with = "nullable")]
        credit_card_account: Option<Option<AccountRef>>,
        /// Active; `false` deactivates.
        active: Option<bool>,
    }
);

/// An employee.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct EmployeeInfo {
    pub guid: Id,
    pub id: String,
    /// The address name.
    pub name: String,
    pub username: String,
    pub addr: AddressInfo,
    pub language: String,
    pub acl: String,
    pub workday: Amount,
    pub rate: Amount,
    pub currency: Option<CommodityId>,
    pub credit_card_account: Option<Id>,
    pub active: bool,
}

impl From<&Employee> for EmployeeInfo {
    fn from(e: &Employee) -> Self {
        Self {
            guid: e.guid.into(),
            id: e.id.clone(),
            name: e.addr.name.clone(),
            username: e.username.clone(),
            addr: (&e.addr).into(),
            language: e.language.clone(),
            acl: e.acl.clone(),
            workday: e.workday.into(),
            rate: e.rate.into(),
            currency: e.currency.clone().map(Into::into),
            credit_card_account: e.ccard.map(Into::into),
            active: e.active,
        }
    }
}

/// Employees.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EmployeeList {
    /// In GUID order.
    pub employees: Vec<EmployeeInfo>,
}

/// Names one employee.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EmployeeKey {
    /// The employee.
    pub employee: Id,
}

impl EmployeeFields {
    fn apply(self, book: &Book, e: &mut Employee) -> Result<(), RpcError> {
        if let Some(a) = self.addr {
            a.apply(&mut e.addr);
        }
        set(&mut e.addr.name, self.name);
        set(&mut e.id, self.id);
        set(&mut e.username, self.username);
        if let Some(cur) = self.currency {
            e.currency = Some(cur.0);
        }
        set(&mut e.language, self.language);
        set(&mut e.acl, self.acl);
        set(&mut e.workday, self.workday.map(|a| a.0));
        set(&mut e.rate, self.rate.map(|a| a.0));
        if let Some(acc) = self.credit_card_account {
            e.ccard = acc.map(|a| a.resolve(book)).transpose()?;
        }
        set(&mut e.active, self.active);
        Ok(())
    }
}

/// A new employee with GnuCash's defaults (`gncEmployeeCreate`, `5.17:libgnucash/engine/gncEmployee.c:428-453`).
fn new_employee(guid: Guid) -> Employee {
    Employee {
        guid,
        username: String::new(),
        id: String::new(),
        addr: blank_address(),
        language: String::new(),
        acl: String::new(),
        active: true,
        workday: Numeric::ZERO,
        rate: Numeric::ZERO,
        currency: None,
        ccard: None,
        slots: Frame::new(),
    }
}

fn get_employee(book: &Book, guid: Guid) -> Result<EmployeeInfo, RpcError> {
    book.business.employees.get(&guid).map(|e| e.as_ref().into()).ok_or_else(|| RpcError::not_found("employee", guid))
}

fn employee_create(ctx: &mut Context, p: EmployeeFields) -> Result<EmployeeInfo, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let mut e = new_employee(env.new_guid());
        p.apply(book, &mut e)?;
        let guid = add_employee(book, e)?;
        get_employee(book, guid)
    })
}

fn employee_get(ctx: &mut Context, p: EmployeeKey) -> Result<EmployeeInfo, RpcError> {
    get_employee(ctx.book()?, p.employee.0)
}

fn employee_list(ctx: &mut Context, p: ListParams) -> Result<EmployeeList, RpcError> {
    let book = ctx.book()?;
    let employees =
        book.business.employees.values().filter(|e| p.matches(&e.addr.name, &e.id, e.active)).map(|e| e.as_ref().into()).collect();
    Ok(EmployeeList { employees })
}

fn employee_update(ctx: &mut Context, p: EmployeeUpdate) -> Result<EmployeeInfo, RpcError> {
    let (key, fields) = p.split();
    ctx.mutate_rpc(|book, _| {
        let mut e = book.business.employees.get(&key.0).map(|e| (**e).clone()).ok_or_else(|| RpcError::not_found("employee", key.0))?;
        fields.apply(book, &mut e)?;
        update_employee(book, e)?;
        get_employee(book, key.0)
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Jobs.

params_pair!(
    /// Params of `job.create`. `name` and `owner` are required.
    JobFields,
    /// Params of `job.update`: the fields to change.
    JobUpdate,
    /// The job.
    job: Id,
    {
        /// Job name.
        name: Option<String>,
        /// Job number. On create, absent or empty takes the next number from the book's `gncJob` counter.
        id: Option<String>,
        /// The customer or vendor the job is for. It cannot change once the job has documents.
        owner: Option<Id>,
        /// Reference (GnuCash's job description).
        reference: Option<String>,
        /// Rate: the default price of new lines on the job's documents. Zero removes it.
        rate: Option<Amount>,
        /// Active; `false` deactivates.
        active: Option<bool>,
    }
);

/// A job.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct JobInfo {
    pub guid: Id,
    pub id: String,
    pub name: String,
    pub owner: OwnerInfo,
    pub reference: String,
    pub rate: Amount,
    pub active: bool,
}

impl From<&Job> for JobInfo {
    fn from(j: &Job) -> Self {
        Self {
            guid: j.guid.into(),
            id: j.id.clone(),
            name: j.name.clone(),
            owner: j.owner.into(),
            reference: j.reference.clone(),
            rate: job_rate(j).into(),
            active: j.active,
        }
    }
}

/// Jobs.
#[derive(Debug, Serialize, JsonSchema)]
pub struct JobList {
    /// In GUID order.
    pub jobs: Vec<JobInfo>,
}

/// Names one job.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct JobKey {
    /// The job.
    pub job: Id,
}

/// Filters for `job.list`. All given filters must match.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct JobListParams {
    /// Only this customer's or vendor's jobs.
    #[serde(default)]
    pub owner: Option<Id>,
    /// Only active jobs.
    #[serde(default)]
    pub active_only: bool,
    /// Case-insensitive part of the name.
    #[serde(default)]
    pub name: Option<String>,
    /// The exact ID.
    #[serde(default)]
    pub id: Option<String>,
}

impl JobFields {
    fn apply(self, book: &Book, j: &mut Job) -> Result<(), RpcError> {
        set(&mut j.name, self.name);
        set(&mut j.id, self.id);
        if let Some(o) = self.owner {
            j.owner = resolve_owner(book, o)?;
        }
        set(&mut j.reference, self.reference);
        if let Some(r) = self.rate {
            set_job_rate(j, r.0);
        }
        set(&mut j.active, self.active);
        Ok(())
    }
}

fn get_job(book: &Book, guid: Guid) -> Result<JobInfo, RpcError> {
    book.business.jobs.get(&guid).map(|j| j.as_ref().into()).ok_or_else(|| RpcError::not_found("job", guid))
}

fn job_create(ctx: &mut Context, p: JobFields) -> Result<JobInfo, RpcError> {
    let owner = p.owner.ok_or_else(|| RpcError::invalid("a job needs an owner"))?;
    ctx.mutate_rpc(|book, env| {
        // gncJobCreate (5.17:libgnucash/engine/gncJob.c:212-232): empty strings, active.
        let owner = resolve_owner(book, owner)?;
        let mut j = Job {
            guid: env.new_guid(),
            id: String::new(),
            name: String::new(),
            reference: String::new(),
            owner,
            active: true,
            slots: Frame::new(),
        };
        p.apply(book, &mut j)?;
        let guid = add_job(book, j)?;
        get_job(book, guid)
    })
}

fn job_get(ctx: &mut Context, p: JobKey) -> Result<JobInfo, RpcError> {
    get_job(ctx.book()?, p.job.0)
}

fn job_list(ctx: &mut Context, p: JobListParams) -> Result<JobList, RpcError> {
    let book = ctx.book()?;
    let filter = ListParams { active_only: p.active_only, name: p.name, id: p.id };
    let jobs = book
        .business
        .jobs
        .values()
        .filter(|j| filter.matches(&j.name, &j.id, j.active) && p.owner.is_none_or(|o| j.owner.guid == o.0))
        .map(|j| j.as_ref().into())
        .collect();
    Ok(JobList { jobs })
}

fn job_update(ctx: &mut Context, p: JobUpdate) -> Result<JobInfo, RpcError> {
    let (key, fields) = p.split();
    ctx.mutate_rpc(|book, _| {
        let mut j = book.business.jobs.get(&key.0).map(|j| (**j).clone()).ok_or_else(|| RpcError::not_found("job", key.0))?;
        fields.apply(book, &mut j)?;
        update_job(book, j)?;
        get_job(book, key.0)
    })
}
