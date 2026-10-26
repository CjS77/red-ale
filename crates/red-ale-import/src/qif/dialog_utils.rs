//! The non-GUI parts of the QIF assistant's helpers: default account names, the account, category and payee maps
//! built from the files with their guesses, and the securities to create.
//!
//! Translated from `5.17:gnucash/import-export/qif-imp/qif-dialog-utils.scm` ("build qif->gnc account maps and put
//! them in a displayable form"), Bill Gribble <grib@billgribble.com> 20 Feb 2000. GPL-2.0-or-later; this
//! translation is GPL-3.0-or-later.
//!
//! The default names are in English: GnuCash translates them (`G_`), red-ale has no translations
//! (`design/api/import.md`, deviations). GnuCash sorts the maps it shows with the locale's collation; red-ale sorts
//! bytewise. GnuCash keeps the maps in hash tables; red-ale's are ordered by QIF name, so every walk over them is
//! deterministic. What `qif-dialog:qif-file-loaded?`, `unload-qif-file`, `any-new-accts?`, `get-all-accts` and
//! `refresh-match-selection` do belongs to the assistant's pages and is not translated.

use std::collections::BTreeMap;

use red_ale_core::{AccountType, CommodityRef};

use super::guess_map::{GncAcctInfo, MapHash, guess_acct};
use super::objects::{QifFile, QifMapEntry, QifXtn, TickerMap};
use super::parse::Action;

/// The account separator, `gnc-get-account-separator-string`.
pub const SEP: &str = ":";

/// `default-stock-acct`: the account of a security in a brokerage account. `qif-dialog-utils.scm:66-67`.
pub fn default_stock_acct(brokerage: &str, security: &str) -> String {
    format!("{brokerage}{SEP}{security}")
}

/// `default-dividend-acct`. `qif-dialog-utils.scm:69-73`.
pub fn default_dividend_acct(brokerage: &str, security: &str) -> String {
    format!("Income{SEP}Dividends{SEP}{brokerage}{SEP}{security}")
}

/// `default-interest-acct`: without a security, the brokerage's interest account. `qif-dialog-utils.scm:75-82`.
pub fn default_interest_acct(brokerage: &str, security: &str) -> String {
    let security = if security.is_empty() { String::new() } else { format!("{SEP}{security}") };
    format!("Income{SEP}Interest{SEP}{brokerage}{security}")
}

/// `default-capital-return-acct`. `qif-dialog-utils.scm:84-88`.
pub fn default_capital_return_acct(brokerage: &str, security: &str) -> String {
    format!("Income{SEP}Cap Return{SEP}{brokerage}{SEP}{security}")
}

/// `default-cglong-acct`. `qif-dialog-utils.scm:90-94`.
pub fn default_cglong_acct(brokerage: &str, security: &str) -> String {
    format!("Income{SEP}Cap. gain (long){SEP}{brokerage}{SEP}{security}")
}

/// `default-cgmid-acct`. `qif-dialog-utils.scm:96-100`.
pub fn default_cgmid_acct(brokerage: &str, security: &str) -> String {
    format!("Income{SEP}Cap. gain (mid){SEP}{brokerage}{SEP}{security}")
}

/// `default-cgshort-acct`. `qif-dialog-utils.scm:102-106`.
pub fn default_cgshort_acct(brokerage: &str, security: &str) -> String {
    format!("Income{SEP}Cap. gain (short){SEP}{brokerage}{SEP}{security}")
}

/// `default-equity-holding`: whatever the security, the retained earnings account. `qif-dialog-utils.scm:108-110`.
pub fn default_equity_holding(_security: &str) -> String {
    default_equity_account()
}

/// `default-equity-account`. `qif-dialog-utils.scm:112-114`.
pub fn default_equity_account() -> String {
    format!("Equity{SEP}Retained Earnings")
}

/// `default-commission-acct`. `qif-dialog-utils.scm:116-119`.
pub fn default_commission_acct(brokerage: &str) -> String {
    format!("Expenses{SEP}Commissions{SEP}{brokerage}")
}

/// `default-margin-interest-acct`. `qif-dialog-utils.scm:121-124`.
pub fn default_margin_interest_acct(brokerage: &str) -> String {
    format!("Expenses{SEP}Margin Interest{SEP}{brokerage}")
}

/// `default-unspec-acct`. `qif-dialog-utils.scm:126-127`.
pub fn default_unspec_acct() -> &'static str {
    "Unspecified"
}

