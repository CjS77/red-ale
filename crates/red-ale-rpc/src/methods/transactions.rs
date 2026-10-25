//! Methods: transactions and their splits — post, get, list, update, delete, void, unvoid, reverse, transfer and
//! find. The rules live in `red_ale_core::ops::transactions`; this module translates wire types.

mod query;

use red_ale_core::ops::commit::{NewSplit, NewTransaction};
use red_ale_core::ops::transactions::{self as ops, ListFilter, SplitEdit, TransactionEdit, find, transfer, void};
use red_ale_core::{Book, CommodityRef, Date, Env, Guid, ReconcileState, Split, Transaction};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, CommodityId, Day, Done, Id, Timestamp};

pub use query::{FindParams, FindResult};

/// The transactions methods.
pub fn methods() -> Vec<Method> {
    vec![
        method("transaction.post", "Post a balanced transaction; refused if unbalanced.", Effect::Write, post),
        method("transaction.get", "A transaction with its splits.", Effect::Read, get),
        method("transaction.list", "Transactions, optionally in one account and a date range, in GnuCash's order.", Effect::Read, list),
        method("transaction.update", "Change a transaction's fields, currency and splits; the result must balance.", Effect::Write, update),
        method("transaction.delete", "Delete a transaction that is not read-only.", Effect::Destructive, delete),
        method("transaction.void", "Void a transaction as GnuCash does: values kept in slots, splits zeroed and marked v.", Effect::Write, void_txn),
        method("transaction.unvoid", "Restore a voided transaction.", Effect::Write, unvoid),
        method("transaction.reverse", "Create the reversing transaction of a transaction.", Effect::Write, reverse),
        method("transaction.transfer", "Transfer between two accounts, at a rate when their commodities differ, recording the price.", Effect::Write, transfer_),
        method("transaction.find", "Find splits with the criteria of GnuCash's Find Transaction dialog.", Effect::Read, query::find_),
    ]
}

/// A split's reconcile state, as GnuCash writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ReconcileFlag {
    /// Not reconciled or cleared.
    #[serde(rename = "n")]
    New,
    /// Cleared.
    #[serde(rename = "c")]
    Cleared,
    /// Reconciled.
    #[serde(rename = "y")]
    Reconciled,
    /// Frozen.
    #[serde(rename = "f")]
    Frozen,
    /// Voided.
    #[serde(rename = "v")]
    Voided,
}

impl From<ReconcileFlag> for ReconcileState {
    fn from(f: ReconcileFlag) -> Self {
        match f {
            ReconcileFlag::New => Self::New,
            ReconcileFlag::Cleared => Self::Cleared,
            ReconcileFlag::Reconciled => Self::Reconciled,
            ReconcileFlag::Frozen => Self::Frozen,
            ReconcileFlag::Voided => Self::Voided,
        }
    }
}

impl From<ReconcileState> for ReconcileFlag {
    fn from(s: ReconcileState) -> Self {
        match s {
            ReconcileState::New => Self::New,
            ReconcileState::Cleared => Self::Cleared,
            ReconcileState::Reconciled => Self::Reconciled,
            ReconcileState::Frozen => Self::Frozen,
            ReconcileState::Voided => Self::Voided,
        }
    }
}

/// A split of a transaction, as returned.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SplitInfo {
    /// The split's GUID.
    pub guid: Id,
    /// The account's GUID; absent for an orphan split.
    pub account: Option<Id>,
    /// The account's full name.
    pub account_name: Option<String>,
    /// The account's commodity, which the amount is in.
    pub commodity: Option<CommodityId>,
    /// Value, in the transaction currency.
    pub value: Amount,
    /// Amount (GnuCash's quantity), in the account's commodity.
    pub amount: Amount,
    /// Memo.
    pub memo: Option<String>,
    /// Action.
    pub action: Option<String>,
    /// Reconcile state.
    pub reconcile_state: ReconcileFlag,
    /// When it was reconciled.
    pub reconcile_date: Option<Timestamp>,
    /// The lot it belongs to.
    pub lot: Option<Id>,
}

