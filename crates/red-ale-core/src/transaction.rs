//! Transactions and their splits: the aggregate that keeps itself balanced.

use std::fmt;

use crate::{CommodityRef, Frame, Guid, Numeric, NumericError, Time64};

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

    /// True when the transaction is void.
    pub fn is_void(&self) -> bool {
        self.slots.get(slot_paths::VOID_REASON).is_some()
    }

    /// The transaction type character from the `trans-txn-type` slot.
    pub fn txn_type(&self) -> Option<char> {
        self.slots.get_str(slot_paths::TXN_TYPE).and_then(|s| s.chars().next())
    }
}
