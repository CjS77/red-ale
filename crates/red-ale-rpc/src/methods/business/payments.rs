//! Methods: processing payments and listing documents that fall due. See `design/api/business-posting.md`.

use red_ale_core::business::{OwnerKind, OwnerRef};
use red_ale_core::ops::business::{calc, contacts, matching, payments};
use red_ale_core::{Book, Guid, Numeric};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::posting::{invoice, utc_day};
use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, Day, Id, Timestamp};

/// The business payments methods.
pub fn methods() -> Vec<Method> {
    vec![
        method("payment.process", "Record an owner's payment and apply it to chosen documents, or to the oldest open ones.", Effect::Write, process),
        method("invoice.pay", "Record a payment of one posted document.", Effect::Write, pay_invoice),
        method(
            "payment.openItems",
            "An owner's open documents and pre-payments in an A/R or A/P account, in the order GnuCash's payment dialog applies them.",
            Effect::Read,
            open_items,
        ),
        method(
            "payment.fromTransaction",
            "Turn an existing transaction into a payment and apply it, or re-match an existing payment; its other splits are deleted.",
            Effect::Destructive,
            from_transaction,
        ),
        method("invoice.due", "Posted, unpaid documents falling due within some days: GnuCash's bills-due and invoices-due reminders.", Effect::Read, due),
    ]
}

/// A kind of owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum OwnerType {
    /// A customer.
    Customer,
    /// A vendor.
    Vendor,
    /// An employee.
    Employee,
    /// A job (of a customer or vendor).
    Job,
}

impl From<OwnerType> for OwnerKind {
    fn from(t: OwnerType) -> Self {
        match t {
            OwnerType::Customer => Self::Customer,
            OwnerType::Vendor => Self::Vendor,
            OwnerType::Employee => Self::Employee,
            OwnerType::Job => Self::Job,
        }
    }
}

/// An owner: a customer, vendor, employee or job.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OwnerParam {
    /// Which kind.
    #[serde(rename = "type")]
    pub owner_type: OwnerType,
    /// Its GUID.
    pub id: Id,
}

impl OwnerParam {
    fn resolve(self, book: &Book) -> Result<OwnerRef, RpcError> {
        let owner = OwnerRef { kind: self.owner_type.into(), guid: self.id.0 };
        if contacts::owner_exists(book, owner) { Ok(owner) } else { Err(RpcError::not_found(owner.kind.type_name(), owner.guid)) }
    }
}

/// The engine amount: GnuCash's payment dialog treats a customer's payment as a credit and a vendor's or employee's
/// as a debit (`5.17:gnucash/gnome/dialog-payment.c:670-690`), so a payment in the usual direction is positive here
/// for every owner.
fn engine_amount(book: &Book, owner: OwnerRef, amount: Numeric) -> Result<Numeric, RpcError> {
    Ok(if calc::require_end_owner(book, owner)?.kind == OwnerKind::Customer { amount } else { amount.neg() })
}

/// Params of `payment.process`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProcessParams {
    /// Who pays or is paid.
    pub owner: OwnerParam,
    /// The A/Receivable or A/Payable account the documents are posted to.
    pub account: AccountRef,
    /// Where the money goes (customer payments) or comes from (vendor and employee payments). Needed unless the
    /// amount is zero.
    #[serde(default)]
    pub transfer_account: Option<AccountRef>,
    /// The payment, in the A/R or A/P account's commodity: positive when a customer pays or when a vendor or employee
    /// is paid; negative for a refund. Zero links the chosen documents with each other only.
    pub amount: Amount,
    /// Units of the transfer account's commodity per unit of the A/R or A/P account's commodity, when they differ.
    #[serde(default)]
    pub exchange_rate: Option<Amount>,
    /// The payment date.
    pub date: Day,
    /// Memo of both splits.
    #[serde(default)]
    pub memo: Option<String>,
    /// Transaction number (a check number).
    #[serde(default)]
    pub num: Option<String>,
    /// Documents (invoice GUIDs) and pre-payment lots (lot GUIDs) to apply the payment to, in order.
    #[serde(default)]
    pub apply_to: Vec<Id>,
    /// With no `applyTo`: apply to all the owner's open documents and pre-payments (GnuCash's preference, default false).
    #[serde(default)]
    pub auto_pay: bool,
}

