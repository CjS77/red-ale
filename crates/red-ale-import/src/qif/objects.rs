//! The records of a QIF file.
//!
//! Translated from `5.17:gnucash/import-export/qif-imp/qif-objects.scm` ("representations for parts of an imported
//! Quicken file"), Bill Gribble <grib@billgribble.com> 20 Feb 2000. GPL-2.0-or-later; this translation is
//! GPL-3.0-or-later.
//!
//! GnuCash builds each record with every field `#f` and fills the fields as it reads lines, as raw strings. Later,
//! `qif-file:parse-fields` replaces some of those strings with parsed values in place. A [`Field`] is such a slot: it
//! holds the text as read until it is parsed, and the parsed value after.
//!
//! **List order follows GnuCash.** GnuCash adds to its lists with `cons`, so most lists are newest first, and the code
//! translated on top of this (`qif-to-gnc.scm`, `qif-dialog-utils.scm`, `qif-merge-groups.scm`) relies on that, for
//! example through `(car (qif-xtn:splits xtn))`. To keep such code a line-by-line translation:
//!
//! - a transaction's [`QifXtn::splits`] are newest first: `splits[0]` is the last `S` line of the record;
//! - [`QifFile::accounts`], [`QifFile::cats`], [`QifFile::classes`] and [`QifFile::prices`] are newest first;
//! - [`QifFile::xtns`] are in file order, as GnuCash reverses them once the file is read (`qif-file.scm:564-566`).

use std::collections::VecDeque;

use red_ale_core::{AccountType, Numeric};

use super::parse::{self, Action, CategoryParseError, Cleared, QifDate};
use super::string::string_replace_char;

/// A field that GnuCash reads as text and may later parse in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field<T> {
    /// The text as read from the file.
    Text(String),
    /// The parsed value.
    Value(T),
}

impl<T> Field<T> {
    /// A field holding the text as read.
    pub fn text_of(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }

    /// The text, while the field is unparsed.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            Self::Value(_) => None,
        }
    }

    /// The parsed value, once parsed.
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Text(_) => None,
            Self::Value(v) => Some(v),
        }
    }
}

/// The text of an optional field, while it is unparsed.
pub(crate) fn field_text<T>(field: Option<&Field<T>>) -> Option<&str> {
    field.and_then(Field::text)
}

/// `<qif-file>`: one QIF file as read. `qif-objects.scm:162-235`, `:632-650`.
#[derive(Debug, Clone, PartialEq)]
pub struct QifFile {
    /// Where the file was loaded from, as the caller named it.
    pub path: Option<String>,
    /// Set to 50 and never read by GnuCash: `qif-parse:fix-year` is always called with 50 (`qif-parse.scm:376`).
    pub y2k_threshold: i64,
    /// The transactions, in file order.
    pub xtns: Vec<QifXtn>,
    /// The `!Account` records, newest first.
    pub accounts: VecDeque<QifAcct>,
    /// The `!Type:Cat` records, newest first.
    pub cats: VecDeque<QifCat>,
    /// The `!Type:Class` records, newest first.
    pub classes: VecDeque<QifClass>,
    /// The `!Type:Prices` records, newest first.
    pub prices: VecDeque<QifPrice>,
}

impl Default for QifFile {
    /// `make-qif-file`. `qif-objects.scm:227-235`.
    fn default() -> Self {
        Self {
            path: None,
            y2k_threshold: 50,
            xtns: Vec::new(),
            accounts: VecDeque::new(),
            cats: VecDeque::new(),
            classes: VecDeque::new(),
            prices: VecDeque::new(),
        }
    }
}

impl QifFile {
    /// `qif-file:add-xtn!`. GnuCash conses and reverses the list once reading ends; appending gives the same order.
    /// `qif-objects.scm:632-634`.
    pub fn add_xtn(&mut self, xtn: QifXtn) {
        self.xtns.push(xtn);
    }

    /// `qif-file:add-cat!`, newest first. `qif-objects.scm:636-638`.
    pub fn add_cat(&mut self, cat: QifCat) {
        self.cats.push_front(cat);
    }

    /// `qif-file:add-class!`, newest first. `qif-objects.scm:640-642`.
    pub fn add_class(&mut self, class: QifClass) {
        self.classes.push_front(class);
    }