/// `qif-import:actions-implicit`: the investment actions that name both their accounts. `qif-dialog-utils.scm:131-134`.
pub const ACTIONS_IMPLICIT: [Action; 19] = [
    Action::Buy,
    Action::CgLong,
    Action::CgMid,
    Action::CgShort,
    Action::Div,
    Action::IntInc,
    Action::MargInt,
    Action::ReinvDiv,
    Action::ReinvInt,
    Action::ReinvLg,
    Action::ReinvMd,
    Action::ReinvSg,
    Action::ReinvSh,
    Action::Reminder,
    Action::RtrnCap,
    Action::Sell,
    Action::ShrsIn,
    Action::ShrsOut,
    Action::StkSplit,
];

/// The balance-sheet types a cash account may have, in GnuCash's order.
const CASH_TYPES: [AccountType; 7] = [
    AccountType::Bank,
    AccountType::Credit,
    AccountType::Cash,
    AccountType::Asset,
    AccountType::Liability,
    AccountType::Receivable,
    AccountType::Payable,
];

/// A transaction's parsed action.
pub(crate) fn action(xtn: &QifXtn) -> Option<Action> {
    xtn.action.as_ref().and_then(|f| f.value()).copied()
}

/// The first split of a transaction in GnuCash's list: `(car (qif-xtn:splits xtn))`.
pub(crate) fn first_split(xtn: &QifXtn) -> Option<&super::objects::QifSplit> {
    xtn.splits.first()
}

/// A split's amount, zero when unparsed or absent.
pub(crate) fn split_amount(split: &super::objects::QifSplit) -> red_ale_core::Numeric {
    split.amount.as_ref().and_then(|f| f.value()).copied().unwrap_or(red_ale_core::Numeric::ZERO)
}

/// `qif-import:gnc-account-exists`: an account of this full name exists. `qif-dialog-utils.scm:137-145`.
fn gnc_account_exists(entry: &QifMapEntry, info: &[GncAcctInfo]) -> bool {
    entry.gnc_name.as_deref().is_some_and(|name| info.iter().any(|a| a.full_name == name))
}

/// Look a name up, guessing an entry when there is none, and show it. The `(if (not entry) (guess-acct …))`,
/// `(set-display?! entry #t)` and `(hash-set! …)` that recur through `qif-dialog-utils.scm`.
fn reference(hash: &mut MapHash, name: &str, types: &[AccountType], info: &[GncAcctInfo]) {
    let entry = hash.entry(name.to_owned()).or_insert_with(|| guess_acct(name, types, info));
    entry.display = true;
}

/// The display list: the shown entries with `new-acct?` set, sorted by QIF name. Returns their keys.
fn display_list(hash: &mut MapHash, info: &[GncAcctInfo]) -> Vec<String> {
    let mut shown: Vec<(String, String)> = hash
        .iter_mut()
        .filter(|(_, e)| e.display)
        .map(|(k, e)| {
            e.new_acct = !gnc_account_exists(e, info);
            (e.qif_name.clone().unwrap_or_default(), k.clone())
        })
        .collect();
    shown.sort();
    shown.into_iter().map(|(_, k)| k).collect()
}

fn clear_display(hash: &mut MapHash) {
    hash.values_mut().for_each(|e| e.display = false);
}

/// `qif-dialog:make-account-display`: the QIF accounts the files name, from their account records first, then from
/// their transactions, guessed where the map has none. Returns the keys shown. `qif-dialog-utils.scm:153-454`.
pub fn make_account_display(files: &[QifFile], acct_hash: &mut MapHash, info: &[GncAcctInfo]) -> Vec<String> {
    clear_display(acct_hash);
    files.iter().flat_map(|f| &f.accounts).for_each(|acct| {
        let types = acct.acct_type.value().cloned().unwrap_or_default();
        let entry = acct_hash.entry(acct.name.clone()).or_insert_with(|| guess_acct(&acct.name, &types, info));
        entry.description.clone_from(&acct.description);
    });
    files.iter().flat_map(|f| &f.xtns).for_each(|xtn| account_display_xtn(xtn, acct_hash, info));
    display_list(acct_hash, info)
}

