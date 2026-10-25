//! Methods: lots, the lot viewer, capital gains and the stock split assistant. See `design/api/lots.md`.

use std::sync::Arc;

use red_ale_core::ops::lots::{self, LotSummary, PriceOutcome, SplitKind, SplitView, StockSplit};
use red_ale_core::{Book, DomainResult, Env, Guid, Time64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, CommodityId, Day, Id};

/// The lots methods.
pub fn methods() -> Vec<Method> {
    vec![
        method("lot.list", "List an account's lots as the lot viewer shows them, optionally only open or closed ones.", Effect::Read, list),
        method("lot.get", "Describe one lot with its splits.", Effect::Read, get),
        method("lot.freeSplits", "List an account's splits that are in no lot, with the gain each would realise against a lot.", Effect::Read, free_splits),
        method("lot.create", "Make an empty lot in an account, titled `Lot <n>` unless a title is given.", Effect::Write, create),
        method("lot.update", "Set a lot's title or notes; an empty string removes one.", Effect::Write, update),
        method("lot.delete", "Delete a lot; its splits stay but leave the lot. A lot with splits needs releaseSplits.", Effect::Destructive, delete),
        method("lot.assignSplit", "Put a split in a lot, cutting it if it would overfill the lot; without a lot, FIFO chooses.", Effect::Write, assign),
        method("lot.removeSplit", "Take a split out of its lot; a lot left empty is deleted.", Effect::Write, remove),
        method("lot.scrub", "Repair one lot: merge cut pieces, refill it and recompute its capital gains.", Effect::Write, scrub),
        method("lot.scrubAccount", "Assign an account's splits to lots by FIFO and repair every lot, optionally for its sub-accounts too.", Effect::Write, scrub_account),
        method("lot.computeGains", "Recompute a lot's capital gains, recording them in gains transactions.", Effect::Write, compute_gains),
        method("stock.split", "Record a stock split, with an optional price and cash in lieu, as GnuCash's assistant does.", Effect::Write, stock_split),
    ]
}

/// Which lots to list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LotState {
    /// Every lot.
    #[default]
    All,
    /// Lots that are not closed.
    Open,
    /// Lots whose balance is zero.
    Closed,
}

/// Params of `lot.list`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListParams {
    /// The account.
    pub account: AccountRef,
    /// `all` (default), `open` or `closed`.
    #[serde(default)]
    pub state: LotState,
}

/// Params naming a lot.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LotParams {
    /// The lot.
    pub lot: Id,
}

/// Params of `lot.freeSplits`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FreeSplitsParams {
    /// The account.
    pub account: AccountRef,
    /// A lot of the account to compute each split's gain against.
    #[serde(default)]
    pub lot: Option<Id>,
}

/// Params of `lot.create`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateParams {
    /// The account to hold the lot.
    pub account: AccountRef,
    /// Title; by default `Lot <n>`.
    #[serde(default)]
    pub title: Option<String>,
    /// Notes.
    #[serde(default)]
    pub notes: Option<String>,
}

/// Params of `lot.update`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateParams {
    /// The lot.
    pub lot: Id,
    /// New title; an empty string removes it.
    #[serde(default)]
    pub title: Option<String>,
    /// New notes; an empty string removes them.
    #[serde(default)]
    pub notes: Option<String>,
}

/// Params of `lot.delete`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeleteParams {
    /// The lot.
    pub lot: Id,
    /// Take the lot's splits out of it first; without this, a lot with splits is refused.
    #[serde(default)]
    pub release_splits: bool,
}

/// Params of `lot.assignSplit`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AssignParams {
    /// The split.
    pub split: Id,
    /// The lot; without it, FIFO chooses and opens lots as needed.
    #[serde(default)]
    pub lot: Option<Id>,
}

/// Params of `lot.removeSplit`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RemoveParams {
    /// The split.
    pub split: Id,
}

/// Params of `lot.scrubAccount`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ScrubAccountParams {
    /// The account.
    pub account: AccountRef,
    /// Scrub every sub-account first, as GnuCash's tree scrub does.
    #[serde(default)]
    pub subaccounts: bool,
}

/// Params of `lot.computeGains`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ComputeGainsParams {
    /// The lot.
    pub lot: Id,
    /// The account new gains transactions credit, used when its commodity is the transaction currency; by default
    /// the lot account's gains account (`Orphaned Gains-<CUR>` unless one is set).
    #[serde(default)]
    pub gains_account: Option<AccountRef>,
}

