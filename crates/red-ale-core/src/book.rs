//! The Book: everything one GnuCash file holds that red-ale models.

use std::collections::BTreeMap;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::business::Business;
use crate::{Account, Budget, Commodity, CommodityRef, Frame, Guid, Lot, Price, Time64, TimeZone, Transaction};

/// Supplies what domain operations need from the outside world: the time, the local time zone and fresh GUIDs. The
/// service passes a real one; tests pass a deterministic one, which keeps the core pure.
pub trait Env {
    /// The current time.
    fn now(&mut self) -> Time64;
    /// A new, unique GUID.
    fn new_guid(&mut self) -> Guid;
    /// The local time zone: day boundaries, "today" and same-day prices are decided in it, as GnuCash decides them.
    fn zone(&self) -> &TimeZone;

    /// Today's date in the local time zone.
    fn today(&mut self) -> Option<crate::Date> {
        let now = self.now();
        self.zone().local_date(now)
    }
}

/// A deterministic [`Env`] for tests and replays: a fixed clock, sequential GUIDs and a fixed zone (UTC unless
/// [`FixedEnv::with_zone`] gives another).
#[derive(Debug, Clone)]
pub struct FixedEnv {
    /// The time `now` returns.
    pub now: Time64,
    next: u128,
    zone: TimeZone,
}

impl FixedEnv {
    /// A fixed clock at `now` in UTC; GUIDs count up from `seed`.
    pub fn new(now: Time64, seed: u128) -> Self {
        Self {
            now,
            next: seed,
            zone: TimeZone::utc(),
        }
    }

    /// The same, in another time zone.
    #[must_use]
    pub fn with_zone(mut self, zone: TimeZone) -> Self {
        self.zone = zone;
        self
    }
}

impl Env for FixedEnv {
    fn now(&mut self) -> Time64 {
        self.now
    }

    fn zone(&self) -> &TimeZone {
        &self.zone
    }

    fn new_guid(&mut self) -> Guid {
        self.next += 1;
        Guid::from_bytes(self.next.to_be_bytes())
    }
}

/// A Book. Entities are held behind `Arc` so that cloning a Book is cheap: a Mutation clones, edits the clone through
/// `Arc::make_mut`, and swaps it in only on success.
///
/// Collections keep the order entities were read in; new entities are appended. Commodities are kept sorted by
/// namespace then mnemonic, the order GnuCash writes them (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1037`).
#[derive(Debug, Clone, PartialEq)]
pub struct Book {
    /// Identity.
    pub guid: Guid,
    /// Book-level slots: options, features, counters.
    pub slots: Frame,
    /// Commodities that are defined in the book (written to the file), by namespace and mnemonic.
    pub commodities: BTreeMap<CommodityRef, Arc<Commodity>>,
    /// The root account's GUID.
    pub root: Guid,
    /// All accounts, including the root.
    pub accounts: IndexMap<Guid, Arc<Account>>,
    /// All transactions.
    pub transactions: IndexMap<Guid, Arc<Transaction>>,
    /// The price database.
    pub prices: IndexMap<Guid, Arc<Price>>,
    /// All lots.
    pub lots: IndexMap<Guid, Arc<Lot>>,
    /// Business objects.
    pub business: Business,
    /// Budgets. GnuCash writes them in an order that changes from save to save
    /// (`design/notes/xml-format-core.md` §7.6), so the order read is kept.
    pub budgets: IndexMap<Guid, Arc<Budget>>,
}

impl Book {
    /// An empty book with a root account.
    pub fn new(guid: Guid, root: Account) -> Self {
        let root_guid = root.guid;
        let mut accounts = IndexMap::new();
        accounts.insert(root_guid, Arc::new(root));
        Self {
            guid,
            slots: Frame::new(),
            commodities: BTreeMap::new(),
            root: root_guid,
            accounts,
            transactions: IndexMap::new(),
            prices: IndexMap::new(),
            lots: IndexMap::new(),
            business: Business::default(),
            budgets: IndexMap::new(),
        }
    }

