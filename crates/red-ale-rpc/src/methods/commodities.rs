//! Methods: the commodity table, the security editor, the ISO 4217 currencies and euro conversion.
//! See `design/api/commodities.md`.

use red_ale_core::currencies::{IsoCurrency, QuoteSourceType, euro_rate, iso_currencies, iso_currency, quote_source_type};
use red_ale_core::ops::commodities::{
    self as ops, CommodityChanges, NewSecurity, Usage, effective_quote, is_iso_namespace, slot_paths, usage_table,
};
use red_ale_core::{Book, Commodity, CommodityRef, DomainError};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{Amount, CommodityId, Id};

/// The commodities methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "commodity.namespaces",
            "List the commodity namespaces with how many commodities each holds.",
            Effect::Read,
            namespaces,
        ),
        method(
            "commodity.list",
            "List commodities: the book's own and the built-in ISO 4217 currencies.",
            Effect::Read,
            list,
        ),
        method(
            "commodity.get",
            "Describe one commodity, with how much of the book uses it.",
            Effect::Read,
            get,
        ),
        method(
            "commodity.create",
            "Create a security, with the security editor's checks.",
            Effect::Write,
            create,
        ),
        method(
            "commodity.update",
            "Edit a security (any field, including a rename) or a currency's quote flag, quote time zone and symbol.",
            Effect::Write,
            update,
        ),
        method(
            "commodity.delete",
            "Delete a security that nothing uses; its prices go too only with deletePrices.",
            Effect::Destructive,
            delete,
        ),
        method(
            "currency.list",
            "List GnuCash 5.17's built-in ISO 4217 currencies.",
            Effect::Read,
            currencies,
        ),
        method(
            "currency.euroConvert",
            "Convert between the euro and a currency it replaced, at GnuCash's fixed rates.",
            Effect::Read,
            euro_convert,
        ),
    ]
}

/// Read a present field as `Some(value)`, so that `null` (clear) and absent (keep) differ.
#[allow(clippy::option_option, clippy::unnecessary_wraps)]
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// A namespace of the commodity table.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceInfo {
    /// Its name, such as `CURRENCY`, `NASDAQ` or `template`.
    pub name: String,
    /// How many commodities it holds.
    pub commodities: usize,
    /// It holds the ISO 4217 currencies.
    pub is_currency: bool,
    /// New securities may not be created in it (`CURRENCY`, `template`).
    pub reserved: bool,
}

/// How much of the book refers to a commodity.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageInfo {
    /// Accounts holding it (GnuCash's usage count).
    pub accounts: usize,
    /// Transactions with it as their currency.
    pub transactions: usize,
    /// Prices with it on either side.
    pub prices: usize,
    /// Customers, vendors, employees and invoices with it as their currency.
    pub business: usize,
}

impl From<Usage> for UsageInfo {
    fn from(u: Usage) -> Self {
        Self {
            accounts: u.accounts,
            transactions: u.transactions,
            prices: u.prices,
            business: u.business,
        }
    }
}

/// A commodity as GnuCash sees it once the book is loaded.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)] // A wire record of independent facts.
pub struct CommodityInfo {
    /// `NAMESPACE::MNEMONIC`.
    pub id: CommodityId,
    /// Namespace.
    pub namespace: String,
    /// Mnemonic (ticker or ISO code).
    pub mnemonic: String,
    /// Full name.
    pub fullname: Option<String>,
    /// Exchange code (CUSIP, ISIN; the ISO numeric code for a currency).
    pub cusip: Option<String>,
    /// Smallest fraction (SCU).
    pub fraction: i64,
    /// In the `CURRENCY` namespace.
    pub is_currency: bool,
    /// One of GnuCash's built-in ISO 4217 currencies.
    pub built_in: bool,
    /// Stored in the book file (a security, or a currency with quotes on or slots).
    pub defined: bool,
    /// Online quotes are wanted. For a currency an account uses this is on unless the user turned it off.
    pub quote_flag: bool,
    /// Quote source internal name; currencies report `currency`.
    pub quote_source: Option<String>,
    /// `single`, `multi`, `currency`, or `unknown` for a source GnuCash 5.17 does not know (kept as read).
    pub quote_source_type: Option<&'static str>,
    /// Quote time zone; null means local time.
    pub quote_tz: Option<String>,
    /// The user's display symbol.
    pub user_symbol: Option<String>,
    /// A currency's built-in symbol.
    pub default_symbol: Option<String>,
    /// The symbol GnuCash shows amounts with: the user symbol, else the default symbol, else the mnemonic.
    pub display_symbol: String,
    /// How much of the book refers to it.
    pub usage: UsageInfo,
}

