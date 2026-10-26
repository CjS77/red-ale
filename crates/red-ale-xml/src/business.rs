//! Business objects in XML: `gnc:GncBillTerm`, `gnc:GncCustomer`, `gnc:GncEmployee`, `gnc:GncEntry`,
//! `gnc:GncInvoice`, `gnc:GncJob`, `gnc:GncOrder`, `gnc:GncTaxTable` and `gnc:GncVendor`.
//!
//! Element order and write conditions follow `design/notes/xml-format-business.md` §5, which cites the 5.17 writers
//! (`5.17:libgnucash/backend/xml/gnc-*-xml-v2.cpp`).

use red_ale_core::business::{
    Address, AmountType, BillTerm, BillTermKind, Customer, DiscountHow, Employee, Entry, Invoice, Job, Order, OwnerKind, OwnerRef,
    PaymentType, TaxIncluded, TaxTable, TaxTableEntry, Vendor,
};
use red_ale_core::{Frame, Guid, Numeric, Time64};

use crate::XmlError;
use crate::dom::Element;
use crate::values::{
    Notices, bool_int_el, commodity_ref_el, guid_el, int_el, malformed, numeric_el, parse_bool_int, parse_commodity_ref, parse_guid,
    parse_int, parse_numeric, parse_slots, parse_time, slots_el, text_el, text_el_nonempty, time_el, unknown_children,
};

const VERSION: &str = "2.0.0";

/// The top-level element names of business objects, in the order GnuCash writes the types.
pub const ELEMENT_NAMES: [&str; 9] = [
    "gnc:GncBillTerm",
    "gnc:GncCustomer",
    "gnc:GncEmployee",
    "gnc:GncEntry",
    "gnc:GncInvoice",
    "gnc:GncJob",
    "gnc:GncOrder",
    "gnc:GncTaxTable",
    "gnc:GncVendor",
];

/// A field reader that walks an element's children once and remembers which names it saw.
struct Fields<'a> {
    el: &'a Element,
}

impl<'a> Fields<'a> {
    fn new(el: &'a Element) -> Self {
        Self { el }
    }

    fn get(&self, name: &str) -> Option<&'a Element> {
        self.el.find(name)
    }

    fn req(&self, name: &str) -> Result<&'a Element, XmlError> {
        self.get(name).ok_or_else(|| malformed(&self.el.name, format!("missing {name}")))
    }

    fn text(&self, name: &str) -> String {
        self.get(name).map(Element::text_content).unwrap_or_default()
    }

    fn req_text(&self, name: &str) -> Result<String, XmlError> {
        self.req(name).map(Element::text_content)
    }

    fn guid(&self, name: &str) -> Result<Option<Guid>, XmlError> {
        self.get(name).map(parse_guid).transpose()
    }

    fn numeric(&self, name: &str) -> Result<Numeric, XmlError> {
        Ok(self.get(name).map(parse_numeric).transpose()?.unwrap_or(Numeric::ZERO))
    }

    fn int(&self, name: &str) -> Result<i64, XmlError> {
        Ok(self.get(name).map(parse_int).transpose()?.unwrap_or(0))
    }

    fn bool(&self, name: &str, default: bool) -> Result<bool, XmlError> {
        Ok(self.get(name).map(parse_bool_int).transpose()?.unwrap_or(default))
    }

    fn time(&self, name: &str) -> Result<Option<Time64>, XmlError> {
        self.get(name).map(parse_time).transpose()
    }

    fn slots(&self, name: &str, notices: &mut Notices) -> Result<Frame, XmlError> {
        Ok(self.get(name).map(|s| parse_slots(s, notices)).transpose()?.unwrap_or_default())
    }

    fn word<T>(&self, name: &str, default: T, parse: impl Fn(&str) -> Option<T>, notices: &mut Notices) -> T {
        match self.get(name) {
            Some(e) => parse(&e.text_content()).unwrap_or_else(|| {
                notices.unrecognised.push(format!("{name} has unknown value {:?}", e.text_content()));
                default
            }),
            None => default,
        }
    }

    fn check_known(&self, known: &[&str], notices: &mut Notices) {
        unknown_children(self.el, known).iter().for_each(|(_, c)| notices.unrecognised.push(format!("unknown element {} in {}", c.name, self.el.name)));
    }
}