    /// `qif-file:add-account!`, newest first. `qif-objects.scm:644-646`.
    pub fn add_account(&mut self, account: QifAcct) {
        self.accounts.push_front(account);
    }

    /// `qif-file:add-price!`, newest first. `qif-objects.scm:648-650`.
    pub fn add_price(&mut self, price: QifPrice) {
        self.prices.push_front(price);
    }

    /// `qif-file:path-to-accountname`: a default account name from the file name, the base name without its
    /// extension and with `-` and `_` turned into spaces; "QIF Import" when there is no path.
    /// `qif-objects.scm:657-675`.
    pub fn path_to_accountname(&self) -> String {
        match self.path.as_deref() {
            Some(path) if !path.is_empty() => {
                let start = path.rfind('/').map_or(0, |i| i + 1);
                let end = path.rfind('.').unwrap_or(path.len());
                // Scheme's `substring` raises when the last dot is before the last slash; take nothing then.
                let name = path.get(start..end).unwrap_or("");
                string_replace_char(&string_replace_char(name, '-', ' '), '_', ' ')
            }
            _ => "QIF Import".to_owned(),
        }
    }
}

/// `<qif-split>`: one split of a transaction, or the transaction's own category line. `qif-objects.scm:237-327`.
///
/// GnuCash's comment calls it "for bank/ccard accounts only", but every transaction holds at least one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifSplit {
    /// The category or account name, without the brackets of an account (`L` or `S`).
    pub category: String,
    /// The class after `/`; empty when there is none.
    pub class: String,
    /// The memo (`M` for the transaction's own split, `E` for a split).
    pub memo: Option<String>,
    /// The amount (`T`/`U` for the transaction's own split, `$` for a split).
    pub amount: Option<Field<Numeric>>,
    /// Whether the category was written as `[account]`.
    pub category_is_account: bool,
    /// Set by `qif-to-gnc.scm` from the cleared flag of the transaction this split matches.
    pub matching_cleared: Option<Field<Cleared>>,
    /// Set by `qif-to-gnc.scm` once the split is matched.
    pub mark: bool,
    /// The "miscx" category after `|`, used by investment transfers.
    pub miscx_category: Option<String>,
    /// Whether the miscx category was written as `[account]`.
    pub miscx_is_account: bool,
    /// The class of the miscx category.
    pub miscx_class: Option<String>,
}

impl QifSplit {
    /// `make-qif-split`: every field `#f`, then the category set to `""`. `qif-objects.scm:324-327`.
    pub fn new() -> Self {
        Self {
            category: String::new(),
            class: String::new(),
            memo: None,
            amount: None,
            category_is_account: false,
            matching_cleared: None,
            mark: false,
            miscx_category: None,
            miscx_is_account: false,
            miscx_class: None,
        }
    }

    /// `qif-split:set-category!`: parse a category line and set the category, class and miscx fields from it.
    /// `qif-objects.scm:254-268`.
    pub fn set_category(&mut self, value: &str) -> Result<(), CategoryParseError> {
        let info = parse::parse_category(value)?;
        self.category = info.name;
        self.class = info.class;
        self.category_is_account = info.is_account;
        self.miscx_category = info.miscx_name;
        self.miscx_is_account = info.miscx_is_account;
        self.miscx_class = info.miscx_class;
        Ok(())
    }
}

impl Default for QifSplit {
    fn default() -> Self {
        Self::new()
    }
}

