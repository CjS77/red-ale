//! Reading and writing whole books, and the Residue that keeps what the core model does not represent.
//!
//! See `design/file-fidelity.md`. An entity that is unchanged since it was read is written from its original element;
//! a changed or new one is generated as GnuCash 5.17 would write it.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::sync::Arc;

use flate2::read::GzDecoder;
use red_ale_core::business::Business;
use red_ale_core::{Account, AccountType, Book, CommodityRef, Frame, Guid, Lot, Transaction};

use crate::XmlError;
use crate::business::{self, BusinessObject};
use crate::dom::{Element, parse_document};
use crate::entities::{
    account_el, commodity_el, commodity_is_written, parse_account, parse_commodity, parse_price, parse_transaction, price_el, transaction_el,
};
use crate::envelope::{BOOK_VERSION, FOOTER, count_line, header};
use crate::values::{Notices, guid_el, parse_guid, parse_slots, slots_el};

/// Where in the book an element sits; used to put unknown elements back where they were.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Section {
    Head,
    Commodities,
    Prices,
    Accounts,
    Transactions,
    Templates,
    ScheduledTransactions,
    Budgets,
    Business,
}

/// Everything in a book file that the core `Book` does not hold, plus what the writer needs to reproduce the file.
#[derive(Debug, Clone)]
pub struct XmlResidue {
    /// The file was gzip-compressed.
    pub compressed: bool,
    book_version: String,
    snapshot: Book,
    book_slots: Option<Element>,
    commodities: BTreeMap<CommodityRef, Element>,
    prices: HashMap<Guid, Element>,
    accounts: HashMap<Guid, Element>,
    transactions: HashMap<Guid, Element>,
    business: HashMap<Guid, Element>,
    out_of_scope: Vec<(Section, Element)>,
    unknown: Vec<(Section, Element)>,
    /// Content red-ale could read but cannot represent; editing such a book could lose it.
    pub notices: Notices,
}

impl XmlResidue {
    /// The residue of a new, empty book.
    pub fn new_book(book: &Book, compressed: bool) -> Self {
        Self {
            compressed,
            book_version: BOOK_VERSION.to_owned(),
            snapshot: book.clone(),
            book_slots: None,
            commodities: BTreeMap::new(),
            prices: HashMap::new(),
            accounts: HashMap::new(),
            transactions: HashMap::new(),
            business: HashMap::new(),
            out_of_scope: Vec::new(),
            unknown: Vec::new(),
            notices: Notices::default(),
        }
    }

    /// Out-of-scope sections kept as read: template transactions, scheduled transactions and budgets.
    pub fn out_of_scope_elements(&self) -> impl Iterator<Item = &Element> {
        self.out_of_scope.iter().chain(&self.unknown).map(|(_, e)| e)
    }

    /// Names of out-of-scope sections whose text mentions `needle` (a GUID or mnemonic), for refusing deletions that
    /// would leave them dangling.
    pub fn sections_mentioning(&self, needle: &str) -> Vec<String> {
        self.out_of_scope_elements().filter(|e| element_mentions(e, needle)).map(|e| e.name.clone()).collect()
    }

    /// Forget the originals: the next write generates every entity from the model. Used to test the generators.
    pub fn forget_originals(&mut self) {
        self.book_slots = None;
        self.commodities.clear();
        self.prices.clear();
        self.accounts.clear();
        self.transactions.clear();
        self.business.clear();
    }

    /// Record `book` as the saved state, so later writes compare against it.
    pub fn mark_saved(&mut self, book: &Book) {
        self.snapshot = book.clone();
    }
}

fn element_mentions(el: &Element, needle: &str) -> bool {
    el.attrs.iter().any(|(_, v)| v.contains(needle))
        || el.children.iter().any(|c| match c {
            crate::dom::Node::Text(t) => t.contains(needle),
            crate::dom::Node::Element(e) => element_mentions(e, needle),
        })
}