fn parse_address(el: &Element, notices: &mut Notices) -> Result<Address, XmlError> {
    let f = Fields::new(el);
    f.check_known(&["addr:name", "addr:addr1", "addr:addr2", "addr:addr3", "addr:addr4", "addr:phone", "addr:fax", "addr:email", "addr:slots"], notices);
    Ok(Address {
        name: f.text("addr:name"),
        addr1: f.text("addr:addr1"),
        addr2: f.text("addr:addr2"),
        addr3: f.text("addr:addr3"),
        addr4: f.text("addr:addr4"),
        phone: f.text("addr:phone"),
        fax: f.text("addr:fax"),
        email: f.text("addr:email"),
        slots: f.slots("addr:slots", notices)?,
    })
}

/// `gnc_address_to_dom_tree`, `5.17:libgnucash/backend/xml/gnc-address-xml-v2.cpp:67-90`.
fn address_el(tag: &str, a: &Address) -> Element {
    Element::new(tag)
        .attr("version", VERSION)
        .child_opt(text_el_nonempty("addr:name", &a.name))
        .child_opt(text_el_nonempty("addr:addr1", &a.addr1))
        .child_opt(text_el_nonempty("addr:addr2", &a.addr2))
        .child_opt(text_el_nonempty("addr:addr3", &a.addr3))
        .child_opt(text_el_nonempty("addr:addr4", &a.addr4))
        .child_opt(text_el_nonempty("addr:phone", &a.phone))
        .child_opt(text_el_nonempty("addr:fax", &a.fax))
        .child_opt(text_el_nonempty("addr:email", &a.email))
        .child_opt(slots_el("addr:slots", &a.slots))
}

fn parse_owner(el: &Element) -> Result<OwnerRef, XmlError> {
    let kind_text = el.find("owner:type").map(Element::text_content).ok_or_else(|| malformed(&el.name, "missing owner:type"))?;
    let kind = OwnerKind::from_type_name(&kind_text).ok_or_else(|| malformed(&el.name, format!("unknown owner type {kind_text:?}")))?;
    let guid = parse_guid(el.find("owner:id").ok_or_else(|| malformed(&el.name, "missing owner:id"))?)?;
    Ok(OwnerRef { kind, guid })
}

/// `gnc_owner_to_dom_tree`, `5.17:libgnucash/backend/xml/gnc-owner-xml-v2.cpp:56-89`.
fn owner_el(tag: &str, o: OwnerRef) -> Element {
    Element::new(tag).attr("version", VERSION).child(Element::text("owner:type", o.kind.type_name())).child(guid_el("owner:id", o.guid))
}

fn top(name: &str) -> Element {
    Element::new(name).attr("version", VERSION)
}

/// A parsed business object.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // Short-lived: each is moved into the Book as soon as it is parsed.
pub enum BusinessObject {
    /// A billing term.
    BillTerm(BillTerm),
    /// A customer.
    Customer(Customer),
    /// An employee.
    Employee(Employee),
    /// An entry.
    Entry(Entry),
    /// A document.
    Invoice(Invoice),
    /// A job.
    Job(Job),
    /// An order.
    Order(Order),
    /// A tax table.
    TaxTable(TaxTable),
    /// A vendor.
    Vendor(Vendor),
}

impl BusinessObject {
    /// The object's GUID.
    pub fn guid(&self) -> Guid {
        match self {
            Self::BillTerm(o) => o.guid,
            Self::Customer(o) => o.guid,
            Self::Employee(o) => o.guid,
            Self::Entry(o) => o.guid,
            Self::Invoice(o) => o.guid,
            Self::Job(o) => o.guid,
            Self::Order(o) => o.guid,
            Self::TaxTable(o) => o.guid,
            Self::Vendor(o) => o.guid,
        }
    }

    /// The element GnuCash 5.17 would write for this object.
    pub fn to_element(&self) -> Element {
        match self {
            Self::BillTerm(o) => bill_term_el(o),
            Self::Customer(o) => customer_el(o),
            Self::Employee(o) => employee_el(o),
            Self::Entry(o) => entry_el(o),
            Self::Invoice(o) => invoice_el(o),
            Self::Job(o) => job_el(o),
            Self::Order(o) => order_el(o),
            Self::TaxTable(o) => tax_table_el(o),
            Self::Vendor(o) => vendor_el(o),
        }
    }
}

