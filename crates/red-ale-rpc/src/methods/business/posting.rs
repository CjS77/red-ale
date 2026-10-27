//! Methods: document totals, due dates, posting and unposting. See `design/api/business-posting.md`.

use red_ale_core::business::Invoice;
use red_ale_core::ops::business::{calc, posting};
use red_ale_core::{Book, Guid, Time64, TimeZone};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, CommodityId, Day, Done, Id, Timestamp};

/// The business posting methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "invoice.totals",
            "A document's subtotal, tax and total, per tax account and per entry, as GnuCash computes them.",
            Effect::Read,
            totals,
        ),
        method(
            "invoice.dueDate",
            "When a document posted on a date falls due under its (or a given) billing term.",
            Effect::Read,
            due_date,
        ),
        method(
            "invoice.post",
            "Post a document to an A/R or A/P account: its transaction, lot, frozen tax tables and terms.",
            Effect::Write,
            post,
        ),
        method(
            "invoice.unpost",
            "Unpost a document: delete its posted transaction and lot links; payments stay as pre-payments.",
            Effect::Destructive,
            unpost,
        ),
    ]
}

/// The invoice, bill or voucher with this GUID.
pub fn invoice(book: &Book, id: Id) -> Result<&Invoice, RpcError> {
    book.business
        .invoices
        .get(&id.0)
        .map(AsRef::as_ref)
        .ok_or_else(|| RpcError::not_found("invoice", id.0))
}

/// Params naming one document.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InvoiceParams {
    /// The invoice, bill or expense voucher.
    pub invoice: Id,
}

/// An amount for one account.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountAmount {
    /// The account.
    pub account: Id,
    /// Its full name.
    pub account_name: String,
    /// The amount.
    pub amount: Amount,
}

/// One entry's values.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntryTotals {
    /// The entry.
    pub entry: Id,
    /// Its value (net of discount and tax), rounded to the document currency.
    pub value: Amount,
    /// Its discount, rounded.
    pub discount: Amount,
    /// Its tax, rounded.
    pub tax: Amount,
}

/// Result of `invoice.totals`. Amounts are signed as printed on the document: positive for invoices and bills,
/// and for credit notes too.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TotalsResult {
    /// Σ of the entries' rounded values.
    pub subtotal: Amount,
    /// Σ of `taxes`.
    pub tax: Amount,
    /// Subtotal plus tax: what the document is for.
    pub total: Amount,
    /// Tax per tax account: each account's unrounded taxes summed over the entries, then rounded once.
    pub taxes: Vec<AccountAmount>,
    /// Each entry, in document order.
    pub entries: Vec<EntryTotals>,
}

/// Account values as wire amounts.
pub fn account_amounts(book: &Book, values: &calc::AccountValues) -> Vec<AccountAmount> {
    values
        .iter()
        .map(|(a, v)| AccountAmount {
            account: Id(*a),
            account_name: book.full_name(*a).unwrap_or_default(),
            amount: Amount(*v),
        })
        .collect()
}

fn totals(ctx: &mut Context, p: InvoiceParams) -> Result<TotalsResult, RpcError> {
    let book = ctx.book()?;
    let inv = invoice(book, p.invoice)?;
    let t = calc::document_totals(book, inv)?;
    let entries = calc::document_lines(book, inv)?
        .into_iter()
        .map(|l| EntryTotals {
            entry: Id(l.entry),
            value: Amount(l.value),
            discount: Amount(l.discount),
            tax: Amount(l.tax),
        })
        .collect();
    Ok(TotalsResult {
        subtotal: Amount(t.subtotal),
        tax: Amount(t.tax),
        total: Amount(t.total),
        taxes: account_amounts(book, &t.taxes),
        entries,
    })
}

/// Params of `invoice.dueDate`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DueDateParams {
    /// A document whose billing term to use. Give this or `term`.
    #[serde(default)]
    pub invoice: Option<Id>,
    /// A billing term to use. Give this or `invoice`.
    #[serde(default)]
    pub term: Option<Id>,
    /// The posting date.
    pub post_date: Day,
}

/// Result of `invoice.dueDate`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DueDateResult {
    /// The due date.
    pub due_date: Day,
    /// The instant GnuCash stores: the day-neutral time of the due date.
    pub due: Timestamp,
}