/// True if the bytes start with the gzip magic number.
pub fn is_gzip(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1f, 0x8b])
}

/// Decompress if gzipped and decode as UTF-8.
pub fn decode(bytes: &[u8]) -> Result<(String, bool), XmlError> {
    if is_gzip(bytes) {
        let mut text = String::new();
        GzDecoder::new(bytes).read_to_string(&mut text).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData { XmlError::Encoding } else { XmlError::Io(e) }
        })?;
        Ok((text, true))
    } else {
        String::from_utf8(bytes.to_vec()).map(|t| (t, false)).map_err(|_| XmlError::Encoding)
    }
}

/// Check the start of a decoded file the way GnuCash's `gnc_is_xml_data_file_v2` does.
fn check_v2(text: &str) -> Result<(), XmlError> {
    let trimmed = text.trim_start();
    let rest = trimmed.strip_prefix("<?xml").ok_or_else(|| XmlError::NotABook("no XML declaration".into()))?;
    let after_decl = rest.split_once('>').map(|(_, r)| r.trim_start()).ok_or_else(|| XmlError::NotABook("unterminated declaration".into()))?;
    if after_decl.starts_with("<gnc-v2") && after_decl[7..].starts_with(|c: char| c.is_whitespace()) {
        Ok(())
    } else if after_decl.starts_with("<gnc>") {
        Err(XmlError::NotABook("this is a GnuCash version 1 file; open and save it with GnuCash first".into()))
    } else if after_decl.starts_with("<gnc-v") {
        Err(XmlError::NotABook("this file was written by a newer GnuCash format version".into()))
    } else {
        Err(XmlError::NotABook("the root element is not gnc-v2".into()))
    }
}

/// Read a book file's bytes.
pub fn read_book(bytes: &[u8]) -> Result<(Book, XmlResidue), XmlError> {
    let (text, compressed) = decode(bytes)?;
    check_v2(&text)?;
    let root = parse_document(&text)?;
    let books: Vec<&Element> = root.elements().filter(|e| e.name == "gnc:book").collect();
    let [book_el] = books.as_slice() else {
        return Err(XmlError::NotABook(format!("expected one gnc:book, found {}", books.len())));
    };
    let mut reader = Reader::default();
    reader.residue_book_version = book_el.get_attr("version").unwrap_or(BOOK_VERSION).to_owned();
    book_el.elements().try_for_each(|el| reader.part(el))?;
    root.elements().filter(|e| e.name != "gnc:book" && e.name != "gnc:count-data").for_each(|e| {
        reader.notices.unrecognised.push(format!("unknown element {} outside the book", e.name));
    });
    reader.finish(compressed)
}

#[derive(Default)]
struct Reader {
    residue_book_version: String,
    section: Option<Section>,
    guid: Option<Guid>,
    slots: Frame,
    book_slots: Option<Element>,
    commodities: BTreeMap<CommodityRef, Arc<red_ale_core::Commodity>>,
    accounts: indexmap::IndexMap<Guid, Arc<Account>>,
    transactions: indexmap::IndexMap<Guid, Arc<Transaction>>,
    prices: indexmap::IndexMap<Guid, Arc<red_ale_core::Price>>,
    lots: indexmap::IndexMap<Guid, Arc<Lot>>,
    business: Business,
    commodity_els: BTreeMap<CommodityRef, Element>,
    price_els: HashMap<Guid, Element>,
    account_els: HashMap<Guid, Element>,
    transaction_els: HashMap<Guid, Element>,
    business_els: HashMap<Guid, Element>,
    out_of_scope: Vec<(Section, Element)>,
    unknown: Vec<(Section, Element)>,
    notices: Notices,
}

impl Reader {
    fn enter(&mut self, s: Section) {
        self.section = Some(s);
    }