fn source_type_name(t: QuoteSourceType) -> &'static str {
    match t {
        QuoteSourceType::Single => "single",
        QuoteSourceType::Multi => "multi",
        QuoteSourceType::Unknown => "unknown",
        QuoteSourceType::Currency => "currency",
    }
}

fn describe(book: &Book, c: &Commodity, usage: Usage) -> CommodityInfo {
    let quote = effective_quote(c, usage.accounts);
    let iso = c.is_currency().then(|| iso_currency(&c.id.mnemonic)).flatten();
    CommodityInfo {
        id: c.id.clone().into(),
        namespace: c.id.namespace.clone(),
        mnemonic: c.id.mnemonic.clone(),
        fullname: c.fullname.clone(),
        cusip: c.cusip.clone(),
        fraction: c.fraction,
        is_currency: c.is_currency(),
        built_in: iso.is_some(),
        defined: book.commodity(&c.id).is_some(),
        quote_flag: quote.flag,
        quote_source_type: quote.source.as_deref().map(|s| source_type_name(quote_source_type(s))),
        quote_source: quote.source,
        quote_tz: c.quote_tz.clone(),
        user_symbol: c.slots.get_str(slot_paths::USER_SYMBOL).map(str::to_owned),
        default_symbol: iso.map(|i| i.symbol.to_owned()).filter(|s| !s.is_empty()),
        display_symbol: ops::display_symbol(book, &c.id).unwrap_or_else(|| c.id.mnemonic.clone()),
        usage: usage.into(),
    }
}

fn describe_one(book: &Book, id: &CommodityRef) -> Result<CommodityInfo, RpcError> {
    let c = ops::lookup(book, id).ok_or_else(|| RpcError::not_found("commodity", id))?;
    Ok(describe(book, &c, ops::usage(book, &c.id)))
}

fn namespaces(ctx: &mut Context, _: crate::wire::NoParams) -> Result<Vec<NamespaceInfo>, RpcError> {
    let book = ctx.book()?;
    Ok(ops::namespaces(book)
        .into_iter()
        .map(|(name, commodities)| NamespaceInfo {
            is_currency: is_iso_namespace(&name),
            reserved: is_iso_namespace(&name) || name == red_ale_core::commodity::TEMPLATE_NAMESPACE,
            name,
            commodities,
        })
        .collect())
}

/// Params of `commodity.list`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListParams {
    /// Only this namespace (`CURRENCY` for the currencies).
    #[serde(default)]
    pub namespace: Option<String>,
    /// Only commodities something in the book uses.
    #[serde(default)]
    pub used_only: bool,
}

fn list(ctx: &mut Context, p: ListParams) -> Result<Vec<CommodityInfo>, RpcError> {
    let book = ctx.book()?;
    let usages = usage_table(book);
    let namespace = p.namespace.map(|n| CommodityRef::new(n, "").namespace);
    Ok(ops::table(book)
        .iter()
        .filter(|c| namespace.as_ref().is_none_or(|n| *n == c.id.namespace))
        .map(|c| (c, usages.get(&c.id).copied().unwrap_or_default()))
        .filter(|(_, u)| !p.used_only || !u.is_unused())
        .map(|(c, u)| describe(book, c, u))
        .collect())
}

/// Params naming one commodity.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetParams {
    /// The commodity, `NAMESPACE::MNEMONIC` (a bare code is a currency).
    pub commodity: CommodityId,
}

fn get(ctx: &mut Context, p: GetParams) -> Result<CommodityInfo, RpcError> {
    describe_one(ctx.book()?, &p.commodity.0)
}

