//! Reading a book file as a stream, as GnuCash's SAX parser does (`5.17:libgnucash/backend/xml/sixtp.cpp`).
//!
//! Each part of the book (a commodity, a price, an account, a transaction, the template transactions, a scheduled
//! transaction, a budget, a business object) is built as a small tree from the tokeniser's events, translated, and
//! dropped, so no tree of the whole file and no copy of its text is ever held. The same walk either collects the entities into a new Book
//! ([`read_book_from`]) or compares each one with a Book already held ([`read_back`]), which is how a save checks
//! what it wrote.
//!
//! Errors keep the precedence they had when the whole file was decoded and parsed before any of it was translated:
//! a file that is not valid UTF-8 or whose gzip stream is broken says so first; then a file that is not a version 2
//! book; then malformed XML; then a document without exactly one book; then the first entity that cannot be read.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::io::BufRead;
use std::sync::Arc;

use red_ale_core::business::Business;
use red_ale_core::{
    Account, AccountType, Book, Budget, Commodity, CommodityRef, Frame, Guid, Lot, Price, ScheduledTransaction, Templates, Transaction,
};

use crate::XmlError;
use crate::book::{Originals, Section, XmlResidue};
use crate::budgets::{budget_el, parse_budget};
use crate::business::{self, BusinessObject};
use crate::dom::{Element, Token, Tokens, TreeBuilder};
use crate::entities::{
    account_el, commodity_el, commodity_is_written, parse_account, parse_commodity, parse_price, parse_transaction, price_el,
    transaction_el,
};
use crate::envelope::BOOK_VERSION;
use crate::input::TextStream;
use crate::scheduled::{give_template_commodity, parse_scheduled, parse_template_account, scheduled_el};
use crate::values::{Notices, parse_guid, parse_slots, slots_el};

/// How much of the start of the text is checked for the XML declaration and the `gnc-v2` root. GnuCash looks at the
/// first 255 bytes only (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1691-1705`, `sixtp.cpp:903-927`).
const SNIFF: usize = 64 * 1024;

/// Read a book file from a stream of its bytes, plain or gzip-compressed.
pub fn read_book_from<R: BufRead>(reader: R) -> Result<(Book, XmlResidue), XmlError> {
    let (Reader { sink, kept }, compressed) = walk(reader, Collect::default())?;
    let book = sink.into_book()?;
    let residue = kept.into_residue(&book, compressed);
    Ok((book, residue))
}

/// What reading a written file back found.
#[derive(Debug)]
pub enum ReadBack {
    /// The file reads as the expected Book. The Residue is the one reading it gives, with the expected Book as its
    /// snapshot.
    Same(Box<XmlResidue>),
    /// The file reads as a different Book: the first difference found.
    Differs(String),
}

/// Read a book file back and compare each entity, as it is read, with `expected`, keeping no second Book.
///
/// This checks what a full read would: every entity GnuCash's file holds reads back equal to the one in `expected`,
/// and every entity of `expected` is in the file, except currencies GnuCash does not write (unused, no quotes),
/// which are not data.
pub fn read_back<R: BufRead>(reader: R, expected: &Book) -> Result<ReadBack, XmlError> {
    let (Reader { sink, kept }, compressed) = walk(reader, Verify::new(expected))?;
    Ok(match sink.finish()? {
        Some(difference) => ReadBack::Differs(difference),
        None => ReadBack::Same(Box::new(kept.into_residue(expected, compressed))),
    })
}

/// Check the start of a decoded file the way GnuCash's `gnc_is_xml_data_file_v2` does.
fn check_v2(text: &str) -> Result<(), XmlError> {
    let trimmed = text.trim_start();
    let rest = trimmed
        .strip_prefix("<?xml")
        .ok_or_else(|| XmlError::NotABook("no XML declaration".into()))?;
    let after_decl = rest
        .split_once('>')
        .map(|(_, r)| r.trim_start())
        .ok_or_else(|| XmlError::NotABook("unterminated declaration".into()))?;
    if after_decl.starts_with("<gnc-v2") && after_decl[7..].starts_with(|c: char| c.is_whitespace()) {
        Ok(())
    } else if after_decl.starts_with("<gnc>") {
        Err(XmlError::NotABook(
            "this is a GnuCash version 1 file; open and save it with GnuCash first".into(),
        ))
    } else if after_decl.starts_with("<gnc-v") {
        Err(XmlError::NotABook("this file was written by a newer GnuCash format version".into()))
    } else {
        Err(XmlError::NotABook("the root element is not gnc-v2".into()))
    }
}

