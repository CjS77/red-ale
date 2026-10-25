//! Business contacts: customers, vendors, employees and jobs, the book counters that number them and the documents,
//! and the owner lookups the other business operations share.
//!
//! Operations take a whole entity: the caller builds the new state (from defaults, or from the stored entity with
//! its changes applied) and the operation checks it, keeps the tax table and billing term reference counts right,
//! and stores it. GnuCash offers no way to delete a saved contact, only to deactivate it (its dialogs destroy a
//! contact only when a new one is cancelled, `5.17:gnucash/gnome/dialog-customer.c:376-388`), so neither does
//! red-ale.

use std::sync::Arc;

use crate::book_options::{FEATURES, GNUCASH_FEATURES};
use crate::business::{Address, Customer, Employee, Job, OwnerKind, OwnerRef, Vendor};
use crate::currencies::iso_currency;
use crate::ops::business::terms::{check_bill_term_ref, check_tax_table_ref, retarget_bill_term, retarget_tax_table};
use crate::{AccountType, Book, CommodityRef, DomainError, DomainResult, Guid, Numeric, SlotValue};

/// The counter customers are numbered from (`5.17:libgnucash/engine/gncCustomer.c:960-963`).
pub const COUNTER_CUSTOMER: &str = "gncCustomer";
/// The counter vendors are numbered from (`5.17:libgnucash/engine/gncVendor.c:1020-1023`).
pub const COUNTER_VENDOR: &str = "gncVendor";
/// The counter employees are numbered from (`5.17:libgnucash/engine/gncEmployee.c:948-951`).
pub const COUNTER_EMPLOYEE: &str = "gncEmployee";
/// The counter jobs are numbered from (`5.17:libgnucash/engine/gncJob.c:600-603`).
pub const COUNTER_JOB: &str = "gncJob";

/// The Book feature GnuCash sets when an address, job, entry or tax table carries slots
/// (`5.17:libgnucash/engine/gncAddress.c:493-497`, `gncJob.c:428-431`, `gncEntry.c:1636-1640`, `gncTaxTable.c:651-656`).
pub const EXTRA_DATA_FEATURE: &str = "Extra data in addresses, jobs or invoice entries";

/// The slot holding a job's rate (`5.17:libgnucash/engine/gncJob.c:59`).
pub const JOB_RATE: &str = "job-rate";

// ---------------------------------------------------------------------------------------------------------------
// Counters: `qof_book_increment_and_format_counter`, `5.17:libgnucash/engine/qofbook.cpp:592-750`.

/// The last number issued from a counter: `counters/<name>`, 0 when absent. A double (GnuCash bug 798930) is
/// truncated, as `qof_book_get_counter` does when the integer reads as zero.
pub fn counter_value(book: &Book, name: &str) -> i64 {
    match book.slots.get(&format!("counters/{name}")) {
        Some(SlotValue::Integer(n)) => *n,
        Some(SlotValue::Double(text)) => text.trim().split('.').next().and_then(|i| i.parse().ok()).unwrap_or(0),
        _ => 0,
    }
}

/// Take the next number from a counter and format it with the book's format for it, as GnuCash's `gnc<Type>NextID`
/// does. The counter goes up even if the caller then discards the ID, as in GnuCash.
pub fn next_id(book: &mut Book, name: &str) -> String {
    let n = counter_value(book, name).saturating_add(1);
    book.slots.set(&format!("counters/{name}"), SlotValue::Integer(n));
    format_counter(book.slots.get_str(&format!("counter_formats/{name}")), n)
}

/// Format a counter value with a user format, falling back to GnuCash's default `"%.6" PRIi64` when the format is
/// absent or invalid (`qof_book_get_counter_format`, `5.17:libgnucash/engine/qofbook.cpp:697-750`).
pub fn format_counter(format: Option<&str>, n: i64) -> String {
    let spec = format.and_then(CounterFormat::parse).unwrap_or(CounterFormat::DEFAULT);
    spec.format(n)
}

