//! Importing customers and vendors (`csvOwners`), as GnuCash's Import Customers & Vendors does.
//!
//! Translated from `gnc_customer_import_fix_customers` and `gnc_customer_import_create_customers`
//! (`5.17:gnucash/import-export/customer-import/dialog-customer-import.c:206-421`, Copyright (C) 2009 Sebastian
//! Held; GPL-2.0-or-later; this translation is GPL-3.0-or-later). Spec: `design/api/import.md`, "Customers and
//! vendors".
//!
//! **Fix-up** (`:206-266`): an empty company takes the name. **Creating** (`:268-421`): an empty ID takes the next
//! number from the Owner's Counter; an Owner with the ID is updated, else one is created in the default currency;
//! either way every field is set from the row, empty ones included, and the Owner is made active. Vendors have no
//! shipping address, so those columns are not read for them. Terms, tax tables and discounts are left alone.
//!
//! Deviations (`design/api/import.md`): a row with neither company nor name is an error row, where GnuCash drops it
//! and only counts it; a first row whose ID and company cells are the column names `id` and `company` gets a
//! `header-row` warning and is skipped, where GnuCash imports it as an Owner named `company` (the caller may take
//! GnuCash's action); a row that would change nothing is proposed as `skip`; a vendor row's shipping cells, which
//! GnuCash ignores silently, give an `ignored-columns` warning.

use red_ale_core::business::{Address, Customer, OwnerKind, TaxIncluded, Vendor};
use red_ale_core::ops::business::contacts::{COUNTER_CUSTOMER, COUNTER_VENDOR, add_customer, add_vendor, update_customer, update_vendor};
use red_ale_core::ops::business::counters::{counter_format, counter_value, format_counter, set_counter};
use red_ale_core::{Book, CommodityRef, DomainError, DomainResult, Env, FixedEnv, Frame, Guid, Numeric};

use super::documents::stripped_warnings;
use super::reader::{self, Row, Separator};
use super::{Context, Created, RecordsApplied};
use crate::matcher::ImportAction;
use crate::pipeline::{IgnoredLine, RowError, Warning};

/// The fields of a line, in GnuCash's order (`5.17:gnucash/import-export/customer-import/dialog-customer-import.h:39-46`).
pub const FIELDS: [&str; 19] = [
    "id",
    "company",
    "name",
    "addr1",
    "addr2",
    "addr3",
    "addr4",
    "phone",
    "fax",
    "email",
    "notes",
    "shipname",
    "shipaddr1",
    "shipaddr2",
    "shipaddr3",
    "shipaddr4",
    "shipphone",
    "shipfax",
    "shipemail",
];

const ID: usize = 0;
const COMPANY: usize = 1;
const NAME: usize = 2;
const NOTES: usize = 10;
const SHIP: usize = 11;

/// The first GUID a preview's copy of the Book counts up from.
const PREVIEW_GUID_SEED: u128 = 0x7265_642d_616c_6520_6369_2d69_6d70_0000;

/// What the user chooses in GnuCash's dialog, and the default currency preference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerOptions {
    /// Customers (GnuCash's default) or vendors.
    pub kind: OwnerKind,
    /// How fields are separated.
    pub separator: Separator,
    /// The currency of new Owners: GnuCash's `gnc_default_currency()`.
    pub currency: CommodityRef,
}

/// An address as a row gives it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddressFields {
    /// Name.
    pub name: String,
    /// Lines 1 to 4.
    pub lines: [String; 4],
    /// Phone.
    pub phone: String,
    /// Fax.
    pub fax: String,
    /// Email.
    pub email: String,
}

impl AddressFields {
    /// The address from the eight columns starting at `at`, the first being the name.
    fn from_row(f: &[String], name: usize, at: usize) -> Self {
        let get = |i: usize| f.get(i).cloned().unwrap_or_default();
        let lines = [get(at), get(at + 1), get(at + 2), get(at + 3)];
        Self {
            name: get(name),
            lines,
            phone: get(at + 4),
            fax: get(at + 5),
            email: get(at + 6),
        }
    }