/// Check the start of the text, then read every token, handing each part of the book to `sink`.
fn walk<R: BufRead, S: Sink>(reader: R, sink: S) -> Result<(Reader<S>, bool), XmlError> {
    let mut text = TextStream::new(reader)?;
    let compressed = text.compressed();
    let checked = match text.peek(SNIFF) {
        Ok(start) => std::str::from_utf8(start).map_err(|_| XmlError::Encoding).and_then(check_v2),
        Err(e) => Err(text.take_error().unwrap_or_else(|| crate::input::from_io(e))),
    };
    if let Err(e) = checked {
        return Err(text.drain().unwrap_or(e));
    }
    let mut tokens = Tokens::new(text);
    let mut walker = Walker::new(sink);
    loop {
        let step = tokens
            .next()
            .and_then(|token| token.map(|t| walker.token(t, tokens.position())).transpose());
        match step {
            Ok(Some(())) => {}
            Ok(None) => break,
            Err(e @ (XmlError::Encoding | XmlError::Io(_))) => return Err(e),
            Err(e) => return Err(tokens.text().drain().unwrap_or(e)),
        }
    }
    walker.end(tokens.position()).map(|reader| (reader, compressed))
}

/// Where the walk is in the document's outer structure: the elements open around the next token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    /// Inside the `gnc-v2` root.
    Root,
    /// Inside `gnc:book`.
    Book,
    /// Inside `gnc:pricedb`, whose children are prices.
    PriceDb,
    /// Inside an element whose content is not read.
    Skip,
}

/// Follows the tokens through the document, building one part of the book at a time.
struct Walker<S> {
    reader: Reader<S>,
    levels: Vec<Level>,
    builder: TreeBuilder,
    roots: usize,
    books: usize,
    outside: Vec<String>,
    failed: Option<XmlError>,
}

impl<S: Sink> Walker<S> {
    fn new(sink: S) -> Self {
        let reader = Reader {
            sink,
            kept: Kept::default(),
        };
        Self {
            reader,
            levels: Vec::new(),
            builder: TreeBuilder::default(),
            roots: 0,
            books: 0,
            outside: Vec::new(),
            failed: None,
        }
    }

    /// Parts are translated until one fails or a second book appears; after that the walk only looks for errors that
    /// take precedence.
    fn reading(&self) -> bool {
        self.failed.is_none() && self.books <= 1
    }

    fn token(&mut self, token: Token, offset: u64) -> Result<(), XmlError> {
        if !self.builder.is_empty() {
            return self.build(token, offset);
        }
        match token {
            Token::Start(el) => self.element(el, false, offset),
            Token::Empty(el) => self.element(el, true, offset),
            Token::End => self.levels.pop().map(drop).ok_or_else(|| crate::dom::Unbalanced::End.at(offset)),
            Token::Text(_) => Ok(()),
        }
    }

    fn build(&mut self, token: Token, offset: u64) -> Result<(), XmlError> {
        if let Some(el) = self.builder.feed(token).map_err(|e| e.at(offset))? {
            self.complete(&el);
        }
        Ok(())
    }

    /// A start or empty-element tag outside any part being built.
    fn element(&mut self, el: Element, empty: bool, offset: u64) -> Result<(), XmlError> {
        let level = match self.levels.last().copied() {
            None => self.root(offset)?,
            Some(Level::Root) => self.root_child(&el),
            Some(Level::Book) if el.name == "gnc:pricedb" => {
                if self.reading() {
                    self.reader.kept.enter(Section::Prices);
                }
                Level::PriceDb
            }
            Some(Level::Book | Level::PriceDb) if self.reading() => {
                return self.build(if empty { Token::Empty(el) } else { Token::Start(el) }, offset);
            }
            Some(_) => Level::Skip,
        };
        if !empty {
            self.levels.push(level);
        }
        Ok(())
    }

