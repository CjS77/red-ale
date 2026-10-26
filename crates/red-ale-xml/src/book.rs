//! Reading and writing whole books, and the Residue that keeps what the core model does not represent.
//!
//! See `design/file-fidelity.md`. Every entity is generated as GnuCash 5.17 would write it, except one that is
//! unchanged since it was read and whose generated form differs from what was read: that one is written from its
//! original text, which the Residue keeps for it alone.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::sync::Arc;

use flate2::read::GzDecoder;
use red_ale_core::{Account, Book, CommodityRef, Guid, Lot, Transaction};

use crate::XmlError;
use crate::business;
use crate::dom::{Element, Node, parse_document};
use crate::entities::{account_el, commodity_el, commodity_is_written, price_el, transaction_el};
use crate::envelope::{BOOK_VERSION, FOOTER, count_line, header};
use crate::read::read_book_from;
use crate::values::{Notices, guid_el, slots_el};

/// Where in the book an element sits; used to put unknown elements back where they were.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Section {
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

/// The original text of entities whose generated form differs from what was read, as `Element::dump` writes it (at
/// level 1 for prices, 0 for the rest). An entity GnuCash 5.17's generator writes back unchanged has no entry.
#[derive(Debug, Clone, Default)]
pub(crate) struct Originals {
    pub(crate) book_slots: Option<String>,
    pub(crate) commodities: HashMap<CommodityRef, String>,
    pub(crate) prices: HashMap<Guid, String>,
    pub(crate) accounts: HashMap<Guid, String>,
    pub(crate) transactions: HashMap<Guid, String>,
    pub(crate) business: HashMap<Guid, String>,
}

impl Originals {
    fn len(&self) -> usize {
        usize::from(self.book_slots.is_some())
            + self.commodities.len()
            + self.prices.len()
            + self.accounts.len()
            + self.transactions.len()
            + self.business.len()
    }
}

/// Everything in a book file that the core `Book` does not hold, plus what the writer needs to reproduce the file.
#[derive(Debug, Clone)]
pub struct XmlResidue {
    /// The file was gzip-compressed.
    pub compressed: bool,
    pub(crate) book_version: String,
    pub(crate) snapshot: Book,
    pub(crate) originals: Originals,
    pub(crate) out_of_scope: Vec<(Section, Element)>,
    pub(crate) unknown: Vec<(Section, Element)>,
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
            originals: Originals::default(),
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

    /// Names of out-of-scope sections holding a reference to `commodity` (a `cmdty:space` and `cmdty:id` pair), for
    /// refusing renames and deletions that would leave them dangling.
    pub fn sections_referring_to(&self, commodity: &CommodityRef) -> Vec<String> {
        self.out_of_scope_elements().filter(|e| refers_to(e, commodity)).map(|e| e.name.clone()).collect()
    }

    /// Forget the originals: the next write generates every entity from the model. Used to test the generators.
    pub fn forget_originals(&mut self) {
        self.originals = Originals::default();
    }

    /// How many entities (the book's own slots counting as one) are written from the text they were read as, because
    /// GnuCash 5.17's generator would write them differently. None, for a book GnuCash 5.17 wrote.
    pub fn originals_kept(&self) -> usize {
        self.originals.len()
    }
}

fn refers_to(el: &Element, commodity: &CommodityRef) -> bool {
    let text = |name: &str| el.find(name).map(|e| e.text_content().trim().to_owned());
    let here = text("cmdty:id").is_some_and(|id| id == commodity.mnemonic)
        && text("cmdty:space").is_some_and(|ns| CommodityRef::new(ns, "").namespace == commodity.namespace);
    here || el.elements().any(|child| refers_to(child, commodity))
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

/// Read a book file's bytes, plain or gzip-compressed. See [`read_book_from`] to read from a stream.
pub fn read_book(bytes: &[u8]) -> Result<(Book, XmlResidue), XmlError> {
    read_book_from(bytes)
}

/// Each account's lots, in the order the Book holds them.
pub(crate) fn lots_by_account(book: &Book) -> HashMap<Guid, Vec<&Lot>> {
    let mut lots: HashMap<Guid, Vec<&Lot>> = HashMap::new();
    book.lots.values().for_each(|l| lots.entry(l.account).or_default().push(l));
    lots
}

fn same<T: PartialEq>(a: &Arc<T>, b: &Arc<T>) -> bool {
    Arc::ptr_eq(a, b) || **a == **b
}

/// How much text the writer gathers before handing it to its sink.
const CHUNK: usize = 64 * 1024;

/// Writes one book; holds the lookups the writers share. Text gathers in `out` and goes to `sink` a chunk at a time,
/// so the whole file is never held.
struct Writer<'a, W: Write> {
    book: &'a Book,
    residue: &'a XmlResidue,
    lots_by_account: HashMap<Guid, Vec<&'a Lot>>,
    out: String,
    sink: W,
    /// Hand `out` to the sink once it holds this much; `usize::MAX` keeps all the text in `out`.
    flush_at: usize,
    error: Option<std::io::Error>,
}