/// Result of a payment.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentResult {
    /// The payment transaction (none when the amount is zero).
    pub transaction: Option<Id>,
    /// The lot holding what was not applied: a pre-payment.
    pub payment_lot: Option<Id>,
}

impl From<payments::PaymentResult> for PaymentResult {
    fn from(r: payments::PaymentResult) -> Self {
        Self { transaction: r.transaction.map(Id), payment_lot: r.payment_lot.map(Id) }
    }
}

/// A document's lot, or a lot named directly.
fn lot_of(book: &Book, id: Id) -> Result<Guid, RpcError> {
    if let Some(inv) = book.business.invoices.get(&id.0) {
        return inv.post_lot.ok_or_else(|| RpcError::invalid(format!("document {} is not posted", inv.id)));
    }
    book.lot(id.0).map(|l| l.guid).ok_or_else(|| RpcError::not_found("invoice or lot", id.0))
}

/// The money side of a payment, as the caller gives it.
struct Money {
    transfer: Option<AccountRef>,
    amount: Numeric,
    rate: Option<Amount>,
    date: Day,
    memo: Option<String>,
    num: Option<String>,
}

impl Money {
    /// The engine's payment. Its date is the day-neutral time, as GnuCash's dialog gives (`gdate_to_time64`,
    /// `5.17:libgnucash/engine/gnc-date.cpp:1323`).
    fn payment(self, book: &Book, owner: OwnerRef, posted: Guid) -> Result<payments::Payment, RpcError> {
        Ok(payments::Payment {
            owner,
            posted_account: posted,
            transfer_account: self.transfer.map(|a| a.resolve(book)).transpose()?,
            amount: engine_amount(book, owner, self.amount)?,
            exchange_rate: self.rate.map_or(Numeric::ONE, |r| r.0),
            date: self.date.0.neutral_time(),
            memo: self.memo.unwrap_or_default(),
            num: self.num.unwrap_or_default(),
        })
    }
}

fn process(ctx: &mut Context, p: ProcessParams) -> Result<PaymentResult, RpcError> {
    let result = ctx.mutate_rpc(|book, env| {
        let owner = p.owner.resolve(book)?;
        let posted = p.account.resolve(book)?;
        let lots = p.apply_to.iter().map(|id| lot_of(book, *id)).collect::<Result<Vec<_>, _>>()?;
        let money = Money { transfer: p.transfer_account, amount: p.amount.0, rate: p.exchange_rate, date: p.date, memo: p.memo, num: p.num };
        let pay = money.payment(book, owner, posted)?;
        Ok(payments::apply_owner_payment(book, env, &pay, &lots, p.auto_pay)?)
    })?;
    Ok(result.into())
}

/// Params of `invoice.pay`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PayParams {
    /// The posted document.
    pub invoice: Id,
    /// Where the money goes or comes from.
    pub transfer_account: AccountRef,
    /// The payment, signed as in `payment.process`.
    pub amount: Amount,
    /// Units of the transfer account's commodity per unit of the A/R or A/P account's commodity, when they differ.
    #[serde(default)]
    pub exchange_rate: Option<Amount>,
    /// The payment date.
    pub date: Day,
    /// Memo of both splits.
    #[serde(default)]
    pub memo: Option<String>,
    /// Transaction number.
    #[serde(default)]
    pub num: Option<String>,
}