/// The billing term named by a document or directly.
fn term_of(book: &Book, invoice_id: Option<Id>, term: Option<Id>) -> Result<Option<Guid>, RpcError> {
    match (invoice_id, term) {
        (Some(i), None) => Ok(invoice(book, i)?.terms),
        (None, Some(t)) => book
            .business
            .bill_terms
            .contains_key(&t.0)
            .then_some(Some(t.0))
            .ok_or_else(|| RpcError::not_found("billing term", t.0)),
        _ => Err(RpcError::invalid_params("give exactly one of invoice and term")),
    }
}

fn due_date(ctx: &mut Context, p: DueDateParams) -> Result<DueDateResult, RpcError> {
    let zone = ctx.zone();
    let book = ctx.book()?;
    let term = term_of(book, p.invoice, p.term)?
        .and_then(|g| book.business.bill_terms.get(&g))
        .map(AsRef::as_ref);
    let due = calc::due_date(term, zone.neutral_time(p.post_date.0), &zone)?;
    Ok(DueDateResult {
        due_date: Day(local_day(due, &zone)?),
        due: Timestamp(due),
    })
}

/// The local calendar date of an instant.
pub fn local_day(t: Time64, zone: &TimeZone) -> Result<red_ale_core::Date, RpcError> {
    zone.local_date(t)
        .ok_or_else(|| RpcError::invalid(format!("time {} is out of range", t.0)))
}

/// The price of one unit of a commodity in the document currency.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PriceParam {
    /// The commodity of an account an entry posts to.
    pub commodity: CommodityId,
    /// Document-currency units per unit of `commodity`: GnuCash's invoice price, the inverse of the rate its
    /// dialog asks for.
    pub price: Amount,
}

/// Params of `invoice.post`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PostParams {
    /// The document.
    pub invoice: Id,
    /// The A/Receivable (customer documents) or A/Payable (bills, vouchers) account, in the document currency.
    pub account: AccountRef,
    /// The posting date.
    pub post_date: Day,
    /// The due date. Default: from the document's billing term, else the posting date.
    #[serde(default)]
    pub due_date: Option<Day>,
    /// Memo for the A/R or A/P split and accumulated splits.
    #[serde(default)]
    pub memo: Option<String>,
    /// One split per account rather than per entry (GnuCash's default: true).
    #[serde(default)]
    pub accumulate_splits: Option<bool>,
    /// Then apply the owner's open pre-payments and opposite documents (GnuCash's default: false).
    #[serde(default)]
    pub auto_pay: bool,
    /// Prices for entries posting to accounts in another commodity.
    #[serde(default)]
    pub prices: Vec<PriceParam>,
}

/// Result of `invoice.post`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostResult {
    /// The posted transaction.
    pub transaction: Id,
    /// The document's lot.
    pub lot: Id,
    /// The due instant stored on the transaction.
    pub due: Timestamp,
}

fn post(ctx: &mut Context, p: PostParams) -> Result<PostResult, RpcError> {
    let posted = ctx.mutate_rpc(|book, env| {
        let req = posting::PostRequest {
            invoice: invoice(book, p.invoice)?.guid,
            account: p.account.resolve(book)?,
            post_date: p.post_date.0,
            // GnuCash's date field gives the chosen day's neutral time (gnc-date-edit.c:1069).
            due: p.due_date.map(|d| env.zone().neutral_time(d.0)),
            memo: p.memo.unwrap_or_default(),
            accumulate: p.accumulate_splits.unwrap_or(true),
            autopay: p.auto_pay,
            prices: p.prices.into_iter().map(|x| (x.commodity.0, x.price.0)).collect(),
        };
        Ok(posting::post(book, env, &req)?)
    })?;
    Ok(PostResult {
        transaction: Id(posted.transaction),
        lot: Id(posted.lot),
        due: Timestamp(posted.due),
    })
}

/// Params of `invoice.unpost`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UnpostParams {
    /// The posted document.
    pub invoice: Id,
    /// Point the entries back at the tax tables their frozen copies came from (GnuCash's dialog default: false).
    #[serde(default)]
    pub reset_tax_tables: bool,
}

fn unpost(ctx: &mut Context, p: UnpostParams) -> Result<Done, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let guid = invoice(book, p.invoice)?.guid;
        Ok(posting::unpost(book, env, guid, p.reset_tax_tables)?)
    })?;
    Ok(Done {})
}