/// A counter format reduced to what `printf` does with one 64-bit integer conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // One per printf flag that changes the output.
struct CounterFormat<'a> {
    prefix: &'a str,
    suffix: &'a str,
    left: bool,
    zero: bool,
    plus: bool,
    space: bool,
    width: usize,
    precision: Option<usize>,
}

impl<'a> CounterFormat<'a> {
    const DEFAULT: CounterFormat<'static> =
        CounterFormat { prefix: "", suffix: "", left: false, zero: false, plus: false, space: false, width: 0, precision: Some(6) };

    /// `qof_book_normalize_counter_format` (`5.17:libgnucash/engine/qofbook.cpp:752-911`): a prefix and suffix whose
    /// only `%` signs are `%%`, and one conversion with flags `#0- +'I`, width and precision digits, and a length and
    /// conversion spelled `li`, `lli` or `I64i`. A width/precision run `printf` itself would not accept (two dots)
    /// is treated as invalid.
    fn parse(s: &'a str) -> Option<Self> {
        let start = find_conversion(s)?;
        let rest = &s[start + 1..];
        let (pos, len) = ["li", "lli", "I64i"].iter().find_map(|spec| Self::spec_at(rest, spec).map(|p| (p, spec.len())))?;
        let suffix = &rest[pos + len..];
        if find_conversion(suffix).is_some() {
            return None;
        }
        let modifiers = &rest[..pos];
        let digits_from = modifiers.find(|c: char| !"#0- +'I".contains(c)).unwrap_or(modifiers.len());
        let (flags, digits) = modifiers.split_at(digits_from);
        let (width, precision) = match digits.split_once('.') {
            Some((w, p)) => (w, Some(p)),
            None => (digits, None),
        };
        let number = |t: &str| if t.is_empty() { Some(0) } else { t.parse::<usize>().ok() };
        let precision = match precision {
            Some(p) => Some(number(p)?),
            None => None,
        };
        Some(Self {
            prefix: &s[..start],
            suffix,
            left: flags.contains('-'),
            zero: flags.contains('0'),
            plus: flags.contains('+'),
            space: flags.contains(' '),
            width: number(width)?,
            precision,
        })
    }

    /// Where `spec` starts in `rest`, if everything before it is flags, then width and precision characters.
    fn spec_at(rest: &str, spec: &str) -> Option<usize> {
        let pos = rest.find(spec)?;
        let before = &rest[..pos];
        let digits_from = before.find(|c: char| !"#0- +'I".contains(c)).unwrap_or(before.len());
        before[digits_from..].chars().all(|c| c.is_ascii_digit() || c == '.').then_some(pos)
    }

    fn format(&self, n: i64) -> String {
        let mut digits = n.unsigned_abs().to_string();
        if let Some(p) = self.precision {
            digits = if p == 0 && n == 0 { String::new() } else { format!("{digits:0>p$}") };
        }
        let sign = if n < 0 {
            "-"
        } else if self.plus {
            "+"
        } else if self.space {
            " "
        } else {
            ""
        };
        let len = sign.len() + digits.len();
        let pad = self.width.saturating_sub(len);
        let body = if self.left {
            format!("{sign}{digits}{}", " ".repeat(pad))
        } else if self.zero && self.precision.is_none() {
            format!("{sign}{}{digits}", "0".repeat(pad))
        } else {
            format!("{}{sign}{digits}", " ".repeat(pad))
        };
        format!("{}{body}{}", unescape(self.prefix), unescape(self.suffix))
    }
}

/// The byte offset of the first `%` that is not part of `%%`.
fn find_conversion(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], bytes.get(i + 1)) {
            (b'%', Some(b'%')) => i += 2,
            (b'%', _) => return Some(i),
            _ => i += 1,
        }
    }
    None
}

fn unescape(s: &str) -> String {
    s.replace("%%", "%")
}

// ---------------------------------------------------------------------------------------------------------------
// Book features.