fn pay_invoice(ctx: &mut Context, p: PayParams) -> Result<PaymentResult, RpcError> {
    let result = ctx.mutate_rpc(|book, env| {
        let inv = invoice(book, p.invoice)?;
        let (guid, owner) = (inv.guid, calc::require_end_owner(book, inv.owner)?);
        let posted = inv.post_acc.ok_or_else(|| RpcError::invalid(format!("document {} is not posted", inv.id)))?;
        let money = Money { transfer: Some(p.transfer_account), amount: p.amount.0, rate: p.exchange_rate, date: p.date, memo: p.memo, num: p.num };
        let pay = money.payment(book, owner, posted)?;
        Ok(payments::apply_invoice_payment(book, env, guid, pay)?)
    })?;
    Ok(result.into())
}

/// Params of `payment.openItems`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OpenItemsParams {
    /// A customer, vendor or employee.
    pub owner: OwnerParam,
    /// The A/Receivable or A/Payable account.
    pub account: AccountRef,
}

/// An open document or pre-payment.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenItem {
    /// The lot: what `applyTo` takes.
    pub lot: Id,
    /// The document; null for a pre-payment.
    pub invoice: Option<Id>,
    /// The document's number; null for a pre-payment.
    pub id: Option<String>,
    /// `Invoice`, `Bill`, `Expense`, `Credit Note` or `Pre-Payment`.
    #[serde(rename = "type")]
    pub item_type: String,
    /// The document's posted date, or the date of the pre-payment's latest split.
    pub date: Timestamp,
    /// What the payment dialog orders by: the document's due date, or the date of the pre-payment's earliest split.
    pub sort_date: Timestamp,
    /// The lot's balance, signed as on the account: positive is a debit.
    pub balance: Amount,
}

/// Result of `payment.openItems`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OpenItems {
    /// In the order GnuCash's payment dialog hands chosen items to the engine.
    pub items: Vec<OpenItem>,
}

fn open_items(ctx: &mut Context, p: OpenItemsParams) -> Result<OpenItems, RpcError> {
    ctx.query(|book, _| {
        let owner = p.owner.resolve(book)?;
        let account = p.account.resolve(book)?;
        if !book.account(account).is_some_and(|a| a.account_type.is_ap_ar()) {
            return Err(RpcError::invalid("the account must be an A/Receivable or A/Payable account"));
        }
        let items = matching::open_items(book, owner, account)?
            .into_iter()
            .map(|i| OpenItem {
                lot: Id(i.lot),
                invoice: i.document.map(|(g, _)| Id(g)),
                id: i.document.and_then(|(g, _)| book.business.invoices.get(&g)).map(|inv| inv.id.clone()),
                item_type: i.document.map_or("Pre-Payment", |(_, k)| k.type_string()).to_owned(),
                date: Timestamp(i.date),
                sort_date: Timestamp(i.sort_date),
                balance: Amount(i.balance),
            })
            .collect();
        Ok(OpenItems { items })
    })
}

/// Params of `payment.fromTransaction`. Absent params take the payment dialog's defaults.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FromTransactionParams {
    /// The transaction.
    pub transaction: Id,
    /// Who pays or is paid. Default: the owner of the document or pre-payment its A/R or A/P split is in.
    #[serde(default)]
    pub owner: Option<OwnerParam>,
    /// The A/Receivable or A/Payable account. Default: that of the transaction's A/R or A/P splits.
    #[serde(default)]
    pub account: Option<AccountRef>,
    /// The split to keep as the payment's transfer split. Default: the only split in a bank, cash, asset, liability
    /// or equity account.
    #[serde(default)]
    pub transfer_split: Option<Id>,
    /// The payment, signed as in `payment.process`. Default: the transfer split's amount; needed when the transfer
    /// and A/R or A/P commodities differ.
    #[serde(default)]
    pub amount: Option<Amount>,
    /// Units of the transfer account's commodity per unit of the A/R or A/P account's commodity, when they differ.
    #[serde(default)]
    pub exchange_rate: Option<Amount>,
    /// The payment date. Default: the transaction's posted date.
    #[serde(default)]
    pub date: Option<Day>,
    /// Memo of the A/R or A/P split. Default: the transaction's description.
    #[serde(default)]
    pub memo: Option<String>,
    /// Transaction number. Default: the transaction's.
    #[serde(default)]
    pub num: Option<String>,
    /// Documents (invoice GUIDs) and pre-payment lots to apply it to. Default: the documents and pre-payments the
    /// transaction is already applied to that would stay open without it.
    #[serde(default)]
    pub apply_to: Option<Vec<Id>>,
    /// With nothing to apply to: apply to all the owner's open documents and pre-payments.
    #[serde(default)]
    pub auto_pay: bool,
}