/// The second pass of `make-account-display`, for one transaction. `qif-dialog-utils.scm:196-432`.
fn account_display_xtn(xtn: &QifXtn, acct_hash: &mut MapHash, info: &[GncAcctInfo]) {
    let from_acct = xtn.from_acct.as_deref().unwrap_or_default();
    if let (Some(security), Some(action)) = (xtn.security_name.as_deref(), action(xtn)) {
        if let Some((name, types)) = stock_near_end(xtn, action, from_acct, security) {
            reference(acct_hash, &name, &types, info);
        }
        if let Some((name, types)) = stock_far_end(xtn, action, from_acct, security) {
            reference(acct_hash, &name, &types, info);
        }
        if xtn.commission.is_some() {
            reference(acct_hash, &default_commission_acct(from_acct), &[AccountType::Expense], info);
        }
    } else {
        reference(acct_hash, from_acct, &CASH_TYPES, info);
        xtn.splits.iter().filter(|s| s.category_is_account).for_each(|s| reference(acct_hash, &s.category, &CASH_TYPES, info));
    }
}

/// The category of the first split when it names an account: `(and (category-is-account? …) (category …))`.
fn first_account_category(xtn: &QifXtn) -> Option<String> {
    first_split(xtn).filter(|s| s.category_is_account).map(|s| s.category.clone())
}

/// The near end of a stock transaction: generally the security's account, but an income or cash account for some
/// actions. `qif-dialog-utils.scm:215-256`.
fn stock_near_end(xtn: &QifXtn, action: Action, from_acct: &str, security: &str) -> Option<(String, Vec<AccountType>)> {
    use Action as A;
    match action {
        A::Buy | A::BuyX | A::Sell | A::SellX | A::ReinvInt | A::ReinvDiv | A::ReinvSh | A::ReinvSg | A::ReinvLg | A::ReinvMd | A::ShrsIn
        | A::ShrsOut | A::StkSplit => Some((default_stock_acct(from_acct, security), vec![AccountType::Stock, AccountType::Mutual])),
        A::Div | A::CgShort | A::CgMid | A::CgLong | A::IntInc | A::MiscInc | A::MiscExp | A::MargInt | A::RtrnCap | A::XIn | A::XOut => {
            Some((from_acct.to_owned(), CASH_TYPES.to_vec()))
        }
        A::DivX | A::CgShortX | A::CgMidX | A::CgLongX | A::IntIncX | A::MargIntX | A::RtrnCapX => {
            first_account_category(xtn).map(|name| (name, CASH_TYPES.to_vec()))
        }
        A::MiscIncX | A::MiscExpX => first_split(xtn).and_then(|s| s.miscx_category.clone()).map(|name| (name, CASH_TYPES.to_vec())),
        _ => None,
    }
}

/// The far end of a stock transaction: the brokerage, the `L` account, or an income, expense or equity account.
/// `qif-dialog-utils.scm:283-370`.
fn stock_far_end(xtn: &QifXtn, action: Action, from_acct: &str, security: &str) -> Option<(String, Vec<AccountType>)> {
    use Action as A;
    let income = |name: String| Some((name, vec![AccountType::Income]));
    match action {
        A::Buy | A::Sell => Some((from_acct.to_owned(), CASH_TYPES.to_vec())),
        A::BuyX | A::SellX | A::XIn | A::XOut => first_account_category(xtn).map(|name| (name, CASH_TYPES.to_vec())),
        A::StkSplit => {
            let types = vec![AccountType::Stock, AccountType::Mutual, AccountType::Asset, AccountType::Liability, AccountType::Receivable, AccountType::Payable];
            Some((default_stock_acct(from_acct, security), types))
        }
        A::CgShort | A::CgShortX | A::ReinvSg | A::ReinvSh => income(default_cgshort_acct(from_acct, security)),
        A::CgMid | A::CgMidX | A::ReinvMd => income(default_cgmid_acct(from_acct, security)),
        A::CgLong | A::CgLongX | A::ReinvLg => income(default_cglong_acct(from_acct, security)),
        A::IntInc | A::IntIncX | A::ReinvInt => income(default_interest_acct(from_acct, security)),
        A::Div | A::DivX | A::ReinvDiv => income(default_dividend_acct(from_acct, security)),
        A::RtrnCap | A::RtrnCapX => income(default_capital_return_acct(from_acct, security)),
        A::ShrsIn | A::ShrsOut => Some((default_equity_holding(security), vec![AccountType::Equity])),
        A::MargInt | A::MargIntX => Some((default_margin_interest_acct(from_acct), vec![AccountType::Expense])),
        _ => None,
    }
}