fn default_fraction() -> i64 {
    // gnc_commodity_init (5.17:libgnucash/engine/gnc-commodity.cpp:563).
    10_000
}

/// Params of `commodity.create`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateParams {
    /// Namespace (exchange or type), such as `NASDAQ`; not `CURRENCY` or `template`.
    pub namespace: String,
    /// Mnemonic (ticker), unique within the namespace.
    pub mnemonic: String,
    /// Full name; required.
    pub fullname: String,
    /// Exchange code such as a CUSIP or ISIN.
    #[serde(default)]
    pub cusip: Option<String>,
    /// Smallest fraction, 1 to 1000000000; default 10000.
    #[serde(default = "default_fraction")]
    pub fraction: i64,
    /// Online quotes are wanted.
    #[serde(default)]
    pub quote_flag: bool,
    /// Quote source internal name, such as `yahoo_json`; unknown names are kept.
    #[serde(default)]
    pub quote_source: Option<String>,
    /// Quote time zone, such as `America/New_York`; absent means local time.
    #[serde(default)]
    pub quote_tz: Option<String>,
    /// Display symbol.
    #[serde(default)]
    pub user_symbol: Option<String>,
}

fn create(ctx: &mut Context, p: CreateParams) -> Result<CommodityInfo, RpcError> {
    let new = NewSecurity {
        namespace: p.namespace,
        mnemonic: p.mnemonic,
        fullname: p.fullname,
        cusip: p.cusip,
        fraction: p.fraction,
        quote_flag: p.quote_flag,
        quote_source: p.quote_source,
        quote_tz: p.quote_tz,
        user_symbol: p.user_symbol,
    };
    let id = ctx.mutate(|book, _| ops::create_security(book, &new))?;
    describe_one(ctx.book()?, &id)
}

/// Params of `commodity.update`. Absent fields are kept; `null` clears an optional one.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(clippy::option_option)]
pub struct UpdateParams {
    /// The commodity to edit.
    pub commodity: CommodityId,
    /// New namespace (securities only); renames it everywhere in the book.
    #[serde(default)]
    pub namespace: Option<String>,
    /// New mnemonic (securities only); renames it everywhere in the book.
    #[serde(default)]
    pub mnemonic: Option<String>,
    /// New full name (securities only).
    #[serde(default)]
    pub fullname: Option<String>,
    /// New exchange code (securities only); null or empty clears it.
    #[serde(default, deserialize_with = "present")]
    pub cusip: Option<Option<String>>,
    /// New smallest fraction (securities only). Stored amounts keep their denominators.
    #[serde(default)]
    pub fraction: Option<i64>,
    /// Online quotes wanted. For a currency, a choice that differs from the automatic one is remembered.
    #[serde(default)]
    pub quote_flag: Option<bool>,
    /// New quote source (securities only); null clears it.
    #[serde(default, deserialize_with = "present")]
    pub quote_source: Option<Option<String>>,
    /// New quote time zone; null means local time.
    #[serde(default, deserialize_with = "present")]
    pub quote_tz: Option<Option<String>>,
    /// New display symbol; null or empty removes it.
    #[serde(default, deserialize_with = "present")]
    pub user_symbol: Option<Option<String>>,
}

fn update(ctx: &mut Context, p: UpdateParams) -> Result<CommodityInfo, RpcError> {
    let current = ops::require(ctx.book()?, &p.commodity.0)?;
    let renames = p.namespace.as_deref().is_some_and(|n| n.trim() != current.namespace)
        || p.mnemonic.as_deref().is_some_and(|m| m.trim() != current.mnemonic);
    if renames && !current.is_currency() {
        refuse_if_residue_mentions(ctx, &current, "renamed")?;
    }
    let changes = CommodityChanges {
        namespace: p.namespace,
        mnemonic: p.mnemonic,
        fullname: p.fullname,
        cusip: p.cusip,
        fraction: p.fraction,
        quote_flag: p.quote_flag,
        quote_source: p.quote_source,
        quote_tz: p.quote_tz,
        user_symbol: p.user_symbol,
    };
    let id = ctx.mutate(|book, _| ops::update_commodity(book, &current, &changes))?;
    describe_one(ctx.book()?, &id)
}

