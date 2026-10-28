//! Scheduled transactions and books holding them, for tests.

use std::sync::Arc;

use crate::commodity::TEMPLATE_NAMESPACE;
use crate::scheduled::SPLIT_FRAME;
use crate::{
    Account, AccountType, Book, Commodity, CommodityRef, Date, Env, FixedEnv, Frame, Guid, Numeric, Occurrences, PeriodType, Recurrence,
    ScheduledTransaction, SlotValue, Split, Time64, Transaction, WeekendAdjust,
};

pub fn date(y: i32, m: u32, d: u32) -> Date {
    Date::from_ymd(y, m, d).unwrap()
}

pub fn every(mult: u16, period_type: PeriodType, start: Date) -> Recurrence {
    Recurrence::new(mult, period_type, start, WeekendAdjust::None).unwrap()
}

/// A scheduled transaction never run, as the editor makes one: instance count 1, no end.
pub fn sx(start: Date, schedule: Vec<Recurrence>) -> ScheduledTransaction {
    ScheduledTransaction {
        guid: Guid::from_bytes([0x5c; 16]),
        name: "Rent".into(),
        enabled: true,
        auto_create: false,
        auto_create_notify: false,
        advance_create_days: 0,
        advance_remind_days: 0,
        instance_count: 1,
        start,
        last: None,
        end: None,
        occurrences: None,
        template_account: None,
        schedule,
        deferred: Vec::new(),
        slots: Frame::new(),
    }
}

/// Monthly from `start`.
pub fn monthly(start: Date) -> ScheduledTransaction {
    sx(start, vec![every(1, PeriodType::Month, start)])
}

/// Daily from `start`.
pub fn daily(start: Date) -> ScheduledTransaction {
    sx(start, vec![every(1, PeriodType::Day, start)])
}

/// At most `total` occurrences, `remaining` still to come.
pub fn limited(mut sx: ScheduledTransaction, total: i32, remaining: i32) -> ScheduledTransaction {
    sx.occurrences = Some(Occurrences { total, remaining });
    sx
}

/// One template split: the real account, the debit formula and the credit formula (empty for none).
pub struct T<'a>(pub Guid, pub &'a str, pub &'a str);

/// A Book with USD accounts Bank (asset) and Food (expense), EUR Euro (bank), the ACME stock in Shares, and a
/// template root, at a fixed clock.
pub struct Fixture {
    pub book: Book,
    pub env: FixedEnv,
    pub bank: Guid,
    pub food: Guid,
    pub euro: Guid,
    pub shares: Guid,
}

pub fn usd() -> CommodityRef {
    CommodityRef::currency("USD")
}

impl Fixture {
    pub fn new() -> Self {
        let mut env = FixedEnv::new(Time64(1_767_225_600), 1); // 2026-01-01 00:00:00 UTC
        let mut book = Book::new_empty(&mut env);
        let acme = CommodityRef::new("NASDAQ", "ACME");
        book.commodities
            .insert(acme.clone(), Arc::new(Commodity::new(acme.clone(), None, 10_000)));
        let mut f = Self {
            bank: Guid::default(),
            food: Guid::default(),
            euro: Guid::default(),
            shares: Guid::default(),
            book,
            env,
        };
        f.bank = f.account("Bank", AccountType::Bank, usd());
        f.food = f.account("Food", AccountType::Expense, usd());
        f.euro = f.account("Euro", AccountType::Bank, CommodityRef::currency("EUR"));
        f.shares = f.account("Shares", AccountType::Stock, acme);
        let root = f.env.new_guid();
        let template = CommodityRef::new(TEMPLATE_NAMESPACE, "template");
        let account = Account::new(root, "Template Root", AccountType::Root, Some(template), None);
        f.book.templates.root = Some(root);
        f.book.templates.accounts.insert(root, Arc::new(account));
        f
    }

    pub fn account(&mut self, name: &str, ty: AccountType, commodity: CommodityRef) -> Guid {
        let a = Account::new(self.env.new_guid(), name, ty, Some(commodity), Some(self.book.root));
        let g = a.guid;
        self.book.accounts.insert(g, Arc::new(a));
        g
    }

    /// Add `sx` with a template account named by its GUID and one template transaction in `currency` per entry of
    /// `templates`.
    pub fn add(&mut self, sx: ScheduledTransaction, currency: &CommodityRef, templates: &[&[T<'_>]]) -> Guid {
        let template_root = self.book.templates.root;
        let template = Account::new(self.env.new_guid(), sx.guid.to_string(), AccountType::Bank, None, template_root);
        let account = template.guid;
        self.book.templates.accounts.insert(account, Arc::new(template));
        templates.iter().for_each(|splits| {
            let t = self.template(account, currency, splits);
            self.book.templates.transactions.insert(t.guid, Arc::new(t));
        });
        let guid = sx.guid;
        self.book.scheduled.insert(
            guid,
            Arc::new(ScheduledTransaction {
                template_account: Some(account),
                ..sx
            }),
        );
        guid
    }

    fn template(&mut self, account: Guid, currency: &CommodityRef, splits: &[T<'_>]) -> Transaction {
        let splits = splits
            .iter()
            .map(|T(real, debit, credit)| {
                let mut frame = Frame::new();
                frame.insert_key("account", SlotValue::Guid(*real));
                frame.insert_key("credit-formula", SlotValue::String((*credit).into()));
                frame.insert_key("debit-formula", SlotValue::String((*debit).into()));
                let mut s = Split::new(self.env.new_guid(), account, Numeric::new(0, 100), Numeric::new(0, 1));
                s.slots.insert_key(SPLIT_FRAME, SlotValue::Frame(frame));
                s
            })
            .collect();
        Transaction {
            guid: self.env.new_guid(),
            currency: currency.clone(),
            num: None,
            date_posted: Time64(1_700_000_000),
            date_entered: Time64(1_700_000_000),
            description: Some("Rent".into()),
            slots: Frame::new(),
            splits,
        }
    }

    pub fn sx(&self, guid: Guid) -> &ScheduledTransaction {
        &self.book.scheduled[&guid]
    }
}
