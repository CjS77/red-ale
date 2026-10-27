//! Transactions as CSV, in the simple or complex layout.
//!
//! Translated from `5.17:gnucash/import-export/csv-exp/csv-transactions-export.cpp`, GPL-2.0-or-later, Copyright (C)
//! 2012 Robert Fewell: the column helpers (`:55-206`), the line makers (`:210-253`), `export_query_splits`
//! (`:263-317`), `account_splits` (`:319-334`) and `csv_transactions_export` (`:341-412`). The register export
//! (`XML_EXPORT_REGISTER`) has no counterpart: red-ale has no registers.

use std::collections::HashSet;

use red_ale_core::book_options::uses_split_action_for_num;
use red_ale_core::ops::transactions::find::share_price;
use red_ale_core::ops::transactions::order::split_order;
use red_ale_core::ops::transactions::other_split;
use red_ale_core::transaction::slot_paths;
use red_ale_core::{
    Account, AccountType, Book, DENOM_AUTO, Date, DenomMode, DomainError, DomainResult, Guid, How, Numeric, ReconcileState, Round,
    SlotValue, Split, Time64, TimeZone, Transaction,
};
use red_ale_report::number::PrintInfo;

use crate::CsvOptions;
use crate::format::{Formats, account_info, commodity_info, price_info, without_separators};

/// The Full Category Path of a split with not exactly one other split (`csv-transactions-export.cpp:135`).
pub const SPLIT_TRANSACTION: &str = "-- Split Transaction --";

/// One line per transaction or one per split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    /// One line per transaction, for its split in the exported account, as a one-line register shows it.
    Simple,
    /// One line per split, every split of the transaction, each repeating the transaction's fields.
    #[default]
    Complex,
}

/// What to export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionsRequest {
    /// The accounts, in the order to process them; a transaction goes under the first one it has a split in. Their
    /// sub-accounts are not included.
    pub accounts: Vec<Guid>,
    /// Posted on or after local 00:00:00 of this date; by default the earliest.
    pub from: Option<Date>,
    /// Posted on or before local 23:59:59 of this date; by default the latest.
    pub to: Option<Date>,
    /// The layout.
    pub layout: Layout,
}

/// Transactions as CSV text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionsCsv {
    /// The text: the header, then the lines, each ending in CRLF.
    pub csv: String,
    /// Transactions written.
    pub transactions: usize,
    /// Lines written, the header not counted.
    pub lines: usize,
}

/// The transactions of `request.accounts` posted in its range, as `csv_transactions_export` writes them. An empty
/// account list, an unknown account, or `from` after `to` is refused.
pub fn export_transactions(
    book: &Book,
    zone: &TimeZone,
    formats: &Formats,
    options: &CsvOptions,
    request: &TransactionsRequest,
) -> DomainResult<TransactionsCsv> {
    let range = check(book, zone, request)?;
    let mut out = Out {
        csv: String::new(),
        transactions: 0,
        lines: 0,
    };
    options.add_line(&mut out.csv, &header(request.layout, uses_split_action_for_num(book)));
    let columns = Columns { book, zone, formats };
    let mut seen = HashSet::new();
    request.accounts.iter().filter_map(|g| book.account(*g)).for_each(|account| {
        let trading = account.account_type == AccountType::Trading;
        account_splits(book, account.guid, range)
            .into_iter()
            .filter(|(t, s)| seen.insert(t.guid) && exported(book, s, trading))
            .for_each(|(t, s)| out.transaction(&columns, options, request.layout, trading, t, s));
    });
    Ok(TransactionsCsv {
        csv: out.csv,
        transactions: out.transactions,
        lines: out.lines,
    })
}

/// The range in local time, after checking the request.
fn check(book: &Book, zone: &TimeZone, request: &TransactionsRequest) -> DomainResult<(Option<Time64>, Option<Time64>)> {
    if request.accounts.is_empty() {
        return Err(DomainError::invalid("name at least one account to export"));
    }
    if let Some(missing) = request.accounts.iter().find(|g| book.account(**g).is_none()) {
        return Err(DomainError::not_found("account", missing));
    }
    if let (Some(from), Some(to)) = (request.from, request.to)
        && from > to
    {
        return Err(DomainError::invalid(format!("from {from} is after to {to}")));
    }
    Ok((request.from.map(|d| zone.day_start(d)), request.to.map(|d| zone.day_end(d))))
}