/// Record that the book uses a feature, as `gnc_features_set_used` does: only a feature GnuCash knows, written with
/// GnuCash's description unless the slot already holds it (`5.17:libgnucash/engine/gnc-features.cpp:107-121`,
/// `5.17:libgnucash/engine/qofbook.cpp:1272-1289`).
pub fn set_feature_used(book: &mut Book, name: &str) {
    let Some((_, description)) = GNUCASH_FEATURES.iter().find(|(n, _)| *n == name) else { return };
    let path = format!("{FEATURES}/{name}");
    if book.slots.get_str(&path) != Some(*description) {
        book.slots.set(&path, SlotValue::String((*description).to_owned()));
    }
}

/// Set the "extra data" feature when an object being committed carries slots, as GnuCash's commit functions do.
pub fn note_extra_data(book: &mut Book, has_slots: bool) {
    if has_slots {
        set_feature_used(book, EXTRA_DATA_FEATURE);
    }
}

fn addresses_have_slots(addrs: &[&Address]) -> bool {
    addrs.iter().any(|a| !a.slots.is_empty())
}

// ---------------------------------------------------------------------------------------------------------------
// Owners.

/// The owner with this GUID, of whichever kind.
pub fn find_owner(book: &Book, guid: Guid) -> Option<OwnerRef> {
    let b = &book.business;
    let kind = if b.customers.contains_key(&guid) {
        OwnerKind::Customer
    } else if b.vendors.contains_key(&guid) {
        OwnerKind::Vendor
    } else if b.employees.contains_key(&guid) {
        OwnerKind::Employee
    } else if b.jobs.contains_key(&guid) {
        OwnerKind::Job
    } else {
        return None;
    };
    Some(OwnerRef { kind, guid })
}

/// True when the owner exists.
pub fn owner_exists(book: &Book, owner: OwnerRef) -> bool {
    find_owner(book, owner.guid).is_some_and(|o| o.kind == owner.kind)
}

/// `gncOwnerGetEndOwner`: a job's owner, or the owner itself (`5.17:libgnucash/engine/gncOwner.c:572-590`).
pub fn end_owner(book: &Book, owner: OwnerRef) -> Option<OwnerRef> {
    match owner.kind {
        OwnerKind::Job => book.business.jobs.get(&owner.guid).map(|j| j.owner),
        _ => Some(owner),
    }
}

/// `gncOwnerGetCurrency`: a job uses its owner's currency (`5.17:libgnucash/engine/gncOwner.c:478-500`).
pub fn owner_currency(book: &Book, owner: OwnerRef) -> Option<CommodityRef> {
    let b = &book.business;
    match end_owner(book, owner)? {
        OwnerRef { kind: OwnerKind::Customer, guid } => b.customers.get(&guid)?.currency.clone(),
        OwnerRef { kind: OwnerKind::Vendor, guid } => b.vendors.get(&guid)?.currency.clone(),
        OwnerRef { kind: OwnerKind::Employee, guid } => b.employees.get(&guid)?.currency.clone(),
        OwnerRef { kind: OwnerKind::Job, .. } => None,
    }
}

/// The owner's billing terms: a customer's or vendor's, or a job's owner's.
pub fn owner_terms(book: &Book, owner: OwnerRef) -> Option<Guid> {
    let b = &book.business;
    match end_owner(book, owner)? {
        OwnerRef { kind: OwnerKind::Customer, guid } => b.customers.get(&guid)?.terms,
        OwnerRef { kind: OwnerKind::Vendor, guid } => b.vendors.get(&guid)?.terms,
        _ => None,
    }
}

/// `gncOwnerGetName`: an employee's name is its address name (`5.17:libgnucash/engine/gncOwner.c:439-457`).
pub fn owner_name(book: &Book, owner: OwnerRef) -> Option<String> {
    let b = &book.business;
    Some(match owner.kind {
        OwnerKind::Customer => b.customers.get(&owner.guid)?.name.clone(),
        OwnerKind::Vendor => b.vendors.get(&owner.guid)?.name.clone(),
        OwnerKind::Employee => b.employees.get(&owner.guid)?.addr.name.clone(),
        OwnerKind::Job => b.jobs.get(&owner.guid)?.name.clone(),
    })
}

/// True when any invoice, bill, voucher or order belongs to the owner directly.
pub fn owner_has_documents(book: &Book, owner: OwnerRef) -> bool {
    let b = &book.business;
    b.invoices.values().any(|i| i.owner == owner) || b.orders.values().any(|o| o.owner == owner)
}