    /// Set every field of `a`, empty ones included (`dialog-customer-import.c:371-388`); the names of those that change.
    fn set(&self, a: &mut Address, prefix: &str, changes: &mut Vec<String>) {
        let [l1, l2, l3, l4] = &self.lines;
        let pairs: [(&str, &mut String, &String); 8] = [
            ("name", &mut a.name, &self.name),
            ("addr1", &mut a.addr1, l1),
            ("addr2", &mut a.addr2, l2),
            ("addr3", &mut a.addr3, l3),
            ("addr4", &mut a.addr4, l4),
            ("phone", &mut a.phone, &self.phone),
            ("fax", &mut a.fax, &self.fax),
            ("email", &mut a.email, &self.email),
        ];
        pairs.into_iter().filter(|(_, old, new)| *old != *new).for_each(|(name, old, new)| {
            changes.push(format!("{prefix}.{name}"));
            old.clone_from(new);
        });
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Everything a row sets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnerFields {
    /// The company: the Owner's name. The name column when the company is empty.
    pub company: String,
    /// The address; its name is the name column.
    pub address: AddressFields,
    /// Notes.
    pub notes: String,
    /// The shipping address (customers only).
    pub shipping: AddressFields,
}

/// One Owner the file gives.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedOwner {
    /// Its position in the proposal.
    pub index: usize,
    /// Its line.
    pub line: usize,
    /// Customer or vendor.
    pub kind: OwnerKind,
    /// The ID: the file's, or the one the Counter would give.
    pub id: String,
    /// The Counter number the ID takes, when the file gives none.
    pub counter: Option<i64>,
    /// The Owner in the Book this updates (or would, when skipped).
    pub existing: Option<Guid>,
    /// The earlier record of this import whose Owner this updates (or would).
    pub same_as: Option<usize>,
    /// `add`, `update` or `skip`.
    pub action: ImportAction,
    /// The action GnuCash would take, when red-ale proposes `skip` for a header row; the caller may choose it.
    pub alternative: Option<ImportAction>,
    /// The fields set.
    pub fields: OwnerFields,
    /// The currency of a new Owner.
    pub currency: Option<CommodityRef>,
    /// The fields an update changes.
    pub changes: Vec<String>,
    /// True when an inactive Owner becomes active.
    pub reactivate: bool,
    /// Warnings about it.
    pub warnings: Vec<Warning>,
}

/// What a preview found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Proposed {
    /// The Owners, by index.
    pub owners: Vec<ProposedOwner>,
    /// Rows that can't be imported.
    pub errors: Vec<RowError>,
    /// Lines that didn't match.
    pub ignored: Vec<IgnoredLine>,
    /// Warnings about the whole file.
    pub warnings: Vec<Warning>,
}

fn counter_name(kind: OwnerKind) -> &'static str {
    if kind == OwnerKind::Customer {
        COUNTER_CUSTOMER
    } else {
        COUNTER_VENDOR
    }
}

/// The Owner with this ID (`gnc_search_customer_on_id`, `gnc_search_vendor_on_id`,
/// `5.17:libgnucash/engine/gncIDSearch.c:46-162`): exact, inactive ones included.
fn find_owner(book: &Book, kind: OwnerKind, id: &str) -> Option<Guid> {
    let b = &book.business;
    match kind {
        OwnerKind::Customer => b.customers.values().find(|c| c.id == id).map(|c| c.guid),
        _ => b.vendors.values().find(|v| v.id == id).map(|v| v.guid),
    }
}

/// `gncCustomerCreate`'s defaults (`5.17:libgnucash/engine/gncCustomer.c:303-331`) and the default currency.
fn new_customer(guid: Guid, currency: &CommodityRef) -> Customer {
    Customer {
        guid,
        name: String::new(),
        id: String::new(),
        addr: Address::default(),
        ship_addr: Address::default(),
        notes: String::new(),
        terms: None,
        tax_included: TaxIncluded::UseGlobal,
        active: true,
        discount: Numeric::ZERO,
        credit: Numeric::ZERO,
        currency: Some(currency.clone()),
        use_tax_table: false,
        tax_table: None,
        slots: Frame::new(),
    }
}