/// Whether the transaction's action names both its accounts, so its categories and memos are not looked at.
fn implicit(xtn: &QifXtn) -> bool {
    action(xtn).is_some_and(|a| ACTIONS_IMPLICIT.contains(&a))
}

/// `qif-dialog:make-category-display`: the categories the files name, from their category records, then from
/// their transactions, guessed where the map has none. Returns the keys shown. `qif-dialog-utils.scm:460-574`.
pub fn make_category_display(files: &[QifFile], cat_hash: &mut MapHash, info: &[GncAcctInfo]) -> Vec<String> {
    clear_display(cat_hash);
    files.iter().flat_map(|f| &f.cats).for_each(|cat| {
        // GnuCash fails on a category record with no name; red-ale passes over it.
        let Some(name) = cat.name.as_deref() else { return };
        let types = if cat.expense_cat { [AccountType::Expense, AccountType::Income] } else { [AccountType::Income, AccountType::Expense] };
        let entry = cat_hash.entry(name.to_owned()).or_insert_with(|| guess_acct(name, &types, info));
        entry.description.clone_from(&cat.description);
    });
    files.iter().flat_map(|f| &f.xtns).filter(|x| !implicit(x)).for_each(|xtn| {
        xtn.splits.iter().filter(|s| !s.category_is_account).for_each(|split| {
            let cat = split.category.as_str();
            let wanted = !cat.is_empty()
                || (split.memo.as_deref().is_none_or(str::is_empty) && (xtn.splits.len() > 1 || xtn.payee.as_deref().is_none_or(str::is_empty)));
            if !cat_hash.contains_key(cat) && !wanted {
                return;
            }
            let types = if split_amount(split).is_positive() { [AccountType::Income, AccountType::Expense] } else { [AccountType::Expense, AccountType::Income] };
            reference(cat_hash, cat, &types, info);
        });
    });
    display_list(cat_hash, info)
}

/// The types a payee or memo entry allows, by the sign of the amount. `qif-dialog-utils.scm:630-643`.
fn memo_types(positive: bool) -> Vec<AccountType> {
    use AccountType as T;
    let first = if positive { [T::Income, T::Expense] } else { [T::Expense, T::Income] };
    first.into_iter().chain([T::Bank, T::Credit, T::Liability, T::Asset, T::Receivable, T::Payable, T::Stock, T::Mutual]).collect()
}

/// `qif-dialog:make-memo-display`: the payees (of transactions with one split) and memos of splits with no
/// category. A new entry maps to `Unspecified`; one for an empty name is kept under `Unspecified`.
/// `qif-dialog-utils.scm:579-666`.
pub fn make_memo_display(files: &[QifFile], memo_hash: &mut MapHash, info: &[GncAcctInfo]) -> Vec<String> {
    clear_display(memo_hash);
    files.iter().flat_map(|f| &f.xtns).filter(|x| !implicit(x)).for_each(|xtn| {
        xtn.splits.iter().for_each(|split| {
            let key = if !split.category.is_empty() {
                None
            } else if xtn.payee.is_some() && xtn.splits.len() == 1 {
                xtn.payee.clone()
            } else {
                split.memo.clone()
            };
            let Some(key) = key else { return };
            let stored = if memo_hash.contains_key(&key) {
                key
            } else {
                // A new entry for an empty name replaces whatever is kept under `Unspecified` (`:620-627`, `:645`).
                let stored = if key.is_empty() { default_unspec_acct().to_owned() } else { key.clone() };
                let entry = QifMapEntry {
                    qif_name: Some(key),
                    allowed_types: memo_types(split_amount(split).is_positive()),
                    gnc_name: Some(default_unspec_acct().to_owned()),
                    ..QifMapEntry::default()
                };
                memo_hash.insert(stored.clone(), entry);
                stored
            };
            memo_hash.entry(stored).and_modify(|e| e.display = true);
        });
    });
    display_list(memo_hash, info)
}

/// `qif-import:fix-from-acct`: the account for the transactions read under none, which the user names when a file
/// has no `!Account` record. `qif-dialog-utils.scm:694-699`.
pub fn fix_from_acct(file: &mut QifFile, new_acct_name: &str) {
    file.xtns.iter_mut().filter(|x| x.from_acct.is_none()).for_each(|x| x.from_acct = Some(new_acct_name.to_owned()));
}

