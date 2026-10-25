//! Methods: tax tables and billing terms. See `design/api/business-contacts.md`.

use std::borrow::Cow;

use red_ale_core::business::{AmountType, BillTerm, BillTermKind, TaxTable, TaxTableEntry};
use red_ale_core::ops::business::terms::{
    add_bill_term, add_tax_table, bill_term_by_name, delete_bill_term, delete_tax_table, tax_table_by_name, update_bill_term, update_tax_table,
};
use red_ale_core::{Book, Guid, Numeric};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, Done, Id};

/// The tax table and billing term methods.
pub fn methods() -> Vec<Method> {
    vec![
        method("taxTable.create", "Create a tax table with one or more lines.", Effect::Write, tax_table_create),
        method("taxTable.get", "A tax table, with its lines and family links.", Effect::Read, tax_table_get),
        method("taxTable.list", "Tax tables; frozen copies made by posting only on request.", Effect::Read, tax_table_list),
        method("taxTable.update", "Rename a tax table or replace its lines.", Effect::Write, tax_table_update),
        method("taxTable.delete", "Delete a tax table nothing uses.", Effect::Destructive, tax_table_delete),
        method("billTerm.create", "Create a billing term (days or proximo).", Effect::Write, bill_term_create),
        method("billTerm.get", "A billing term, with its family links.", Effect::Read, bill_term_get),
        method("billTerm.list", "Billing terms; frozen copies made by posting only on request.", Effect::Read, bill_term_list),
        method("billTerm.update", "Change a billing term.", Effect::Write, bill_term_update),
        method("billTerm.delete", "Delete a billing term nothing uses.", Effect::Destructive, bill_term_delete),
    ]
}

macro_rules! named_ref {
    ($name:ident, $what:literal, $schema_name:literal, $collection:ident, $by_name:ident) => {
        #[doc = concat!("A ", $what, ", by GUID or by name. A name finds visible ones only; a GUID also finds frozen copies.")]
        #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            #[doc = concat!("The ", $what, "'s GUID in `book`.")]
            pub fn resolve(&self, book: &Book) -> Result<Guid, RpcError> {
                let text = self.0.trim();
                if let Ok(g) = Guid::parse(text)
                    && book.business.$collection.contains_key(&g)
                {
                    return Ok(g);
                }
                $by_name(book, text).map(|t| t.guid).ok_or_else(|| RpcError::not_found($what, text))
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                $schema_name.into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({ "type": "string", "description": concat!("A ", $what, ", by GUID or by name.") })
            }
        }
    };
}

named_ref!(TaxTableRef, "tax table", "TaxTableRef", tax_tables, tax_table_by_name);
named_ref!(BillTermRef, "billing term", "BillTermRef", bill_terms, bill_term_by_name);

// ---------------------------------------------------------------------------------------------------------------
// Tax tables.

/// Percentage or fixed amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AmountTypeWire {
    /// A percentage (`10` is 10 %).
    Percent,
    /// A fixed amount in the document's currency.
    Value,
}

impl From<AmountTypeWire> for AmountType {
    fn from(t: AmountTypeWire) -> Self {
        match t {
            AmountTypeWire::Percent => Self::Percent,
            AmountTypeWire::Value => Self::Value,
        }
    }
}

impl From<AmountType> for AmountTypeWire {
    fn from(t: AmountType) -> Self {
        match t {
            AmountType::Percent => Self::Percent,
            AmountType::Value => Self::Value,
        }
    }
}

/// One line of a tax table.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaxLineParams {
    /// The account the tax is posted to.
    pub account: AccountRef,
    /// The percentage (-100 to 100) or the fixed amount per line.
    pub amount: Amount,
    /// Percentage or fixed amount.
    #[serde(rename = "type")]
    pub kind: AmountTypeWire,
}

/// Params of `taxTable.create`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaxTableCreate {
    /// A name no other visible tax table has.
    pub name: String,
    /// At least one line.
    pub entries: Vec<TaxLineParams>,
}