    /// The root element. libxml2 refuses a second one ("Extra content at the end of the document").
    fn root(&mut self, offset: u64) -> Result<Level, XmlError> {
        self.roots += 1;
        if self.roots > 1 {
            return Err(XmlError::Syntax {
                offset,
                message: "content after the root element".into(),
            });
        }
        Ok(Level::Root)
    }

    fn root_child(&mut self, el: &Element) -> Level {
        match el.name.as_str() {
            "gnc:book" => {
                self.books += 1;
                el.get_attr("version")
                    .unwrap_or(BOOK_VERSION)
                    .clone_into(&mut self.reader.kept.book_version);
                if self.books == 1 { Level::Book } else { Level::Skip }
            }
            "gnc:count-data" => Level::Skip,
            other => {
                self.outside.push(format!("unknown element {other} outside the book"));
                Level::Skip
            }
        }
    }

    /// A part of the book, or a price, built whole.
    fn complete(&mut self, el: &Element) {
        let result = match self.levels.last() {
            Some(Level::PriceDb) => self.reader.price(el),
            _ => self.reader.part(el),
        };
        if let Err(e) = result {
            self.failed.get_or_insert(e);
        }
    }

    fn end(mut self, offset: u64) -> Result<Reader<S>, XmlError> {
        if !self.levels.is_empty() || !self.builder.is_empty() {
            return Err(crate::dom::Unbalanced::Open.at(offset));
        }
        if self.roots == 0 {
            return Err(XmlError::Empty);
        }
        if self.books != 1 {
            return Err(XmlError::NotABook(format!("expected one gnc:book, found {}", self.books)));
        }
        if let Some(e) = self.failed {
            return Err(e);
        }
        self.reader.kept.notices.unrecognised.append(&mut self.outside);
        Ok(self.reader)
    }
}

/// What the reader does with each entity it translates: keep it in a new Book, or check it against one.
trait Sink {
    fn book_id(&mut self, guid: Guid);
    fn book_slots(&mut self, slots: Frame);
    fn commodity(&mut self, commodity: Commodity);
    fn price(&mut self, price: Price);
    fn account(&mut self, account: Account, lots: Vec<Lot>);
    fn transaction(&mut self, transaction: Transaction);
    fn business(&mut self, object: BusinessObject);
    fn budget(&mut self, budget: Budget);
    fn template_account(&mut self, account: Account);
    fn template_transaction(&mut self, transaction: Transaction);
    fn scheduled(&mut self, scheduled: ScheduledTransaction);
}

/// Translates each part of the book, keeping in the Residue what the Book does not hold.
struct Reader<S> {
    sink: S,
    kept: Kept,
}

/// What reading keeps for the Residue.
#[derive(Default)]
struct Kept {
    book_version: String,
    section: Option<Section>,
    originals: Originals,
    /// Account elements, compared with their generated form once every lot is read (see [`account_originals`]).
    account_els: HashMap<Guid, Element>,
    /// The template root read so far, under which a parentless template account goes.
    template_root: Option<Guid>,
    unknown: Vec<(Section, Element)>,
    notices: Notices,
}

impl Kept {
    fn enter(&mut self, s: Section) {
        self.section = Some(s);
    }

    fn into_residue(self, book: &Book, compressed: bool) -> XmlResidue {
        let mut originals = self.originals;
        originals.accounts = account_originals(book, &self.account_els);
        XmlResidue {
            compressed,
            book_version: self.book_version,
            snapshot: book.clone(),
            originals,
            unknown: self.unknown,
            notices: self.notices,
        }
    }
}

/// The text `read` was read as, at indentation `level`, if `generated` (what red-ale writes for the entity read from
/// it) would not give the same bytes.
fn original_if_needed(read: &Element, generated: Option<&Element>, level: usize) -> Option<String> {
    (generated != Some(read)).then(|| read.dump(level))
}

/// Record an entity's original text, or forget an earlier one for the same key: as in the Book, the last element read
/// with a key wins.
fn keep<K: Hash + Eq>(originals: &mut HashMap<K, String>, key: K, original: Option<String>) {
    match original {
        Some(text) => originals.insert(key, text),
        None => originals.remove(&key),
    };
}

