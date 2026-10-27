//! Domain operations: the commodity table, the security editor and the currencies' quote flag.
//!
//! The table a caller sees is the book's own commodities plus every built-in ISO 4217 currency, as GnuCash loads the
//! ISO table into every book (`5.17:libgnucash/engine/gnc-commodity.cpp:2317-2327`). Only commodities GnuCash would
//! write are stored in [`Book::commodities`]; a built-in currency enters it when it gains a quote flag or slots.
//! See `design/notes/engine-commodities-prices-lots.md` §1.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::commodity::{CURRENCY_NAMESPACE, LEGACY_CURRENCY_NAMESPACE, TEMPLATE_NAMESPACE};
use crate::currencies::{canonical_code, iso_currency};
use crate::{Book, Commodity, CommodityRef, DomainError, DomainResult, Guid, SlotValue};

/// The largest fraction the security editor accepts (`GNC_COMMODITY_MAX_FRACTION`, `5.17:libgnucash/engine/gnc-commodity.h:116`).
pub const MAX_FRACTION: i64 = 1_000_000_000;
/// The namespace GnuCash's file I/O uses for old currencies; `gnc_commodity_is_currency` is true for it
/// (`5.17:libgnucash/engine/gnc-commodity.cpp:1953`).
pub const LEGACY_CURRENCIES_NAMESPACE: &str = "GNC_LEGACY_CURRENCIES";
/// The quote source GnuCash gives currencies (`5.17:libgnucash/engine/gnc-commodity.cpp:180`).
pub const CURRENCY_QUOTE_SOURCE: &str = "currency";

/// Slot paths on a commodity.
pub mod slot_paths {
    /// The user's display symbol (`5.17:libgnucash/engine/gnc-commodity.cpp:1355`).
    pub const USER_SYMBOL: &str = "user_symbol";
    /// `"false"` when the user overrode a currency's automatic quote flag (`5.17:libgnucash/engine/gnc-commodity.cpp:1015`).
    pub const AUTO_QUOTE_CONTROL: &str = "auto_quote_control";
}

/// The reference GnuCash's table resolves `id` to: `ISO4217` is already `CURRENCY`, and renamed ISO codes map to
/// their replacements (`5.17:libgnucash/engine/gnc-commodity.cpp:131-145`, applied at `:1650`).
pub fn canonical(id: &CommodityRef) -> CommodityRef {
    if id.is_currency() {
        CommodityRef::currency(canonical_code(&id.mnemonic))
    } else {
        id.clone()
    }
}

/// The commodity `id` names: the book's own definition, else a built-in currency.
pub fn lookup<'a>(book: &'a Book, id: &CommodityRef) -> Option<Cow<'a, Commodity>> {
    let id = canonical(id);
    match book.commodity(&id) {
        Some(c) => Some(Cow::Borrowed(c)),
        None if id.is_currency() => iso_currency(&id.mnemonic).map(|iso| Cow::Owned(iso.to_commodity())),
        None => None,
    }
}

/// The commodity `id` names, or `NotFound`. Returns the canonical reference.
pub fn require(book: &Book, id: &CommodityRef) -> DomainResult<CommodityRef> {
    lookup(book, id)
        .map(|c| c.id.clone())
        .ok_or_else(|| DomainError::not_found("commodity", id))
}

/// The Display symbol GnuCash prints amounts with (`gnc_commodity_get_nice_symbol`,
/// `5.17:libgnucash/engine/gnc-commodity.cpp:1094-1115`): the user symbol, else a currency's default symbol, else the
/// mnemonic. GnuCash's locale step compares the mnemonic with `int_curr_symbol`, which always ends in a space
/// (`5.17:libgnucash/core-utils/gnc-locale-utils.c:89`, `:124-127`), so it never applies. `None` when `id` names no
/// commodity.
pub fn display_symbol(book: &Book, id: &CommodityRef) -> Option<String> {
    let c = lookup(book, id)?;
    let default =
        c.id.is_currency()
            .then(|| iso_currency(&c.id.mnemonic).map(|iso| iso.symbol))
            .flatten();
    let symbol = c
        .slots
        .get_str(slot_paths::USER_SYMBOL)
        .filter(|s| !s.is_empty())
        .or(default.filter(|s| !s.is_empty()));
    Some(symbol.unwrap_or(&c.id.mnemonic).to_owned())
}

/// True for a commodity in the `CURRENCY` namespace: GnuCash's `gnc_commodity_is_iso`.
pub fn is_iso_namespace(namespace: &str) -> bool {
    namespace == CURRENCY_NAMESPACE || namespace == LEGACY_CURRENCY_NAMESPACE
}