impl<'a, W: Write> Writer<'a, W> {
    fn new(book: &'a Book, residue: &'a XmlResidue, sink: W, flush_at: usize) -> Self {
        Self { book, residue, lots_by_account: lots_by_account(book), out: String::new(), sink, flush_at, error: None }
    }

    fn line(&mut self, el: &Element) {
        self.text_line(&el.dump(0));
    }

    fn text_line(&mut self, text: &str) {
        self.out.push_str(text);
        self.out.push('\n');
        if self.out.len() >= self.flush_at {
            self.flush();
        }
    }

    /// Hand the text gathered so far to the sink. The first error is kept and reported when the book is written.
    fn flush(&mut self) {
        if self.error.is_none() {
            self.error = self.sink.write_all(self.out.as_bytes()).err();
        }
        self.out.clear();
    }

    /// Write an entity from its original text if it has one, else as generated.
    fn entity(&mut self, original: Option<&String>, generate: impl FnOnce() -> Element) {
        match original {
            Some(text) => self.text_line(text),
            None => self.line(&generate()),
        }
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

    fn written_transactions(&self) -> impl Iterator<Item = &'a Arc<Transaction>> + use<'a, W> {
        let accounts = &self.book.accounts;
        self.book.transactions.values().filter(move |t| t.splits.iter().any(|s| s.account.is_some_and(|a| accounts.contains_key(&a))))
    }

    /// Write the book. Returns the text not yet handed to the sink (all of it, if `flush_at` is `usize::MAX`) and
    /// the sink.
    fn write(mut self) -> Result<(String, W), std::io::Error> {
        self.out.push_str(&header());
        count_line(&mut self.out, "book", 1);
        self.out.push_str("<gnc:book version=\"");
        self.out.push_str(&self.residue.book_version);
        self.out.push_str("\">\n");
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
        self.finish()
    }

    /// Write what File ▸ Export ▸ Export Accounts writes (`gnc_book_write_accounts_to_xml_filehandle_v2`,
    /// `5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1340-1374`): the header of a full save, the commodity and account
    /// counts, the commodities as a full save writes them, and the account tree, root first, each account as a full
    /// save writes it but without its lots (`5.17:libgnucash/backend/xml/gnc-account-xml-v2.cpp:146-158`). No
    /// `gnc:book` element, so no book GUID, book slots, prices, transactions or anything else.
    fn write_accounts_export(mut self) -> Result<(String, W), std::io::Error> {
        let accounts = exported_accounts(self.book);
        self.out.push_str(&header());
        count_line(&mut self.out, "commodity", self.book.commodities.keys().filter(|c| !c.is_currency()).count());
        count_line(&mut self.out, "account", accounts.len());
        self.commodity_elements();
        accounts.into_iter().for_each(|a| self.account_without_lots(a));
        self.out.push_str(FOOTER);
        self.finish()
    }

    /// An account as a full save writes it, less its `act:lots`: from its original text, if it is unchanged and has
    /// one, else as generated.
    fn account_without_lots(&mut self, account: &Arc<Account>) {
        let unchanged = self.residue.snapshot.accounts.get(&account.guid).is_some_and(|old| same(account, old));
        let original = self.residue.originals.accounts.get(&account.guid).filter(|_| unchanged).and_then(|text| parse_document(text).ok());
        let el = original.map_or_else(
            || account_el(account, &[]),
            |mut el| {
                el.children.retain(|c| !matches!(c, Node::Element(e) if e.name == "act:lots"));
                el
            },
        );
        self.line(&el);
    }

    /// Hand the rest of the text to the sink, unless all of it is kept, and report the first write error.
    fn finish(mut self) -> Result<(String, W), std::io::Error> {
        if self.flush_at != usize::MAX {
            self.flush();
        }
        match self.error {
            Some(e) => Err(e),
            None => Ok((self.out, self.sink)),
        }
    }

    fn book_slots(&mut self) {
        let unchanged = self.book.slots == self.residue.snapshot.slots;
        match (&self.residue.originals.book_slots, unchanged) {
            (Some(original), true) => self.text_line(original),
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
        self.commodity_elements();
        self.unknown_after(Section::Commodities);
    }

    fn commodity_elements(&mut self) {
        let book = self.book;
        for (id, c) in &book.commodities {
            if !commodity_is_written(c) {
                continue;
            }
            let original =
                self.residue.originals.commodities.get(id).filter(|_| self.residue.snapshot.commodities.get(id).is_some_and(|o| same(c, o)));
            match original {
                Some(text) => self.text_line(text),
                None => {
                    if let Some(el) = commodity_el(c) {
                        self.line(&el);
                    }
                }
            }
        }
    }

    /// Prices in GnuCash's stable order: commodity, then currency, then newest first, then GUID ascending
    /// (`stable_price_traversal`, `5.17:libgnucash/engine/gnc-pricedb.cpp:2708`; `compare_prices_by_date`, `:674-692`).
    fn prices(&mut self) {
        if !self.book.prices.is_empty() {
            let mut prices: Vec<&Arc<red_ale_core::Price>> = self.book.prices.values().collect();
            prices.sort_by(|a, b| a.commodity.cmp(&b.commodity).then_with(|| a.currency.cmp(&b.currency)).then_with(|| b.time.cmp(&a.time)).then_with(|| a.guid.cmp(&b.guid)));
            self.out.push_str("<gnc:pricedb version=\"1\">\n");
            for p in prices {
                let original =
                    self.residue.originals.prices.get(&p.guid).filter(|_| self.residue.snapshot.prices.get(&p.guid).is_some_and(|o| same(p, o)));
                let text = original.cloned().unwrap_or_else(|| price_el(p).dump(1));
                self.out.push_str("  ");
                self.text_line(&text);
            }
            self.out.push_str("</gnc:pricedb>\n");
        }
        self.unknown_after(Section::Prices);
    }

    fn accounts(&mut self) {
        let book = self.book;
        for account in book.accounts.values() {
            let original = self.residue.originals.accounts.get(&account.guid).filter(|_| self.account_unchanged(account));
            let lots = self.lots_by_account.get(&account.guid).cloned().unwrap_or_default();
            self.entity(original, || account_el(account, &lots));
        }
        self.unknown_after(Section::Accounts);
    }

    fn transactions(&mut self) {
        let written: Vec<&Arc<Transaction>> = self.written_transactions().collect();
        for t in written {
            let original = self
                .residue
                .originals
                .transactions
                .get(&t.guid)
                .filter(|_| self.residue.snapshot.transactions.get(&t.guid).is_some_and(|o| same(t, o)));
            self.entity(original, || transaction_el(t));
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
        let original = self.residue.originals.business.get(&guid).filter(|_| before.is_some_and(|o| same(now, o)));
        self.entity(original, || generate(now));
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

/// The accounts an account-tree export writes: the root and its descendants, in the order a full save writes them.
fn exported_accounts(book: &Book) -> Vec<&Arc<Account>> {
    let tree: HashSet<Guid> = std::iter::once(book.root).chain(book.descendants(book.root).iter().map(|a| a.guid)).collect();
    book.accounts.values().filter(|a| tree.contains(&a.guid)).collect()
}

/// Write the account tree and commodities of `book` as GnuCash's Export Accounts does, uncompressed as GnuCash writes
/// it, into `sink` a chunk at a time (see `Writer::write_accounts_export`). Returns the sink.
pub fn write_accounts_to<W: Write>(book: &Book, residue: &XmlResidue, sink: W) -> Result<W, XmlError> {
    Ok(Writer::new(book, residue, sink, CHUNK).write_accounts_export()?.1)
}

/// Serialise a book to XML text.
pub fn write_book_text(book: &Book, residue: &XmlResidue) -> String {
    // The text stays in the writer, so the sink is never written to and cannot fail.
    Writer::new(book, residue, std::io::sink(), usize::MAX).write().map(|(text, _)| text).unwrap_or_default()
}

/// Serialise a book to the bytes of a file, gzipped if `compress` (zlib's default level, mtime 0 and OS Unix, as
/// GnuCash's `gzopen(…, "wb")` writes).
pub fn write_book(book: &Book, residue: &XmlResidue, compress: bool) -> Result<Vec<u8>, XmlError> {
    write_book_to(book, residue, compress, Vec::new())
}

/// [`write_book`] into `sink` a chunk at a time, without holding the file's text or bytes. Returns the sink, with
/// every byte handed to it.
pub fn write_book_to<W: Write>(book: &Book, residue: &XmlResidue, compress: bool, sink: W) -> Result<W, XmlError> {
    if !compress {
        return Ok(Writer::new(book, residue, sink, CHUNK).write()?.1);
    }
    let encoder = flate2::GzBuilder::new().mtime(0).operating_system(3).write(sink, flate2::Compression::default());
    let (_, encoder) = Writer::new(book, residue, encoder, CHUNK).write()?;
    Ok(encoder.finish()?)
}