// ---------------------------------------------------------------------------------------------------------------
// Shared checks.

/// A contact's currency must be set and known: GnuCash resolves it in its commodity table when it reads the file and
/// drops a reference it cannot resolve.
pub fn check_currency(book: &Book, currency: Option<&CommodityRef>) -> DomainResult<()> {
    let c = currency.ok_or_else(|| DomainError::invalid("a currency is required"))?;
    if !c.is_currency() {
        return Err(DomainError::invalid(format!("{c} is not a currency")));
    }
    if iso_currency(&c.mnemonic).is_none() && book.commodity(c).is_none() {
        return Err(DomainError::not_found("currency", c));
    }
    Ok(())
}

fn check_name(what: &str, name: &str) -> DomainResult<()> {
    if name.trim().is_empty() { Err(DomainError::invalid(format!("the {what} needs a name"))) } else { Ok(()) }
}

/// An existing contact keeps a non-empty ID: GnuCash does not save one without (`*_should_be_saved`,
/// `5.17:libgnucash/backend/xml/gnc-customer-xml-v2.cpp:421-432`).
fn check_kept_id(id: &str) -> DomainResult<()> {
    if id.is_empty() { Err(DomainError::invalid("the ID cannot be empty")) } else { Ok(()) }
}

fn assign_id(book: &mut Book, id: &mut String, counter: &str) {
    if id.is_empty() {
        *id = next_id(book, counter);
    }
}

fn is_new_guid(book: &Book, guid: Guid) -> DomainResult<()> {
    if find_owner(book, guid).is_some() { Err(DomainError::conflict(format!("{guid} is already in use"))) } else { Ok(()) }
}

// ---------------------------------------------------------------------------------------------------------------
// Customers.

/// The customer checks of GnuCash's dialog: a company name, a discount from 0 to 100 and a credit limit that is
/// not negative (`5.17:gnucash/gnome/dialog-customer.c:313-340`).
fn check_customer(book: &Book, c: &Customer, old: Option<&Customer>) -> DomainResult<()> {
    check_name("customer", &c.name)?;
    check_currency(book, c.currency.as_ref())?;
    if c.discount.is_negative() || c.discount.cmp_value(Numeric::from_int(100)).is_gt() {
        return Err(DomainError::invalid("the discount must be a percentage from 0 to 100"));
    }
    if c.credit.is_negative() {
        return Err(DomainError::invalid("the credit limit cannot be negative"));
    }
    check_bill_term_ref(book, c.terms, old.and_then(|o| o.terms))?;
    check_tax_table_ref(book, c.tax_table, old.and_then(|o| o.tax_table))
}

/// Add a customer, numbering it from the book counter when its ID is empty. Returns its GUID.
pub fn add_customer(book: &mut Book, mut c: Customer) -> DomainResult<Guid> {
    is_new_guid(book, c.guid)?;
    check_customer(book, &c, None)?;
    assign_id(book, &mut c.id, COUNTER_CUSTOMER);
    retarget_bill_term(book, None, c.terms);
    retarget_tax_table(book, None, c.tax_table);
    note_extra_data(book, addresses_have_slots(&[&c.addr, &c.ship_addr]));
    let guid = c.guid;
    book.business.customers.insert(guid, Arc::new(c));
    Ok(guid)
}