/// Read a top-level business element.
pub fn parse(el: &Element, notices: &mut Notices) -> Result<BusinessObject, XmlError> {
    Ok(match el.name.as_str() {
        "gnc:GncBillTerm" => BusinessObject::BillTerm(parse_bill_term(el, notices)?),
        "gnc:GncCustomer" => BusinessObject::Customer(parse_customer(el, notices)?),
        "gnc:GncEmployee" => BusinessObject::Employee(parse_employee(el, notices)?),
        "gnc:GncEntry" => BusinessObject::Entry(parse_entry(el, notices)?),
        "gnc:GncInvoice" => BusinessObject::Invoice(parse_invoice(el, notices)?),
        "gnc:GncJob" => BusinessObject::Job(parse_job(el, notices)?),
        "gnc:GncOrder" => BusinessObject::Order(parse_order(el, notices)?),
        "gnc:GncTaxTable" => BusinessObject::TaxTable(parse_tax_table(el, notices)?),
        "gnc:GncVendor" => BusinessObject::Vendor(parse_vendor(el, notices)?),
        other => return Err(malformed(other, "not a business object")),
    })
}

const BILL_TERM_FIELDS: [&str; 10] = [
    "billterm:guid",
    "billterm:name",
    "billterm:desc",
    "billterm:refcount",
    "billterm:invisible",
    "billterm:slots",
    "billterm:child",
    "billterm:parent",
    "billterm:days",
    "billterm:proximo",
];

fn parse_bill_term(el: &Element, notices: &mut Notices) -> Result<BillTerm, XmlError> {
    let f = Fields::new(el);
    f.check_known(&BILL_TERM_FIELDS, notices);
    let kind = if let Some(days) = f.get("billterm:days") {
        let d = Fields::new(days);
        d.check_known(&["bt-days:due-days", "bt-days:disc-days", "bt-days:discount"], notices);
        Some(BillTermKind::Days { due_days: d.int("bt-days:due-days")?, disc_days: d.int("bt-days:disc-days")?, discount: d.numeric("bt-days:discount")? })
    } else if let Some(prox) = f.get("billterm:proximo") {
        let p = Fields::new(prox);
        p.check_known(&["bt-prox:due-day", "bt-prox:disc-day", "bt-prox:discount", "bt-prox:cutoff-day"], notices);
        Some(BillTermKind::Proximo {
            due_day: p.int("bt-prox:due-day")?,
            disc_day: p.int("bt-prox:disc-day")?,
            discount: p.numeric("bt-prox:discount")?,
            cutoff_day: p.int("bt-prox:cutoff-day")?,
        })
    } else {
        None
    };
    Ok(BillTerm {
        guid: parse_guid(f.req("billterm:guid")?)?,
        name: f.req_text("billterm:name")?,
        description: f.req_text("billterm:desc")?,
        refcount: parse_int(f.req("billterm:refcount")?)?,
        invisible: parse_bool_int(f.req("billterm:invisible")?)?,
        slots: f.slots("billterm:slots", notices)?,
        child: f.guid("billterm:child")?,
        parent: f.guid("billterm:parent")?,
        kind,
    })
}

fn maybe_int(tag: &str, v: i64) -> Option<Element> {
    (v != 0).then(|| int_el(tag, v))
}

fn maybe_numeric(tag: &str, v: Numeric) -> Option<Element> {
    (!v.is_zero()).then(|| numeric_el(tag, v))
}

/// `5.17:libgnucash/backend/xml/gnc-bill-term-xml-v2.cpp:76-130`.
pub fn bill_term_el(t: &BillTerm) -> Element {
    let kind = t.kind.map(|k| match k {
        BillTermKind::Days { due_days, disc_days, discount } => Element::new("billterm:days")
            .child_opt(maybe_int("bt-days:due-days", due_days))
            .child_opt(maybe_int("bt-days:disc-days", disc_days))
            .child_opt(maybe_numeric("bt-days:discount", discount)),
        BillTermKind::Proximo { due_day, disc_day, discount, cutoff_day } => Element::new("billterm:proximo")
            .child_opt(maybe_int("bt-prox:due-day", due_day))
            .child_opt(maybe_int("bt-prox:disc-day", disc_day))
            .child_opt(maybe_numeric("bt-prox:discount", discount))
            .child_opt(maybe_int("bt-prox:cutoff-day", cutoff_day)),
    });
    top("gnc:GncBillTerm")
        .child(guid_el("billterm:guid", t.guid))
        .child(text_el("billterm:name", &t.name))
        .child(text_el("billterm:desc", &t.description))
        .child(int_el("billterm:refcount", t.refcount))
        .child(bool_int_el("billterm:invisible", t.invisible))
        .child_opt(slots_el("billterm:slots", &t.slots))
        .child_opt(t.child.filter(|c| *c != t.guid).map(|c| guid_el("billterm:child", c)))
        .child_opt(t.parent.map(|p| guid_el("billterm:parent", p)))
        .child_opt(kind)
}