/// Every commodity in the table, sorted by namespace then mnemonic: the book's own and the built-in currencies.
pub fn table(book: &Book) -> Vec<Cow<'_, Commodity>> {
    let builtin = crate::currencies::iso_currencies()
        .map(|iso| CommodityRef::currency(iso.code))
        .filter(|id| book.commodity(id).is_none())
        .filter_map(|id| lookup(book, &id));
    let mut all: Vec<Cow<'_, Commodity>> = book
        .commodities
        .values()
        .map(|c| Cow::Borrowed(c.as_ref()))
        .chain(builtin)
        .collect();
    all.sort_by(|a, b| a.id.cmp(&b.id));
    all
}

/// How much of the book refers to a commodity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Accounts holding it: GnuCash's usage count (`5.17:libgnucash/engine/Account.cpp:2680-2682`).
    pub accounts: usize,
    /// Transactions with it as their currency.
    pub transactions: usize,
    /// Prices with it on either side.
    pub prices: usize,
    /// Customers, vendors, employees and invoices with it as their currency.
    pub business: usize,
}

impl Usage {
    /// True when nothing refers to the commodity.
    pub fn is_unused(&self) -> bool {
        *self == Self::default()
    }
}

/// Usage of every commodity referred to anywhere in the book, in one pass over it.
pub fn usage_table(book: &Book) -> BTreeMap<CommodityRef, Usage> {
    let mut table: BTreeMap<CommodityRef, Usage> = BTreeMap::new();
    let mut bump = |id: &CommodityRef, field: fn(&mut Usage) -> &mut usize| *field(table.entry(canonical(id)).or_default()) += 1;
    book.accounts
        .values()
        .filter_map(|a| a.commodity.as_ref())
        .for_each(|c| bump(c, |u| &mut u.accounts));
    book.transactions.values().for_each(|t| bump(&t.currency, |u| &mut u.transactions));
    book.prices.values().for_each(|p| {
        bump(&p.commodity, |u| &mut u.prices);
        if p.currency != p.commodity {
            bump(&p.currency, |u| &mut u.prices);
        }
    });
    business_currencies(book).for_each(|c| bump(c, |u| &mut u.business));
    table
}

/// How much of the book refers to `id`.
pub fn usage(book: &Book, id: &CommodityRef) -> Usage {
    usage_table(book).remove(&canonical(id)).unwrap_or_default()
}

fn business_currencies(book: &Book) -> impl Iterator<Item = &CommodityRef> {
    let b = &book.business;
    let customers = b.customers.values().filter_map(|x| x.currency.as_ref());
    let vendors = b.vendors.values().filter_map(|x| x.currency.as_ref());
    let employees = b.employees.values().filter_map(|x| x.currency.as_ref());
    let invoices = b.invoices.values().filter_map(|x| x.currency.as_ref());
    customers.chain(vendors).chain(employees).chain(invoices)
}

fn accounts_using(book: &Book, id: &CommodityRef) -> usize {
    book.accounts
        .values()
        .filter(|a| a.commodity.as_ref().is_some_and(|c| canonical(c) == *id))
        .count()
}

/// `gnc_commodity_get_auto_quote_control_flag`: true unless the slot holds `"false"`
/// (`5.17:libgnucash/engine/gnc-commodity.cpp:1012-1018`).
pub fn auto_quote_control(c: &Commodity) -> bool {
    c.slots.get_str(slot_paths::AUTO_QUOTE_CONTROL) != Some("false")
}

/// A commodity's quote settings as GnuCash holds them once the book is loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteSettings {
    /// Online quotes are wanted.
    pub flag: bool,
    /// The quote source's internal name; currencies report `currency` when none is stored.
    pub source: Option<String>,
}

/// The quote flag and source GnuCash reports. Loading a book runs `gnc_commodity_increment_usage_count` for every
/// account, which turns quotes on (source `currency`) for a currency an account uses unless the user overrode it
/// (`5.17:libgnucash/engine/gnc-commodity.cpp:1402-1425`); a currency with no stored source reports `currency`
/// (`:1038-1047`).
pub fn effective_quote(c: &Commodity, account_users: usize) -> QuoteSettings {
    let iso = c.id.is_currency();
    if c.quote_flag {
        let source = c.quote_source.clone().or_else(|| iso.then(|| CURRENCY_QUOTE_SOURCE.to_owned()));
        QuoteSettings { flag: true, source }
    } else if iso && account_users > 0 && auto_quote_control(c) {
        QuoteSettings {
            flag: true,
            source: Some(CURRENCY_QUOTE_SOURCE.to_owned()),
        }
    } else {
        let source = c.quote_source.clone().or_else(|| iso.then(|| CURRENCY_QUOTE_SOURCE.to_owned()));
        QuoteSettings { flag: false, source }
    }
}