/// `gncVendorCreate`'s defaults (`5.17:libgnucash/engine/gncVendor.c:290-312`) and the default currency.
fn new_vendor(guid: Guid, currency: &CommodityRef) -> Vendor {
    Vendor {
        guid,
        name: String::new(),
        id: String::new(),
        addr: Address::default(),
        notes: String::new(),
        terms: None,
        tax_included: TaxIncluded::UseGlobal,
        active: true,
        currency: Some(currency.clone()),
        use_tax_table: false,
        tax_table: None,
        slots: Frame::new(),
    }
}

/// Set a text field, noting the change.
fn set_text(field: &mut String, value: &str, name: &str, changes: &mut Vec<String>) {
    if field != value {
        changes.push(name.to_owned());
        value.clone_into(field);
    }
}

/// Set a customer's fields from the row (`dialog-customer-import.c:354-391`).
fn set_customer(c: &mut Customer, o: &ProposedOwner) -> Vec<String> {
    let mut changes = Vec::new();
    set_text(&mut c.id, &o.id, "id", &mut changes);
    set_text(&mut c.name, &o.fields.company, "company", &mut changes);
    set_text(&mut c.notes, &o.fields.notes, "notes", &mut changes);
    o.fields.address.set(&mut c.addr, "address", &mut changes);
    o.fields.shipping.set(&mut c.ship_addr, "shipping", &mut changes);
    if !c.active {
        changes.push("active".into());
        c.active = true;
    }
    changes
}

/// Set a vendor's fields from the row (`dialog-customer-import.c:363-396`).
fn set_vendor(v: &mut Vendor, o: &ProposedOwner) -> Vec<String> {
    let mut changes = Vec::new();
    set_text(&mut v.id, &o.id, "id", &mut changes);
    set_text(&mut v.name, &o.fields.company, "company", &mut changes);
    set_text(&mut v.notes, &o.fields.notes, "notes", &mut changes);
    o.fields.address.set(&mut v.addr, "address", &mut changes);
    if !v.active {
        changes.push("active".into());
        v.active = true;
    }
    changes
}

/// The Owner an update changes: the Book's, or the one an earlier record created.
fn target(o: &ProposedOwner, created: &Created) -> DomainResult<Guid> {
    o.existing
        .or_else(|| o.same_as.and_then(|i| created.get(&i).copied()))
        .ok_or_else(|| DomainError::invalid(format!("owner {} has nothing to update", o.id)))
}

/// Create or update one Owner; returns it. A Counter number the ID took is used up, as `gnc<Owner>NextID` uses it.
fn apply_one(book: &mut Book, env: &mut dyn Env, o: &ProposedOwner, action: ImportAction, created: &Created) -> DomainResult<Guid> {
    let currency = || {
        o.currency
            .as_ref()
            .ok_or_else(|| DomainError::invalid(format!("owner {} has no currency", o.id)))
    };
    let guid = match (o.kind, action) {
        (OwnerKind::Customer, ImportAction::Add) => {
            let mut c = new_customer(env.new_guid(), currency()?);
            set_customer(&mut c, o);
            add_customer(book, c)?
        }
        (OwnerKind::Customer, _) => {
            let guid = target(o, created)?;
            let mut c = book
                .business
                .customers
                .get(&guid)
                .map(|c| (**c).clone())
                .ok_or_else(|| DomainError::not_found("customer", &guid))?;
            set_customer(&mut c, o);
            update_customer(book, c)?;
            guid
        }
        (_, ImportAction::Add) => {
            let mut v = new_vendor(env.new_guid(), currency()?);
            set_vendor(&mut v, o);
            add_vendor(book, v)?
        }
        _ => {
            let guid = target(o, created)?;
            let mut v = book
                .business
                .vendors
                .get(&guid)
                .map(|v| (**v).clone())
                .ok_or_else(|| DomainError::not_found("vendor", &guid))?;
            set_vendor(&mut v, o);
            update_vendor(book, v)?;
            guid
        }
    };
    if let Some(n) = o.counter {
        let name = counter_name(o.kind);
        set_counter(book, name, Some(n.max(counter_value(book, name))), None)?;
    }
    Ok(guid)
}