const CUSTOMER_FIELDS: [&str; 16] = [
    "cust:guid",
    "cust:name",
    "cust:id",
    "cust:addr",
    "cust:shipaddr",
    "cust:notes",
    "cust:terms",
    "cust:taxincluded",
    "cust:active",
    "cust:discount",
    "cust:credit",
    "cust:currency",
    "cust:commodity",
    "cust:use-tt",
    "cust:taxtable",
    "cust:slots",
];

fn parse_customer(el: &Element, notices: &mut Notices) -> Result<Customer, XmlError> {
    let f = Fields::new(el);
    f.check_known(&CUSTOMER_FIELDS, notices);
    Ok(Customer {
        guid: parse_guid(f.req("cust:guid")?)?,
        name: f.req_text("cust:name")?,
        id: f.req_text("cust:id")?,
        addr: parse_address(f.req("cust:addr")?, notices)?,
        ship_addr: parse_address(f.req("cust:shipaddr")?, notices)?,
        notes: f.text("cust:notes"),
        terms: f.guid("cust:terms")?,
        tax_included: f.word("cust:taxincluded", TaxIncluded::UseGlobal, TaxIncluded::parse, notices),
        active: f.bool("cust:active", true)?,
        discount: f.numeric("cust:discount")?,
        credit: f.numeric("cust:credit")?,
        currency: f.get("cust:currency").or_else(|| f.get("cust:commodity")).map(parse_commodity_ref).transpose()?,
        use_tax_table: f.bool("cust:use-tt", false)?,
        tax_table: f.guid("cust:taxtable")?,
        slots: f.slots("cust:slots", notices)?,
    })
}

/// `5.17:libgnucash/backend/xml/gnc-customer-xml-v2.cpp:75-138`.
pub fn customer_el(c: &Customer) -> Element {
    top("gnc:GncCustomer")
        .child(guid_el("cust:guid", c.guid))
        .child(text_el("cust:name", &c.name))
        .child(text_el("cust:id", &c.id))
        .child(address_el("cust:addr", &c.addr))
        .child(address_el("cust:shipaddr", &c.ship_addr))
        .child_opt(text_el_nonempty("cust:notes", &c.notes))
        .child_opt(c.terms.map(|t| guid_el("cust:terms", t)))
        .child(text_el("cust:taxincluded", c.tax_included.as_str()))
        .child(bool_int_el("cust:active", c.active))
        .child(numeric_el("cust:discount", c.discount))
        .child(numeric_el("cust:credit", c.credit))
        .child_opt(c.currency.as_ref().map(|cur| commodity_ref_el("cust:currency", cur)))
        .child(bool_int_el("cust:use-tt", c.use_tax_table))
        .child_opt(c.tax_table.map(|t| guid_el("cust:taxtable", t)))
        .child_opt(slots_el("cust:slots", &c.slots))
}

const EMPLOYEE_FIELDS: [&str; 13] = [
    "employee:guid",
    "employee:username",
    "employee:id",
    "employee:addr",
    "employee:language",
    "employee:acl",
    "employee:active",
    "employee:workday",
    "employee:rate",
    "employee:currency",
    "employee:commodity",
    "employee:ccard",
    "employee:slots",
];

fn parse_employee(el: &Element, notices: &mut Notices) -> Result<Employee, XmlError> {
    let f = Fields::new(el);
    f.check_known(&EMPLOYEE_FIELDS, notices);
    Ok(Employee {
        guid: parse_guid(f.req("employee:guid")?)?,
        username: f.req_text("employee:username")?,
        id: f.req_text("employee:id")?,
        addr: parse_address(f.req("employee:addr")?, notices)?,
        language: f.text("employee:language"),
        acl: f.text("employee:acl"),
        active: f.bool("employee:active", true)?,
        workday: f.numeric("employee:workday")?,
        rate: f.numeric("employee:rate")?,
        currency: f.get("employee:currency").or_else(|| f.get("employee:commodity")).map(parse_commodity_ref).transpose()?,
        ccard: f.guid("employee:ccard")?,
        slots: f.slots("employee:slots", notices)?,
    })
}

/// `5.17:libgnucash/backend/xml/gnc-employee-xml-v2.cpp:73-122`.
pub fn employee_el(e: &Employee) -> Element {
    top("gnc:GncEmployee")
        .child(guid_el("employee:guid", e.guid))
        .child(text_el("employee:username", &e.username))
        .child(text_el("employee:id", &e.id))
        .child(address_el("employee:addr", &e.addr))
        .child_opt(text_el_nonempty("employee:language", &e.language))
        .child_opt(text_el_nonempty("employee:acl", &e.acl))
        .child(bool_int_el("employee:active", e.active))
        .child(numeric_el("employee:workday", e.workday))
        .child(numeric_el("employee:rate", e.rate))
        .child_opt(e.currency.as_ref().map(|cur| commodity_ref_el("employee:currency", cur)))
        .child_opt(e.ccard.map(|a| guid_el("employee:ccard", a)))
        .child_opt(slots_el("employee:slots", &e.slots))
}

