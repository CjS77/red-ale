//! Accounts and the account tree.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::{CommodityRef, Frame, Guid, SlotValue};

/// An account type. Names are as GnuCash writes them (`xaccAccountTypeEnumAsString`,
/// `5.17:libgnucash/engine/Account.cpp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum AccountType {
    None,
    Bank,
    Cash,
    Credit,
    Asset,
    Liability,
    Stock,
    Mutual,
    Currency,
    Income,
    Expense,
    Equity,
    Receivable,
    Payable,
    Root,
    Trading,
    Checking,
    Savings,
    MoneyMarket,
    CreditLine,
}

const TYPE_NAMES: [(AccountType, &str); 20] = [
    (AccountType::None, "NONE"),
    (AccountType::Bank, "BANK"),
    (AccountType::Cash, "CASH"),
    (AccountType::Credit, "CREDIT"),
    (AccountType::Asset, "ASSET"),
    (AccountType::Liability, "LIABILITY"),
    (AccountType::Stock, "STOCK"),
    (AccountType::Mutual, "MUTUAL"),
    (AccountType::Currency, "CURRENCY"),
    (AccountType::Income, "INCOME"),
    (AccountType::Expense, "EXPENSE"),
    (AccountType::Equity, "EQUITY"),
    (AccountType::Receivable, "RECEIVABLE"),
    (AccountType::Payable, "PAYABLE"),
    (AccountType::Root, "ROOT"),
    (AccountType::Trading, "TRADING"),
    (AccountType::Checking, "CHECKING"),
    (AccountType::Savings, "SAVINGS"),
    (AccountType::MoneyMarket, "MONEYMRKT"),
    (AccountType::CreditLine, "CREDITLINE"),
];

/// A string that is not an account type name.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown account type {0:?}")]
pub struct ParseAccountTypeError(pub String);

impl AccountType {
    /// The name GnuCash writes.
    pub fn as_str(self) -> &'static str {
        TYPE_NAMES.iter().find(|(t, _)| *t == self).map_or("NONE", |(_, n)| n)
    }

    /// Asset-side types: their balances are normally debits.
    pub fn is_asset_side(self) -> bool {
        matches!(
            self,
            Self::Bank | Self::Cash | Self::Asset | Self::Stock | Self::Mutual | Self::Currency | Self::Receivable | Self::Checking
                | Self::Savings | Self::MoneyMarket
        )
    }

    /// Liability-side types.
    pub fn is_liability_side(self) -> bool {
        matches!(self, Self::Credit | Self::Liability | Self::Payable | Self::CreditLine)
    }

    /// Income and expense: the types whose balances close to equity.
    pub fn is_income_expense(self) -> bool {
        matches!(self, Self::Income | Self::Expense)
    }

    /// Types that hold a security rather than a currency.
    pub fn holds_security(self) -> bool {
        matches!(self, Self::Stock | Self::Mutual)
    }

    /// A/Receivable or A/Payable: types that carry business documents.
    pub fn is_ap_ar(self) -> bool {
        matches!(self, Self::Receivable | Self::Payable)
    }

    /// Whether an account of this type may sit under a parent of type `parent`.
    /// `xaccParentAccountTypesCompatibleWith`, `5.17:libgnucash/engine/Account.cpp`.
    pub fn compatible_with_parent(self, parent: Self) -> bool {
        use AccountType as T;
        let asset_liability = [
            T::Bank, T::Cash, T::Asset, T::Stock, T::Mutual, T::Currency, T::Credit, T::Liability, T::Receivable, T::Payable,
        ];
        match self {
            T::Bank | T::Cash | T::Asset | T::Stock | T::Mutual | T::Currency | T::Credit | T::Liability | T::Receivable | T::Payable => {
                parent == T::Root || asset_liability.contains(&parent)
            }
            T::Income | T::Expense => matches!(parent, T::Income | T::Expense | T::Root),
            T::Equity => matches!(parent, T::Equity | T::Root),
            T::Trading => matches!(parent, T::Trading | T::Root),
            T::Root | T::None | T::Checking | T::Savings | T::MoneyMarket | T::CreditLine => false,
        }
    }
}

impl FromStr for AccountType {
    type Err = ParseAccountTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TYPE_NAMES.iter().find(|(_, n)| *n == s).map(|(t, _)| *t).ok_or_else(|| ParseAccountTypeError(s.to_owned()))
    }
}

