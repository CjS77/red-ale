//! Scheduled transactions and the template accounts and template transactions they create transactions from.
//! GnuCash's `SchedXaction` (`5.17:libgnucash/engine/SchedXaction.h:91-123`) and the template tree under the Book's
//! template root (`5.17:libgnucash/engine/SX-book.cpp:113-125`).

use std::sync::Arc;

use indexmap::IndexMap;

use crate::{Account, Book, Date, Frame, Guid, Numeric, Recurrence, SlotValue, Split, Transaction};

/// The Slot of a template split holding its `sched-xaction` Frame (`GNC_SX_ID`, `5.17:libgnucash/engine/Split.cpp:69`).
pub const SPLIT_FRAME: &str = "sched-xaction";

/// A Scheduled transaction: when to create transactions from its template transactions, and how far it has got.
/// Every field is held as read; none is normalised (`design/notes/engine-scheduled-transactions.md` §1.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledTransaction {
    /// Identity.
    pub guid: Guid,
    /// Name.
    pub name: String,
    /// Since Last Run leaves out a disabled one.
    pub enabled: bool,
    /// Instances are created without asking.
    pub auto_create: bool,
    /// The user is told about instances created without asking.
    pub auto_create_notify: bool,
    /// Days ahead of an occurrence to create it.
    pub advance_create_days: i32,
    /// Days ahead of an occurrence to remind of it.
    pub advance_remind_days: i32,
    /// The Instance count: one more for each instance acted on.
    pub instance_count: i32,
    /// The first possible occurrence.
    pub start: Date,
    /// The last occurrence acted on.
    pub last: Option<Date>,
    /// The last possible occurrence. Kept beside `occurrences` if a file has both; GnuCash then writes only the count.
    pub end: Option<Date>,
    /// An occurrence limit, as `sx:num-occur` and `sx:rem-occur` give it.
    pub occurrences: Option<Occurrences>,
    /// The template account, `None` if the file names none; GnuCash then finds it by name, the scheduled
    /// transaction's GUID (`5.17:libgnucash/backend/xml/gnc-schedxaction-xml-v2.cpp:722-761`).
    pub template_account: Option<Guid>,
    /// The Schedule: occurrences are the union of these Recurrences'. Empty means it never occurs.
    pub schedule: Vec<Recurrence>,
    /// Postponed instances, in file order.
    pub deferred: Vec<DeferredInstance>,
    /// Slots.
    pub slots: Frame,
}

/// An occurrence limit. GnuCash has one when `total` is not 0 (`xaccSchedXactionHasOccurDef`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrences {
    /// How many occurrences in all.
    pub total: i32,
    /// How many remain after the last occurrence acted on.
    pub remaining: i32,
}

/// A Deferred instance: the state of the scheduled transaction before the instance that was postponed
/// (`SXTmpStateData`, `5.17:libgnucash/engine/SchedXaction.h:130-136`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeferredInstance {
    /// The last occurrence before the postponed one.
    pub last: Option<Date>,
    /// Occurrences remaining then.
    pub remaining: i32,
    /// The Instance count then.
    pub instance_count: i32,
}

/// Template accounts and template transactions: their own aggregate, never part of the Account tree or the Book's
/// transactions, so no balance, report, query, import, scrub or export sees them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Templates {
    /// The template root, once the Book has one. GnuCash makes one in every book but writes it only when it has
    /// children (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1179-1199`).
    pub root: Option<Guid>,
    /// The template root and the Template accounts, in read order.
    pub accounts: IndexMap<Guid, Arc<Account>>,
    /// Template transactions, in read order.
    pub transactions: IndexMap<Guid, Arc<Transaction>>,
}

impl Templates {
    /// Every split of every template transaction, with its transaction.
    pub fn splits(&self) -> impl Iterator<Item = (&Transaction, &Split)> {
        self.transactions.values().flat_map(|t| t.splits.iter().map(move |s| (t.as_ref(), s)))
    }

    /// Whether any template account sits under the template root. GnuCash's XML writer writes the template block,
    /// root included, only then (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1179-1199`); a root left childless,
    /// by deleting the last scheduled transaction, is not in the file.
    pub fn has_descendants(&self) -> bool {
        self.accounts.keys().any(|g| Some(*g) != self.root)
    }
}

/// What a template split's `sched-xaction` Frame says (`5.17:libgnucash/engine/Split.cpp:69-75`): the Account the
/// created split goes to, and its credit and debit Formulas with their values. The split keeps the whole Frame as read;
/// this only reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TemplateSplit<'a> {
    frame: &'a Frame,
}

impl<'a> TemplateSplit<'a> {
    /// The split's `sched-xaction` Frame, if it has one.
    pub fn of(split: &'a Split) -> Option<Self> {
        split.slots.get_key(SPLIT_FRAME).and_then(SlotValue::as_frame).map(|frame| Self { frame })
    }

    /// The Account the created split goes to.
    pub fn account(&self) -> Option<Guid> {
        self.frame.get_key("account").and_then(SlotValue::as_guid)
    }