/// A transaction, as returned by every transaction method.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInfo {
    /// The transaction's GUID.
    pub guid: Id,
    /// The transaction currency, which values are in.
    pub currency: CommodityId,
    /// Check or reference number.
    pub num: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// The posted date.
    pub date_posted: Option<Day>,
    /// The posted instant as stored (10:59 UTC on the posted date for transactions GnuCash or red-ale enters).
    pub date_posted_time: Timestamp,
    /// When it was entered.
    pub date_entered: Timestamp,
    /// Void, by GnuCash 5.17's rule: the void timestamp is set.
    pub void: bool,
    /// Why it was voided.
    pub void_reason: Option<String>,
    /// When it was voided.
    pub void_time: Option<Timestamp>,
    /// Why it may not be changed, when read-only (voided, or generated by a business document).
    pub read_only: Option<String>,
    /// The transaction that reverses this one.
    pub reversed_by: Option<Id>,
    /// A book-closing transaction.
    pub closing: bool,
    /// The split values sum to zero.
    pub balanced: bool,
    /// Its splits, in stored order (debits first).
    pub splits: Vec<SplitInfo>,
}

fn split_info(book: &Book, s: &Split) -> SplitInfo {
    let account = s.account.and_then(|a| book.account(a));
    SplitInfo {
        guid: s.guid.into(),
        account: s.account.map(Id),
        account_name: s.account.and_then(|a| book.full_name(a)),
        commodity: account.and_then(|a| a.commodity.clone()).map(CommodityId),
        value: s.value.into(),
        amount: s.amount.into(),
        memo: s.memo.clone(),
        action: s.action.clone(),
        reconcile_state: s.reconcile_state.into(),
        reconcile_date: s.reconcile_date.map(Timestamp),
        lot: s.lot.map(Id),
    }
}

/// Describe a transaction.
pub fn info(book: &Book, t: &Transaction) -> TransactionInfo {
    use red_ale_core::transaction::slot_paths;
    TransactionInfo {
        guid: t.guid.into(),
        currency: CommodityId(t.currency.clone()),
        num: t.num.clone(),
        description: t.description.clone(),
        notes: t.notes().map(str::to_owned),
        date_posted: ops::posted_date(t).map(Day),
        date_posted_time: Timestamp(t.date_posted),
        date_entered: Timestamp(t.date_entered),
        void: t.is_void(),
        void_reason: t.slots.get_str(slot_paths::VOID_REASON).map(str::to_owned),
        void_time: t.void_time().map(Timestamp),
        read_only: t.read_only_reason().map(str::to_owned),
        reversed_by: t.reversed_by().map(Id),
        closing: t.is_closing(),
        balanced: t.is_balanced(),
        splits: t.splits.iter().map(|s| split_info(book, s)).collect(),
    }
}

fn info_of(book: &Book, guid: Guid) -> Result<TransactionInfo, RpcError> {
    Ok(info(book, ops::get(book, guid)?))
}

/// A split to create.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SplitParams {
    /// The account.
    pub account: AccountRef,
    /// Value, in the transaction currency. Rounded half up to the currency's fraction; a non-zero value that would
    /// round to zero is refused.
    pub value: Amount,
    /// Amount, in the account's commodity, rounded half up to the account's SCU. Required when the account's
    /// commodity is not the transaction currency; otherwise it defaults to the value.
    #[serde(default)]
    pub amount: Option<Amount>,
    /// Memo.
    #[serde(default)]
    pub memo: Option<String>,
    /// Action.
    #[serde(default)]
    pub action: Option<String>,
    /// Reconcile state; `n` by default. `v` is refused: use `transaction.void`.
    #[serde(default)]
    pub reconcile_state: Option<ReconcileFlag>,
    /// When it was reconciled.
    #[serde(default)]
    pub reconcile_date: Option<Timestamp>,
    /// A lot of the same account to put the split in.
    #[serde(default)]
    pub lot: Option<Id>,
}

impl SplitParams {
    fn resolve(self, book: &Book) -> Result<NewSplit, RpcError> {
        Ok(NewSplit {
            account: self.account.resolve(book)?,
            value: self.value.0,
            amount: self.amount.map(|a| a.0),
            memo: self.memo,
            action: self.action,
            reconcile_state: self.reconcile_state.map_or(ReconcileState::New, Into::into),
            reconcile_date: self.reconcile_date.map(|t| t.0),
            lot: self.lot.map(|l| l.0),
        })
    }
}