/// Call after an account's commodity changed from `old` to `new` (either may be `None`, for a new or deleted
/// account), with the book already holding the change. Applies GnuCash's quote-flag automation for currencies:
/// the first account to use a currency turns its quotes on with source `currency`, and the last to stop using it
/// turns them off, unless the user overrode the flag (`gnc_commodity_increment_usage_count` and
/// `gnc_commodity_decrement_usage_count`, `5.17:libgnucash/engine/gnc-commodity.cpp:1402-1450`, called from
/// `xaccAccountSetCommodity`, `5.17:libgnucash/engine/Account.cpp:2680-2682`).
pub fn account_commodity_changed(book: &mut Book, old: Option<&CommodityRef>, new: Option<&CommodityRef>) {
    let (old, new) = (old.map(canonical), new.map(canonical));
    if old == new {
        return;
    }
    if let Some(old) = old.filter(CommodityRef::is_currency)
        && accounts_using(book, &old) == 0
    {
        stop_auto_quotes(book, &old);
    }
    if let Some(new) = new.filter(CommodityRef::is_currency)
        && accounts_using(book, &new) == 1
    {
        start_auto_quotes(book, &new);
    }
}

fn start_auto_quotes(book: &mut Book, id: &CommodityRef) {
    let Some(current) = lookup(book, id) else { return };
    if current.quote_flag || !auto_quote_control(&current) {
        return;
    }
    let mut c = current.into_owned();
    c.quote_flag = true;
    c.quote_source = Some(CURRENCY_QUOTE_SOURCE.to_owned());
    // A new GnuCash commodity's time zone is "" (`5.17:libgnucash/engine/gnc-commodity.cpp:566`), written as
    // `<cmdty:quote_tz/>` once quotes are on.
    c.quote_tz.get_or_insert_with(String::new);
    book.commodities.insert(c.id.clone(), Arc::new(c));
}

fn stop_auto_quotes(book: &mut Book, id: &CommodityRef) {
    if let Some(c) = book.commodities.get_mut(id)
        && c.quote_flag
        && auto_quote_control(c)
    {
        let c = Arc::make_mut(c);
        c.quote_flag = false;
        keep_persisted(c);
    }
}

/// Keep only what the file can hold: GnuCash writes the quote source and time zone only with the quote flag, and a
/// currency with quotes on and no source writes `currency` (`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:97-108`).
/// Without this, what red-ale holds would differ from what it reads back after saving.
fn keep_persisted(c: &mut Commodity) {
    if !c.quote_flag {
        c.quote_source = None;
        c.quote_tz = None;
    } else if c.is_currency() && c.quote_source.is_none() {
        c.quote_source = Some(CURRENCY_QUOTE_SOURCE.to_owned());
    }
}

/// A new security, as the security editor takes it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewSecurity {
    /// Namespace (the exchange or type), such as `NASDAQ` or `FUND`.
    pub namespace: String,
    /// Mnemonic (ticker).
    pub mnemonic: String,
    /// Full name.
    pub fullname: String,
    /// Exchange code such as a CUSIP or ISIN.
    pub cusip: Option<String>,
    /// Smallest fraction; GnuCash's default is 10000.
    pub fraction: i64,
    /// Whether online quotes are wanted.
    pub quote_flag: bool,
    /// Quote source internal name.
    pub quote_source: Option<String>,
    /// Quote time zone; `None` is "use local time".
    pub quote_tz: Option<String>,
    /// Display symbol.
    pub user_symbol: Option<String>,
}

/// Changes to a commodity. `None` leaves a field as it is; for optional fields `Some(None)` clears it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::option_option)] // Absent, cleared and set are three different requests.
pub struct CommodityChanges {
    /// New namespace (securities only).
    pub namespace: Option<String>,
    /// New mnemonic (securities only).
    pub mnemonic: Option<String>,
    /// New full name (securities only).
    pub fullname: Option<String>,
    /// New exchange code (securities only).
    pub cusip: Option<Option<String>>,
    /// New fraction (securities only).
    pub fraction: Option<i64>,
    /// New quote flag.
    pub quote_flag: Option<bool>,
    /// New quote source (securities only).
    pub quote_source: Option<Option<String>>,
    /// New quote time zone.
    pub quote_tz: Option<Option<String>>,
    /// New display symbol; empty or `None` removes it.
    pub user_symbol: Option<Option<String>>,
}

/// Trim as the XML reader will (`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:174-188`), so what is stored
/// reads back the same.
fn trimmed(s: &str) -> String {
    s.trim().to_owned()
}