    fn part(&mut self, el: &Element) -> Result<(), XmlError> {
        match el.name.as_str() {
            "book:id" => self.guid = Some(parse_guid(el)?),
            "book:slots" => {
                self.slots = parse_slots(el, &mut self.notices)?;
                self.book_slots = Some(el.clone());
            }
            "gnc:count-data" => {}
            "gnc:commodity" => {
                self.enter(Section::Commodities);
                let c = parse_commodity(el, &mut self.notices)?;
                self.commodity_els.insert(c.id.clone(), el.clone());
                self.commodities.insert(c.id.clone(), Arc::new(c));
            }
            "gnc:pricedb" => {
                self.enter(Section::Prices);
                for p in el.elements() {
                    let price = parse_price(p, &mut self.notices)?;
                    self.price_els.insert(price.guid, p.clone());
                    self.prices.insert(price.guid, Arc::new(price));
                }
            }
            "gnc:account" => {
                self.enter(Section::Accounts);
                let (account, lots) = parse_account(el, &mut self.notices)?;
                self.account_els.insert(account.guid, el.clone());
                lots.into_iter().for_each(|l| {
                    self.lots.insert(l.guid, Arc::new(l));
                });
                self.accounts.insert(account.guid, Arc::new(account));
            }
            "gnc:transaction" => {
                self.enter(Section::Transactions);
                let t = parse_transaction(el, &mut self.notices)?;
                self.transaction_els.insert(t.guid, el.clone());
                self.transactions.insert(t.guid, Arc::new(t));
            }
            "gnc:template-transactions" => self.out_of_scope_part(Section::Templates, el),
            "gnc:schedxaction" => self.out_of_scope_part(Section::ScheduledTransactions, el),
            "gnc:budget" => self.out_of_scope_part(Section::Budgets, el),
            name if business::ELEMENT_NAMES.contains(&name) => {
                self.enter(Section::Business);
                let obj = business::parse(el, &mut self.notices)?;
                self.business_els.insert(obj.guid(), el.clone());
                insert_business(&mut self.business, obj);
            }
            other => {
                self.notices.unrecognised.push(format!("unknown element {other} in the book"));
                self.unknown.push((self.section.unwrap_or(Section::Head), el.clone()));
            }
        }
        Ok(())
    }

    fn out_of_scope_part(&mut self, section: Section, el: &Element) {
        self.enter(section);
        self.out_of_scope.push((section, el.clone()));
    }

    fn finish(self, compressed: bool) -> Result<(Book, XmlResidue), XmlError> {
        let guid = self.guid.ok_or_else(|| XmlError::NotABook("the book has no book:id".into()))?;
        let roots: Vec<Guid> = self.accounts.values().filter(|a| a.account_type == AccountType::Root && a.parent.is_none()).map(|a| a.guid).collect();
        let root = match roots.as_slice() {
            [root] => *root,
            [] => return Err(XmlError::NotABook("the book has no root account".into())),
            _ => return Err(XmlError::NotABook("the book has more than one root account".into())),
        };
        let book = Book {
            guid,
            slots: self.slots,
            commodities: self.commodities,
            root,
            accounts: self.accounts,
            transactions: self.transactions,
            prices: self.prices,
            lots: self.lots,
            business: self.business,
        };
        let residue = XmlResidue {
            compressed,
            book_version: self.residue_book_version,
            snapshot: book.clone(),
            book_slots: self.book_slots,
            commodities: self.commodity_els,
            prices: self.price_els,
            accounts: self.account_els,
            transactions: self.transaction_els,
            business: self.business_els,
            out_of_scope: self.out_of_scope,
            unknown: self.unknown,
            notices: self.notices,
        };
        Ok((book, residue))
    }
}