/// Apply every Owner not skipped, in file order.
pub(super) fn apply_all(
    book: &mut Book,
    env: &mut dyn Env,
    owners: &[ProposedOwner],
    actions: &[ImportAction],
) -> DomainResult<(RecordsApplied, Vec<Warning>)> {
    let mut out = RecordsApplied::default();
    let mut created = Created::new();
    owners.iter().zip(actions).try_for_each(|(o, action)| -> DomainResult<()> {
        if *action == ImportAction::Skip {
            out.skipped.push(o.index);
            return Ok(());
        }
        let guid = apply_one(book, env, o, *action, &created)?;
        if *action == ImportAction::Add {
            created.insert(o.index, guid);
            out.created.push((o.index, guid));
        } else {
            out.updated.push((o.index, guid));
        }
        Ok(())
    })?;
    Ok((out, Vec::new()))
}

// ---------------------------------------------------------------------------------------------------------------
// The preview.

/// True for a header row: its ID and company cells are those columns' names.
fn is_header(row: &Row) -> bool {
    row.fields[ID].eq_ignore_ascii_case(FIELDS[ID]) && row.fields[COMPANY].eq_ignore_ascii_case(FIELDS[COMPANY])
}

/// The row's fields after GnuCash's fix-up; `None` with neither company nor name.
fn fields(row: &Row, kind: OwnerKind) -> Option<(OwnerFields, bool)> {
    let f = &row.fields;
    let fixed = f[COMPANY].is_empty();
    let company = if fixed { f[NAME].clone() } else { f[COMPANY].clone() };
    if company.is_empty() {
        return None;
    }
    let shipping = if kind == OwnerKind::Customer {
        AddressFields::from_row(f, SHIP, SHIP + 1)
    } else {
        AddressFields::default()
    };
    Some((
        OwnerFields {
            company,
            address: AddressFields::from_row(f, NAME, NAME + 1),
            notes: f[NOTES].clone(),
            shipping,
        },
        fixed,
    ))
}

/// Works each row out on a copy of the Book, in file order.
struct Planner<'a> {
    ctx: &'a Context<'a>,
    opts: &'a OwnerOptions,
    copy: Book,
    env: FixedEnv,
    created: Created,
    out: Proposed,
}