/// Params of `transaction.post`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PostParams {
    /// The transaction currency. Defaults to the first split's account commodity, when that is a currency.
    #[serde(default)]
    pub currency: Option<CommodityId>,
    /// The posted date; today by default. Stored at 10:59 UTC with a `date-posted` slot, as GnuCash does.
    #[serde(default)]
    pub date_posted: Option<Day>,
    /// When it was entered; now by default.
    #[serde(default)]
    pub date_entered: Option<Timestamp>,
    /// Check or reference number.
    #[serde(default)]
    pub num: Option<String>,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
    /// Notes; empty means none.
    #[serde(default)]
    pub notes: Option<String>,
    /// The splits. Their values must sum to zero.
    pub splits: Vec<SplitParams>,
    /// Record each priced split's value/amount in the price database, as GnuCash's register does
    /// (`xaccTransRecordPrice` with source `user:split-register`).
    #[serde(default)]
    pub record_price: bool,
}

fn default_currency(book: &Book, splits: &[NewSplit]) -> Result<CommodityRef, RpcError> {
    splits
        .first()
        .and_then(|s| book.account(s.account))
        .and_then(|a| a.commodity.clone())
        .filter(CommodityRef::is_currency)
        .ok_or_else(|| RpcError::invalid("give the transaction currency: the first split's account does not hold a currency"))
}

/// Today's date in UTC, the default posted date (red-ale has no local time zone yet).
fn today(env: &mut dyn Env) -> Result<Date, RpcError> {
    env.now().utc_date().ok_or_else(|| RpcError::internal("the clock is out of range"))
}

/// A given date, or today.
fn date_or_today(env: &mut dyn Env, d: Option<Day>) -> Result<Date, RpcError> {
    d.map_or_else(|| today(env), |d| Ok(d.0))
}

fn record(book: &mut Book, env: &mut dyn Env, guid: Guid, on: bool) -> Result<(), RpcError> {
    if on {
        transfer::record_prices(book, env, guid, transfer::SOURCE_SPLIT_REGISTER)?;
    }
    Ok(())
}

fn post(ctx: &mut Context, p: PostParams) -> Result<TransactionInfo, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let splits = p.splits.into_iter().map(|s| s.resolve(book)).collect::<Result<Vec<_>, _>>()?;
        let currency = match p.currency {
            Some(c) => c.0,
            None => default_currency(book, &splits)?,
        };
        let new = NewTransaction {
            currency,
            date_posted: date_or_today(env, p.date_posted)?,
            date_entered: p.date_entered.map(|t| t.0),
            num: p.num,
            description: p.description,
            notes: p.notes,
            splits,
        };
        let guid = ops::post(book, env, new)?;
        record(book, env, guid, p.record_price)?;
        info_of(book, guid)
    })
}

/// Params naming one transaction.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GuidParams {
    /// The transaction's GUID.
    pub guid: Id,
}

fn get(ctx: &mut Context, p: GuidParams) -> Result<TransactionInfo, RpcError> {
    let book = ctx.book()?;
    info_of(book, p.guid.0)
}

/// Params of `transaction.list`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListParams {
    /// Only transactions with a split in this account.
    #[serde(default)]
    pub account: Option<AccountRef>,
    /// Only transactions posted on or after this date.
    #[serde(default)]
    pub from: Option<Day>,
    /// Only transactions posted on or before this date.
    #[serde(default)]
    pub to: Option<Day>,
}

/// The result of `transaction.list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ListResult {
    /// The transactions, by posted date, then num, date entered, description and GUID (`xaccTransOrder`).
    pub transactions: Vec<TransactionInfo>,
}

fn list(ctx: &mut Context, p: ListParams) -> Result<ListResult, RpcError> {
    let book = ctx.book()?;
    let filter = ListFilter { account: p.account.map(|a| a.resolve(book)).transpose()?, from: p.from.map(|d| d.0), to: p.to.map(|d| d.0) };
    Ok(ListResult { transactions: ops::list(book, &filter).into_iter().map(|t| info(book, t)).collect() })
}