const ENTRY_FIELDS: [&str; 27] = [
    "entry:guid",
    "entry:date",
    "entry:entered",
    "entry:description",
    "entry:action",
    "entry:notes",
    "entry:qty",
    "entry:i-acct",
    "entry:i-price",
    "entry:i-discount",
    "entry:invoice",
    "entry:i-disc-type",
    "entry:i-disc-how",
    "entry:i-taxable",
    "entry:i-taxincluded",
    "entry:i-taxtable",
    "entry:b-acct",
    "entry:b-price",
    "entry:bill",
    "entry:billable",
    "entry:billto",
    "entry:b-taxable",
    "entry:b-taxincluded",
    "entry:b-pay",
    "entry:b-taxtable",
    "entry:order",
    "entry:slots",
];

fn parse_entry(el: &Element, notices: &mut Notices) -> Result<Entry, XmlError> {
    let f = Fields::new(el);
    f.check_known(&ENTRY_FIELDS, notices);
    let date = f.time("entry:date")?.ok_or_else(|| malformed(&el.name, "missing entry:date"))?;
    let entered = f.time("entry:entered")?.ok_or_else(|| malformed(&el.name, "missing entry:entered"))?;
    let mut e = Entry::new(parse_guid(f.req("entry:guid")?)?, date, entered);
    e.description = f.text("entry:description");
    e.action = f.text("entry:action");
    e.notes = f.text("entry:notes");
    e.quantity = f.numeric("entry:qty")?;
    e.i_account = f.guid("entry:i-acct")?;
    e.i_price = f.numeric("entry:i-price")?;
    e.i_discount = f.numeric("entry:i-discount")?;
    e.invoice = f.guid("entry:invoice")?;
    e.i_disc_type = f.word("entry:i-disc-type", AmountType::Percent, AmountType::parse, notices);
    e.i_disc_how = f.word("entry:i-disc-how", DiscountHow::PreTax, DiscountHow::parse, notices);
    e.i_taxable = f.bool("entry:i-taxable", true)?;
    e.i_tax_included = f.bool("entry:i-taxincluded", false)?;
    e.i_tax_table = f.guid("entry:i-taxtable")?;
    e.b_account = f.guid("entry:b-acct")?;
    e.b_price = f.numeric("entry:b-price")?;
    e.bill = f.guid("entry:bill")?;
    e.billable = f.bool("entry:billable", false)?;
    e.bill_to = f.get("entry:billto").map(parse_owner).transpose()?;
    e.b_taxable = f.bool("entry:b-taxable", true)?;
    e.b_tax_included = f.bool("entry:b-taxincluded", false)?;
    e.b_payment = f.word("entry:b-pay", PaymentType::Cash, PaymentType::parse, notices);
    e.b_tax_table = f.guid("entry:b-taxtable")?;
    e.order = f.guid("entry:order")?;
    e.slots = f.slots("entry:slots", notices)?;
    Ok(e)
}

/// `5.17:libgnucash/backend/xml/gnc-entry-xml-v2.cpp:107-216`.
pub fn entry_el(e: &Entry) -> Element {
    let mut el = top("gnc:GncEntry")
        .child(guid_el("entry:guid", e.guid))
        .child(time_el("entry:date", e.date))
        .child(time_el("entry:entered", e.entered))
        .child_opt(text_el_nonempty("entry:description", &e.description))
        .child_opt(text_el_nonempty("entry:action", &e.action))
        .child_opt(text_el_nonempty("entry:notes", &e.notes))
        .child_opt(maybe_numeric("entry:qty", e.quantity))
        .child_opt(e.i_account.map(|a| guid_el("entry:i-acct", a)))
        .child_opt(maybe_numeric("entry:i-price", e.i_price))
        .child_opt(maybe_numeric("entry:i-discount", e.i_discount));
    if let Some(inv) = e.invoice {
        el = el
            .child(guid_el("entry:invoice", inv))
            .child(text_el("entry:i-disc-type", e.i_disc_type.as_str()))
            .child(text_el("entry:i-disc-how", e.i_disc_how.as_str()))
            .child(bool_int_el("entry:i-taxable", e.i_taxable))
            .child(bool_int_el("entry:i-taxincluded", e.i_tax_included));
    }
    el = el
        .child_opt(e.i_tax_table.map(|t| guid_el("entry:i-taxtable", t)))
        .child_opt(e.b_account.map(|a| guid_el("entry:b-acct", a)))
        .child_opt(maybe_numeric("entry:b-price", e.b_price));
    if let Some(bill) = e.bill {
        el = el
            .child(guid_el("entry:bill", bill))
            .child(bool_int_el("entry:billable", e.billable))
            .child_opt(e.bill_to.map(|o| owner_el("entry:billto", o)))
            .child(bool_int_el("entry:b-taxable", e.b_taxable))
            .child(bool_int_el("entry:b-taxincluded", e.b_tax_included))
            .child(text_el("entry:b-pay", e.b_payment.as_str()));
    }
    el.child_opt(e.b_tax_table.map(|t| guid_el("entry:b-taxtable", t)))
        .child_opt(e.order.map(|o| guid_el("entry:order", o)))
        .child_opt(slots_el("entry:slots", &e.slots))
}