impl fmt::Display for AccountType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Slot paths GnuCash uses for account properties. `5.17:libgnucash/engine/Account.cpp`.
pub mod slot_paths {
    /// `"true"` when the account is a placeholder.
    pub const PLACEHOLDER: &str = "placeholder";
    /// `"true"` when the account is hidden.
    pub const HIDDEN: &str = "hidden";
    /// Free-text notes.
    pub const NOTES: &str = "notes";
    /// `"true"` when the account is tax-related.
    pub const TAX_RELATED: &str = "tax-related";
    /// US TXF code.
    pub const TAX_US_CODE: &str = "tax-US/code";
    /// US TXF payer name source.
    pub const TAX_US_PAYER_NAME_SOURCE: &str = "tax-US/payer-name-source";
    /// US TXF copy number.
    pub const TAX_US_COPY_NUMBER: &str = "tax-US/copy-number";
    /// Date of the last reconciliation (integer seconds).
    pub const RECONCILE_LAST_DATE: &str = "reconcile-info/last-date";
    /// Interval of the last reconciliation.
    pub const RECONCILE_LAST_INTERVAL: &str = "reconcile-info/last-interval";
    /// Postponed reconciliation date.
    pub const RECONCILE_POSTPONE_DATE: &str = "reconcile-info/postpone/date";
    /// Postponed reconciliation balance.
    pub const RECONCILE_POSTPONE_BALANCE: &str = "reconcile-info/postpone/balance";
}

/// An account. Placeholder, hidden, notes and tax settings live in its slots, as in GnuCash.
#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    /// Identity.
    pub guid: Guid,
    /// Name, unique among siblings.
    pub name: String,
    /// Account type.
    pub account_type: AccountType,
    /// The commodity the account holds. GnuCash's root account and some old files have none.
    pub commodity: Option<CommodityRef>,
    /// The account's SCU (`act:commodity-scu`), when written.
    pub commodity_scu: Option<i64>,
    /// True when the SCU differs from the commodity's fraction on purpose (`act:non-standard-scu`).
    pub non_standard_scu: bool,
    /// Account code.
    pub code: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Slots.
    pub slots: Frame,
    /// Parent account; `None` only for a root account.
    pub parent: Option<Guid>,
}

impl Account {
    /// A new account with no code, description or slots.
    pub fn new(guid: Guid, name: impl Into<String>, account_type: AccountType, commodity: Option<CommodityRef>, parent: Option<Guid>) -> Self {
        Self {
            guid,
            name: name.into(),
            account_type,
            commodity,
            commodity_scu: None,
            non_standard_scu: false,
            code: None,
            description: None,
            slots: Frame::new(),
            parent,
        }
    }

    fn flag(&self, path: &str) -> bool {
        self.slots.get_str(path) == Some("true")
    }

    fn set_flag(&mut self, path: &str, on: bool) {
        if on {
            self.slots.set(path, SlotValue::String("true".into()));
        } else {
            self.slots.remove(path);
        }
    }

    /// True when the account accepts no splits.
    pub fn is_placeholder(&self) -> bool {
        self.flag(slot_paths::PLACEHOLDER)
    }

    /// Set or clear the placeholder flag.
    pub fn set_placeholder(&mut self, on: bool) {
        self.set_flag(slot_paths::PLACEHOLDER, on);
    }

    /// True when the account is hidden.
    pub fn is_hidden(&self) -> bool {
        self.flag(slot_paths::HIDDEN)
    }

    /// Set or clear the hidden flag.
    pub fn set_hidden(&mut self, on: bool) {
        self.set_flag(slot_paths::HIDDEN, on);
    }

    /// True when the account is tax-related.
    pub fn is_tax_related(&self) -> bool {
        self.flag(slot_paths::TAX_RELATED)
    }

    /// Set or clear the tax-related flag.
    pub fn set_tax_related(&mut self, on: bool) {
        self.set_flag(slot_paths::TAX_RELATED, on);
    }

    /// Notes.
    pub fn notes(&self) -> Option<&str> {
        self.slots.get_str(slot_paths::NOTES)
    }

    /// The SCU quantities are kept in: the account's own when non-standard, else the commodity's fraction.
    pub fn scu(&self, commodity_fraction: i64) -> i64 {
        match self.commodity_scu {
            Some(scu) if self.non_standard_scu => scu,
            Some(scu) if scu > 0 => scu,
            _ => commodity_fraction,
        }
    }
}