/// A change to an existing split; absent fields are kept.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SplitChange {
    /// The split's GUID.
    pub guid: Id,
    /// A new account. Moving to an account in another commodity needs a new amount.
    #[serde(default)]
    pub account: Option<AccountRef>,
    /// A new value. When the account holds the transaction currency, the amount follows it.
    #[serde(default)]
    pub value: Option<Amount>,
    /// A new amount.
    #[serde(default)]
    pub amount: Option<Amount>,
    /// A new memo; empty clears it.
    #[serde(default)]
    pub memo: Option<String>,
    /// A new action; empty clears it.
    #[serde(default)]
    pub action: Option<String>,
    /// A new reconcile state; `v` is refused.
    #[serde(default)]
    pub reconcile_state: Option<ReconcileFlag>,
    /// A new reconcile date.
    #[serde(default)]
    pub reconcile_date: Option<Timestamp>,
}

impl SplitChange {
    fn resolve(self, book: &Book) -> Result<SplitEdit, RpcError> {
        Ok(SplitEdit {
            guid: self.guid.0,
            account: self.account.map(|a| a.resolve(book)).transpose()?,
            value: self.value.map(|v| v.0),
            amount: self.amount.map(|a| a.0),
            memo: self.memo,
            action: self.action,
            reconcile_state: self.reconcile_state.map(Into::into),
            reconcile_date: self.reconcile_date.map(|t| t.0),
        })
    }
}

/// Params of `transaction.update`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateParams {
    /// The transaction's GUID.
    pub guid: Id,
    /// A new currency. Existing values are re-valued as `xaccTransSetCurrency` does, before the split changes.
    #[serde(default)]
    pub currency: Option<CommodityId>,
    /// A new posted date.
    #[serde(default)]
    pub date_posted: Option<Day>,
    /// A new entered date.
    #[serde(default)]
    pub date_entered: Option<Timestamp>,
    /// A new num; empty clears it.
    #[serde(default)]
    pub num: Option<String>,
    /// A new description.
    #[serde(default)]
    pub description: Option<String>,
    /// New notes; empty deletes them.
    #[serde(default)]
    pub notes: Option<String>,
    /// Splits to remove, by GUID.
    #[serde(default)]
    pub remove_splits: Vec<Id>,
    /// Splits to change.
    #[serde(default)]
    pub change_splits: Vec<SplitChange>,
    /// Splits to add.
    #[serde(default)]
    pub add_splits: Vec<SplitParams>,
    /// Record prices from priced splits afterwards, as for `transaction.post`.
    #[serde(default)]
    pub record_price: bool,
}

fn update(ctx: &mut Context, p: UpdateParams) -> Result<TransactionInfo, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let edit = TransactionEdit {
            currency: p.currency.map(|c| c.0),
            date_posted: p.date_posted.map(|d| d.0),
            date_entered: p.date_entered.map(|t| t.0),
            num: p.num,
            description: p.description,
            notes: p.notes,
            remove_splits: p.remove_splits.into_iter().map(|g| g.0).collect(),
            change_splits: p.change_splits.into_iter().map(|c| c.resolve(book)).collect::<Result<_, _>>()?,
            add_splits: p.add_splits.into_iter().map(|s| s.resolve(book)).collect::<Result<_, _>>()?,
        };
        ops::update(book, env, p.guid.0, edit)?;
        record(book, env, p.guid.0, p.record_price)?;
        info_of(book, p.guid.0)
    })
}

fn delete(ctx: &mut Context, p: GuidParams) -> Result<Done, RpcError> {
    ctx.mutate(|book, _| ops::delete(book, p.guid.0))?;
    Ok(Done {})
}

/// Params of `transaction.void`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VoidParams {
    /// The transaction's GUID.
    pub guid: Id,
    /// Why; empty writes no `void-reason` slot, as GnuCash does, and the transaction is still void.
    #[serde(default)]
    pub reason: String,
}

fn void_txn(ctx: &mut Context, p: VoidParams) -> Result<TransactionInfo, RpcError> {
    ctx.mutate_rpc(|book, env| {
        void::void(book, env, p.guid.0, &p.reason)?;
        info_of(book, p.guid.0)
    })
}

fn unvoid(ctx: &mut Context, p: GuidParams) -> Result<TransactionInfo, RpcError> {
    ctx.mutate_rpc(|book, _| {
        void::unvoid(book, p.guid.0)?;
        info_of(book, p.guid.0)
    })
}

/// Params of `transaction.reverse`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReverseParams {
    /// The transaction to reverse.
    pub guid: Id,
    /// The reversing transaction's posted date; today by default, as in GnuCash's dialog.
    #[serde(default)]
    pub date: Option<Day>,
}