const INVOICE_FIELDS: [&str; 17] = [
    "invoice:guid",
    "invoice:id",
    "invoice:owner",
    "invoice:opened",
    "invoice:posted",
    "invoice:terms",
    "invoice:billing_id",
    "invoice:notes",
    "invoice:active",
    "invoice:posttxn",
    "invoice:postlot",
    "invoice:postacc",
    "invoice:currency",
    "invoice:commodity",
    "invoice:billto",
    "invoice:charge-amt",
    "invoice:slots",
];

fn parse_invoice(el: &Element, notices: &mut Notices) -> Result<Invoice, XmlError> {
    let f = Fields::new(el);
    f.check_known(&INVOICE_FIELDS, notices);
    Ok(Invoice {
        guid: parse_guid(f.req("invoice:guid")?)?,
        id: f.req_text("invoice:id")?,
        owner: parse_owner(f.req("invoice:owner")?)?,
        opened: parse_time(f.req("invoice:opened")?)?,
        posted: f.time("invoice:posted")?,
        terms: f.guid("invoice:terms")?,
        billing_id: f.text("invoice:billing_id"),
        notes: f.text("invoice:notes"),
        active: f.bool("invoice:active", true)?,
        post_txn: f.guid("invoice:posttxn")?,
        post_lot: f.guid("invoice:postlot")?,
        post_acc: f.guid("invoice:postacc")?,
        currency: f.get("invoice:currency").or_else(|| f.get("invoice:commodity")).map(parse_commodity_ref).transpose()?,
        bill_to: f.get("invoice:billto").map(parse_owner).transpose()?,
        charge_amount: f.numeric("invoice:charge-amt")?,
        slots: f.slots("invoice:slots", notices)?,
    })
}

/// `5.17:libgnucash/backend/xml/gnc-invoice-xml-v2.cpp:87-160`.
pub fn invoice_el(i: &Invoice) -> Element {
    top("gnc:GncInvoice")
        .child(guid_el("invoice:guid", i.guid))
        .child(text_el("invoice:id", &i.id))
        .child(owner_el("invoice:owner", i.owner))
        .child(time_el("invoice:opened", i.opened))
        .child_opt(i.posted.map(|p| time_el("invoice:posted", p)))
        .child_opt(i.terms.map(|t| guid_el("invoice:terms", t)))
        .child_opt(text_el_nonempty("invoice:billing_id", &i.billing_id))
        .child_opt(text_el_nonempty("invoice:notes", &i.notes))
        .child(bool_int_el("invoice:active", i.active))
        .child_opt(i.post_txn.map(|t| guid_el("invoice:posttxn", t)))
        .child_opt(i.post_lot.map(|l| guid_el("invoice:postlot", l)))
        .child_opt(i.post_acc.map(|a| guid_el("invoice:postacc", a)))
        .child_opt(i.currency.as_ref().map(|c| commodity_ref_el("invoice:currency", c)))
        .child_opt(i.bill_to.map(|o| owner_el("invoice:billto", o)))
        .child_opt(maybe_numeric("invoice:charge-amt", i.charge_amount))
        .child_opt(slots_el("invoice:slots", &i.slots))
}

fn parse_job(el: &Element, notices: &mut Notices) -> Result<Job, XmlError> {
    let f = Fields::new(el);
    f.check_known(&["job:guid", "job:id", "job:name", "job:reference", "job:owner", "job:active", "job:slots"], notices);
    Ok(Job {
        guid: parse_guid(f.req("job:guid")?)?,
        id: f.req_text("job:id")?,
        name: f.req_text("job:name")?,
        reference: f.text("job:reference"),
        owner: parse_owner(f.req("job:owner")?)?,
        active: f.bool("job:active", true)?,
        slots: f.slots("job:slots", notices)?,
    })
}