/// `qif-import:get-account-name`: the last part of a full name. `qif-dialog-utils.scm:710-715`.
pub fn get_account_name(fullname: &str) -> &str {
    fullname.rfind(SEP).map_or(fullname, |i| &fullname[i + SEP.len()..])
}

/// The namespace of securities with no better guess: `GNC_COMMODITY_NS_NONCURRENCY`
/// (`5.17:libgnucash/engine/gnc-commodity.h:108`).
pub const NONCURRENCY: &str = "NONCURRENCY";

fn currency_ns(ns: &str) -> bool {
    ["CURRENCY", "GNC_LEGACY_CURRENCIES", "ISO4217"].contains(&ns)
}

/// `qif-dialog:default-namespace`: the namespace of a saved security with this symbol, unless it is a currency
/// namespace and the QIF type is a stock, ETF, mutual fund or index; else `NONCURRENCY`.
/// `qif-dialog-utils.scm:725-759`.
pub fn default_namespace(qif_symbol: Option<&str>, qif_type: Option<&str>, prefs: &[(String, String, String)]) -> String {
    let Some(symbol) = qif_symbol else { return NONCURRENCY.to_owned() };
    let security_type = |t: &str| ["stock", "etf", "mutual fund", "index"].iter().any(|s| t.eq_ignore_ascii_case(s));
    prefs
        .iter()
        .find(|(_, ns, mnemonic)| mnemonic == symbol && (!qif_type.is_some_and(security_type) || currency_ns(ns)))
        .map_or_else(|| NONCURRENCY.to_owned(), |(_, ns, _)| ns.clone())
}

/// A QIF security's commodity: one the Book has, or one to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityChoice {
    /// An existing commodity.
    Existing(CommodityRef),
    /// A commodity to create, unless the Book has one of this namespace and mnemonic by then
    /// (`gnc_ui_qif_import_commodity_update`, `5.17:gnucash/import-export/qif-imp/assistant-qif-import.c:1070-1110`).
    New {
        /// Namespace and mnemonic.
        id: CommodityRef,
        /// Full name.
        fullname: String,
    },
}

impl SecurityChoice {
    /// The commodity's namespace and mnemonic.
    pub fn id(&self) -> &CommodityRef {
        match self {
            Self::Existing(id) | Self::New { id, .. } => id,
        }
    }
}

/// The fraction of a new QIF security. `qif-dialog-utils.scm:821-828`.
pub const SECURITY_FRACTION: i64 = 100_000;