/// The header (`csv-transactions-export.cpp:347-388`). The num field source changes two names; the data stays the
/// transaction's num and the split's action.
fn header(layout: Layout, num_action: bool) -> Vec<&'static str> {
    let number = if num_action { "Transaction Number" } else { "Number" };
    match layout {
        Layout::Simple => vec![
            "Date",
            "Account Name",
            number,
            "Description",
            "Full Category Path",
            "Reconcile",
            "Amount With Sym",
            "Amount Num.",
            "Value With Sym",
            "Value Num.",
            "Rate/Price",
        ],
        Layout::Complex => vec![
            "Date",
            "Transaction ID",
            number,
            "Description",
            "Notes",
            "Commodity/Currency",
            "Void Reason",
            if num_action { "Number/Action" } else { "Action" },
            "Memo",
            "Full Account Name",
            "Account Name",
            "Amount With Sym",
            "Amount Num.",
            "Value With Sym",
            "Value Num.",
            "Reconcile",
            "Reconcile Date",
            "Rate/Price",
        ],
    }
}

/// An account's own splits posted in `range`, inclusive, by posted date and then split order: the query of
/// `account_splits` (`csv-transactions-export.cpp:325-331`).
fn account_splits(book: &Book, account: Guid, range: (Option<Time64>, Option<Time64>)) -> Vec<(&Transaction, &Split)> {
    let in_range = |t: Time64| range.0.is_none_or(|from| t >= from) && range.1.is_none_or(|to| t <= to);
    let mut found: Vec<(&Transaction, &Split)> = book.splits_of(account).filter(|(t, _)| in_range(t.date_posted)).collect();
    let num_action = uses_split_action_for_num(book);
    found.sort_by(|(ta, sa), (tb, sb)| {
        ta.date_posted
            .cmp(&tb.date_posted)
            .then_with(|| split_order(ta, sa, tb, sb, num_action))
    });
    found
}

/// Whether a split is written: not one with no account, and not a trading split unless a trading account is being
/// exported (`csv-transactions-export.cpp:280-288`, `:305-309`).
fn exported(book: &Book, s: &Split, trading_export: bool) -> bool {
    match s.account.and_then(|a| book.account(a)) {
        Some(a) => trading_export || a.account_type != AccountType::Trading,
        None => false,
    }
}

/// The text being written and what it holds.
struct Out {
    csv: String,
    transactions: usize,
    lines: usize,
}

impl Out {
    /// One transaction met through `split`: its simple line, or a complex line per exported split.
    fn transaction(&mut self, c: &Columns<'_>, options: &CsvOptions, layout: Layout, trading: bool, t: &Transaction, split: &Split) {
        self.transactions += 1;
        match layout {
            Layout::Simple => self.line(options, &c.simple(t, split)),
            Layout::Complex => t
                .splits
                .iter()
                .filter(|s| {
                    trading
                        || s.account
                            .and_then(|a| c.book.account(a))
                            .is_none_or(|a| a.account_type != AccountType::Trading)
                })
                .for_each(|s| self.line(options, &c.complex(t, s))),
        }
    }

    fn line(&mut self, options: &CsvOptions, fields: &[String]) {
        options.add_line(&mut self.csv, fields);
        self.lines += 1;
    }
}

/// What the column helpers read.
struct Columns<'a> {
    book: &'a Book,
    zone: &'a TimeZone,
    formats: &'a Formats,
}