impl<S: Sink> Reader<S> {
    fn part(&mut self, el: &Element) -> Result<(), XmlError> {
        let kept = &mut self.kept;
        match el.name.as_str() {
            "book:id" => self.sink.book_id(parse_guid(el)?),
            "book:slots" => {
                let slots = parse_slots(el, &mut kept.notices)?;
                kept.originals.book_slots = original_if_needed(el, slots_el("book:slots", &slots).as_ref(), 0);
                self.sink.book_slots(slots);
            }
            "gnc:count-data" => {}
            "gnc:commodity" => {
                kept.enter(Section::Commodities);
                let c = parse_commodity(el, &mut kept.notices)?;
                keep(
                    &mut kept.originals.commodities,
                    c.id.clone(),
                    original_if_needed(el, commodity_el(&c).as_ref(), 0),
                );
                self.sink.commodity(c);
            }
            "gnc:account" => {
                kept.enter(Section::Accounts);
                let (account, lots) = parse_account(el, &mut kept.notices)?;
                kept.account_els.insert(account.guid, el.clone());
                self.sink.account(account, lots);
            }
            "gnc:transaction" => {
                kept.enter(Section::Transactions);
                let t = parse_transaction(el, &mut kept.notices)?;
                keep(
                    &mut kept.originals.transactions,
                    t.guid,
                    original_if_needed(el, Some(&transaction_el(&t)), 0),
                );
                self.sink.transaction(t);
            }
            "gnc:template-transactions" => {
                kept.enter(Section::Templates);
                el.elements().try_for_each(|child| self.template_part(child))?;
            }
            "gnc:schedxaction" => {
                kept.enter(Section::ScheduledTransactions);
                let sx = parse_scheduled(el, &mut kept.notices)?;
                keep(
                    &mut kept.originals.scheduled,
                    sx.guid,
                    original_if_needed(el, Some(&scheduled_el(&sx)), 0),
                );
                self.sink.scheduled(sx);
            }
            "gnc:budget" => {
                kept.enter(Section::Budgets);
                let b = parse_budget(el, &mut kept.notices)?;
                keep(&mut kept.originals.budgets, b.guid, original_if_needed(el, Some(&budget_el(&b)), 0));
                self.sink.budget(b);
            }
            name if business::ELEMENT_NAMES.contains(&name) => {
                kept.enter(Section::Business);
                let obj = business::parse(el, &mut kept.notices)?;
                keep(
                    &mut kept.originals.business,
                    obj.guid(),
                    original_if_needed(el, Some(&obj.to_element()), 0),
                );
                self.sink.business(obj);
            }
            other => {
                kept.notices.unrecognised.push(format!("unknown element {other} in the book"));
                kept.unknown.push((kept.section.unwrap_or(Section::Head), el.clone()));
            }
        }
        Ok(())
    }

    /// One price of the price database, written at indentation level 1.
    fn price(&mut self, el: &Element) -> Result<(), XmlError> {
        let price = parse_price(el, &mut self.kept.notices)?;
        keep(
            &mut self.kept.originals.prices,
            price.guid,
            original_if_needed(el, Some(&price_el(&price)), 1),
        );
        self.sink.price(price);
        Ok(())
    }
}

impl<S: Sink> Reader<S> {
    /// One child of `gnc:template-transactions`, which holds only accounts and transactions
    /// (`5.17:libgnucash/backend/xml/gnc-schedxaction-xml-v2.cpp:847-852`).
    fn template_part(&mut self, el: &Element) -> Result<(), XmlError> {
        let kept = &mut self.kept;
        match el.name.as_str() {
            "gnc:account" => {
                let account = parse_template_account(el, &mut kept.template_root, &mut kept.notices)?;
                let original = original_if_needed(el, Some(&account_el(&account, &[])), 0);
                keep(&mut kept.originals.template_accounts, account.guid, original);
                self.sink.template_account(account);
            }
            "gnc:transaction" => {
                let t = parse_transaction(el, &mut kept.notices)?;
                let original = original_if_needed(el, Some(&transaction_el(&t)), 0);
                keep(&mut kept.originals.template_transactions, t.guid, original);
                self.sink.template_transaction(t);
            }
            other => {
                kept.notices
                    .unrecognised
                    .push(format!("unknown element {other} in gnc:template-transactions"));
                kept.unknown.push((Section::Templates, el.clone()));
            }
        }
        Ok(())
    }
}