/// `qif-import:update-security-hash`: for each shown account of a security type whose security is not yet known, the
/// commodity of the existing account of that full name if it holds a security, else a new one named after the
/// security with the ticker map's symbol (or the name) and [`default_namespace`]. Returns the new securities' names,
/// sorted. `qif-dialog-utils.scm:771-837`.
pub fn update_security_hash(
    security_hash: &mut BTreeMap<String, SecurityChoice>,
    ticker_map: &TickerMap,
    acct_hash: &MapHash,
    sec_prefs: &[(String, String, String)],
    existing: impl Fn(&str) -> Option<(AccountType, Option<CommodityRef>)>,
) -> Vec<String> {
    let mut names = Vec::new();
    acct_hash.iter().for_each(|(qif_name, entry)| {
        let security_name = get_account_name(qif_name);
        let stock = entry.allowed_types.iter().any(|t| matches!(t, AccountType::Stock | AccountType::Mutual));
        if !entry.display || !stock || security_hash.contains_key(security_name) {
            return;
        }
        let held = entry.gnc_name.as_deref().and_then(&existing).filter(|(t, _)| matches!(t, AccountType::Stock | AccountType::Mutual));
        let choice = if let Some(commodity) = held.and_then(|(_, c)| c) {
            SecurityChoice::Existing(commodity)
        } else {
            let symbol = ticker_map.lookup_symbol(security_name);
            let namespace = default_namespace(symbol, ticker_map.lookup_type(security_name), sec_prefs);
            names.push(security_name.to_owned());
            SecurityChoice::New { id: CommodityRef::new(namespace, symbol.unwrap_or(security_name)), fullname: security_name.to_owned() }
        };
        security_hash.insert(security_name.to_owned(), choice);
    });
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qif::objects::{QifStockSymbol, QifSplit};
    use crate::qif::{Field, QifXtn};

    #[test]
    fn default_names() {
        assert_eq!(default_stock_acct("Broker", "Acme"), "Broker:Acme");
        assert_eq!(default_interest_acct("Broker", ""), "Income:Interest:Broker");
        assert_eq!(default_interest_acct("Broker", "Acme"), "Income:Interest:Broker:Acme");
        assert_eq!(default_cgshort_acct("B", "S"), "Income:Cap. gain (short):B:S");
        assert_eq!(default_commission_acct("B"), "Expenses:Commissions:B");
        assert_eq!(get_account_name("Assets:Broker:Acme"), "Acme");
        assert_eq!(get_account_name("Acme"), "Acme");
    }

    #[test]
    fn namespaces_from_saved_securities() {
        let prefs = [("McD".to_owned(), "NYSE".to_owned(), "MCD".to_owned()), ("Euro".to_owned(), "CURRENCY".to_owned(), "EUR".to_owned())];
        assert_eq!(default_namespace(Some("MCD"), None, &prefs), "NYSE");
        assert_eq!(default_namespace(Some("MCD"), Some("Bond"), &prefs), "NYSE");
        // The code, unlike its comment (`:740-748`), passes over a saved security of a stock-like type whose
        // namespace is *not* a currency one.
        assert_eq!(default_namespace(Some("MCD"), Some("Stock"), &prefs), NONCURRENCY);
        assert_eq!(default_namespace(Some("EUR"), Some("stock"), &prefs), "CURRENCY");
        assert_eq!(default_namespace(None, None, &prefs), NONCURRENCY);
    }

    fn xtn(from: &str, payee: Option<&str>, category: &str, amount: i64) -> QifXtn {
        let mut x = QifXtn::new(1);
        x.from_acct = Some(from.to_owned());
        x.payee = payee.map(str::to_owned);
        let mut split = QifSplit::new();
        split.category = category.to_owned();
        split.amount = Some(Field::Value(red_ale_core::Numeric::from_int(amount)));
        x.splits = vec![split];
        x
    }

    #[test]
    fn displays_guess_categories_and_payees() {
        let file = QifFile { xtns: vec![xtn("Checking", Some("Shop"), "Food", -5), xtn("Checking", Some("Shell"), "", -9)], ..QifFile::default() };
        let files = [file];
        let mut accounts = MapHash::new();
        assert_eq!(make_account_display(&files, &mut accounts, &[]), ["Checking"]);
        assert_eq!(accounts["Checking"].allowed_types, CASH_TYPES);
        let mut cats = MapHash::new();
        assert_eq!(make_category_display(&files, &mut cats, &[]), ["Food"]);
        assert_eq!(cats["Food"].allowed_types, [AccountType::Expense, AccountType::Income]);
        let mut payees = MapHash::new();
        assert_eq!(make_memo_display(&files, &mut payees, &[]), ["Shell"]);
        assert_eq!(payees["Shell"].gnc_name.as_deref(), Some("Unspecified"));
        assert!(payees["Shell"].new_acct);
    }

    #[test]
    fn securities_are_new_unless_an_account_holds_one() {
        let mut acct_hash = MapHash::new();
        let entry = |name: &str| QifMapEntry {
            qif_name: Some(name.into()),
            allowed_types: vec![AccountType::Stock, AccountType::Mutual],
            gnc_name: Some(name.into()),
            display: true,
            ..QifMapEntry::default()
        };
        acct_hash.insert("Broker:Acme".into(), entry("Broker:Acme"));
        acct_hash.insert("Broker:Held".into(), entry("Broker:Held"));
        let mut tickers = TickerMap::default();
        tickers.add_ticker(QifStockSymbol { name: "Acme".into(), symbol: "ACM".into(), security_type: "Stock".into() });
        let mut hash = BTreeMap::new();
        let existing = |name: &str| (name == "Broker:Held").then(|| (AccountType::Stock, Some(CommodityRef::new("NASDAQ", "HLD"))));
        let names = update_security_hash(&mut hash, &tickers, &acct_hash, &[], existing);
        assert_eq!(names, ["Acme"]);
        assert_eq!(hash["Acme"], SecurityChoice::New { id: CommodityRef::new(NONCURRENCY, "ACM"), fullname: "Acme".into() });
        assert_eq!(hash["Held"], SecurityChoice::Existing(CommodityRef::new("NASDAQ", "HLD")));
    }
}