/// `5.17:libgnucash/backend/xml/gnc-job-xml-v2.cpp:63-93`.
pub fn job_el(j: &Job) -> Element {
    top("gnc:GncJob")
        .child(guid_el("job:guid", j.guid))
        .child(text_el("job:id", &j.id))
        .child(text_el("job:name", &j.name))
        .child_opt(text_el_nonempty("job:reference", &j.reference))
        .child(owner_el("job:owner", j.owner))
        .child(bool_int_el("job:active", j.active))
        .child_opt(slots_el("job:slots", &j.slots))
}

fn parse_order(el: &Element, notices: &mut Notices) -> Result<Order, XmlError> {
    let f = Fields::new(el);
    f.check_known(
        &["order:guid", "order:id", "order:owner", "order:opened", "order:closed", "order:notes", "order:reference", "order:active", "order:slots"],
        notices,
    );
    Ok(Order {
        guid: parse_guid(f.req("order:guid")?)?,
        id: f.req_text("order:id")?,
        owner: parse_owner(f.req("order:owner")?)?,
        opened: parse_time(f.req("order:opened")?)?,
        closed: f.time("order:closed")?,
        notes: f.text("order:notes"),
        reference: f.text("order:reference"),
        active: f.bool("order:active", true)?,
        slots: f.slots("order:slots", notices)?,
    })
}

/// `5.17:libgnucash/backend/xml/gnc-order-xml-v2.cpp:71-107`.
pub fn order_el(o: &Order) -> Element {
    top("gnc:GncOrder")
        .child(guid_el("order:guid", o.guid))
        .child(text_el("order:id", &o.id))
        .child(owner_el("order:owner", o.owner))
        .child(time_el("order:opened", o.opened))
        .child_opt(o.closed.map(|c| time_el("order:closed", c)))
        .child_opt(text_el_nonempty("order:notes", &o.notes))
        .child_opt(text_el_nonempty("order:reference", &o.reference))
        .child(bool_int_el("order:active", o.active))
        .child_opt(slots_el("order:slots", &o.slots))
}

fn parse_tax_table(el: &Element, notices: &mut Notices) -> Result<TaxTable, XmlError> {
    let f = Fields::new(el);
    f.check_known(
        &["taxtable:guid", "taxtable:name", "taxtable:refcount", "taxtable:invisible", "taxtable:child", "taxtable:parent", "taxtable:entries", "taxtable:slots"],
        notices,
    );
    let entries = f
        .req("taxtable:entries")?
        .elements()
        .map(|e| {
            if e.name != "gnc:GncTaxTableEntry" {
                notices.unrecognised.push(format!("unknown element {} in taxtable:entries", e.name));
                return Ok(None);
            }
            let ef = Fields::new(e);
            ef.check_known(&["tte:acct", "tte:amount", "tte:type"], notices);
            let kind_text = ef.req_text("tte:type")?;
            let kind = AmountType::parse(&kind_text).ok_or_else(|| malformed("tte:type", format!("unknown type {kind_text:?}")))?;
            Ok(Some(TaxTableEntry { account: ef.guid("tte:acct")?, amount: parse_numeric(ef.req("tte:amount")?)?, kind }))
        })
        .collect::<Result<Vec<_>, XmlError>>()?
        .into_iter()
        .flatten()
        .collect();
    let guid = parse_guid(f.req("taxtable:guid")?)?;
    Ok(TaxTable {
        guid,
        name: f.req_text("taxtable:name")?,
        refcount: parse_int(f.req("taxtable:refcount")?)?,
        invisible: parse_bool_int(f.req("taxtable:invisible")?)?,
        child: f.guid("taxtable:child")?.filter(|c| *c != guid),
        parent: f.guid("taxtable:parent")?.filter(|p| *p != guid),
        entries,
        slots: f.slots("taxtable:slots", notices)?,
    })
}

