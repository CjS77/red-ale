//! Transactions and their splits: the aggregate that keeps itself balanced.

use std::fmt;

use crate::{CommodityRef, Frame, Guid, Numeric, NumericError, SlotValue, Time64};

/// A split's reconcile state. `5.17:libgnucash/engine/Split.h` (`NREC`, `CREC`, `YREC`, `FREC`, `VREC`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ReconcileState {
    /// `n`: not reconciled or cleared.
    #[default]
    New,
    /// `c`: cleared.
    Cleared,
    /// `y`: reconciled.
    Reconciled,
    /// `f`: frozen.
    Frozen,
    /// `v`: voided.
    Voided,
}

impl ReconcileState {
    /// The character GnuCash writes.
    pub fn as_char(self) -> char {
        match self {
            Self::New => 'n',
            Self::Cleared => 'c',
            Self::Reconciled => 'y',
            Self::Frozen => 'f',
            Self::Voided => 'v',
        }
    }

    /// Parse the character GnuCash writes.
    pub fn from_char(c: char) -> Option<Self> {
        Some(match c {
            'n' => Self::New,
            'c' => Self::Cleared,
            'y' => Self::Reconciled,
            'f' => Self::Frozen,
            'v' => Self::Voided,
            _ => return None,
        })
    }
}

impl fmt::Display for ReconcileState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_char())
    }
}

/// One leg of a transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    /// Identity.
    pub guid: Guid,
    /// Memo.
    pub memo: Option<String>,
    /// Action.
    pub action: Option<String>,
    /// Reconcile state.
    pub reconcile_state: ReconcileState,
    /// When the split was reconciled.
    pub reconcile_date: Option<Time64>,
    /// Size in the transaction currency.
    pub value: Numeric,
    /// Size in the account's commodity (GnuCash's *amount*; `split:quantity` in XML).
    pub amount: Numeric,
    /// The account; `None` for an orphan.
    pub account: Option<Guid>,
    /// The lot this split belongs to.
    pub lot: Option<Guid>,
    /// Slots.
    pub slots: Frame,
}

impl Split {
    /// A new split with value and amount, state `n`, and no memo or action.
    pub fn new(guid: Guid, account: Guid, value: Numeric, amount: Numeric) -> Self {
        Self {
            guid,
            memo: None,
            action: None,
            reconcile_state: ReconcileState::New,
            reconcile_date: None,
            value,
            amount,
            account: Some(account),
            lot: None,
            slots: Frame::new(),
        }
    }
}

/// Slot paths GnuCash uses on transactions and splits. `5.17:libgnucash/engine/Transaction.cpp`.
pub mod slot_paths {
    /// Transaction notes.
    pub const NOTES: &str = "notes";
    /// Reason a transaction was voided.
    pub const VOID_REASON: &str = "void-reason";
    /// When a transaction was voided, as a string.
    pub const VOID_TIME: &str = "void-time";
    /// The split's value before voiding.
    pub const VOID_FORMER_VALUE: &str = "void-former-value";
    /// The split's amount before voiding.
    pub const VOID_FORMER_AMOUNT: &str = "void-former-amount";
    /// The transaction's notes before voiding.
    pub const VOID_FORMER_NOTES: &str = "void-former-notes";
    /// The transaction that reverses this one.
    pub const REVERSED_BY: &str = "reversed-by";
    /// The date posted, as a date (`date-posted`).
    pub const DATE_POSTED: &str = "date-posted";
    /// Transaction type: `I` invoice, `P` payment, `L` link.
    pub const TXN_TYPE: &str = "trans-txn-type";
    /// Read-only reason.
    pub const READ_ONLY: &str = "trans-read-only";
    /// The posted invoice a transaction belongs to.
    pub const INVOICE: &str = "gncInvoice/invoice-guid";
    /// A book-closing transaction: integer 1 (`5.17:libgnucash/engine/Transaction.cpp:178`).
    pub const BOOK_CLOSING: &str = "book_closing";
    /// A split's online (import) id, which a cloned split does not keep (`5.17:libgnucash/engine/Split.cpp:104`).
    pub const ONLINE_ID: &str = "online_id";
}

/// A balanced exchange of value between accounts.
#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    /// Identity.
    pub guid: Guid,
    /// The transaction currency.
    pub currency: CommodityRef,
    /// Check or reference number.
    pub num: Option<String>,
    /// When the transaction takes effect.
    pub date_posted: Time64,
    /// When it was entered.
    pub date_entered: Time64,
    /// Description.
    pub description: Option<String>,
    /// Slots.
    pub slots: Frame,
    /// Splits, in order.
    pub splits: Vec<Split>,
}

impl Transaction {
    /// The sum of split values: zero for a balanced transaction.
    pub fn imbalance(&self) -> Result<Numeric, NumericError> {
        Numeric::sum_fixed(self.splits.iter().map(|s| s.value))
    }

    /// True when split values sum to exactly zero.
    pub fn is_balanced(&self) -> bool {
        self.imbalance().is_ok_and(Numeric::is_zero)
    }

    /// The split with this GUID.
    pub fn split(&self, guid: Guid) -> Option<&Split> {
        self.splits.iter().find(|s| s.guid == guid)
    }

    /// The split with this GUID, mutably.
    pub fn split_mut(&mut self, guid: Guid) -> Option<&mut Split> {
        self.splits.iter_mut().find(|s| s.guid == guid)
    }

    /// Notes.
    pub fn notes(&self) -> Option<&str> {
        self.slots.get_str(slot_paths::NOTES)
    }