/// `<qif-xtn>`: one transaction. `qif-objects.scm:330-443`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifXtn {
    /// The line its first field was on (red-ale's, for messages).
    pub line: usize,
    /// The line of its `^` (red-ale's, for messages).
    pub last_line: usize,
    /// `D`: the date.
    pub date: Option<Field<QifDate>>,
    /// `P`: the payee.
    pub payee: Option<String>,
    /// `A`: the address lines, each one preceded by a newline (`qif-file.scm:286-292`).
    pub address: Option<String>,
    /// `N` outside investment accounts: the check or transaction number.
    pub number: Option<String>,
    /// `N` in `Invst` and `Port` accounts: the investment action.
    pub action: Option<Field<Action>>,
    /// `C`: the cleared flag.
    pub cleared: Option<Field<Cleared>>,
    /// The name of the account the transaction was read under, if known.
    pub from_acct: Option<String>,
    /// `I`: the share price.
    pub share_price: Option<Field<Numeric>>,
    /// `Q`: the number of shares.
    pub num_shares: Option<Field<Numeric>>,
    /// `Y`: the security name; set to `""` at the end of an investment record without one.
    pub security_name: Option<String>,
    /// `O`: the commission.
    pub commission: Option<Field<Numeric>>,
    /// The transaction's own split (`T`, `L`, `M`) when it also has `S` splits; `None` otherwise.
    pub default_split: Option<QifSplit>,
    /// The splits, newest first. A transaction without `S` lines has one split: its own.
    pub splits: Vec<QifSplit>,
    /// Set by `qif-to-gnc.scm` once the transaction is matched.
    pub mark: bool,
}

impl QifXtn {
    /// `make-qif-xtn`. `qif-objects.scm:439-443`.
    pub fn new(line: usize) -> Self {
        Self {
            line,
            last_line: line,
            date: None,
            payee: None,
            address: None,
            number: None,
            action: None,
            cleared: None,
            from_acct: None,
            share_price: None,
            num_shares: None,
            security_name: None,
            commission: None,
            default_split: None,
            splits: Vec::new(),
            mark: false,
        }
    }

    /// `qif-xtn:split-amounts`: the unparsed amounts of the own split (if the transaction has `S` splits) and then
    /// of each split, `None` where a split has no amount. `qif-objects.scm:449-455`.
    pub fn split_amounts(&self) -> Vec<Option<&str>> {
        self.default_split.iter().chain(&self.splits).map(|s| field_text(s.amount.as_ref())).collect()
    }

    /// `qif-xtn:set-split-amounts!`: set the parsed amounts from [`QifXtn::split_amounts`] onto the splits. With
    /// an own split, its amount is skipped (it stays as text) and the splits' amounts are negated when the own amount
    /// and the split amounts sum to zero, since some programs write splits with the opposite sign.
    /// `qif-objects.scm:457-475`.
    pub fn set_split_amounts(&mut self, amounts: &[Numeric]) {
        let (negate, amounts) = match self.default_split {
            Some(_) => (sums_to_zero(amounts), amounts.get(1..).unwrap_or(&[])),
            None => (false, amounts),
        };
        self.splits.iter_mut().zip(amounts).for_each(|(split, amount)| {
            split.amount = Some(Field::Value(if negate { amount.neg() } else { *amount }));
        });
    }
}

/// `need-neg` of `qif-xtn:set-split-amounts!`: `gnc-numeric-add` with `GNC-DENOM-LCD`, then `gnc-numeric-zero-p`.
/// An overflowing sum is GnuCash's error value, which is not zero. `qif-objects.scm:464-470`.
fn sums_to_zero(amounts: &[Numeric]) -> bool {
    amounts
        .iter()
        .try_fold(Numeric::ZERO, |sum, amount| sum.add(*amount, 0, red_ale_core::How::LCD_NEVER))
        .is_ok_and(Numeric::is_zero)
}

/// `<qif-acct>`: an `!Account` record. `qif-objects.scm:478-530`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifAcct {
    /// The line its first field was on (red-ale's, for messages).
    pub line: usize,
    /// `N`: the account name.
    pub name: String,
    /// `T`: the QIF account type, parsed into the GnuCash types it may map to.
    pub acct_type: Field<Vec<AccountType>>,
    /// `D`: the description.
    pub description: Option<String>,
    /// `L`: the credit limit.
    pub limit: Option<Field<Numeric>>,
    /// `B`: the budget amount.
    pub budget: Option<Field<Numeric>>,
}

impl QifAcct {
    /// `make-qif-acct`: type "Bank", name "Default Account". `qif-objects.scm:520-524`.
    pub fn new(line: usize) -> Self {
        Self {
            line,
            name: "Default Account".to_owned(),
            acct_type: Field::text_of("Bank"),
            description: None,
            limit: None,
            budget: None,
        }
    }
}