/// Params of `taxTable.update`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaxTableUpdate {
    /// The tax table (visible ones only).
    pub tax_table: TaxTableRef,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// New lines, replacing all the old ones. Posted documents keep their frozen copy of the old lines.
    #[serde(default)]
    pub entries: Option<Vec<TaxLineParams>>,
}

/// Names one tax table.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaxTableKey {
    /// The tax table.
    pub tax_table: TaxTableRef,
}

/// Params of the lists of tax tables and billing terms.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FamilyListParams {
    /// Include the invisible frozen copies posting makes.
    #[serde(default)]
    pub include_frozen: bool,
}

/// One line of a tax table.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaxLineInfo {
    /// The tax account.
    pub account: Option<Id>,
    /// Its full name.
    pub account_name: Option<String>,
    /// Percentage or fixed amount.
    pub amount: Amount,
    /// Percentage or fixed amount.
    #[serde(rename = "type")]
    pub kind: AmountTypeWire,
}

/// A tax table.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaxTableInfo {
    /// Its GUID.
    pub guid: Id,
    /// Its name; frozen copies share their parent's.
    pub name: String,
    /// How many customers, vendors and lines use it (always 0 for a frozen copy).
    pub refcount: i64,
    /// True for frozen copies.
    pub invisible: bool,
    /// The table it is a frozen copy of.
    pub parent: Option<Id>,
    /// Its current frozen copy, reused by the next posting.
    pub child: Option<Id>,
    /// Lines, sorted by account full name then amount, as GnuCash keeps them.
    pub entries: Vec<TaxLineInfo>,
}

fn tax_table_info(book: &Book, t: &TaxTable) -> TaxTableInfo {
    TaxTableInfo {
        guid: t.guid.into(),
        name: t.name.clone(),
        refcount: t.refcount,
        invisible: t.invisible,
        parent: t.parent.map(Into::into),
        child: t.child.map(Into::into),
        entries: t
            .entries
            .iter()
            .map(|e| TaxLineInfo {
                account: e.account.map(Into::into),
                account_name: e.account.and_then(|a| book.full_name(a)),
                amount: e.amount.into(),
                kind: e.kind.into(),
            })
            .collect(),
    }
}

/// Tax tables.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaxTableList {
    /// Sorted by name, then GUID.
    pub tax_tables: Vec<TaxTableInfo>,
}

fn tax_lines(book: &Book, lines: Vec<TaxLineParams>) -> Result<Vec<TaxTableEntry>, RpcError> {
    lines.into_iter().map(|l| Ok(TaxTableEntry { account: Some(l.account.resolve(book)?), amount: l.amount.0, kind: l.kind.into() })).collect()
}

fn get_tax_table(book: &Book, guid: Guid) -> Result<TaxTableInfo, RpcError> {
    book.business.tax_tables.get(&guid).map(|t| tax_table_info(book, t)).ok_or_else(|| RpcError::not_found("tax table", guid))
}

fn tax_table_create(ctx: &mut Context, p: TaxTableCreate) -> Result<TaxTableInfo, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let lines = tax_lines(book, p.entries)?;
        let guid = add_tax_table(book, env, &p.name, lines)?;
        get_tax_table(book, guid)
    })
}

fn tax_table_get(ctx: &mut Context, p: TaxTableKey) -> Result<TaxTableInfo, RpcError> {
    let book = ctx.book()?;
    get_tax_table(book, p.tax_table.resolve(book)?)
}

fn tax_table_list(ctx: &mut Context, p: FamilyListParams) -> Result<TaxTableList, RpcError> {
    let book = ctx.book()?;
    let mut tables: Vec<&TaxTable> =
        book.business.tax_tables.values().map(AsRef::as_ref).filter(|t| p.include_frozen || (t.parent.is_none() && !t.invisible)).collect();
    tables.sort_by(|a, b| (&a.name, a.guid).cmp(&(&b.name, b.guid)));
    Ok(TaxTableList { tax_tables: tables.into_iter().map(|t| tax_table_info(book, t)).collect() })
}