fn trimmed_opt(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// Create a security with the security editor's checks (`gnc_ui_commodity_dialog_to_object`,
/// `5.17:gnucash/gnome-utils/dialog-commodity.cpp:1231-1370`). Returns its reference.
pub fn create_security(book: &mut Book, new: &NewSecurity) -> DomainResult<CommodityRef> {
    let id = CommodityRef::new(trimmed(&new.namespace), trimmed(&new.mnemonic));
    check_security_id(&id)?;
    if lookup(book, &id).is_some() {
        return Err(DomainError::conflict(format!("commodity {id} already exists")));
    }
    let mut c = Commodity::new(id.clone(), Some(trimmed(&new.fullname)), new.fraction);
    c.cusip = trimmed_opt(new.cusip.as_deref());
    c.quote_flag = new.quote_flag;
    c.quote_source = new.quote_source.as_deref().map(str::trim).map(str::to_owned);
    c.quote_tz = new.quote_tz.as_deref().map(trimmed);
    set_user_symbol(&mut c, new.user_symbol.as_deref());
    check_security(&c)?;
    keep_persisted(&mut c);
    book.commodities.insert(id.clone(), Arc::new(c));
    Ok(id)
}

/// Refusals that depend on the namespace and mnemonic alone.
fn check_security_id(id: &CommodityRef) -> DomainResult<()> {
    if is_iso_namespace(&id.namespace) {
        return Err(DomainError::invalid("you may not create a new national currency"));
    }
    if id.namespace == TEMPLATE_NAMESPACE {
        return Err(DomainError::invalid(format!("{TEMPLATE_NAMESPACE} is a reserved namespace")));
    }
    if id.namespace == LEGACY_CURRENCIES_NAMESPACE {
        return Err(DomainError::invalid(format!(
            "{LEGACY_CURRENCIES_NAMESPACE} is a reserved namespace"
        )));
    }
    if id.namespace.is_empty() || id.mnemonic.is_empty() {
        return Err(DomainError::invalid("a security needs a namespace and a mnemonic"));
    }
    if id.namespace.contains("::") {
        return Err(DomainError::invalid(
            "a namespace may not contain \"::\", which separates it from the mnemonic",
        ));
    }
    Ok(())
}

/// Refusals on a security's other fields.
fn check_security(c: &Commodity) -> DomainResult<()> {
    if c.fullname.as_deref().is_none_or(str::is_empty) {
        return Err(DomainError::invalid("a security needs a full name"));
    }
    if !(1..=MAX_FRACTION).contains(&c.fraction) {
        return Err(DomainError::invalid(format!(
            "the fraction must be from 1 to {MAX_FRACTION}, got {}",
            c.fraction
        )));
    }
    match c.quote_source.as_deref() {
        Some("") => Err(DomainError::invalid("the quote source may not be empty")),
        Some(CURRENCY_QUOTE_SOURCE) => Err(DomainError::invalid("the currency quote source is for currencies only")),
        _ => Ok(()),
    }
}

/// `gnc_commodity_set_user_symbol` without the locale rule: empty, or equal to a currency's built-in symbol,
/// removes the slot (`5.17:libgnucash/engine/gnc-commodity.cpp:1355-1380`).
fn set_user_symbol(c: &mut Commodity, symbol: Option<&str>) {
    let default =
        c.id.is_currency()
            .then(|| iso_currency(&c.id.mnemonic).map(|iso| iso.symbol))
            .flatten();
    match symbol {
        Some(s) if !s.is_empty() && Some(s) != default => c.slots.set(slot_paths::USER_SYMBOL, SlotValue::String(s.to_owned())),
        _ => {
            c.slots.remove(slot_paths::USER_SYMBOL);
        }
    }
}

/// Edit a commodity as the security editor does. A currency accepts only the quote flag, quote time zone and user
/// symbol (`5.17:gnucash/gnome-utils/dialog-commodity.cpp:1250-1272`); a security accepts every field, including its
/// namespace and mnemonic, in which case every account, transaction and price that refers to it is changed too.
/// Returns the commodity's (possibly new) reference.
pub fn update_commodity(book: &mut Book, id: &CommodityRef, changes: &CommodityChanges) -> DomainResult<CommodityRef> {
    let current = lookup(book, id)
        .ok_or_else(|| DomainError::not_found("commodity", id))?
        .into_owned();
    if current.id.namespace == TEMPLATE_NAMESPACE {
        return Err(DomainError::invalid(
            "the template commodity is reserved for scheduled transactions",
        ));
    }
    if current.is_currency() {
        update_currency(book, current, changes)
    } else {
        update_security(book, current, changes)
    }
}

fn update_currency(book: &mut Book, mut c: Commodity, changes: &CommodityChanges) -> DomainResult<CommodityRef> {
    let security_only = changes.namespace.is_some()
        || changes.mnemonic.is_some()
        || changes.fullname.is_some()
        || changes.cusip.is_some()
        || changes.fraction.is_some()
        || changes.quote_source.is_some();
    if security_only {
        return Err(DomainError::invalid(
            "only the quote flag, quote time zone and user symbol of a currency can be changed",
        ));
    }
    if let Some(flag) = changes.quote_flag {
        let users = accounts_using(book, &c.id);
        user_set_quote_flag(&mut c, flag, users);
        if !flag {
            // The editor clears the time zone of a currency without quotes (dialog-commodity.cpp:1263-1264).
            c.quote_tz = None;
        }
    }
    if let Some(tz) = &changes.quote_tz {
        c.quote_tz = tz.as_deref().map(trimmed);
    }
    if let Some(symbol) = &changes.user_symbol {
        set_user_symbol(&mut c, symbol.as_deref());
    }
    keep_persisted(&mut c);
    let id = c.id.clone();
    book.commodities.insert(id.clone(), Arc::new(c));
    Ok(id)
}

/// `gnc_commodity_user_set_quote_flag`: for a currency, record whether the user's choice differs from the automatic
/// one, which is "on" exactly when an account uses it (`5.17:libgnucash/engine/gnc-commodity.cpp:1256-1283`).
fn user_set_quote_flag(c: &mut Commodity, flag: bool, account_users: usize) {
    c.quote_flag = flag;
    if c.is_currency() {
        let automatic = flag == (account_users > 0);
        if automatic {
            c.slots.remove(slot_paths::AUTO_QUOTE_CONTROL);
        } else {
            c.slots.set(slot_paths::AUTO_QUOTE_CONTROL, SlotValue::String("false".into()));
        }
    }
}

fn update_security(book: &mut Book, current: Commodity, changes: &CommodityChanges) -> DomainResult<CommodityRef> {
    let old_id = current.id.clone();
    let mut c = current;
    let namespace = changes.namespace.as_deref().map_or_else(|| old_id.namespace.clone(), trimmed);
    let mnemonic = changes.mnemonic.as_deref().map_or_else(|| old_id.mnemonic.clone(), trimmed);
    let new_id = CommodityRef::new(namespace, mnemonic);
    if new_id != old_id {
        check_security_id(&new_id)?;
        if lookup(book, &new_id).is_some() {
            return Err(DomainError::conflict(format!("commodity {new_id} already exists")));
        }
        if usage(book, &old_id).business > 0 {
            return Err(DomainError::conflict(format!(
                "{old_id} is the currency of business objects; it cannot be renamed"
            )));
        }
    }
    c.id = new_id.clone();
    apply_security_fields(&mut c, changes);
    check_security(&c)?;
    keep_persisted(&mut c);
    let fraction = c.fraction;
    book.commodities.remove(&old_id);
    book.commodities.insert(new_id.clone(), Arc::new(c));
    follow_fraction(book, &old_id, fraction);
    if new_id != old_id {
        rename_references(book, &old_id, &new_id);
    }
    Ok(new_id)
}

fn apply_security_fields(c: &mut Commodity, changes: &CommodityChanges) {
    if let Some(name) = &changes.fullname {
        c.fullname = Some(trimmed(name));
    }
    if let Some(cusip) = &changes.cusip {
        c.cusip = trimmed_opt(cusip.as_deref());
    }
    if let Some(fraction) = changes.fraction {
        // GnuCash changes the fraction of a commodity in use without any check (gnc-commodity.cpp:1219); amounts
        // already stored keep their denominators.
        c.fraction = fraction;
    }
    if let Some(flag) = changes.quote_flag {
        c.quote_flag = flag;
    }
    if let Some(source) = &changes.quote_source {
        c.quote_source = source.as_deref().map(str::trim).map(str::to_owned);
    }
    if let Some(tz) = &changes.quote_tz {
        c.quote_tz = tz.as_deref().map(trimmed);
    }
    if let Some(symbol) = &changes.user_symbol {
        set_user_symbol(c, symbol.as_deref());
    }
}

/// Accounts that follow their commodity's fraction keep doing so after it changes. GnuCash changes only the commodity
/// (`5.17:libgnucash/engine/gnc-commodity.cpp:1219`), leaving the account's stored SCU at the old fraction, so after a
/// reload `xaccAccountSetCommoditySCU` marks the account non-standard at the old SCU
/// (`5.17:libgnucash/engine/Account.cpp:2710-2722`). red-ale stores the new fraction instead, so the book reads back
/// as GnuCash showed it before the reload.
fn follow_fraction(book: &mut Book, id: &CommodityRef, fraction: i64) {
    book.accounts
        .values_mut()
        .filter(|a| a.commodity.as_ref() == Some(id) && !a.non_standard_scu && a.commodity_scu.is_some_and(|s| s != fraction))
        .for_each(|a| Arc::make_mut(a).commodity_scu = Some(fraction));
}

/// GnuCash's entities hold the commodity object itself, so renaming it renames it everywhere, template accounts and
/// template transactions included; red-ale's hold its name, so each reference is rewritten.
fn rename_references(book: &mut Book, old: &CommodityRef, new: &CommodityRef) {
    book.accounts
        .values_mut()
        .chain(book.templates.accounts.values_mut())
        .filter(|a| a.commodity.as_ref() == Some(old))
        .for_each(|a| Arc::make_mut(a).commodity = Some(new.clone()));
    book.transactions
        .values_mut()
        .chain(book.templates.transactions.values_mut())
        .filter(|t| t.currency == *old)
        .for_each(|t| Arc::make_mut(t).currency = new.clone());
    book.prices
        .values_mut()
        .filter(|p| p.commodity == *old || p.currency == *old)
        .for_each(|p| {
            let p = Arc::make_mut(p);
            if p.commodity == *old {
                p.commodity = new.clone();
            }
            if p.currency == *old {
                p.currency = new.clone();
            }
        });
}

/// Delete a security. Refused for currencies and the template commodity (`5.17:gnucash/gnome/dialog-commodities.cpp:345`)
/// and while an account uses it (`:176-198`); red-ale also refuses while a transaction or business object uses it as
/// its currency. Prices of it (either side) block the deletion unless `delete_prices`, in which case they go too.
/// Returns the deleted prices.
pub fn delete_commodity(book: &mut Book, id: &CommodityRef, delete_prices: bool) -> DomainResult<Vec<Guid>> {
    let id = canonical(id);
    if id.is_currency() {
        return Err(DomainError::invalid("a currency cannot be deleted"));
    }
    if id.namespace == TEMPLATE_NAMESPACE {
        return Err(DomainError::invalid(
            "the template commodity is reserved for scheduled transactions",
        ));
    }
    if book.commodity(&id).is_none() {
        return Err(DomainError::not_found("commodity", &id));
    }
    let used = usage(book, &id);
    if used.accounts + used.transactions + used.business > 0 {
        return Err(DomainError::conflict(format!(
            "{id} is in use by {} accounts, {} transactions and {} business objects",
            used.accounts, used.transactions, used.business
        )));
    }
    if templates_use(book, &id) {
        return Err(DomainError::conflict(format!("{id} is in use by template transactions of scheduled transactions")));
    }
    if used.prices > 0 && !delete_prices {
        return Err(DomainError::conflict(format!(
            "{id} has {} prices; pass deletePrices to delete them with it",
            used.prices
        )));
    }
    let prices: Vec<Guid> = book
        .prices
        .values()
        .filter(|p| p.commodity == id || p.currency == id)
        .map(|p| p.guid)
        .collect();
    prices.iter().for_each(|g| {
        book.prices.shift_remove(g);
    });
    book.commodities.remove(&id);
    Ok(prices)
}

/// True if a template account or template transaction refers to `id`. [`usage`] does not count them: they are not part
/// of the Account tree.
fn templates_use(book: &Book, id: &CommodityRef) -> bool {
    let t = &book.templates;
    t.accounts.values().any(|a| a.commodity.as_ref().is_some_and(|c| canonical(c) == *id))
        || t.transactions.values().any(|x| canonical(&x.currency) == *id)
}

/// Namespaces of the table with their commodity counts, sorted. `CURRENCY` is always present.
pub fn namespaces(book: &Book) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    table(book)
        .iter()
        .for_each(|c| *counts.entry(c.id.namespace.clone()).or_default() += 1);
    counts.into_iter().collect()
}