/// Original text for the accounts whose generated form differs. An account is written with its lots, so it is
/// generated with the lots the Book gives it, as the writer generates it.
fn account_originals(book: &Book, elements: &HashMap<Guid, Element>) -> HashMap<Guid, String> {
    let lots = crate::book::lots_by_account(book);
    elements
        .iter()
        .filter_map(|(guid, el)| {
            let generated = book
                .accounts
                .get(guid)
                .map(|a| account_el(a, lots.get(guid).map_or(&[][..], Vec::as_slice)));
            original_if_needed(el, generated.as_ref(), 0).map(|text| (*guid, text))
        })
        .collect()
}

/// The book's one root account, from the GUIDs of the accounts of type ROOT with no parent.
fn single_root(roots: &[Guid]) -> Result<Guid, XmlError> {
    match roots {
        [root] => Ok(*root),
        [] => Err(XmlError::NotABook("the book has no root account".into())),
        _ => Err(XmlError::NotABook("the book has more than one root account".into())),
    }
}

fn no_book_id() -> XmlError {
    XmlError::NotABook("the book has no book:id".into())
}

fn is_root(account: &Account) -> bool {
    account.account_type == AccountType::Root && account.parent.is_none()
}

/// Collects the entities into a new Book.
#[derive(Default)]
struct Collect {
    guid: Option<Guid>,
    slots: Frame,
    commodities: BTreeMap<CommodityRef, Arc<Commodity>>,
    accounts: indexmap::IndexMap<Guid, Arc<Account>>,
    transactions: indexmap::IndexMap<Guid, Arc<Transaction>>,
    prices: indexmap::IndexMap<Guid, Arc<Price>>,
    lots: indexmap::IndexMap<Guid, Arc<Lot>>,
    business: Business,
    budgets: indexmap::IndexMap<Guid, Arc<Budget>>,
    templates: Templates,
    scheduled: indexmap::IndexMap<Guid, Arc<ScheduledTransaction>>,
}

impl Collect {
    fn into_book(self) -> Result<Book, XmlError> {
        let guid = self.guid.ok_or_else(no_book_id)?;
        let roots: Vec<Guid> = self.accounts.values().filter(|a| is_root(a)).map(|a| a.guid).collect();
        let root = single_root(&roots)?;
        Ok(Book {
            guid,
            slots: self.slots,
            commodities: self.commodities,
            root,
            accounts: self.accounts,
            transactions: self.transactions,
            prices: self.prices,
            lots: self.lots,
            business: self.business,
            budgets: self.budgets,
            templates: self.templates,
            scheduled: self.scheduled,
        })
    }
}

impl Sink for Collect {
    fn book_id(&mut self, guid: Guid) {
        self.guid = Some(guid);
    }

    fn book_slots(&mut self, slots: Frame) {
        self.slots = slots;
    }

    fn commodity(&mut self, commodity: Commodity) {
        self.commodities.insert(commodity.id.clone(), Arc::new(commodity));
    }

    fn price(&mut self, price: Price) {
        self.prices.insert(price.guid, Arc::new(price));
    }

    fn account(&mut self, account: Account, lots: Vec<Lot>) {
        lots.into_iter().for_each(|l| {
            self.lots.insert(l.guid, Arc::new(l));
        });
        self.accounts.insert(account.guid, Arc::new(account));
    }

    fn transaction(&mut self, transaction: Transaction) {
        self.transactions.insert(transaction.guid, Arc::new(transaction));
    }

    fn business(&mut self, object: BusinessObject) {
        insert_business(&mut self.business, object);
    }

    fn budget(&mut self, budget: Budget) {
        self.budgets.insert(budget.guid, Arc::new(budget));
    }