    /// The credit Formula; empty means no credit.
    pub fn credit_formula(&self) -> Option<&'a str> {
        self.frame.get_key("credit-formula").and_then(SlotValue::as_str)
    }

    /// The debit Formula; empty means no debit.
    pub fn debit_formula(&self) -> Option<&'a str> {
        self.frame.get_key("debit-formula").and_then(SlotValue::as_str)
    }

    /// The credit Formula's value when it has no variables, else 0.
    pub fn credit_numeric(&self) -> Option<Numeric> {
        self.frame.get_key("credit-numeric").and_then(SlotValue::as_numeric)
    }

    /// The debit Formula's value when it has no variables, else 0.
    pub fn debit_numeric(&self) -> Option<Numeric> {
        self.frame.get_key("debit-numeric").and_then(SlotValue::as_numeric)
    }

    /// The register's constant `shares` text, which nothing reads (`5.17:libgnucash/engine/Split.cpp:440-444`).
    pub fn shares(&self) -> Option<&'a str> {
        self.frame.get_key("shares").and_then(SlotValue::as_str)
    }
}

impl Book {
    /// The scheduled transaction whose template account this is: the one naming it, or, for one that names none, the
    /// one whose GUID is its name, as GnuCash finds it (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:378-388`).
    pub fn scheduled_of_template_account(&self, account: Guid) -> Option<&ScheduledTransaction> {
        let name = self.templates.accounts.get(&account).map(|a| a.name.as_str());
        self.scheduled
            .values()
            .map(AsRef::as_ref)
            .find(|sx| sx.template_account.map_or_else(|| name.is_some_and(|n| n == sx.guid.to_string()), |g| g == account))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AccountType;

    fn template_split(frame: Frame) -> Split {
        let mut split = Split::new(Guid::from_bytes([3; 16]), Guid::from_bytes([2; 16]), Numeric::new(0, 100), Numeric::new(0, 1));
        split.slots.insert_key(SPLIT_FRAME, SlotValue::Frame(frame));
        split
    }

    #[test]
    fn the_frame_is_read_as_written() {
        let real = Guid::from_bytes([7; 16]);
        let mut frame = Frame::new();
        frame.insert_key("account", SlotValue::Guid(real));
        frame.insert_key("credit-formula", SlotValue::String(String::new()));
        frame.insert_key("debit-formula", SlotValue::String("rent * 2".into()));
        frame.insert_key("debit-numeric", SlotValue::Numeric(Numeric::new(0, 1)));
        frame.insert_key("shares", SlotValue::String("(x + y)/42".into()));
        let split = template_split(frame);
        let t = TemplateSplit::of(&split).unwrap();
        assert_eq!(t.account(), Some(real));
        assert_eq!((t.credit_formula(), t.debit_formula()), (Some(""), Some("rent * 2")));
        assert_eq!((t.credit_numeric(), t.debit_numeric()), (None, Some(Numeric::new(0, 1))));
        assert_eq!(t.shares(), Some("(x + y)/42"));
        let plain = Split::new(Guid::from_bytes([4; 16]), real, Numeric::new(0, 1), Numeric::new(0, 1));
        assert!(TemplateSplit::of(&plain).is_none());
    }

    #[test]
    fn a_template_account_belongs_to_the_scheduled_transaction_naming_it_or_named_by_it() {
        let root = Account::new(Guid::from_bytes([1; 16]), "Root Account", AccountType::Root, None, None);
        let mut book = Book::new(Guid::from_bytes([9; 16]), root);
        let (named, by_name) = (Guid::from_bytes([5; 16]), Guid::from_bytes([6; 16]));
        let sx = |guid: Guid, template_account| ScheduledTransaction {
            guid,
            name: guid.to_string(),
            enabled: true,
            auto_create: false,
            auto_create_notify: false,
            advance_create_days: 0,
            advance_remind_days: 0,
            instance_count: 1,
            start: Date::from_ymd(2024, 1, 1).unwrap(),
            last: None,
            end: None,
            occurrences: None,
            template_account,
            schedule: Vec::new(),
            deferred: Vec::new(),
            slots: Frame::new(),
        };
        let (a, b) = (Guid::from_bytes([10; 16]), Guid::from_bytes([11; 16]));
        [(a, named), (b, by_name)].into_iter().for_each(|(account, sx_guid)| {
            let template = Account::new(account, sx_guid.to_string(), AccountType::Bank, None, None);
            book.templates.accounts.insert(account, Arc::new(template));
        });
        book.scheduled.insert(named, Arc::new(sx(named, Some(a))));
        book.scheduled.insert(by_name, Arc::new(sx(by_name, None)));
        assert_eq!(book.scheduled_of_template_account(a).map(|s| s.guid), Some(named));
        assert_eq!(book.scheduled_of_template_account(b).map(|s| s.guid), Some(by_name));
        assert!(book.scheduled_of_template_account(Guid::from_bytes([12; 16])).is_none());
    }
}