/// `5.17:libgnucash/backend/xml/gnc-tax-table-xml-v2.cpp:76-135`.
pub fn tax_table_el(t: &TaxTable) -> Element {
    let mut entries = Element::new("taxtable:entries");
    t.entries.iter().for_each(|e| {
        entries.push(
            Element::new("gnc:GncTaxTableEntry")
                .child_opt(e.account.map(|a| guid_el("tte:acct", a)))
                .child(numeric_el("tte:amount", e.amount))
                .child(text_el("tte:type", e.kind.as_str())),
        );
    });
    top("gnc:GncTaxTable")
        .child(guid_el("taxtable:guid", t.guid))
        .child(text_el("taxtable:name", &t.name))
        .child(int_el("taxtable:refcount", t.refcount))
        .child(bool_int_el("taxtable:invisible", t.invisible))
        .child_opt(t.child.filter(|c| *c != t.guid).map(|c| guid_el("taxtable:child", c)))
        .child_opt(t.parent.map(|p| guid_el("taxtable:parent", p)))
        .child(entries)
        .child_opt(slots_el("taxtable:slots", &t.slots))
}

const VENDOR_FIELDS: [&str; 13] = [
    "vendor:guid",
    "vendor:name",
    "vendor:id",
    "vendor:addr",
    "vendor:notes",
    "vendor:terms",
    "vendor:taxincluded",
    "vendor:active",
    "vendor:currency",
    "vendor:commodity",
    "vendor:use-tt",
    "vendor:taxtable",
    "vendor:slots",
];

fn parse_vendor(el: &Element, notices: &mut Notices) -> Result<Vendor, XmlError> {
    let f = Fields::new(el);
    f.check_known(&VENDOR_FIELDS, notices);
    Ok(Vendor {
        guid: parse_guid(f.req("vendor:guid")?)?,
        name: f.req_text("vendor:name")?,
        id: f.req_text("vendor:id")?,
        addr: parse_address(f.req("vendor:addr")?, notices)?,
        notes: f.text("vendor:notes"),
        terms: f.guid("vendor:terms")?,
        tax_included: f.word("vendor:taxincluded", TaxIncluded::UseGlobal, TaxIncluded::parse, notices),
        active: f.bool("vendor:active", true)?,
        currency: f.get("vendor:currency").or_else(|| f.get("vendor:commodity")).map(parse_commodity_ref).transpose()?,
        use_tax_table: f.bool("vendor:use-tt", false)?,
        tax_table: f.guid("vendor:taxtable")?,
        slots: f.slots("vendor:slots", notices)?,
    })
}

/// `5.17:libgnucash/backend/xml/gnc-vendor-xml-v2.cpp:71-123`.
pub fn vendor_el(v: &Vendor) -> Element {
    top("gnc:GncVendor")
        .child(guid_el("vendor:guid", v.guid))
        .child(text_el("vendor:name", &v.name))
        .child(text_el("vendor:id", &v.id))
        .child(address_el("vendor:addr", &v.addr))
        .child_opt(text_el_nonempty("vendor:notes", &v.notes))
        .child_opt(v.terms.map(|t| guid_el("vendor:terms", t)))
        .child(text_el("vendor:taxincluded", v.tax_included.as_str()))
        .child(bool_int_el("vendor:active", v.active))
        .child_opt(v.currency.as_ref().map(|c| commodity_ref_el("vendor:currency", c)))
        .child(bool_int_el("vendor:use-tt", v.use_tax_table))
        .child_opt(v.tax_table.map(|t| guid_el("vendor:taxtable", t)))
        .child_opt(slots_el("vendor:slots", &v.slots))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::parse_document;

    fn regenerate(obj: &BusinessObject) -> Element {
        match obj {
            BusinessObject::BillTerm(o) => bill_term_el(o),
            BusinessObject::Customer(o) => customer_el(o),
            BusinessObject::Employee(o) => employee_el(o),
            BusinessObject::Entry(o) => entry_el(o),
            BusinessObject::Invoice(o) => invoice_el(o),
            BusinessObject::Job(o) => job_el(o),
            BusinessObject::Order(o) => order_el(o),
            BusinessObject::TaxTable(o) => tax_table_el(o),
            BusinessObject::Vendor(o) => vendor_el(o),
        }
    }

    #[test]
    fn every_business_object_regenerates_byte_for_byte() {
        let path = format!("{}/../../fixtures/synthetic/business.gnucash", env!("CARGO_MANIFEST_DIR"));
        let root = parse_document(&std::fs::read_to_string(path).unwrap()).unwrap();
        let book = root.find("gnc:book").unwrap();
        let mut seen = 0;
        for el in book.elements().filter(|e| ELEMENT_NAMES.contains(&e.name.as_str())) {
            let mut notices = Notices::default();
            let obj = parse(el, &mut notices).unwrap();
            assert!(notices.unrecognised.is_empty(), "{:?}", notices.unrecognised);
            assert_eq!(regenerate(&obj).dump(0), el.dump(0));
            seen += 1;
        }
        assert!(seen > 15, "only {seen} business objects in the fixture");
    }
}