    /// When the transaction was voided: the `void-time` slot, if it holds a timestamp GnuCash can parse.
    /// `xaccTransGetVoidTime`, `5.17:libgnucash/engine/Transaction.cpp:2546-2551`.
    pub fn void_time(&self) -> Option<Time64> {
        self.slots.get_str(slot_paths::VOID_TIME).and_then(parse_void_time)
    }

    /// True when the transaction is void. In GnuCash 5.17 this follows the void timestamp, not the reason: a
    /// transaction voided with an empty reason is void, and a `void-reason` without a `void-time` is not
    /// (`xaccTransGetVoidStatus`, `5.17:libgnucash/engine/Transaction.cpp:2533-2538`).
    pub fn is_void(&self) -> bool {
        self.void_time().is_some()
    }

    /// Why the transaction may not be changed, if it is read-only (`trans-read-only`).
    pub fn read_only_reason(&self) -> Option<&str> {
        self.slots.get_str(slot_paths::READ_ONLY).filter(|r| !r.is_empty())
    }

    /// The transaction that reverses this one, as recorded in `reversed-by`.
    pub fn reversed_by(&self) -> Option<Guid> {
        self.slots.get(slot_paths::REVERSED_BY).and_then(SlotValue::as_guid)
    }

    /// True for a book-closing transaction (`xaccTransGetIsClosingTxn`,
    /// `5.17:libgnucash/engine/Transaction.cpp:2228-2234`).
    pub fn is_closing(&self) -> bool {
        self.slots.get(slot_paths::BOOK_CLOSING).and_then(SlotValue::as_int).is_some_and(|i| i != 0)
    }

    /// The transaction type character from the `trans-txn-type` slot.
    pub fn txn_type(&self) -> Option<char> {
        self.slots.get_str(slot_paths::TXN_TYPE).and_then(|s| s.chars().next())
    }
}

/// Parse a `void-time` string as `GncDateTime(const char*)` does: `YYYY-MM-DD HH:MM:SS` or `YYYYMMDDHHMMSS`, with
/// optional fractional seconds and an optional `+HH`, `+HHMM` or `+HH:MM` offset; no offset means UTC
/// (`5.17:libgnucash/engine/gnc-datetime.cpp:432-479`).
fn parse_void_time(s: &str) -> Option<Time64> {
    let (body, offset) = split_zone(s.trim_end());
    let (whole, frac) = body.split_once('.').unwrap_or((body, ""));
    if frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let format = match whole.len() {
        14 => "%Y%m%d%H%M%S",
        19 => "%Y-%m-%d %H:%M:%S",
        _ => return None,
    };
    let naive = chrono::NaiveDateTime::parse_from_str(whole, format).ok()?;
    // An offset of under an hour is read as UTC: old Windows builds wrote bogus ones (Bug 767824, same source:493-497).
    let offset = offset.filter(|o| o.abs() >= 3600).unwrap_or(0);
    Some(Time64(naive.and_utc().timestamp() - offset))
}

/// Split a trailing `[+-]HH[[:]MM]` zone, after optional spaces, from a time string; the offset in seconds.
fn split_zone(s: &str) -> (&str, Option<i64>) {
    let Some(idx) = s.rfind(['+', '-']).filter(|&i| i >= 10) else { return (s, None) };
    let zone: String = s[idx + 1..].chars().filter(|c| *c != ':').collect();
    if !matches!(zone.len(), 2 | 4) || !zone.bytes().all(|b| b.is_ascii_digit()) {
        return (s, None);
    }
    let hours: i64 = zone[..2].parse().unwrap_or(0);
    let mins: i64 = zone.get(2..).filter(|m| !m.is_empty()).and_then(|m| m.parse().ok()).unwrap_or(0);
    let sign = if s[idx..].starts_with('-') { -1 } else { 1 };
    (s[..idx].trim_end(), Some(sign * (hours * 3600 + mins * 60)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn void_time_parses_as_gnucash_does() {
        let t = Some(Time64(1_729_861_638));
        assert_eq!(parse_void_time("2024-10-25 13:07:18"), t);
        assert_eq!(parse_void_time("20241025130718"), t);
        assert_eq!(parse_void_time("2024-10-25 15:07:18 +0200"), t);
        assert_eq!(parse_void_time("2024-10-25 15:07:18.25+02:00"), t);
        assert_eq!(parse_void_time("2024-10-25 13:07:18 +0030"), t, "sub-hour offsets are read as UTC");
        assert_eq!(parse_void_time(""), None);
        assert_eq!(parse_void_time("yesterday"), None);
        assert_eq!(parse_void_time("2024-10-25"), None);
    }

    #[test]
    fn void_status_follows_the_void_time() {
        let mut t = Transaction {
            guid: Guid::from_bytes([1; 16]),
            currency: CommodityRef::currency("USD"),
            num: None,
            date_posted: Time64(0),
            date_entered: Time64(0),
            description: None,
            slots: Frame::new(),
            splits: Vec::new(),
        };
        t.slots.set(slot_paths::VOID_REASON, SlotValue::String("typo".into()));
        assert!(!t.is_void(), "a reason alone is not void in 5.17");
        t.slots.set(slot_paths::VOID_TIME, SlotValue::String("2024-01-01 00:00:00".into()));
        assert!(t.is_void());
        t.slots.remove(slot_paths::VOID_REASON);
        assert!(t.is_void(), "an empty reason writes no slot, and the transaction is still void");
    }
}