/// Params of `stock.split`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StockSplitParams {
    /// The STOCK, MUTUAL or CURRENCY account; it must hold shares and not be a placeholder.
    pub account: AccountRef,
    /// The date of the split.
    pub date: Day,
    /// Shares added by the split (negative for a reverse split), rounded to the account's SCU.
    pub shares: Amount,
    /// Description; default `Stock Split`.
    #[serde(default)]
    pub description: Option<String>,
    /// The transaction currency; default the currency of the nearest ancestor account held in a currency.
    #[serde(default)]
    pub currency: Option<CommodityId>,
    /// Price per share after the split; recorded in the price database when positive.
    #[serde(default)]
    pub price: Option<Amount>,
    /// The price's currency; default the other side of the security's latest price, else the transaction currency.
    #[serde(default)]
    pub price_currency: Option<CommodityId>,
    /// Cash paid in lieu of fractional shares; recorded when positive.
    #[serde(default)]
    pub cash: Option<Amount>,
    /// The INCOME account the cash comes from (required with cash).
    #[serde(default)]
    pub income_account: Option<AccountRef>,
    /// The BANK, CASH or ASSET account the cash goes to (required with cash).
    #[serde(default)]
    pub asset_account: Option<AccountRef>,
    /// Memo of the cash splits; default `Cash in lieu`.
    #[serde(default)]
    pub memo: Option<String>,
}

/// A lot, as the lot viewer lists it.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LotInfo {
    /// The lot.
    pub guid: Id,
    /// Its account.
    pub account: Id,
    /// Title.
    pub title: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// The invoice the lot belongs to (the viewer's type `I`).
    pub invoice: Option<Id>,
    /// The lot has splits and its balance is zero.
    pub closed: bool,
    /// Posted date of its earliest split.
    pub open_date: Option<Day>,
    /// Posted date of its latest split, when closed.
    pub close_date: Option<Day>,
    /// Sum of its splits' amounts, restated after stock splits.
    pub balance: Amount,
    /// Currency of its realised gains.
    pub gains_currency: Option<CommodityId>,
    /// Realised gains: the values of its gains splits in that currency.
    pub realized_gains: Amount,
    /// How many splits it holds.
    pub split_count: usize,
}

/// A split, as the lot viewer lists it.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LotSplitInfo {
    /// The split.
    pub split: Id,
    /// Its transaction.
    pub transaction: Id,
    /// The transaction's posted date (UTC calendar date; GnuCash posts at 10:59 UTC).
    pub date_posted: Option<Day>,
    /// The transaction's number.
    pub num: Option<String>,
    /// The transaction's description.
    pub description: Option<String>,
    /// Memo.
    pub memo: Option<String>,
    /// Action.
    pub action: Option<String>,
    /// Amount (shares).
    pub amount: Amount,
    /// Amount restated in units after later stock splits.
    pub adjusted_amount: Amount,
    /// Value, in the transaction currency.
    pub value: Amount,
    /// The transaction currency.
    pub currency: CommodityId,
    /// Gain recorded for the split (in a lot), or the gain it would realise against the chosen lot (free splits).
    pub gain: Option<Amount>,
    /// `normal`, `gains` (a gains transaction's split; the viewer hides these) or `stockSplit` (hidden too).
    pub kind: &'static str,
}

/// The result of `lot.get`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LotDetail {
    /// The lot.
    #[serde(flatten)]
    pub lot: LotInfo,
    /// Its splits, earliest first.
    pub splits: Vec<LotSplitInfo>,
}

/// The result of `lot.delete`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    /// Splits taken out of the lot.
    pub released_splits: Vec<Id>,
}

/// A split and the lot it joined.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Piece {
    /// The split (the original, or a piece cut from it).
    pub split: Id,
    /// The lot.
    pub lot: Id,
}

/// The result of `lot.assignSplit`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssignResult {
    /// Each piece and its lot.
    pub pieces: Vec<Piece>,
    /// When the split was cut to close the chosen lot: the new split holding the rest, in no lot.
    pub rest: Option<Id>,
    /// Lots FIFO opened.
    pub new_lots: Vec<Id>,
}

/// The result of `lot.removeSplit`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoveResult {
    /// The lot was left empty and deleted.
    pub lot_deleted: bool,
}