/// Replace a customer with its new state.
pub fn update_customer(book: &mut Book, c: Customer) -> DomainResult<()> {
    let old = book.business.customers.get(&c.guid).cloned().ok_or_else(|| DomainError::not_found("customer", &c.guid))?;
    check_kept_id(&c.id)?;
    check_customer(book, &c, Some(&old))?;
    retarget_bill_term(book, old.terms, c.terms);
    retarget_tax_table(book, old.tax_table, c.tax_table);
    note_extra_data(book, addresses_have_slots(&[&c.addr, &c.ship_addr]));
    book.business.customers.insert(c.guid, Arc::new(c));
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Vendors.

/// A vendor needs a company name (`5.17:gnucash/gnome/dialog-vendor.c:204-208`).
fn check_vendor(book: &Book, v: &Vendor, old: Option<&Vendor>) -> DomainResult<()> {
    check_name("vendor", &v.name)?;
    check_currency(book, v.currency.as_ref())?;
    check_bill_term_ref(book, v.terms, old.and_then(|o| o.terms))?;
    check_tax_table_ref(book, v.tax_table, old.and_then(|o| o.tax_table))
}

/// Add a vendor, numbering it from the book counter when its ID is empty. Returns its GUID.
pub fn add_vendor(book: &mut Book, mut v: Vendor) -> DomainResult<Guid> {
    is_new_guid(book, v.guid)?;
    check_vendor(book, &v, None)?;
    assign_id(book, &mut v.id, COUNTER_VENDOR);
    retarget_bill_term(book, None, v.terms);
    retarget_tax_table(book, None, v.tax_table);
    note_extra_data(book, addresses_have_slots(&[&v.addr]));
    let guid = v.guid;
    book.business.vendors.insert(guid, Arc::new(v));
    Ok(guid)
}

/// Replace a vendor with its new state.
pub fn update_vendor(book: &mut Book, v: Vendor) -> DomainResult<()> {
    let old = book.business.vendors.get(&v.guid).cloned().ok_or_else(|| DomainError::not_found("vendor", &v.guid))?;
    check_kept_id(&v.id)?;
    check_vendor(book, &v, Some(&old))?;
    retarget_bill_term(book, old.terms, v.terms);
    retarget_tax_table(book, old.tax_table, v.tax_table);
    note_extra_data(book, addresses_have_slots(&[&v.addr]));
    book.business.vendors.insert(v.guid, Arc::new(v));
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Employees.

/// An employee needs a name, which is its address name (`5.17:gnucash/gnome/dialog-employee.c:200-205`,
/// `5.17:libgnucash/engine/gncEmployee.c:512-520`), and its credit card account, when set, is a credit card
/// account (the dialog offers only those, `5.17:gnucash/gnome/dialog-employee.c:458`).
fn check_employee(book: &Book, e: &Employee, old: Option<&Employee>) -> DomainResult<()> {
    check_name("employee", &e.addr.name)?;
    check_currency(book, e.currency.as_ref())?;
    match e.ccard {
        Some(acc) if old.and_then(|o| o.ccard) != Some(acc) => {
            let a = book.account(acc).ok_or_else(|| DomainError::not_found("account", &acc))?;
            if a.account_type != AccountType::Credit {
                return Err(DomainError::invalid(format!("the credit card account must be of type CREDIT, not {}", a.account_type.as_str())));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Add an employee, numbering it from the book counter when its ID is empty. Returns its GUID.
pub fn add_employee(book: &mut Book, mut e: Employee) -> DomainResult<Guid> {
    is_new_guid(book, e.guid)?;
    check_employee(book, &e, None)?;
    assign_id(book, &mut e.id, COUNTER_EMPLOYEE);
    note_extra_data(book, addresses_have_slots(&[&e.addr]));
    let guid = e.guid;
    book.business.employees.insert(guid, Arc::new(e));
    Ok(guid)
}

/// Replace an employee with its new state.
pub fn update_employee(book: &mut Book, e: Employee) -> DomainResult<()> {
    let old = book.business.employees.get(&e.guid).cloned().ok_or_else(|| DomainError::not_found("employee", &e.guid))?;
    check_kept_id(&e.id)?;
    check_employee(book, &e, Some(&old))?;
    note_extra_data(book, addresses_have_slots(&[&e.addr]));
    book.business.employees.insert(e.guid, Arc::new(e));
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Jobs.

/// A job's rate: the `job-rate` slot, zero when absent (`gncJobGetRate`, `5.17:libgnucash/engine/gncJob.c:459-466`).
pub fn job_rate(job: &Job) -> Numeric {
    job.slots.get(JOB_RATE).and_then(SlotValue::as_numeric).unwrap_or(Numeric::ZERO)
}

/// `gncJobSetRate`: a zero rate removes the slot (`5.17:libgnucash/engine/gncJob.c:314-335`).
pub fn set_job_rate(job: &mut Job, rate: Numeric) {
    if rate.is_zero() {
        job.slots.remove(JOB_RATE);
    } else {
        job.slots.set(JOB_RATE, SlotValue::Numeric(rate));
    }
}

/// A job needs a name and an owner that is a customer or vendor (`5.17:gnucash/gnome/dialog-job.c:134-150`,
/// `5.17:libgnucash/engine/gncJob.c:336-352`).
fn check_job(book: &Book, j: &Job) -> DomainResult<()> {
    check_name("job", &j.name)?;
    if !matches!(j.owner.kind, OwnerKind::Customer | OwnerKind::Vendor) {
        return Err(DomainError::invalid("a job's owner must be a customer or a vendor"));
    }
    if !owner_exists(book, j.owner) {
        return Err(DomainError::not_found("owner", &j.owner.guid));
    }
    Ok(())
}

/// Add a job, numbering it from the book counter when its ID is empty. Returns its GUID.
pub fn add_job(book: &mut Book, mut j: Job) -> DomainResult<Guid> {
    is_new_guid(book, j.guid)?;
    check_job(book, &j)?;
    assign_id(book, &mut j.id, COUNTER_JOB);
    note_extra_data(book, !j.slots.is_empty());
    let guid = j.guid;
    book.business.jobs.insert(guid, Arc::new(j));
    Ok(guid)
}

/// Replace a job with its new state. Moving a job that has documents to another owner is refused: GnuCash allows it,
/// but it would silently change which customer or vendor those documents, posted or not, belong to.
pub fn update_job(book: &mut Book, j: Job) -> DomainResult<()> {
    let old = book.business.jobs.get(&j.guid).cloned().ok_or_else(|| DomainError::not_found("job", &j.guid))?;
    check_kept_id(&j.id)?;
    check_job(book, &j)?;
    if old.owner != j.owner && owner_has_documents(book, OwnerRef { kind: OwnerKind::Job, guid: j.guid }) {
        return Err(DomainError::conflict("the job has invoices, bills or orders, so its owner cannot change"));
    }
    note_extra_data(book, !j.slots.is_empty());
    book.business.jobs.insert(j.guid, Arc::new(j));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Account, FixedEnv, Time64};

    fn book() -> Book {
        Book::new_empty(&mut FixedEnv::new(Time64(0), 1))
    }

    #[test]
    fn counters_count_up_and_use_the_default_format() {
        let mut b = book();
        assert_eq!(next_id(&mut b, COUNTER_CUSTOMER), "000001");
        assert_eq!(next_id(&mut b, COUNTER_CUSTOMER), "000002");
        assert_eq!(b.slots.get("counters/gncCustomer"), Some(&SlotValue::Integer(2)));
        b.slots.set("counters/gncVendor", SlotValue::Integer(1_234_567));
        assert_eq!(next_id(&mut b, COUNTER_VENDOR), "1234568");
        b.slots.set("counters/gncJob", SlotValue::Double("4.0".into()));
        assert_eq!(next_id(&mut b, COUNTER_JOB), "000005");
    }

    #[test]
    fn counter_formats_follow_gnucash() {
        // Oracle results in design/notes/engine-business.md §1.7.
        assert_eq!(format_counter(Some("INV-%04li"), 3), "INV-0003");
        assert_eq!(format_counter(Some("C-%li%%"), 2), "C-2%");
        assert_eq!(format_counter(Some("%d"), 1), "000001");
        assert_eq!(format_counter(Some("%lli"), 7), "7");
        assert_eq!(format_counter(Some("%I64i"), 7), "7");
        assert_eq!(format_counter(Some("%-5li|"), 7), "7    |");
        assert_eq!(format_counter(Some("%+li"), 7), "+7");
        assert_eq!(format_counter(Some("%5.3li"), 7), "  007");
        assert_eq!(format_counter(Some("%li-%li"), 7), "000007");
        assert_eq!(format_counter(Some("no conversion"), 7), "000007");
        assert_eq!(format_counter(Some("%1.2.3li"), 7), "000007");
        assert_eq!(format_counter(Some("%%li %li"), 9), "%li 9");
    }

    #[test]
    fn features_are_set_once_with_gnucash_descriptions() {
        let mut b = book();
        set_feature_used(&mut b, EXTRA_DATA_FEATURE);
        assert_eq!(
            b.slots.get_str("features/Extra data in addresses, jobs or invoice entries"),
            Some("Extra data for addresses, jobs or invoice entries (requires at least GnuCash 2.6.4)")
        );
    }

    fn customer(guid: Guid) -> Customer {
        Customer {
            guid,
            name: "Acme".into(),
            id: String::new(),
            addr: Address::default(),
            ship_addr: Address::default(),
            notes: String::new(),
            terms: None,
            tax_included: crate::business::TaxIncluded::UseGlobal,
            active: true,
            discount: Numeric::ZERO,
            credit: Numeric::ZERO,
            currency: Some(CommodityRef::currency("EUR")),
            use_tax_table: false,
            tax_table: None,
            slots: crate::Frame::new(),
        }
    }

    #[test]
    fn customers_are_checked_and_numbered() {
        let mut b = book();
        let g = Guid::from_bytes([9; 16]);
        let mut bad = customer(g);
        bad.name = " ".into();
        assert!(add_customer(&mut b, bad).is_err());
        let mut bad = customer(g);
        bad.currency = Some(CommodityRef::currency("XYZ"));
        assert!(add_customer(&mut b, bad).is_err());
        let mut bad = customer(g);
        bad.discount = Numeric::from_int(101);
        assert!(add_customer(&mut b, bad).is_err());
        add_customer(&mut b, customer(g)).unwrap();
        assert_eq!(b.business.customers[&g].id, "000001");
        assert!(add_customer(&mut b, customer(g)).is_err(), "the GUID is taken");
        let mut c = (*b.business.customers[&g]).clone();
        c.id = String::new();
        assert!(update_customer(&mut b, c).is_err());
    }

    #[test]
    fn jobs_need_a_customer_or_vendor_and_keep_their_rate_in_a_slot() {
        let mut b = book();
        let c = Guid::from_bytes([1; 16]);
        add_customer(&mut b, customer(c)).unwrap();
        let mut j = Job {
            guid: Guid::from_bytes([2; 16]),
            id: String::new(),
            name: "Build".into(),
            reference: String::new(),
            owner: OwnerRef { kind: OwnerKind::Employee, guid: c },
            active: true,
            slots: crate::Frame::new(),
        };
        assert!(add_job(&mut b, j.clone()).is_err());
        j.owner.kind = OwnerKind::Customer;
        set_job_rate(&mut j, Numeric::new(9000, 100));
        add_job(&mut b, j.clone()).unwrap();
        assert_eq!(job_rate(&b.business.jobs[&j.guid]), Numeric::new(90, 1));
        assert!(b.slots.get(&format!("features/{EXTRA_DATA_FEATURE}")).is_some());
        set_job_rate(&mut j, Numeric::ZERO);
        assert!(j.slots.get(JOB_RATE).is_none());
    }

    #[test]
    fn an_employee_card_must_be_a_credit_account() {
        let mut b = book();
        let acc = Account::new(Guid::from_bytes([5; 16]), "Bank", AccountType::Bank, Some(CommodityRef::currency("EUR")), Some(b.root));
        b.accounts.insert(acc.guid, Arc::new(acc));
        let e = Employee {
            guid: Guid::from_bytes([6; 16]),
            username: String::new(),
            id: String::new(),
            addr: Address { name: "Jane".into(), ..Address::default() },
            language: String::new(),
            acl: String::new(),
            active: true,
            workday: Numeric::ZERO,
            rate: Numeric::ZERO,
            currency: Some(CommodityRef::currency("EUR")),
            ccard: Some(Guid::from_bytes([5; 16])),
            slots: crate::Frame::new(),
        };
        assert!(add_employee(&mut b, e.clone()).is_err());
        add_employee(&mut b, Employee { ccard: None, ..e }).unwrap();
    }
}
