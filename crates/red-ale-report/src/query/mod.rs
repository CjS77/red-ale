//! The query API (`design/reports.md` §5): the read-only handles through which a report script reads the Book.
//!
//! A run works on a [`Snapshot`]: a clone of the Book (one `Arc` per entity) behind an `Rc` that only the handles
//! hold. Handles return values and copies, never a reference a script could write through, and `Book` has no interior
//! mutability, so a script has no path to a Mutation or a `&mut Book`.
//!
//! Each script-facing function reads the Book through one `red_ale_core` read, named in `design/reports.md` §5,
//! "Core reads behind the query API". Lists come back as Rhai arrays in a fixed order, never in hash order.
//!
//! | Rhai type     | Handle                | Identity                       |
//! |---------------|-----------------------|--------------------------------|
//! | `Book`        | [`BookHandle`]        | the snapshot                   |
//! | `Account`     | [`AccountHandle`]     | GUID                           |
//! | `Transaction` | [`TransactionHandle`] | GUID                           |
//! | `Split`       | [`SplitHandle`]       | its transaction's GUID and its own |
//! | `Commodity`   | [`CommodityHandle`]   | `NAMESPACE::MNEMONIC`          |
//! | `Price`       | [`PriceHandle`]       | GUID                           |
//! | `Lot`         | [`LotHandle`]         | GUID                           |
//! | `Budget`      | [`BudgetHandle`]      | GUID                           |

// Functions registered with Rhai take their arguments by value: Rhai passes owned values to every parameter but
// the first.
#[allow(clippy::needless_pass_by_value)]
mod account;
mod args;
#[allow(clippy::needless_pass_by_value)]
mod book;
#[allow(clippy::needless_pass_by_value)]
mod budget;
mod entities;
#[allow(clippy::needless_pass_by_value)]
mod split_query;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use red_ale_core::formula::NumberFormat;
use red_ale_core::ops::balances::{Balances, running_balances};
use red_ale_core::{Account, Book, CommodityRef, Guid, Lot, Price, Split, Time64, TimeZone, Transaction};
use rhai::{Engine, EvalAltResult};

pub(crate) use args::{account_arg, commodity_arg, rational_arg, small_count, time_arg};
pub use budget::BudgetHandle;

/// The result of a script-facing function: a value, or an error the engine turns into `report-script`.
pub(crate) type Fallible<T> = Result<T, Box<EvalAltResult>>;

/// A script error with `message`. Rhai's errors are boxed.
#[allow(clippy::unnecessary_box_returns)]
pub(crate) fn script_error(message: impl AsRef<str>) -> Box<EvalAltResult> {
    message.as_ref().into()
}

/// The Book a run reads, with the running totals of the accounts read so far.
#[derive(Debug)]
pub(crate) struct Snapshot {
    book: Book,
    /// The run's now: what the latest-price lookup reads instead of the clock.
    now: Time64,
    /// The local time zone, which places budget periods' bounds as GnuCash's process time zone does.
    zone: TimeZone,
    /// How formulas read numbers: the Locale's, as GnuCash's expression parser takes them.
    formula_format: NumberFormat,
    /// The running totals of each account's splits, by split GUID, computed when first read.
    running: RefCell<BTreeMap<Guid, Rc<BTreeMap<Guid, Balances>>>>,
}

impl Snapshot {
    /// A snapshot of `book` for a run at `now` in `zone`, reading formulas' numbers in `formula_format`: a
    /// copy-on-write clone, one `Arc` per entity.
    pub(crate) fn new(book: &Book, now: Time64, zone: TimeZone, formula_format: NumberFormat) -> Rc<Self> {
        Rc::new(Self {
            book: book.clone(),
            now,
            zone,
            formula_format,
            running: RefCell::new(BTreeMap::new()),
        })
    }

    /// The Book, read-only.
    pub(crate) fn book(&self) -> &Book {
        &self.book
    }

    /// The run's now.
    pub(crate) fn now(&self) -> Time64 {
        self.now
    }

    /// The run's local time zone.
    pub(crate) fn zone(&self) -> &TimeZone {
        &self.zone
    }

    /// How the run's formulas read numbers.
    pub(crate) fn formula_format(&self) -> NumberFormat {
        self.formula_format
    }

    /// The running totals of `account`'s splits: `ops::balances::running_balances`, computed once per run.
    fn running(&self, account: Guid) -> Fallible<Rc<BTreeMap<Guid, Balances>>> {
        if let Some(found) = self.running.borrow().get(&account) {
            return Ok(Rc::clone(found));
        }
        let totals: BTreeMap<Guid, Balances> = running_balances(&self.book, account)
            .map_err(|e| script_error(e.to_string()))?
            .iter()
            .map(|s| (s.split.guid, s.running))
            .collect();
        let totals = Rc::new(totals);
        self.running.borrow_mut().insert(account, Rc::clone(&totals));
        Ok(totals)
    }
}