/// What a scrub or gains computation changed.
#[derive(Debug, Default, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScrubResult {
    /// Lots made.
    pub lots_created: Vec<Id>,
    /// Lots deleted.
    pub lots_deleted: Vec<Id>,
    /// Transactions made: gains transactions.
    pub transactions_created: Vec<Id>,
    /// Transactions changed: cut or merged splits, updated gains.
    pub transactions_changed: Vec<Id>,
    /// Transactions deleted: gains transactions of merged splits.
    pub transactions_deleted: Vec<Id>,
    /// Accounts made: `Orphaned Gains-<CUR>`.
    pub accounts_created: Vec<Id>,
}

/// The result of `stock.split`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockSplitResult {
    /// The transaction recorded.
    pub transaction: Id,
    /// `none` (no positive price given), `added`, `duplicate` (an equal price that day exists) or `rejected` (a
    /// same-day price from a source of higher precedence blocks it; the transaction is still recorded).
    pub price_status: &'static str,
    /// The price added.
    pub price: Option<Id>,
    /// The price that blocked it.
    pub blocking_price: Option<Id>,
}

fn lot_info(s: LotSummary) -> LotInfo {
    LotInfo {
        guid: s.guid.into(),
        account: s.account.into(),
        title: s.title,
        notes: s.notes,
        invoice: s.invoice.map(Id),
        closed: s.closed,
        open_date: s.opened.and_then(Time64::utc_date).map(Day),
        close_date: s.closed_at.and_then(Time64::utc_date).map(Day),
        balance: s.balance.into(),
        gains_currency: s.gains_currency.map(CommodityId),
        realized_gains: s.gains.into(),
        split_count: s.split_count,
    }
}

fn split_info(s: SplitView) -> LotSplitInfo {
    LotSplitInfo {
        split: s.split.into(),
        transaction: s.transaction.into(),
        date_posted: s.date_posted.utc_date().map(Day),
        num: s.num.filter(|n| !n.is_empty()),
        description: s.description,
        memo: s.memo,
        action: s.action,
        amount: s.amount.into(),
        adjusted_amount: s.adjusted_amount.into(),
        value: s.value.into(),
        currency: CommodityId(s.currency),
        gain: s.gain.map(Amount),
        kind: match s.kind {
            SplitKind::Normal => "normal",
            SplitKind::Gains => "gains",
            SplitKind::StockSplit => "stockSplit",
        },
    }
}

fn list(ctx: &mut Context, p: ListParams) -> Result<Vec<LotInfo>, RpcError> {
    ctx.query(|book, _| {
        let account = p.account.resolve(book)?;
        let lots = lots::lot_summaries(book, account)?;
        Ok(lots
            .into_iter()
            .filter(|l| match p.state {
                LotState::All => true,
                LotState::Open => !l.closed,
                LotState::Closed => l.closed,
            })
            .map(lot_info)
            .collect())
    })
}

fn get(ctx: &mut Context, p: LotParams) -> Result<LotDetail, RpcError> {
    ctx.query(|book, _| {
        let lot = lot_info(lots::lot_summary(book, p.lot.0)?);
        let splits = lots::lot_splits(book, p.lot.0)?.into_iter().map(split_info).collect();
        Ok(LotDetail { lot, splits })
    })
}

fn free_splits(ctx: &mut Context, p: FreeSplitsParams) -> Result<Vec<LotSplitInfo>, RpcError> {
    ctx.query(|book, _| {
        let account = p.account.resolve(book)?;
        Ok(lots::free_splits(book, account, p.lot.map(|l| l.0))?.into_iter().map(split_info).collect())
    })
}

fn create(ctx: &mut Context, p: CreateParams) -> Result<LotInfo, RpcError> {
    let guid = ctx.mutate_rpc(|book, env| {
        let account = p.account.resolve(book)?;
        Ok(lots::create_lot(book, env, account, p.title.as_deref(), p.notes.as_deref())?)
    })?;
    ctx.query(|book, _| Ok(lot_info(lots::lot_summary(book, guid)?)))
}

fn update(ctx: &mut Context, p: UpdateParams) -> Result<LotInfo, RpcError> {
    ctx.mutate(|book, _| lots::update_lot(book, p.lot.0, p.title.as_deref(), p.notes.as_deref()))?;
    ctx.query(|book, _| Ok(lot_info(lots::lot_summary(book, p.lot.0)?)))
}

fn delete(ctx: &mut Context, p: DeleteParams) -> Result<DeleteResult, RpcError> {
    let released = ctx.mutate(|book, _| lots::delete_lot(book, p.lot.0, p.release_splits))?;
    Ok(DeleteResult { released_splits: released.into_iter().map(Id).collect() })
}

