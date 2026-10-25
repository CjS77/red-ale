//! Accounts and the account tree.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::{CommodityRef, Frame, Guid, Numeric, SlotValue};

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
    /// Display colour (`Account.cpp:2601-2604`).
    pub const COLOR: &str = "color";
    /// Higher balance limit, a numeric (`Account.cpp:77-80`).
    pub const HIGHER_BALANCE_LIMIT: &str = "balance-limit/higher-value";
    /// Lower balance limit, a numeric.
    pub const LOWER_BALANCE_LIMIT: &str = "balance-limit/lower-value";
    /// `"true"` when the limits apply with sub-accounts. GnuCash spells it `inlude`.
    pub const INCLUDE_SUBACCOUNT_BALANCES: &str = "balance-limit/inlude-sub-accts";
    /// `"opening-balance"` on the opening balance account (`Account.cpp:4131-4147`).
    pub const EQUITY_TYPE: &str = "equity-type";
    /// Frame of associated accounts: one GUID per tag (`Account.cpp:2656-2663`).
    pub const ASSOCIATED_ACCOUNT: &str = "associated-account";
}

/// The `equity-type` value of an opening balance account.
pub const OPENING_BALANCE: &str = "opening-balance";

/// The colour GnuCash 2.4 to 3.3 wrote for "no colour"; the 5.17 GUI removes it when it opens a book
/// (`xaccAccountScrubColorNotSet`, `5.17:libgnucash/engine/Scrub.cpp:1414-1452`).
pub const COLOR_NOT_SET: &str = "Not Set";

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

    /// `get_kvp_boolean_path`: a non-zero integer or the string `"true"` (`5.17:libgnucash/engine/Account.cpp:2564-2577`).
    fn flag(&self, path: &str) -> bool {
        match self.slots.get(path) {
            Some(SlotValue::Integer(i)) => *i != 0,
            Some(SlotValue::String(s)) => s == "true",
            _ => false,
        }
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

    /// The colour as GnuCash's GUI sees it: the `color` slot, with `Not Set` and empty read as none.
    pub fn color(&self) -> Option<&str> {
        self.slots.get_str(slot_paths::COLOR).filter(|c| !c.is_empty() && *c != COLOR_NOT_SET)
    }

    /// The `color` slot as stored, `Not Set` included (`xaccAccountGetColor`).
    pub fn stored_color(&self) -> Option<&str> {
        self.slots.get_str(slot_paths::COLOR)
    }

    /// `xaccAccountSetColor`: an empty or absent colour removes the slot.
    pub fn set_color(&mut self, color: Option<&str>) {
        self.slots.set_opt_str(slot_paths::COLOR, color);
    }

    /// `xaccAccountGetHigherBalanceLimit` (`5.17:libgnucash/engine/Account.cpp:4706-4711`).
    pub fn higher_balance_limit(&self) -> Option<Numeric> {
        self.slots.get(slot_paths::HIGHER_BALANCE_LIMIT).and_then(SlotValue::as_numeric)
    }

    /// `xaccAccountGetLowerBalanceLimit`.
    pub fn lower_balance_limit(&self) -> Option<Numeric> {
        self.slots.get(slot_paths::LOWER_BALANCE_LIMIT).and_then(SlotValue::as_numeric)
    }

    /// `xaccAccountSetHigherBalanceLimit`, or `xaccAccountClearHigherBalanceLimit` with `None`.
    pub fn set_higher_balance_limit(&mut self, limit: Option<Numeric>) {
        self.set_numeric(slot_paths::HIGHER_BALANCE_LIMIT, limit);
    }

    /// `xaccAccountSetLowerBalanceLimit`, or `xaccAccountClearLowerBalanceLimit` with `None`.
    pub fn set_lower_balance_limit(&mut self, limit: Option<Numeric>) {
        self.set_numeric(slot_paths::LOWER_BALANCE_LIMIT, limit);
    }

    fn set_numeric(&mut self, path: &str, value: Option<Numeric>) {
        match value {
            Some(n) => self.slots.set(path, SlotValue::Numeric(n)),
            None => drop(self.slots.remove(path)),
        }
    }

    /// `xaccAccountGetIncludeSubAccountBalances`.
    pub fn includes_subaccount_balances(&self) -> bool {
        self.flag(slot_paths::INCLUDE_SUBACCOUNT_BALANCES)
    }

    /// `xaccAccountSetIncludeSubAccountBalances`.
    pub fn set_include_subaccount_balances(&mut self, on: bool) {
        self.set_flag(slot_paths::INCLUDE_SUBACCOUNT_BALANCES, on);
    }

    /// `xaccAccountGetIsOpeningBalance`: an EQUITY account whose `equity-type` is `opening-balance`.
    pub fn is_opening_balance(&self) -> bool {
        self.account_type == AccountType::Equity && self.slots.get_str(slot_paths::EQUITY_TYPE) == Some(OPENING_BALANCE)
    }

    /// `xaccAccountSetIsOpeningBalance`: does nothing unless the account is EQUITY.
    pub fn set_opening_balance(&mut self, on: bool) {
        if self.account_type == AccountType::Equity {
            self.slots.set_opt_str(slot_paths::EQUITY_TYPE, on.then_some(OPENING_BALANCE));
        }
    }

    /// Every associated account's tag and GUID as stored, in tag order. `xaccAccountGetAssociatedAccount` also
    /// requires the GUID to name an account in the book.
    pub fn associated_accounts(&self) -> Vec<(&str, Guid)> {
        self.slots
            .get_key(slot_paths::ASSOCIATED_ACCOUNT)
            .and_then(SlotValue::as_frame)
            .map(|f| f.iter().filter_map(|(tag, v)| v.as_guid().map(|g| (tag.as_str(), g))).collect())
            .unwrap_or_default()
    }

    /// `xaccAccountSetAssociatedAccount`: `None` removes the tag.
    pub fn set_associated_account(&mut self, tag: &str, account: Option<Guid>) {
        let path = format!("{}/{tag}", slot_paths::ASSOCIATED_ACCOUNT);
        match account {
            Some(g) => self.slots.set(&path, SlotValue::Guid(g)),
            None => drop(self.slots.remove(&path)),
        }
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