impl Columns<'_> {
    /// `make_simple_trans_line` (`csv-transactions-export.cpp:210-227`). The Account Name column holds the full name.
    fn simple(&self, t: &Transaction, s: &Split) -> Vec<String> {
        let void = t.is_void();
        vec![
            self.date(t.date_posted),
            self.full_name(s.account),
            text(t.num.as_deref()),
            text(t.description.as_deref()),
            self.category(t, s),
            reconcile(s.reconcile_state),
            self.amount(s, void, true),
            self.amount(s, void, false),
            self.value(t, s, void, true),
            self.value(t, s, void, false),
            self.rate(s, void),
        ]
    }

    /// `make_complex_trans_line` (`csv-transactions-export.cpp:229-253`).
    fn complex(&self, t: &Transaction, s: &Split) -> Vec<String> {
        let void = t.is_void();
        vec![
            self.date(t.date_posted),
            t.guid.to_string(),
            text(t.num.as_deref()),
            text(t.description.as_deref()),
            text(t.notes()),
            t.currency.to_string(),
            text(t.slots.get_str(slot_paths::VOID_REASON)),
            text(s.action.as_deref()),
            text(s.memo.as_deref()),
            self.full_name(s.account),
            self.account(s).map(|a| a.name.clone()).unwrap_or_default(),
            self.amount(s, void, true),
            self.amount(s, void, false),
            self.value(t, s, void, true),
            self.value(t, s, void, false),
            reconcile(s.reconcile_state),
            self.reconcile_date(s),
            self.price(s, void),
        ]
    }

    fn account(&self, s: &Split) -> Option<&Account> {
        s.account.and_then(|a| self.book.account(a))
    }

    fn full_name(&self, account: Option<Guid>) -> String {
        account.and_then(|a| self.book.full_name(a)).unwrap_or_default()
    }

    /// `get_date`: the posted time's local date (`csv-transactions-export.cpp:55-61`).
    fn date(&self, t: Time64) -> String {
        self.zone.local_date(t).map(|d| self.formats.date(d)).unwrap_or_default()
    }

    /// `get_reconcile_date`: only for a reconciled split (`csv-transactions-export.cpp:70-80`).
    fn reconcile_date(&self, s: &Split) -> String {
        if s.reconcile_state == ReconcileState::Reconciled {
            self.date(s.reconcile_date.unwrap_or_default())
        } else {
            String::new()
        }
    }

    /// `get_category`: the other split's account, or the split-transaction placeholder (`csv-transactions-export.cpp:130-136`).
    fn category(&self, t: &Transaction, s: &Split) -> String {
        other_split(self.book, t, s.guid).map_or_else(|| SPLIT_TRANSACTION.to_owned(), |o| self.full_name(o.account))
    }

    /// `get_amount`: the amount, or a void transaction's former amount (`csv-transactions-export.cpp:161-170`).
    fn amount(&self, s: &Split, void: bool, symbol: bool) -> String {
        let amount = if void {
            former(s, slot_paths::VOID_FORMER_AMOUNT)
        } else {
            s.amount
        };
        let info = self.account(s).map_or_else(
            || self.formats.default_info(self.book, symbol),
            |a| account_info(self.book, a, symbol),
        );
        self.formats.amount(amount, &plain_unless(symbol, info))
    }

    /// `get_value`: the value, or a void transaction's former value, in the transaction currency
    /// (`csv-transactions-export.cpp:172-183`).
    fn value(&self, t: &Transaction, s: &Split, void: bool, symbol: bool) -> String {
        let value = if void { former(s, slot_paths::VOID_FORMER_VALUE) } else { s.value };
        let info = commodity_info(self.book, &t.currency, symbol).unwrap_or_else(|| self.formats.default_info(self.book, symbol));
        self.formats.amount(value, &plain_unless(symbol, info))
    }

    /// `get_rate`: the share price, or 0 for a void transaction (`csv-transactions-export.cpp:185-192`).
    fn rate(&self, s: &Split, void: bool) -> String {
        let rate = if void { Numeric::ZERO } else { share_price(s) };
        self.formats.amount(rate, &price_info(self.book, self.account(s)))
    }

    /// `get_price`: the share price, or a void transaction's former value over its former amount to six significant
    /// figures, half up; nothing when that cannot be computed (`csv-transactions-export.cpp:194-206`).
    fn price(&self, s: &Split, void: bool) -> String {
        let info = price_info(self.book, self.account(s));
        let price = if void {
            let how = How::new(DenomMode::SigFigs(6), Round::HalfUp);
            former(s, slot_paths::VOID_FORMER_VALUE)
                .div(former(s, slot_paths::VOID_FORMER_AMOUNT), DENOM_AUTO, how)
                .ok()
        } else {
            Some(share_price(s))
        };
        price.map(|p| self.formats.amount(p, &info)).unwrap_or_default()
    }
}

/// A `Num.` column's print info: no symbol and no separators when `symbol` is off.
fn plain_unless(symbol: bool, info: PrintInfo) -> PrintInfo {
    if symbol { info } else { without_separators(info) }
}

/// `xaccSplitVoidFormerAmount` and `xaccSplitVoidFormerValue`: the slot, else zero
/// (`5.17:libgnucash/engine/Split.cpp:2190-2204`).
fn former(s: &Split, path: &str) -> Numeric {
    s.slots.get(path).and_then(SlotValue::as_numeric).unwrap_or(Numeric::ZERO)
}

/// `gnc_get_reconcile_str`: the flag's letter, untranslated (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:468-487`).
fn reconcile(state: ReconcileState) -> String {
    state.as_char().to_string()
}

fn text(s: Option<&str>) -> String {
    s.unwrap_or_default().to_owned()
}