fn assign(ctx: &mut Context, p: AssignParams) -> Result<AssignResult, RpcError> {
    let out = ctx.mutate(|book, env| lots::assign_split(book, env, p.split.0, p.lot.map(|l| l.0)))?;
    Ok(AssignResult {
        pieces: out.pieces.into_iter().map(|(split, lot)| Piece { split: split.into(), lot: lot.into() }).collect(),
        rest: out.rest.map(Id),
        new_lots: out.new_lots.into_iter().map(Id).collect(),
    })
}

fn remove(ctx: &mut Context, p: RemoveParams) -> Result<RemoveResult, RpcError> {
    let lot_deleted = ctx.mutate(|book, _| lots::remove_split(book, p.split.0))?;
    Ok(RemoveResult { lot_deleted })
}

/// Run `f` and report what it changed by comparing the book before and after; entities are shared `Arc`s, so an
/// unchanged one is the same pointer.
fn changes(book: &mut Book, env: &mut dyn Env, f: impl FnOnce(&mut Book, &mut dyn Env) -> DomainResult<()>) -> DomainResult<ScrubResult> {
    let before = book.clone();
    f(book, env)?;
    let ids = |v: Vec<&Guid>| v.into_iter().copied().map(Id).collect::<Vec<_>>();
    Ok(ScrubResult {
        lots_created: ids(book.lots.keys().filter(|k| !before.lots.contains_key(*k)).collect()),
        lots_deleted: ids(before.lots.keys().filter(|k| !book.lots.contains_key(*k)).collect()),
        transactions_created: ids(book.transactions.keys().filter(|k| !before.transactions.contains_key(*k)).collect()),
        transactions_changed: ids(
            book.transactions.iter().filter(|(k, t)| before.transactions.get(*k).is_some_and(|o| !Arc::ptr_eq(o, t))).map(|(k, _)| k).collect(),
        ),
        transactions_deleted: ids(before.transactions.keys().filter(|k| !book.transactions.contains_key(*k)).collect()),
        accounts_created: ids(book.accounts.keys().filter(|k| !before.accounts.contains_key(*k)).collect()),
    })
}

fn scrub(ctx: &mut Context, p: LotParams) -> Result<ScrubResult, RpcError> {
    ctx.mutate(|book, env| changes(book, env, |b, e| lots::scrub_lot(b, e, p.lot.0)))
}

fn scrub_account(ctx: &mut Context, p: ScrubAccountParams) -> Result<ScrubResult, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let account = p.account.resolve(book)?;
        let run = |b: &mut Book, e: &mut dyn Env| if p.subaccounts { lots::scrub_tree_lots(b, e, account) } else { lots::scrub_account_lots(b, e, account) };
        Ok(changes(book, env, run)?)
    })
}

fn compute_gains(ctx: &mut Context, p: ComputeGainsParams) -> Result<ScrubResult, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let gains = p.gains_account.as_ref().map(|a| a.resolve(book)).transpose()?;
        Ok(changes(book, env, |b, e| lots::compute_lot_gains(b, e, p.lot.0, gains))?)
    })
}

fn stock_split(ctx: &mut Context, p: StockSplitParams) -> Result<StockSplitResult, RpcError> {
    let out = ctx.mutate_rpc(|book, env| {
        let resolve = |a: &Option<AccountRef>| a.as_ref().map(|a| a.resolve(book)).transpose();
        let req = StockSplit {
            account: p.account.resolve(book)?,
            date: p.date.0,
            shares: p.shares.0,
            description: p.description.clone(),
            currency: p.currency.clone().map(|c| c.0),
            price: p.price.map(|a| a.0),
            price_currency: p.price_currency.clone().map(|c| c.0),
            cash: p.cash.map(|a| a.0),
            income_account: resolve(&p.income_account)?,
            asset_account: resolve(&p.asset_account)?,
            memo: p.memo.clone(),
        };
        Ok(lots::stock_split(book, env, &req)?)
    })?;
    let (price_status, price, blocking_price) = match out.price {
        PriceOutcome::None => ("none", None, None),
        PriceOutcome::Added(g) => ("added", Some(Id(g)), None),
        PriceOutcome::Duplicate => ("duplicate", None, None),
        PriceOutcome::Rejected { existing } => ("rejected", None, Some(Id(existing))),
    };
    Ok(StockSplitResult { transaction: out.transaction.into(), price_status, price, blocking_price })
}