fn insert_business(b: &mut Business, obj: BusinessObject) {
    match obj {
        BusinessObject::BillTerm(o) => {
            b.bill_terms.insert(o.guid, Arc::new(o));
        }
        BusinessObject::Customer(o) => {
            b.customers.insert(o.guid, Arc::new(o));
        }
        BusinessObject::Employee(o) => {
            b.employees.insert(o.guid, Arc::new(o));
        }
        BusinessObject::Entry(o) => {
            b.entries.insert(o.guid, Arc::new(o));
        }
        BusinessObject::Invoice(o) => {
            b.invoices.insert(o.guid, Arc::new(o));
        }
        BusinessObject::Job(o) => {
            b.jobs.insert(o.guid, Arc::new(o));
        }
        BusinessObject::Order(o) => {
            b.orders.insert(o.guid, Arc::new(o));
        }
        BusinessObject::TaxTable(o) => {
            b.tax_tables.insert(o.guid, Arc::new(o));
        }
        BusinessObject::Vendor(o) => {
            b.vendors.insert(o.guid, Arc::new(o));
        }
    }
}

fn same<T: PartialEq>(a: &Arc<T>, b: &Arc<T>) -> bool {
    Arc::ptr_eq(a, b) || **a == **b
}

/// Writes one book; holds the lookups the writers share.
struct Writer<'a> {
    book: &'a Book,
    residue: &'a XmlResidue,
    lots_by_account: HashMap<Guid, Vec<&'a Lot>>,
    out: String,
}

impl<'a> Writer<'a> {
    fn new(book: &'a Book, residue: &'a XmlResidue) -> Self {
        let mut lots_by_account: HashMap<Guid, Vec<&Lot>> = HashMap::new();
        book.lots.values().for_each(|l| lots_by_account.entry(l.account).or_default().push(l));
        Self { book, residue, lots_by_account, out: String::new() }
    }

    fn line(&mut self, el: &Element) {
        self.out.push_str(&el.dump(0));
        self.out.push('\n');
    }

    fn unknown_after(&mut self, section: Section) {
        let parts: Vec<&Element> = self.residue.unknown.iter().filter(|(s, _)| *s == section).map(|(_, e)| e).collect();
        parts.into_iter().for_each(|e| self.line(e));
    }

    fn account_unchanged(&self, account: &Arc<Account>) -> bool {
        let Some(old) = self.residue.snapshot.accounts.get(&account.guid) else { return false };
        if !same(account, old) {
            return false;
        }
        let now: Vec<&Lot> = self.lots_by_account.get(&account.guid).cloned().unwrap_or_default();
        let before: Vec<&Arc<Lot>> = self.residue.snapshot.lots.values().filter(|l| l.account == account.guid).collect();
        now.len() == before.len() && now.iter().all(|l| self.residue.snapshot.lots.get(&l.guid).is_some_and(|o| **o == **l))
    }

    fn written_transactions(&self) -> impl Iterator<Item = &'a Arc<Transaction>> + use<'a> {
        let accounts = &self.book.accounts;
        self.book.transactions.values().filter(move |t| t.splits.iter().any(|s| s.account.is_some_and(|a| accounts.contains_key(&a))))
    }

    fn write(mut self) -> String {
        self.out.push_str(&header());
        count_line(&mut self.out, "book", 1);
        self.out.push_str(&format!("<gnc:book version=\"{}\">\n", self.residue.book_version));
        self.line(&guid_el("book:id", self.book.guid));
        self.book_slots();
        self.counts();
        self.unknown_after(Section::Head);
        self.commodities();
        self.prices();
        self.accounts();
        self.transactions();
        self.out_of_scope();
        self.business();
        self.out.push_str("</gnc:book>\n");
        self.out.push_str(FOOTER);
        self.out
    }

    fn book_slots(&mut self) {
        let unchanged = self.book.slots == self.residue.snapshot.slots;
        match (&self.residue.book_slots, unchanged) {
            (Some(original), true) => {
                let original = original.clone();
                self.line(&original);
            }
            _ => {
                if let Some(el) = slots_el("book:slots", &self.book.slots) {
                    self.line(&el);
                }
            }
        }
    }