/// `<qif-class>`: a `!Type:Class` record. `qif-objects.scm:532-562`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QifClass {
    /// The line its first field was on (red-ale's, for messages).
    pub line: usize,
    /// `N`: the class name.
    pub name: Option<String>,
    /// `D`: the description.
    pub description: Option<String>,
}

/// `<qif-cat>`: a `!Type:Cat` record. `qif-objects.scm:564-630`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QifCat {
    /// The line its first field was on (red-ale's, for messages).
    pub line: usize,
    /// `N`: the category name.
    pub name: Option<String>,
    /// `D`: the description.
    pub description: Option<String>,
    /// `T`: a taxable category.
    pub taxable: bool,
    /// `E`: an expense category.
    pub expense_cat: bool,
    /// `I`: an income category.
    pub income_cat: bool,
    /// `R`: the tax form line, parsed as a number.
    pub tax_class: Option<Field<Numeric>>,
    /// `B`: the budget amount ("not really supported").
    pub budget_amt: Option<Field<Numeric>>,
}

/// `<qif-stock-symbol>`: a `!Type:Security` record. `qif-objects.scm:772-810`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QifStockSymbol {
    /// `N`: the security's name, as transactions name it in `Y`.
    pub name: String,
    /// `S`: the ticker symbol.
    pub symbol: String,
    /// `T`: the security type, such as "Stock" or "Mutual Fund".
    pub security_type: String,
}

/// `<qif-price>`: one line of a `!Type:Prices` list. `qif-objects.scm:812-848`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifPrice {
    /// The line it was on (red-ale's, for messages).
    pub line: usize,
    /// The ticker symbol.
    pub symbol: String,
    /// The price per share.
    pub share_price: Field<Numeric>,
    /// The date.
    pub date: Field<QifDate>,
}

/// `<qif-ticker-map>`: the securities of every file loaded in one import, which GnuCash shares across files.
/// `qif-objects.scm:850-893`.
///
/// GnuCash keeps the list newest first and its lookups walk all of it, so the *oldest* match wins. This list is in
/// the order added, and lookups return the first match: the same answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TickerMap {
    stocks: Vec<QifStockSymbol>,
}

impl TickerMap {
    /// `qif-ticker-map:add-ticker!`. `qif-objects.scm:866-869`.
    pub fn add_ticker(&mut self, stock: QifStockSymbol) {
        self.stocks.push(stock);
    }

    /// The securities, in the order added.
    pub fn stocks(&self) -> &[QifStockSymbol] {
        &self.stocks
    }

    /// `qif-ticker-map:lookup-symbol`: the ticker of the security named `name`, `None` when unknown or empty.
    /// `qif-objects.scm:871-881`.
    pub fn lookup_symbol(&self, name: &str) -> Option<&str> {
        self.lookup(name).map(|s| s.symbol.as_str()).filter(|s| !s.is_empty())
    }

    /// `qif-ticker-map:lookup-type`: the type of the security named `name`, `None` when unknown or empty.
    /// `qif-objects.scm:883-893`.
    pub fn lookup_type(&self, name: &str) -> Option<&str> {
        self.lookup(name).map(|s| s.security_type.as_str()).filter(|s| !s.is_empty())
    }

    fn lookup(&self, name: &str) -> Option<&QifStockSymbol> {
        self.stocks.iter().find(|s| s.name == name)
    }
}

/// `<qif-map-entry>`: how one QIF account or category name maps to a GnuCash account. Filled by the translations
/// of `qif-dialog-utils.scm` and `qif-guess-map.scm`. `qif-objects.scm:678-770`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QifMapEntry {
    /// The QIF name, set while parsing the file.
    pub qif_name: Option<String>,
    /// The GnuCash account types the name may map to, set while parsing the file.
    pub allowed_types: Vec<AccountType>,
    /// The description, from the QIF account if there is one.
    pub description: Option<String>,
    /// The GnuCash account's full name, set from the guess map.
    pub gnc_name: Option<String>,
    /// Whether the GnuCash account is new, set from the guess map.
    pub new_acct: bool,
    /// Whether to show the entry: set when it has non-zero transactions.
    pub display: bool,
}