fn tax_table_update(ctx: &mut Context, p: TaxTableUpdate) -> Result<TaxTableInfo, RpcError> {
    ctx.mutate_rpc(|book, _| {
        let guid = p.tax_table.resolve(book)?;
        let lines = p.entries.map(|l| tax_lines(book, l)).transpose()?;
        update_tax_table(book, guid, p.name.as_deref(), lines)?;
        get_tax_table(book, guid)
    })
}

fn tax_table_delete(ctx: &mut Context, p: TaxTableKey) -> Result<Done, RpcError> {
    ctx.mutate_rpc(|book, _| {
        let guid = p.tax_table.resolve(book)?;
        delete_tax_table(book, guid)?;
        Ok(Done {})
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Billing terms.

/// How a billing term sets the due date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum BillTermTypeWire {
    /// Due a number of days after posting.
    Days,
    /// Due on a day of a following month.
    Proximo,
}

/// Params of `billTerm.create`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BillTermCreate {
    /// A name no other visible billing term has.
    pub name: String,
    /// Description.
    #[serde(default)]
    pub description: String,
    /// Days or proximo.
    #[serde(rename = "type")]
    pub kind: BillTermTypeWire,
    /// Days: days until due. Proximo: the day of the month it is due (1-31).
    #[serde(default)]
    pub due_days: i64,
    /// Days: days the early-payment discount lasts. Proximo: the day of the month it ends. Stored only.
    #[serde(default)]
    pub discount_days: i64,
    /// Early-payment discount percentage. Stored only.
    #[serde(default)]
    pub discount: Option<Amount>,
    /// Proximo only: documents posted after this day of the month are due a month later; 0 or less counts back
    /// from the end of the month.
    #[serde(default)]
    pub cutoff: i64,
}

/// Params of `billTerm.update`: the fields to change.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BillTermUpdate {
    /// The billing term (visible ones only).
    pub bill_term: BillTermRef,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// New description.
    #[serde(default)]
    pub description: Option<String>,
    /// Days or proximo.
    #[serde(default, rename = "type")]
    pub kind: Option<BillTermTypeWire>,
    /// Days until due, or the proximo due day.
    #[serde(default)]
    pub due_days: Option<i64>,
    /// Discount days, or the proximo discount day.
    #[serde(default)]
    pub discount_days: Option<i64>,
    /// Discount percentage.
    #[serde(default)]
    pub discount: Option<Amount>,
    /// Proximo cutoff day.
    #[serde(default)]
    pub cutoff: Option<i64>,
}

/// Names one billing term.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BillTermKey {
    /// The billing term.
    pub bill_term: BillTermRef,
}

/// A billing term.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BillTermInfo {
    /// Its GUID.
    pub guid: Id,
    /// Its name; frozen copies share their parent's.
    pub name: String,
    /// Description.
    pub description: String,
    /// Days or proximo; absent for a term whose type was never set.
    #[serde(rename = "type")]
    pub kind: Option<BillTermTypeWire>,
    /// Days until due, or the proximo due day.
    pub due_days: i64,
    /// Discount days, or the proximo discount day.
    pub discount_days: i64,
    /// Discount percentage.
    pub discount: Amount,
    /// Proximo cutoff day.
    pub cutoff: i64,
    /// How many customers, vendors and documents use it (always 0 for a frozen copy).
    pub refcount: i64,
    /// True for frozen copies.
    pub invisible: bool,
    /// The term it is a frozen copy of.
    pub parent: Option<Id>,
    /// Its current frozen copy.
    pub child: Option<Id>,
}

/// The kind fields as GnuCash's accessors return them: type, due days, discount days, discount, cutoff.
fn kind_parts(kind: Option<BillTermKind>) -> (Option<BillTermTypeWire>, i64, i64, Numeric, i64) {
    match kind {
        Some(BillTermKind::Days { due_days, disc_days, discount }) => (Some(BillTermTypeWire::Days), due_days, disc_days, discount, 0),
        Some(BillTermKind::Proximo { due_day, disc_day, discount, cutoff_day }) => {
            (Some(BillTermTypeWire::Proximo), due_day, disc_day, discount, cutoff_day)
        }
        None => (None, 0, 0, Numeric::ZERO, 0),
    }
}