    fn counts(&mut self) {
        let b = &self.book.business;
        let commodities = self.book.commodities.keys().filter(|c| !c.is_currency()).count();
        let accounts = 1 + self.book.descendants(self.book.root).len();
        let transactions = self.written_transactions().count();
        let count_raw = |s: Section| self.residue.out_of_scope.iter().filter(|(sec, _)| *sec == s).count();
        let counts = [
            ("commodity", commodities),
            ("account", accounts),
            ("transaction", transactions),
            ("schedxaction", count_raw(Section::ScheduledTransactions)),
            ("budget", count_raw(Section::Budgets)),
            ("price", self.book.prices.len()),
            ("gnc:GncBillTerm", b.bill_terms.len()),
            ("gnc:GncCustomer", b.customers.values().filter(|o| !o.id.is_empty()).count()),
            ("gnc:GncEmployee", b.employees.values().filter(|o| !o.id.is_empty()).count()),
            ("gnc:GncEntry", b.entries.len()),
            ("gnc:GncInvoice", b.invoices.values().filter(|o| !o.id.is_empty()).count()),
            ("gnc:GncJob", b.jobs.values().filter(|o| !o.id.is_empty()).count()),
            ("gnc:GncOrder", b.orders.values().filter(|o| !o.id.is_empty()).count()),
            ("gnc:GncTaxTable", b.tax_tables.len()),
            ("gnc:GncVendor", b.vendors.values().filter(|o| !o.id.is_empty()).count()),
        ];
        counts.iter().for_each(|(kind, n)| count_line(&mut self.out, kind, *n));
    }

    fn commodities(&mut self) {
        let book = self.book;
        for (id, c) in &book.commodities {
            if !commodity_is_written(c) {
                continue;
            }
            let original = self.residue.commodities.get(id).filter(|_| self.residue.snapshot.commodities.get(id).is_some_and(|o| same(c, o)));
            match original {
                Some(el) => {
                    let el = el.clone();
                    self.line(&el);
                }
                None => {
                    if let Some(el) = commodity_el(c) {
                        self.line(&el);
                    }
                }
            }
        }
        self.unknown_after(Section::Commodities);
    }

    /// Prices in GnuCash's stable order: commodity, then currency, then newest first
    /// (`stable_price_traversal`, `5.17:libgnucash/engine/gnc-pricedb.cpp:2708`).
    fn prices(&mut self) {
        if !self.book.prices.is_empty() {
            let mut prices: Vec<&Arc<red_ale_core::Price>> = self.book.prices.values().collect();
            prices.sort_by(|a, b| a.commodity.cmp(&b.commodity).then_with(|| a.currency.cmp(&b.currency)).then_with(|| b.time.cmp(&a.time)));
            self.out.push_str("<gnc:pricedb version=\"1\">\n");
            for p in prices {
                let original = self.residue.prices.get(&p.guid).filter(|_| self.residue.snapshot.prices.get(&p.guid).is_some_and(|o| same(p, o)));
                let el = original.cloned().unwrap_or_else(|| price_el(p));
                self.out.push_str("  ");
                self.out.push_str(&el.dump(1));
                self.out.push('\n');
            }
            self.out.push_str("</gnc:pricedb>\n");
        }
        self.unknown_after(Section::Prices);
    }

    fn accounts(&mut self) {
        let book = self.book;
        for account in book.accounts.values() {
            let original = self.residue.accounts.get(&account.guid).filter(|_| self.account_unchanged(account));
            let el = match original {
                Some(el) => el.clone(),
                None => account_el(account, self.lots_by_account.get(&account.guid).map_or(&[][..], Vec::as_slice)),
            };
            self.line(&el);
        }
        self.unknown_after(Section::Accounts);
    }