    fn template_account(&mut self, account: Account) {
        if is_root(&account) {
            self.templates.root = Some(account.guid);
        }
        self.templates.accounts.insert(account.guid, Arc::new(account));
    }

    fn template_transaction(&mut self, transaction: Transaction) {
        self.templates.transactions.insert(transaction.guid, Arc::new(transaction));
    }

    fn scheduled(&mut self, scheduled: ScheduledTransaction) {
        self.scheduled.insert(scheduled.guid, Arc::new(scheduled));
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

/// Compares each entity read with the one `expected` holds, and remembers which it has seen, so that at the end it
/// can tell whether any was missing.
struct Verify<'a> {
    expected: &'a Book,
    guid: Option<Guid>,
    roots: Vec<Guid>,
    template_root: Option<Guid>,
    seen: Seen,
    difference: Option<String>,
}

#[derive(Default)]
struct Seen {
    commodities: HashSet<CommodityRef>,
    accounts: HashSet<Guid>,
    transactions: HashSet<Guid>,
    prices: HashSet<Guid>,
    lots: HashSet<Guid>,
    business: HashSet<Guid>,
    budgets: HashSet<Guid>,
    template_accounts: HashSet<Guid>,
    template_transactions: HashSet<Guid>,
    scheduled: HashSet<Guid>,
}

/// True if `found` is the entity `expected` holds under its key, read for the first time.
fn matches<K: Hash + Eq + Copy, T: PartialEq>(expected: Option<&Arc<T>>, found: &T, seen: &mut HashSet<K>, key: K) -> bool {
    expected.is_some_and(|e| **e == *found) && seen.insert(key)
}

impl<'a> Verify<'a> {
    fn new(expected: &'a Book) -> Self {
        Self {
            expected,
            guid: None,
            roots: Vec::new(),
            template_root: None,
            seen: Seen::default(),
            difference: None,
        }
    }

    /// Record a difference unless `same`; the first one found is the one reported.
    fn check(&mut self, same: bool, difference: impl FnOnce() -> String) {
        if !same && self.difference.is_none() {
            self.difference = Some(difference());
        }
    }

    /// The first difference, once every entity has been read: one already found, or an entity the file lacks.
    fn finish(mut self) -> Result<Option<String>, XmlError> {
        self.guid.ok_or_else(no_book_id)?;
        let root = single_root(&self.roots)?;
        let expected = self.expected;
        self.check(root == expected.root, || "book identity differ after writing".into());
        let template_root = self.template_root;
        let templates_written = expected.templates.has_descendants();
        let written_root = expected.templates.root.filter(|_| templates_written);
        self.check(template_root == written_root, || "template root differs after writing".into());
        let commodity = expected
            .commodities
            .iter()
            .find(|(k, c)| commodity_is_written(c) && !self.seen.commodities.contains(*k));
        let missing = commodity
            .map(|(k, _)| format!("commodity {k} differs after writing"))
            .or_else(|| first_missing("account", expected.accounts.keys(), &self.seen.accounts))
            .or_else(|| first_missing("transaction", expected.transactions.keys(), &self.seen.transactions))
            .or_else(|| first_missing("price", expected.prices.keys(), &self.seen.prices))
            .or_else(|| first_missing("lot", expected.lots.keys(), &self.seen.lots))
            .or_else(|| first_missing("budget", expected.budgets.keys(), &self.seen.budgets))
            .or_else(|| {
                let accounts = expected.templates.accounts.keys().filter(|_| templates_written);
                first_missing("template account", accounts, &self.seen.template_accounts)
            })
            .or_else(|| {
                first_missing(
                    "template transaction",
                    expected.templates.transactions.keys(),
                    &self.seen.template_transactions,
                )
            })
            .or_else(|| first_missing("scheduled transaction", expected.scheduled.keys(), &self.seen.scheduled))
            .or_else(|| {
                (business_count(&expected.business) != self.seen.business.len()).then(|| "business objects differ after writing".into())
            });
        Ok(self.difference.or(missing))
    }
}

fn first_missing<'g>(kind: &str, mut expected: impl Iterator<Item = &'g Guid>, seen: &HashSet<Guid>) -> Option<String> {
    expected
        .find(|g| !seen.contains(g))
        .map(|g| format!("{kind} {g} differs after writing"))
}