    /// A new, empty book as GnuCash creates one: a root account named "Root Account" with no commodity, and the
    /// reserved `template` commodity (`5.17:libgnucash/engine/gnc-commodity.cpp:2317-2327`).
    pub fn new_empty(env: &mut dyn Env) -> Self {
        let root = Account::new(env.new_guid(), "Root Account", crate::AccountType::Root, None, None);
        let mut book = Self::new(env.new_guid(), root);
        let template = CommodityRef::new(crate::commodity::TEMPLATE_NAMESPACE, "template");
        let mut c = Commodity::new(template.clone(), Some("template".into()), 1);
        c.cusip = Some("template".into());
        book.commodities.insert(template, Arc::new(c));
        book
    }

    /// The account with this GUID.
    pub fn account(&self, guid: Guid) -> Option<&Account> {
        self.accounts.get(&guid).map(AsRef::as_ref)
    }

    /// The transaction with this GUID.
    pub fn transaction(&self, guid: Guid) -> Option<&Transaction> {
        self.transactions.get(&guid).map(AsRef::as_ref)
    }

    /// The lot with this GUID.
    pub fn lot(&self, guid: Guid) -> Option<&Lot> {
        self.lots.get(&guid).map(AsRef::as_ref)
    }

    /// The budget with this GUID.
    pub fn budget(&self, guid: Guid) -> Option<&Budget> {
        self.budgets.get(&guid).map(AsRef::as_ref)
    }

    /// The defined commodity with this reference.
    pub fn commodity(&self, id: &CommodityRef) -> Option<&Commodity> {
        self.commodities.get(id).map(AsRef::as_ref)
    }

    /// The children of an account, in book order.
    pub fn children(&self, parent: Guid) -> impl Iterator<Item = &Account> {
        self.accounts.values().map(AsRef::as_ref).filter(move |a| a.parent == Some(parent))
    }

    /// Every descendant of an account, depth first, children in book order.
    pub fn descendants(&self, parent: Guid) -> Vec<&Account> {
        self.children(parent)
            .flat_map(|child| std::iter::once(child).chain(self.descendants(child.guid)))
            .collect()
    }

    /// An account's ancestors, nearest first, excluding the root.
    pub fn ancestors(&self, guid: Guid) -> Vec<&Account> {
        std::iter::successors(self.account(guid).and_then(|a| a.parent).and_then(|p| self.account(p)), |a| {
            a.parent.and_then(|p| self.account(p))
        })
        .filter(|a| a.guid != self.root)
        .collect()
    }

    /// The account's full name: ancestor names joined by `:`, excluding the root.
    pub fn full_name(&self, guid: Guid) -> Option<String> {
        let account = self.account(guid)?;
        if guid == self.root {
            return Some(String::new());
        }
        let mut names: Vec<&str> = self.ancestors(guid).iter().map(|a| a.name.as_str()).collect();
        names.reverse();
        names.push(&account.name);
        Some(names.join(":"))
    }

    /// The account with this full name.
    pub fn account_by_full_name(&self, full_name: &str) -> Option<&Account> {
        let mut current = self.root;
        for part in full_name.split(':') {
            current = self.children(current).find(|a| a.name == part)?.guid;
        }
        self.account(current)
    }

    /// Every split in an account, with its transaction, in book order of transactions.
    pub fn splits_of(&self, account: Guid) -> impl Iterator<Item = (&Transaction, &crate::Split)> {
        self.transactions.values().flat_map(move |t| {
            t.splits
                .iter()
                .filter(move |s| s.account == Some(account))
                .map(move |s| (t.as_ref(), s))
        })
    }

    /// Every split in a lot, with its transaction.
    pub fn splits_in_lot(&self, lot: Guid) -> impl Iterator<Item = (&Transaction, &crate::Split)> {
        self.transactions
            .values()
            .flat_map(move |t| t.splits.iter().filter(move |s| s.lot == Some(lot)).map(move |s| (t.as_ref(), s)))
    }

    /// The transaction holding a split.
    pub fn transaction_of_split(&self, split: Guid) -> Option<&Transaction> {
        self.transactions
            .values()
            .map(AsRef::as_ref)
            .find(|t| t.splits.iter().any(|s| s.guid == split))
    }
}