fn from_transaction(ctx: &mut Context, p: FromTransactionParams) -> Result<PaymentResult, RpcError> {
    let result = ctx.mutate_rpc(|book, env| {
        let t = book.transaction(p.transaction.0).ok_or_else(|| RpcError::not_found("transaction", p.transaction.0))?;
        let owner = match p.owner {
            Some(o) => o.resolve(book)?,
            None => matching::transaction_owner(book, t).ok_or_else(|| RpcError::invalid("the transaction names no owner: give the owner"))?,
        };
        let amount = p.amount.map(|a| engine_amount(book, owner, a.0)).transpose()?;
        let apply_to = p.apply_to.map(|ids| ids.into_iter().map(|id| lot_of(book, id)).collect::<Result<Vec<_>, _>>()).transpose()?;
        let req = matching::FromTransaction {
            transaction: p.transaction.0,
            owner,
            account: p.account.map(|a| a.resolve(book)).transpose()?,
            transfer_split: p.transfer_split.map(|s| s.0),
            amount,
            exchange_rate: p.exchange_rate.map_or(Numeric::ONE, |r| r.0),
            date: p.date.map(|d| d.0.neutral_time()),
            memo: p.memo,
            num: p.num,
            apply_to,
            auto_pay: p.auto_pay,
        };
        Ok(matching::payment_from_transaction(book, env, &req)?)
    })?;
    Ok(result.into())
}

/// Which reminder.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DueKind {
    /// Customer invoices and credit notes ("invoices due").
    Invoices,
    /// Bills, expense vouchers and their credit notes ("bills due").
    Bills,
}

/// Params of `invoice.due`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DueParams {
    /// Which documents.
    pub kind: DueKind,
    /// How many days ahead to look (GnuCash's `days-in-advance` preference, default 7). Negative counts as 0.
    #[serde(default = "default_days")]
    pub days_in_advance: i64,
    /// The moment to look from. Default: now.
    #[serde(default)]
    pub now: Option<Timestamp>,
}

fn default_days() -> i64 {
    7
}

/// A document in a reminder.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DueDocument {
    /// The document.
    pub invoice: Id,
    /// Its number.
    pub id: String,
    /// `Invoice`, `Bill`, `Expense` or `Credit Note`.
    #[serde(rename = "type")]
    pub doc_type: String,
    /// The customer, vendor or employee (a job's owner for a job's document).
    pub company: String,
    /// When it falls due.
    pub due: Timestamp,
    /// The due date.
    pub due_date: Day,
    /// What is still open: the balance of its lot, signed as on the A/R or A/P account.
    pub amount: Amount,
}

fn due(ctx: &mut Context, p: DueParams) -> Result<Vec<DueDocument>, RpcError> {
    ctx.query(|book, env| {
        let now = p.now.map_or_else(|| env.now(), |t| t.0);
        let which = match p.kind {
            DueKind::Invoices => payments::DueFor::Customers,
            DueKind::Bills => payments::DueFor::Vendors,
        };
        payments::documents_due(book, which, p.days_in_advance, now)?
            .into_iter()
            .map(|d| {
                let inv = invoice(book, Id(d.invoice))?;
                Ok(DueDocument {
                    invoice: Id(d.invoice),
                    id: inv.id.clone(),
                    doc_type: d.kind.type_string().to_owned(),
                    company: contacts::owner_name(book, d.kind.end_owner).unwrap_or_default(),
                    due: Timestamp(d.due),
                    due_date: Day(utc_day(d.due)?),
                    amount: Amount(d.balance),
                })
            })
            .collect()
    })
}