fn business_count(b: &Business) -> usize {
    b.bill_terms.len()
        + b.customers.len()
        + b.employees.len()
        + b.entries.len()
        + b.invoices.len()
        + b.jobs.len()
        + b.orders.len()
        + b.tax_tables.len()
        + b.vendors.len()
}

/// True if `obj` is the business object `b` holds under its GUID.
fn business_matches(b: &Business, obj: &BusinessObject) -> bool {
    fn same<T: PartialEq>(held: Option<&Arc<T>>, found: &T) -> bool {
        held.is_some_and(|h| **h == *found)
    }
    match obj {
        BusinessObject::BillTerm(o) => same(b.bill_terms.get(&o.guid), o),
        BusinessObject::Customer(o) => same(b.customers.get(&o.guid), o),
        BusinessObject::Employee(o) => same(b.employees.get(&o.guid), o),
        BusinessObject::Entry(o) => same(b.entries.get(&o.guid), o),
        BusinessObject::Invoice(o) => same(b.invoices.get(&o.guid), o),
        BusinessObject::Job(o) => same(b.jobs.get(&o.guid), o),
        BusinessObject::Order(o) => same(b.orders.get(&o.guid), o),
        BusinessObject::TaxTable(o) => same(b.tax_tables.get(&o.guid), o),
        BusinessObject::Vendor(o) => same(b.vendors.get(&o.guid), o),
    }
}