/// The balance-sheet types that may hold one another, in GnuCash's order. `qif-objects.scm:719-726`.
const BALANCE_SHEET_TYPES: [AccountType; 9] = [
    AccountType::Bank,
    AccountType::Cash,
    AccountType::Credit,
    AccountType::Stock,
    AccountType::Mutual,
    AccountType::Asset,
    AccountType::Liability,
    AccountType::Receivable,
    AccountType::Payable,
];

impl QifMapEntry {
    /// `qif-map-entry:allowed-parent-types`: the allowed types, then every type that may be the parent of one of
    /// them, without repeats. `qif-objects.scm:707-732`.
    pub fn allowed_parent_types(&self) -> Vec<AccountType> {
        let mut types: Vec<AccountType> = Vec::new();
        let mut add = |t: AccountType| {
            if !types.contains(&t) {
                types.push(t);
            }
        };
        self.allowed_types.iter().for_each(|t| add(*t));
        self.allowed_types.iter().for_each(|t| match t {
            t if BALANCE_SHEET_TYPES.contains(t) => BALANCE_SHEET_TYPES.iter().for_each(|b| add(*b)),
            AccountType::Income | AccountType::Expense => [AccountType::Income, AccountType::Expense].into_iter().for_each(&mut add),
            t => add(*t),
        });
        types
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_match_gnucash_defaults() {
        // test-qif-imp.scm:54-59 only checks that the constructors run; these are their field values.
        let file = QifFile::default();
        assert_eq!(file.y2k_threshold, 50);
        assert!(file.xtns.is_empty() && file.accounts.is_empty() && file.prices.is_empty());
        let acct = QifAcct::new(1);
        assert_eq!(acct.name, "Default Account");
        assert_eq!(acct.acct_type, Field::text_of("Bank"));
        assert_eq!(QifSplit::new().category, "");
        assert!(TickerMap::default().stocks().is_empty());
    }

    #[test]
    fn path_to_accountname() {
        let named = |p: &str| QifFile { path: Some(p.to_owned()), ..QifFile::default() }.path_to_accountname();
        assert_eq!(named("/home/me/my-bank_account.qif"), "my bank account");
        assert_eq!(named("plain"), "plain");
        assert_eq!(named("dir.v2/file"), "");
        assert_eq!(QifFile::default().path_to_accountname(), "QIF Import");
    }

    #[test]
    fn ticker_lookups_take_the_first_added() {
        let mut map = TickerMap::default();
        let stock = |name: &str, symbol: &str| QifStockSymbol { name: name.into(), symbol: symbol.into(), security_type: String::new() };
        map.add_ticker(stock("Acme", "ACM"));
        map.add_ticker(stock("Acme", "ACX"));
        map.add_ticker(stock("Blank", ""));
        assert_eq!(map.lookup_symbol("Acme"), Some("ACM"));
        assert_eq!(map.lookup_symbol("Blank"), None);
        assert_eq!(map.lookup_type("Acme"), None);
    }

    #[test]
    fn allowed_parent_types() {
        let entry = QifMapEntry { allowed_types: vec![AccountType::Expense, AccountType::Equity], ..QifMapEntry::default() };
        assert_eq!(entry.allowed_parent_types(), vec![AccountType::Expense, AccountType::Equity, AccountType::Income]);
        let bank = QifMapEntry { allowed_types: vec![AccountType::Cash], ..QifMapEntry::default() };
        assert_eq!(bank.allowed_parent_types()[..2], [AccountType::Cash, AccountType::Bank]);
        assert_eq!(bank.allowed_parent_types().len(), 9);
    }

    #[test]
    fn split_amounts_negate_when_they_cancel_the_total() {
        let mut xtn = QifXtn::new(1);
        xtn.default_split = Some(QifSplit::new());
        xtn.splits = vec![QifSplit::new(), QifSplit::new()];
        let n = |v| Numeric::from_int(v);
        xtn.set_split_amounts(&[n(-10), n(4), n(6)]);
        let got: Vec<_> = xtn.splits.iter().map(|s| s.amount.clone()).collect();
        assert_eq!(got, vec![Some(Field::Value(n(-4))), Some(Field::Value(n(-6)))]);
        xtn.set_split_amounts(&[n(10), n(4), n(6)]);
        assert_eq!(xtn.splits[0].amount, Some(Field::Value(n(4))));
    }
}