/// The open Book, as scripts see it (Rhai type `Book`).
#[derive(Debug, Clone)]
pub struct BookHandle(pub(crate) Rc<Snapshot>);

/// An account (Rhai type `Account`).
#[derive(Debug, Clone)]
pub struct AccountHandle {
    snap: Rc<Snapshot>,
    guid: Guid,
}

/// A transaction (Rhai type `Transaction`).
#[derive(Debug, Clone)]
pub struct TransactionHandle {
    snap: Rc<Snapshot>,
    guid: Guid,
}

/// A split (Rhai type `Split`).
#[derive(Debug, Clone)]
pub struct SplitHandle {
    snap: Rc<Snapshot>,
    transaction: Guid,
    guid: Guid,
}

/// A commodity (Rhai type `Commodity`).
#[derive(Debug, Clone)]
pub struct CommodityHandle {
    snap: Rc<Snapshot>,
    id: CommodityRef,
}

/// A price (Rhai type `Price`).
#[derive(Debug, Clone)]
pub struct PriceHandle {
    snap: Rc<Snapshot>,
    guid: Guid,
}

/// A lot (Rhai type `Lot`).
#[derive(Debug, Clone)]
pub struct LotHandle {
    snap: Rc<Snapshot>,
    guid: Guid,
}

impl BookHandle {
    pub(crate) fn account(&self, guid: Guid) -> AccountHandle {
        AccountHandle {
            snap: Rc::clone(&self.0),
            guid,
        }
    }

    pub(crate) fn commodity(&self, id: &CommodityRef) -> CommodityHandle {
        CommodityHandle {
            snap: Rc::clone(&self.0),
            id: red_ale_core::ops::commodities::canonical(id),
        }
    }

    fn price(&self, guid: Guid) -> PriceHandle {
        PriceHandle {
            snap: Rc::clone(&self.0),
            guid,
        }
    }

    fn split(&self, t: &Transaction, s: &Split) -> SplitHandle {
        SplitHandle {
            snap: Rc::clone(&self.0),
            transaction: t.guid,
            guid: s.guid,
        }
    }

    fn splits<'a>(&self, found: impl IntoIterator<Item = (&'a Transaction, &'a Split)>) -> rhai::Array {
        found.into_iter().map(|(t, s)| rhai::Dynamic::from(self.split(t, s))).collect()
    }
}

impl AccountHandle {
    /// The account's GUID.
    pub fn guid(&self) -> Guid {
        self.guid
    }

    fn get(&self) -> Fallible<&Account> {
        self.snap
            .book
            .account(self.guid)
            .ok_or_else(|| script_error(format!("no account {}", self.guid)))
    }

    fn book(&self) -> BookHandle {
        BookHandle(Rc::clone(&self.snap))
    }
}

impl TransactionHandle {
    /// The transaction's GUID.
    pub fn guid(&self) -> Guid {
        self.guid
    }

    fn get(&self) -> Fallible<&Transaction> {
        self.snap
            .book
            .transaction(self.guid)
            .ok_or_else(|| script_error(format!("no transaction {}", self.guid)))
    }
}

impl SplitHandle {
    /// The split's GUID.
    pub fn guid(&self) -> Guid {
        self.guid
    }

    fn get(&self) -> Fallible<(&Transaction, &Split)> {
        let t = self
            .snap
            .book
            .transaction(self.transaction)
            .ok_or_else(|| script_error(format!("no transaction {}", self.transaction)))?;
        let s = t
            .splits
            .iter()
            .find(|s| s.guid == self.guid)
            .ok_or_else(|| script_error(format!("no split {}", self.guid)))?;
        Ok((t, s))
    }
}

impl CommodityHandle {
    /// The commodity's identity.
    pub fn id(&self) -> &CommodityRef {
        &self.id
    }
}

impl PriceHandle {
    /// The price's GUID.
    pub fn guid(&self) -> Guid {
        self.guid
    }

    fn get(&self) -> Fallible<&Price> {
        self.snap
            .book
            .prices
            .get(&self.guid)
            .map(AsRef::as_ref)
            .ok_or_else(|| script_error(format!("no price {}", self.guid)))
    }
}

impl LotHandle {
    fn get(&self) -> Fallible<&Lot> {
        self.snap
            .book
            .lot(self.guid)
            .ok_or_else(|| script_error(format!("no lot {}", self.guid)))
    }
}

/// Register the query API's types and functions.
pub(crate) fn register(engine: &mut Engine) {
    book::register(engine);
    budget::register(engine);
    account::register(engine);
    entities::register(engine);
}