impl Sink for Verify<'_> {
    fn book_id(&mut self, guid: Guid) {
        self.guid = Some(guid);
        self.check(guid == self.expected.guid, || "book identity differ after writing".into());
    }

    fn book_slots(&mut self, slots: Frame) {
        self.check(slots == self.expected.slots, || "book slots differ after writing".into());
    }

    /// Currencies GnuCash does not write may be absent or differ; they are not data.
    fn commodity(&mut self, commodity: Commodity) {
        let Some(held) = self.expected.commodities.get(&commodity.id).filter(|c| commodity_is_written(c)) else {
            return;
        };
        let same = **held == commodity && self.seen.commodities.insert(commodity.id.clone());
        self.check(same, || format!("commodity {} differs after writing", commodity.id));
    }

    fn price(&mut self, price: Price) {
        let same = matches(self.expected.prices.get(&price.guid), &price, &mut self.seen.prices, price.guid);
        self.check(same, || format!("price {} differs after writing", price.guid));
    }

    fn account(&mut self, account: Account, lots: Vec<Lot>) {
        if is_root(&account) && !self.roots.contains(&account.guid) {
            self.roots.push(account.guid);
        }
        let same = matches(
            self.expected.accounts.get(&account.guid),
            &account,
            &mut self.seen.accounts,
            account.guid,
        );
        self.check(same, || format!("account {} differs after writing", account.guid));
        lots.iter().for_each(|lot| {
            let same = matches(self.expected.lots.get(&lot.guid), lot, &mut self.seen.lots, lot.guid);
            self.check(same, || format!("lot {} differs after writing", lot.guid));
        });
    }

    fn transaction(&mut self, transaction: Transaction) {
        let guid = transaction.guid;
        let same = matches(
            self.expected.transactions.get(&guid),
            &transaction,
            &mut self.seen.transactions,
            guid,
        );
        self.check(same, || format!("transaction {guid} differs after writing"));
    }

    fn business(&mut self, object: BusinessObject) {
        let guid = object.guid();
        let same = business_matches(&self.expected.business, &object) && self.seen.business.insert(guid);
        self.check(same, || "business objects differ after writing".into());
    }

    fn budget(&mut self, budget: Budget) {
        let guid = budget.guid;
        let same = matches(self.expected.budgets.get(&guid), &budget, &mut self.seen.budgets, guid);
        self.check(same, || format!("budget {guid} differs after writing"));
    }

    fn template_account(&mut self, account: Account) {
        let guid = account.guid;
        if is_root(&account) {
            self.template_root = Some(guid);
        }
        // The Book's account as GnuCash would load it from what was written: a template root made in this session
        // has no commodity, and reads back with the template commodity.
        let expected = self.expected.templates.accounts.get(&guid).map(|a| {
            let mut loaded = Account::clone(a);
            give_template_commodity(&mut loaded);
            Arc::new(loaded)
        });
        let same = matches(expected.as_ref(), &account, &mut self.seen.template_accounts, guid);
        self.check(same, || format!("template account {guid} differs after writing"));
    }

    fn template_transaction(&mut self, transaction: Transaction) {
        let guid = transaction.guid;
        let expected = self.expected.templates.transactions.get(&guid);
        let same = matches(expected, &transaction, &mut self.seen.template_transactions, guid);
        self.check(same, || format!("template transaction {guid} differs after writing"));
    }

    fn scheduled(&mut self, scheduled: ScheduledTransaction) {
        let guid = scheduled.guid;
        let same = matches(self.expected.scheduled.get(&guid), &scheduled, &mut self.seen.scheduled, guid);
        self.check(same, || format!("scheduled transaction {guid} differs after writing"));
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::{read_book, write_book};

    fn sample() -> String {
        std::fs::read_to_string(format!("{}/../../fixtures/gnucash/sample1.gnucash", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn differs(written: &[u8], expected: &Book) -> String {
        match read_back(written, expected).unwrap() {
            ReadBack::Differs(difference) => difference,
            ReadBack::Same(_) => panic!("no difference found"),
        }
    }

    #[test]
    fn a_written_book_reads_back_as_itself() {
        let (book, residue) = read_book(sample().as_bytes()).unwrap();
        let written = write_book(&book, &residue, true).unwrap();
        let ReadBack::Same(again) = read_back(&written[..], &book).unwrap() else {
            panic!("the book reads back differently")
        };
        assert!(again.compressed);
        assert_eq!(again.originals_kept(), residue.originals_kept());
    }

    #[test]
    fn reading_back_finds_changed_missing_and_extra_entities() {
        let (book, residue) = read_book(sample().as_bytes()).unwrap();
        let written = write_book(&book, &residue, false).unwrap();
        let (guid, first) = book.transactions.first().map(|(g, t)| (*g, t.clone())).unwrap();

        let mut changed = book.clone();
        Arc::make_mut(&mut changed.transactions[0]).description = Some("changed".into());
        assert_eq!(differs(&written, &changed), format!("transaction {guid} differs after writing"));

        let mut more = book.clone();
        let extra = Guid::from_str("00000000000000000000000000000001").unwrap();
        more.transactions.insert(
            extra,
            Arc::new(Transaction {
                guid: extra,
                ..(*first).clone()
            }),
        );
        assert_eq!(differs(&written, &more), format!("transaction {extra} differs after writing"));

        let mut fewer = book.clone();
        fewer.transactions.shift_remove(&guid);
        assert_eq!(differs(&written, &fewer), format!("transaction {guid} differs after writing"));
    }

    #[test]
    fn errors_keep_the_precedence_of_a_whole_file_parse() {
        let text = sample();
        let bad_guid = text.replacen("<act:id type=\"guid\">", "<act:id type=\"guid\">zz", 1);
        let read = |bytes: &[u8]| read_book(bytes).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(read(bad_guid.as_bytes()).starts_with("bad act:id"));
        let mut bad_utf8 = bad_guid.clone().into_bytes();
        bad_utf8.insert(bad_utf8.len() - 20, 0xff);
        assert_eq!(
            read(&bad_utf8),
            XmlError::Encoding.to_string(),
            "a bad encoding anywhere comes first"
        );
        let two_books = bad_guid.replacen("</gnc:book>", "</gnc:book>\n<gnc:book version=\"2.0.0\"></gnc:book>", 1);
        assert_eq!(read(two_books.as_bytes()), "not a GnuCash XML book: expected one gnc:book, found 2");
        let bad_syntax = bad_guid.replacen("</gnc:book>", "</gnc:boo>", 1);
        assert!(
            read(bad_syntax.as_bytes()).starts_with("XML syntax error"),
            "malformed XML before a malformed entity"
        );
        assert!(read(format!("{text}<other/>").as_bytes()).contains("content after the root element"));
    }
}