    fn transactions(&mut self) {
        let written: Vec<&Arc<Transaction>> = self.written_transactions().collect();
        for t in written {
            let original =
                self.residue.transactions.get(&t.guid).filter(|_| self.residue.snapshot.transactions.get(&t.guid).is_some_and(|o| same(t, o)));
            let el = original.cloned().unwrap_or_else(|| transaction_el(t));
            self.line(&el);
        }
        self.unknown_after(Section::Transactions);
    }

    fn out_of_scope(&mut self) {
        for section in [Section::Templates, Section::ScheduledTransactions, Section::Budgets] {
            let parts: Vec<&Element> = self.residue.out_of_scope.iter().filter(|(s, _)| *s == section).map(|(_, e)| e).collect();
            for el in parts {
                if section == Section::Templates {
                    self.out.push_str("<gnc:template-transactions>\n");
                    el.elements().for_each(|c| {
                        self.out.push_str(&c.dump(0));
                        self.out.push('\n');
                    });
                    self.out.push_str("</gnc:template-transactions>\n");
                } else {
                    self.line(el);
                }
            }
            self.unknown_after(section);
        }
    }

    fn business_one<T: PartialEq>(&mut self, guid: Guid, now: &Arc<T>, before: Option<&Arc<T>>, generate: impl Fn(&T) -> Element) {
        let original = self.residue.business.get(&guid).filter(|_| before.is_some_and(|o| same(now, o)));
        let el = original.cloned().unwrap_or_else(|| generate(now));
        self.line(&el);
    }

    fn business(&mut self) {
        let b = &self.book.business;
        let old = &self.residue.snapshot.business;
        b.bill_terms.iter().for_each(|(g, o)| self.business_one(*g, o, old.bill_terms.get(g), business::bill_term_el));
        b.customers.iter().filter(|(_, o)| !o.id.is_empty()).for_each(|(g, o)| self.business_one(*g, o, old.customers.get(g), business::customer_el));
        b.employees.iter().filter(|(_, o)| !o.id.is_empty()).for_each(|(g, o)| self.business_one(*g, o, old.employees.get(g), business::employee_el));
        b.entries
            .iter()
            .filter(|(_, o)| o.invoice.is_some() || o.bill.is_some() || o.order.is_some())
            .for_each(|(g, o)| self.business_one(*g, o, old.entries.get(g), business::entry_el));
        b.invoices.iter().filter(|(_, o)| !o.id.is_empty()).for_each(|(g, o)| self.business_one(*g, o, old.invoices.get(g), business::invoice_el));
        b.jobs.iter().filter(|(_, o)| !o.id.is_empty()).for_each(|(g, o)| self.business_one(*g, o, old.jobs.get(g), business::job_el));
        b.orders.iter().filter(|(_, o)| !o.id.is_empty()).for_each(|(g, o)| self.business_one(*g, o, old.orders.get(g), business::order_el));
        b.tax_tables.iter().for_each(|(g, o)| self.business_one(*g, o, old.tax_tables.get(g), business::tax_table_el));
        b.vendors.iter().filter(|(_, o)| !o.id.is_empty()).for_each(|(g, o)| self.business_one(*g, o, old.vendors.get(g), business::vendor_el));
        self.unknown_after(Section::Business);
    }
}

/// Serialise a book to XML text.
pub fn write_book_text(book: &Book, residue: &XmlResidue) -> String {
    Writer::new(book, residue).write()
}

/// Serialise a book to the bytes of a file, gzipped if `compress` (zlib's default level, mtime 0 and OS Unix, as
/// GnuCash's `gzopen(…, "wb")` writes).
pub fn write_book(book: &Book, residue: &XmlResidue, compress: bool) -> Result<Vec<u8>, XmlError> {
    let text = write_book_text(book, residue);
    if !compress {
        return Ok(text.into_bytes());
    }
    let mut encoder = flate2::GzBuilder::new().mtime(0).operating_system(3).write(Vec::new(), flate2::Compression::default());
    encoder.write_all(text.as_bytes())?;
    Ok(encoder.finish()?)
}