impl Planner<'_> {
    /// The ID: the file's, or the Counter's next (`dialog-customer-import.c:321-327`).
    fn id(&self, row: &Row) -> (String, Option<i64>) {
        if !row.fields[ID].is_empty() {
            return (row.fields[ID].clone(), None);
        }
        let name = counter_name(self.opts.kind);
        let n = counter_value(&self.copy, name).saturating_add(1);
        (format_counter(counter_format(&self.copy, name), n), Some(n))
    }

    fn proposed(&self, row: &Row, index: usize, fields: OwnerFields) -> ProposedOwner {
        let (id, counter) = self.id(row);
        let found = find_owner(&self.copy, self.opts.kind, &id);
        let same_as = found.and_then(|g| self.created.iter().find(|(_, c)| **c == g).map(|(i, _)| *i));
        ProposedOwner {
            index,
            line: row.line,
            kind: self.opts.kind,
            id,
            counter,
            existing: found.filter(|_| same_as.is_none()),
            same_as,
            action: if found.is_some() { ImportAction::Update } else { ImportAction::Add },
            alternative: None,
            fields,
            currency: found.is_none().then(|| self.opts.currency.clone()),
            changes: Vec::new(),
            reactivate: false,
            warnings: Vec::new(),
        }
    }

    /// What an update changes, on the copy.
    fn changes(&self, o: &ProposedOwner) -> Vec<String> {
        let Ok(guid) = target(o, &self.created) else { return Vec::new() };
        match o.kind {
            OwnerKind::Customer => self
                .copy
                .business
                .customers
                .get(&guid)
                .map(|c| set_customer(&mut (**c).clone(), o))
                .unwrap_or_default(),
            _ => self
                .copy
                .business
                .vendors
                .get(&guid)
                .map(|v| set_vendor(&mut (**v).clone(), o))
                .unwrap_or_default(),
        }
    }

    fn warnings(&self, row: &Row, o: &mut ProposedOwner, fixed: bool, header: bool) {
        let i = o.index;
        if fixed {
            o.warnings.push(Warning::about(
                i,
                "fixed",
                format!("line {}: no company; the name {} is used", row.line, o.fields.company),
            ));
        }
        if self.opts.kind != OwnerKind::Customer && !AddressFields::from_row(&row.fields, SHIP, SHIP + 1).is_empty() {
            let message = format!("line {}: vendors have no shipping address; its columns are ignored", row.line);
            o.warnings.push(Warning::about(i, "ignored-columns", message));
        }
        if o.counter.is_some() && o.action == ImportAction::Update {
            let message = format!("the Counter's next ID {} is already an Owner's; GnuCash updates that Owner", o.id);
            o.warnings.push(Warning::about(i, "id-in-use", message));
        }
        if header {
            let (line, company) = (row.line, &o.fields.company);
            let message =
                format!("line {line}: this looks like a header row; it is skipped (GnuCash imports it as an Owner named {company:?})");
            o.warnings.push(Warning::about(i, "header-row", message));
        } else if o.action == ImportAction::Skip {
            o.warnings.push(Warning::about(
                i,
                "unchanged",
                format!("line {}: {} {} would not change", row.line, o.kind.type_name(), o.id),
            ));
        }
    }

    fn plan(&mut self, row: &Row, header: bool) {
        let Some((fields, fixed)) = fields(row, self.opts.kind) else {
            let message = "Neither company nor name is set; GnuCash drops this row.".to_owned();
            self.out.errors.push(RowError {
                file: self.ctx.file,
                line: row.line,
                column: Some(COMPANY + 1),
                message,
            });
            return;
        };
        let index = self.out.owners.len();
        let mut o = self.proposed(row, index, fields);
        if o.action == ImportAction::Update {
            o.changes = self.changes(&o);
            o.reactivate = o.changes.iter().any(|c| c == "active");
            if o.changes.is_empty() && o.counter.is_none() {
                o.action = ImportAction::Skip;
            }
        }
        if header {
            o.alternative = Some(o.action).filter(|a| *a != ImportAction::Skip);
            o.action = ImportAction::Skip;
        }
        self.warnings(row, &mut o, fixed, header);
        if o.action != ImportAction::Skip && !self.trial(&o, row) {
            return;
        }
        self.out.owners.push(o);
    }

    /// Run the row on a copy of the copy; an error row when the Book's rules refuse it.
    fn trial(&mut self, o: &ProposedOwner, row: &Row) -> bool {
        let mut trial = self.copy.clone();
        let mut env = self.env.clone();
        match apply_one(&mut trial, &mut env, o, o.action, &self.created) {
            Ok(guid) => {
                if o.action == ImportAction::Add {
                    self.created.insert(o.index, guid);
                }
                (self.copy, self.env) = (trial, env);
                true
            }
            Err(e) => {
                let message = format!("{} {}: {e}", o.kind.type_name(), o.id);
                self.out.errors.push(RowError {
                    file: self.ctx.file,
                    line: row.line,
                    column: None,
                    message,
                });
                false
            }
        }
    }
}

/// Read a `csvOwners` file and work out what importing it would do.
pub fn propose(text: &str, ctx: &Context<'_>, opts: &OwnerOptions) -> Result<Proposed, regex::Error> {
    let pattern = reader::line_pattern(&FIELDS, &[], opts.separator)?;
    let read = reader::read(text, ctx.file, &FIELDS, &pattern, opts.separator);
    let env = FixedEnv::new(ctx.now, PREVIEW_GUID_SEED).with_zone(ctx.zone.clone());
    let mut planner = Planner {
        ctx,
        opts,
        copy: ctx.book.clone(),
        env,
        created: Created::new(),
        out: Proposed::default(),
    };
    planner.out.ignored = read.ignored;
    planner.out.warnings = stripped_warnings(&read.stripped);
    read.rows
        .iter()
        .enumerate()
        .for_each(|(i, row)| planner.plan(row, i == 0 && is_header(row)));
    Ok(planner.out)
}