fn make_kind(kind: BillTermTypeWire, due: i64, disc: i64, discount: Numeric, cutoff: i64) -> BillTermKind {
    match kind {
        BillTermTypeWire::Days => BillTermKind::Days { due_days: due, disc_days: disc, discount },
        BillTermTypeWire::Proximo => BillTermKind::Proximo { due_day: due, disc_day: disc, discount, cutoff_day: cutoff },
    }
}

impl From<&BillTerm> for BillTermInfo {
    fn from(t: &BillTerm) -> Self {
        let (kind, due_days, discount_days, discount, cutoff) = kind_parts(t.kind);
        Self {
            guid: t.guid.into(),
            name: t.name.clone(),
            description: t.description.clone(),
            kind,
            due_days,
            discount_days,
            discount: discount.into(),
            cutoff,
            refcount: t.refcount,
            invisible: t.invisible,
            parent: t.parent.map(Into::into),
            child: t.child.map(Into::into),
        }
    }
}

/// Billing terms.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BillTermList {
    /// Sorted by name, then description, then GUID, as GnuCash lists them.
    pub bill_terms: Vec<BillTermInfo>,
}

fn get_bill_term(book: &Book, guid: Guid) -> Result<BillTermInfo, RpcError> {
    book.business.bill_terms.get(&guid).map(|t| t.as_ref().into()).ok_or_else(|| RpcError::not_found("billing term", guid))
}

fn bill_term_create(ctx: &mut Context, p: BillTermCreate) -> Result<BillTermInfo, RpcError> {
    let kind = make_kind(p.kind, p.due_days, p.discount_days, p.discount.map_or(Numeric::ZERO, |d| d.0), p.cutoff);
    ctx.mutate_rpc(|book, env| {
        let guid = add_bill_term(book, env, &p.name, &p.description, kind)?;
        get_bill_term(book, guid)
    })
}

fn bill_term_get(ctx: &mut Context, p: BillTermKey) -> Result<BillTermInfo, RpcError> {
    let book = ctx.book()?;
    get_bill_term(book, p.bill_term.resolve(book)?)
}

fn bill_term_list(ctx: &mut Context, p: FamilyListParams) -> Result<BillTermList, RpcError> {
    let book = ctx.book()?;
    let mut terms: Vec<&BillTerm> =
        book.business.bill_terms.values().map(AsRef::as_ref).filter(|t| p.include_frozen || (t.parent.is_none() && !t.invisible)).collect();
    terms.sort_by(|a, b| (&a.name, &a.description, a.guid).cmp(&(&b.name, &b.description, b.guid)));
    Ok(BillTermList { bill_terms: terms.into_iter().map(Into::into).collect() })
}

fn bill_term_update(ctx: &mut Context, p: BillTermUpdate) -> Result<BillTermInfo, RpcError> {
    ctx.mutate_rpc(|book, _| {
        let guid = p.bill_term.resolve(book)?;
        let current = book.business.bill_terms.get(&guid).ok_or_else(|| RpcError::not_found("billing term", guid))?;
        let (kind, due, disc, discount, cutoff) = kind_parts(current.kind);
        let changes_kind = p.kind.is_some() || p.due_days.is_some() || p.discount_days.is_some() || p.discount.is_some() || p.cutoff.is_some();
        let new_kind = if changes_kind {
            let kind = p.kind.or(kind).ok_or_else(|| RpcError::invalid("the billing term needs a type"))?;
            let discount = p.discount.map_or(discount, |d| d.0);
            Some(make_kind(kind, p.due_days.unwrap_or(due), p.discount_days.unwrap_or(disc), discount, p.cutoff.unwrap_or(cutoff)))
        } else {
            None
        };
        update_bill_term(book, guid, p.name.as_deref(), p.description.as_deref(), new_kind)?;
        get_bill_term(book, guid)
    })
}

fn bill_term_delete(ctx: &mut Context, p: BillTermKey) -> Result<Done, RpcError> {
    ctx.mutate_rpc(|book, _| {
        let guid = p.bill_term.resolve(book)?;
        delete_bill_term(book, guid)?;
        Ok(Done {})
    })
}