fn reverse(ctx: &mut Context, p: ReverseParams) -> Result<TransactionInfo, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let date = date_or_today(env, p.date)?;
        let guid = void::reverse(book, env, p.guid.0, date)?;
        info_of(book, guid)
    })
}

/// Params of `transaction.transfer`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TransferParams {
    /// The account the amount leaves; it must hold a currency.
    pub from: AccountRef,
    /// The account it reaches.
    pub to: AccountRef,
    /// The amount, in the from account's currency; not zero.
    pub amount: Amount,
    /// The posted date; today by default.
    #[serde(default)]
    pub date: Option<Day>,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
    /// Num.
    #[serde(default)]
    pub num: Option<String>,
    /// Notes.
    #[serde(default)]
    pub notes: Option<String>,
    /// Memo, on both splits.
    #[serde(default)]
    pub memo: Option<String>,
    /// When the commodities differ: units of the to commodity per unit of the from currency. The amount received is
    /// amount × rate rounded half up to the to account's SCU.
    #[serde(default)]
    pub rate: Option<Amount>,
    /// When the commodities differ: the amount received, in the to commodity; the rate is computed from it. Give
    /// `rate` or `toAmount`, or neither to use the rate the price database gives on the date (or the nearest one).
    #[serde(default)]
    pub to_amount: Option<Amount>,
    /// Record the rate in the price database, as the dialog does. True by default.
    #[serde(default = "yes")]
    pub record_price: bool,
    /// The user's default currency (a GnuCash preference): a new price between it and another currency is stored as
    /// the other currency priced in it.
    #[serde(default)]
    pub default_currency: Option<CommodityId>,
}

fn yes() -> bool {
    true
}

/// What happened to the price database.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PriceChangeInfo {
    /// `created`, `updated`, or `kept` (a same-day price was preferred or already equal).
    pub action: &'static str,
    /// The price's GUID.
    pub guid: Id,
}

impl From<transfer::PriceChange> for PriceChangeInfo {
    fn from(c: transfer::PriceChange) -> Self {
        let (action, g) = match c {
            transfer::PriceChange::Created(g) => ("created", g),
            transfer::PriceChange::Updated(g) => ("updated", g),
            transfer::PriceChange::Kept(g) => ("kept", g),
        };
        Self { action, guid: g.into() }
    }
}

/// The result of `transaction.transfer`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransferResult {
    /// The transaction created.
    pub transaction: TransactionInfo,
    /// The amount received, in the to commodity.
    pub to_amount: Amount,
    /// The rate used, when the commodities differ.
    pub rate: Option<Amount>,
    /// The price recorded, when the commodities differ.
    pub price: Option<PriceChangeInfo>,
}

fn transfer_(ctx: &mut Context, p: TransferParams) -> Result<TransferResult, RpcError> {
    let rate = match (p.rate, p.to_amount) {
        (Some(_), Some(_)) => return Err(RpcError::invalid("give rate or toAmount, not both")),
        (Some(r), None) => transfer::Rate::Rate(r.0),
        (None, Some(to)) => transfer::Rate::ToAmount(to.0),
        (None, None) => transfer::Rate::FromPriceDb,
    };
    ctx.mutate_rpc(|book, env| {
        let x = transfer::Transfer {
            from: p.from.resolve(book)?,
            to: p.to.resolve(book)?,
            amount: p.amount.0,
            date: date_or_today(env, p.date)?,
            description: p.description,
            num: p.num,
            notes: p.notes,
            memo: p.memo,
            rate,
            record_price: p.record_price,
            default_currency: p.default_currency.map(|c| c.0),
        };
        let out = transfer::transfer(book, env, &x)?;
        Ok(TransferResult {
            transaction: info_of(book, out.transaction)?,
            to_amount: out.to_amount.into(),
            rate: out.rate.map(Amount),
            price: out.price.map(Into::into),
        })
    })
}

/// The splits found, as returned by `transaction.find`.
fn found(book: &Book, hits: &[find::Found]) -> Vec<query::Match> {
    hits.iter()
        .map(|h| query::Match {
            transaction: h.transaction.into(),
            split: h.split.into(),
            account: book.transaction(h.transaction).and_then(|t| t.split(h.split)).and_then(|s| s.account).map(Id),
        })
        .collect()
}