/// Convert between the euro and a currency it replaced, with GnuCash's fixed rates and rounding: to euros at cents,
/// from euros at the legacy currency's fraction, both rounding half away from zero (`gnc_convert_to_euro`,
/// `gnc_convert_from_euro`, `5.17:libgnucash/engine/gnc-euro.cpp:80-106`). GnuCash returns zero for a currency
/// with no euro rate; red-ale refuses instead, so a zero is never a silent wrong answer.
pub fn euro_convert(from: &CommodityRef, to: &CommodityRef, amount: crate::Numeric) -> DomainResult<crate::Numeric> {
    use crate::currencies::{convert_from_euro, convert_to_euro, is_euro_currency};
    let (from, to) = (canonical(from), canonical(to));
    let euro = CommodityRef::currency("EUR");
    let legacy = |c: &CommodityRef| {
        if is_euro_currency(c) {
            Ok(())
        } else {
            Err(DomainError::invalid(format!("{c} has no fixed euro rate")))
        }
    };
    if to == euro {
        legacy(&from)?;
        Ok(convert_to_euro(&from, amount)?)
    } else if from == euro {
        legacy(&to)?;
        Ok(convert_from_euro(&to, amount)?)
    } else {
        Err(DomainError::invalid("one side of a euro conversion must be CURRENCY::EUR"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Account, AccountType, FixedEnv, Time64};

    fn book() -> Book {
        Book::new_empty(&mut FixedEnv::new(Time64(0), 1))
    }

    fn acme() -> NewSecurity {
        NewSecurity {
            namespace: " NASDAQ ".into(),
            mnemonic: "ACME".into(),
            fullname: "Acme Corp ".into(),
            fraction: 10_000,
            ..NewSecurity::default()
        }
    }

    #[test]
    fn display_symbols_fall_back_from_user_to_default_to_mnemonic() {
        let mut b = book();
        let id = create_security(&mut b, &acme()).unwrap();
        assert_eq!(display_symbol(&b, &id).as_deref(), Some("ACME"));
        assert_eq!(display_symbol(&b, &CommodityRef::currency("USD")).as_deref(), Some("$"));
        assert_eq!(
            display_symbol(&b, &CommodityRef::currency("AFN")).as_deref(),
            Some("AFN"),
            "an empty default symbol"
        );
        let changes = CommodityChanges {
            user_symbol: Some(Some("US$".into())),
            ..CommodityChanges::default()
        };
        update_commodity(&mut b, &CommodityRef::currency("USD"), &changes).unwrap();
        assert_eq!(display_symbol(&b, &CommodityRef::currency("USD")).as_deref(), Some("US$"));
        assert_eq!(display_symbol(&b, &CommodityRef::new("NASDAQ", "NONE")), None);
    }

    #[test]
    fn the_table_holds_the_iso_currencies_and_the_template() {
        let b = book();
        assert_eq!(table(&b).len(), 228);
        assert_eq!(
            lookup(&b, &CommodityRef::currency("RUR")).unwrap().id,
            CommodityRef::currency("RUB")
        );
        assert_eq!(namespaces(&b), vec![("CURRENCY".to_owned(), 227), ("template".to_owned(), 1)]);
    }

    #[test]
    fn securities_are_validated_like_the_editor() {
        let mut b = book();
        let id = create_security(&mut b, &acme()).unwrap();
        assert_eq!(id, CommodityRef::new("NASDAQ", "ACME"));
        assert_eq!(b.commodity(&id).unwrap().fullname.as_deref(), Some("Acme Corp"));
        assert!(matches!(create_security(&mut b, &acme()), Err(DomainError::Conflict(_))));
        let bad = |f: fn(&mut NewSecurity)| {
            let mut s = acme();
            s.mnemonic = "NEW".into();
            f(&mut s);
            create_security(&mut book(), &s)
        };
        assert!(bad(|s| s.namespace = "CURRENCY".into()).is_err());
        assert!(bad(|s| s.namespace = "ISO4217".into()).is_err());
        assert!(bad(|s| s.namespace = "template".into()).is_err());
        assert!(bad(|s| s.fullname = " ".into()).is_err());
        assert!(bad(|s| s.fraction = 0).is_err());
        assert!(bad(|s| s.fraction = MAX_FRACTION + 1).is_err());
        assert!(bad(|s| s.quote_source = Some("currency".into())).is_err());
        assert!(bad(|s| s.quote_source = Some("my_private_src".into())).is_ok());
    }

    #[test]
    fn renaming_a_security_rewrites_its_references() {
        let mut b = book();
        let id = create_security(&mut b, &acme()).unwrap();
        let acct = Account::new(
            Guid::from_bytes([9; 16]),
            "Acme",
            AccountType::Stock,
            Some(id.clone()),
            Some(b.root),
        );
        b.accounts.insert(acct.guid, Arc::new(acct));
        let changes = CommodityChanges {
            namespace: Some("NYSE".into()),
            fraction: Some(1000),
            ..CommodityChanges::default()
        };
        let new = update_commodity(&mut b, &id, &changes).unwrap();
        assert_eq!(new, CommodityRef::new("NYSE", "ACME"));
        assert!(b.commodity(&id).is_none());
        assert_eq!(b.commodity(&new).unwrap().fraction, 1000);
        assert_eq!(b.account(Guid::from_bytes([9; 16])).unwrap().commodity, Some(new));
    }

    #[test]
    fn currencies_change_only_quote_settings_and_record_overrides() {
        let mut b = book();
        let usd = CommodityRef::currency("USD");
        let renamed = CommodityChanges {
            fullname: Some("Dollar".into()),
            ..CommodityChanges::default()
        };
        assert!(update_commodity(&mut b, &usd, &renamed).is_err());
        let on = CommodityChanges {
            quote_flag: Some(true),
            user_symbol: Some(Some("$".into())),
            ..CommodityChanges::default()
        };
        update_commodity(&mut b, &usd, &on).unwrap();
        let c = b.commodity(&usd).unwrap();
        assert!(c.quote_flag);
        assert_eq!(c.slots.get_str(slot_paths::AUTO_QUOTE_CONTROL), Some("false"));
        assert!(c.slots.get(slot_paths::USER_SYMBOL).is_none(), "the built-in symbol is not stored");
    }

    #[test]
    fn account_use_turns_currency_quotes_on_and_off() {
        let mut b = book();
        let eur = CommodityRef::currency("EUR");
        let acct = Account::new(
            Guid::from_bytes([7; 16]),
            "Bank",
            AccountType::Bank,
            Some(eur.clone()),
            Some(b.root),
        );
        b.accounts.insert(acct.guid, Arc::new(acct));
        account_commodity_changed(&mut b, None, Some(&eur));
        let c = b.commodity(&eur).unwrap();
        assert!(c.quote_flag);
        assert_eq!(c.quote_source.as_deref(), Some("currency"));
        assert_eq!(c.quote_tz.as_deref(), Some(""));
        b.accounts.shift_remove(&Guid::from_bytes([7; 16]));
        account_commodity_changed(&mut b, Some(&eur), None);
        assert!(!b.commodity(&eur).unwrap().quote_flag);
    }

    #[test]
    fn euro_conversion_uses_the_fixed_rates() {
        let (dem, eur) = (CommodityRef::currency("DEM"), CommodityRef::currency("EUR"));
        assert!(
            euro_convert(&dem, &eur, crate::Numeric::new(10_000, 100))
                .unwrap()
                .same_repr(crate::Numeric::new(5113, 100))
        );
        let bgn = CommodityRef::currency("BGN");
        assert!(
            euro_convert(&eur, &bgn, crate::Numeric::new(100, 100))
                .unwrap()
                .same_repr(crate::Numeric::new(196, 100))
        );
        assert!(euro_convert(&CommodityRef::currency("USD"), &eur, crate::Numeric::ONE).is_err());
        assert!(euro_convert(&dem, &bgn, crate::Numeric::ONE).is_err());
    }

    #[test]
    fn deletion_is_refused_while_in_use() {
        let mut b = book();
        let id = create_security(&mut b, &acme()).unwrap();
        b.prices.insert(
            Guid::from_bytes([3; 16]),
            Arc::new(crate::Price {
                guid: Guid::from_bytes([3; 16]),
                commodity: id.clone(),
                currency: CommodityRef::currency("USD"),
                time: Time64(0),
                source: None,
                price_type: None,
                value: crate::Numeric::new(1, 1),
            }),
        );
        assert!(matches!(delete_commodity(&mut b, &id, false), Err(DomainError::Conflict(_))));
        assert_eq!(delete_commodity(&mut b, &id, true).unwrap(), vec![Guid::from_bytes([3; 16])]);
        assert!(b.prices.is_empty());
        assert!(delete_commodity(&mut b, &CommodityRef::currency("USD"), true).is_err());
    }

    #[test]
    fn template_transactions_follow_a_rename_and_stop_a_deletion() {
        let mut b = book();
        let id = create_security(&mut b, &acme()).unwrap();
        let guid = Guid::from_bytes([5; 16]);
        let template = crate::Transaction {
            guid,
            currency: id.clone(),
            num: None,
            date_posted: Time64(0),
            date_entered: Time64(0),
            description: None,
            slots: crate::Frame::new(),
            splits: Vec::new(),
        };
        b.templates.transactions.insert(guid, Arc::new(template));
        let renamed = CommodityChanges {
            mnemonic: Some("ACM".into()),
            ..CommodityChanges::default()
        };
        let new = update_commodity(&mut b, &id, &renamed).unwrap();
        assert_eq!(b.templates.transactions[&guid].currency, new);
        assert_eq!(
            delete_commodity(&mut b, &new, true).unwrap_err().to_string(),
            "NASDAQ::ACM is in use by template transactions of scheduled transactions"
        );
    }
}