/// Template and scheduled transactions are kept as read, not modelled; a commodity they mention must not be renamed
/// or deleted under them. Budgets name no commodity.
fn refuse_if_residue_mentions(ctx: &mut Context, id: &CommodityRef, what: &str) -> Result<(), RpcError> {
    let sections = ctx.session()?.residue().sections_referring_to(id);
    if sections.is_empty() {
        Ok(())
    } else {
        Err(DomainError::conflict(format!("{id} is referred to by {}; it cannot be {what}", sections.join(", "))).into())
    }
}

/// Params of `commodity.delete`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeleteParams {
    /// The security to delete.
    pub commodity: CommodityId,
    /// Also delete its prices (either side). Without this, a commodity with prices is not deleted.
    #[serde(default)]
    pub delete_prices: bool,
}

/// What `commodity.delete` removed.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    /// The prices deleted with it.
    pub deleted_prices: Vec<Id>,
}

fn delete(ctx: &mut Context, p: DeleteParams) -> Result<DeleteResult, RpcError> {
    let id = ops::canonical(&p.commodity.0);
    if !id.is_currency() && ctx.book()?.commodity(&id).is_some() {
        refuse_if_residue_mentions(ctx, &id, "deleted")?;
    }
    let deleted = ctx.mutate(|book, _| ops::delete_commodity(book, &id, p.delete_prices))?;
    Ok(DeleteResult {
        deleted_prices: deleted.into_iter().map(Id).collect(),
    })
}

/// A built-in ISO 4217 currency.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyInfo {
    /// `CURRENCY::<code>`.
    pub id: CommodityId,
    /// ISO code.
    pub code: &'static str,
    /// Full name (English).
    pub fullname: &'static str,
    /// ISO numeric code, GnuCash's exchange code for it.
    pub exchange_code: &'static str,
    /// Smallest fraction.
    pub fraction: i64,
    /// Local symbol, if any.
    pub symbol: Option<&'static str>,
    /// Units per euro, for the euro and the currencies it replaced.
    pub euro_rate: Option<Amount>,
}

fn currency_info(c: IsoCurrency) -> CurrencyInfo {
    let id = CommodityRef::currency(c.code);
    CurrencyInfo {
        euro_rate: euro_rate(&id).map(Amount),
        id: id.into(),
        code: c.code,
        fullname: c.fullname,
        exchange_code: c.exchange_code,
        fraction: c.fraction,
        symbol: Some(c.symbol).filter(|s| !s.is_empty()),
    }
}

#[allow(clippy::unnecessary_wraps)] // Every handler returns a Result.
fn currencies(_: &mut Context, _: crate::wire::NoParams) -> Result<Vec<CurrencyInfo>, RpcError> {
    Ok(iso_currencies().map(currency_info).collect())
}

/// Params of `currency.euroConvert`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EuroConvertParams {
    /// The amount, in `from`.
    pub amount: Amount,
    /// The currency of the amount: `EUR` or a currency the euro replaced.
    pub from: CommodityId,
    /// The currency wanted: `EUR` or a currency the euro replaced; one side must be `EUR`.
    pub to: CommodityId,
}

/// The result of `currency.euroConvert`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EuroConvertResult {
    /// The converted amount, rounded half away from zero to cents (to EUR) or to the currency's fraction.
    pub amount: Amount,
    /// The fixed rate used: units of the legacy currency per euro.
    pub rate: Amount,
}

fn euro_convert(_: &mut Context, p: EuroConvertParams) -> Result<EuroConvertResult, RpcError> {
    let amount = ops::euro_convert(&p.from.0, &p.to.0, p.amount.0)?;
    let legacy = if ops::canonical(&p.to.0).mnemonic == "EUR" {
        &p.from.0
    } else {
        &p.to.0
    };
    let rate = euro_rate(&ops::canonical(legacy)).ok_or_else(|| RpcError::invalid("no euro rate"))?;
    Ok(EuroConvertResult {
        amount: Amount(amount),
        rate: Amount(rate),
    })
}
