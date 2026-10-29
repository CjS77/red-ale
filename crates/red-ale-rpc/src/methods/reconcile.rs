//! Methods: reconcile states, an account's reconcile information, and reconciling against a statement.
//!
//! Reconciling is stateless: the caller carries the statement and its ticks from call to call, and only
//! `reconcile.finish` and `reconcile.postpone` change the book. See `design/api/reconcile.md`.

use red_ale_core::ops::reconcile::{self as rec, Balances, Candidate, Interval, Postponed, ReconcileInfo, Request, StateChange, Statement};
use red_ale_core::{Book, Guid, ReconcileState, TimeZone};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, Day, Id, Timestamp};

/// The reconcile methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "reconcile.setState",
            "Set a split's reconcile state (n, c, y or f); y stamps the reconcile date.",
            Effect::Write,
            set_state,
        ),
        method(
            "reconcile.info",
            "An account's reconcile information: last reconcile date and interval, postponed reconciliation.",
            Effect::Read,
            info,
        ),
        method(
            "reconcile.setInfo",
            "Set or remove an account's last reconcile date, interval, postponed reconciliation or sub-account flag.",
            Effect::Write,
            set_info,
        ),
        method(
            "reconcile.start",
            "Start reconciling an account against a statement: GnuCash's defaults, the splits to tick and the balances.",
            Effect::Read,
            start,
        ),
        method(
            "reconcile.status",
            "The splits, balances and difference of a reconciliation for a set of ticks, with marks and unmarks applied.",
            Effect::Read,
            status,
        ),
        method(
            "reconcile.finish",
            "Finish a reconciliation: ticked splits become reconciled and the statement date becomes the last reconcile date.",
            Effect::Write,
            finish,
        ),
        method(
            "reconcile.postpone",
            "Postpone a reconciliation: ticked splits become cleared and the statement is kept for next time.",
            Effect::Write,
            postpone,
        ),
    ]
}

/// A reconcile state as GnuCash writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum State {
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
    /// Voided (set only by voiding the transaction).
    #[serde(rename = "v")]
    Voided,
}

impl From<State> for ReconcileState {
    fn from(s: State) -> Self {
        match s {
            State::New => Self::New,
            State::Cleared => Self::Cleared,
            State::Reconciled => Self::Reconciled,
            State::Frozen => Self::Frozen,
            State::Voided => Self::Voided,
        }
    }
}

impl From<ReconcileState> for State {
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

/// Params of `reconcile.setState`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetStateParams {
    /// The split.
    pub split: Id,
    /// The new state: `n`, `c`, `y` or `f`. `v` comes only from voiding the transaction.
    pub state: State,
    /// With `y` only: the reconcile date, `YYYY-MM-DD` for the end of that local day (as the reconcile window stamps
    /// its statement date), or an RFC 3339 timestamp for an exact time. Defaults to now.
    #[serde(default)]
    pub date: Option<ReconcileDate>,
}

/// A reconcile date: a day, or an exact time. v0.5 took only a timestamp; a day was added later.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ReconcileDate {
    /// The end of this local day, 23:59:59.
    Day(Day),
    /// This instant.
    Time(Timestamp),
}

impl ReconcileDate {
    /// The instant it names in `zone`.
    fn time(self, zone: &TimeZone) -> red_ale_core::Time64 {
        match self {
            Self::Day(d) => zone.day_end(d.0),
            Self::Time(t) => t.0,
        }
    }
}

/// A split's reconcile state.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SplitState {
    /// The split.
    pub split: Id,
    /// Its transaction.
    pub transaction: Id,
    /// Its reconcile state.
    pub state: State,
    /// When it was reconciled, if a date is stored.
    pub reconcile_date: Option<Timestamp>,
}

fn set_state(ctx: &mut Context, p: SetStateParams) -> Result<SplitState, RpcError> {
    let split = p.split.0;
    let date = p.date.map(|d| d.time(&ctx.zone()));
    ctx.mutate(|book, env| rec::set_split_state(book, env, split, p.state.into(), date))?;
    let book = ctx.book()?;
    let t = book
        .transaction_of_split(split)
        .ok_or_else(|| RpcError::not_found("split", split))?;
    let s = t.split(split).ok_or_else(|| RpcError::not_found("split", split))?;
    let reconcile_date = s.reconcile_date.map(Into::into);
    Ok(SplitState {
        split: p.split,
        transaction: t.guid.into(),
        state: s.reconcile_state.into(),
        reconcile_date,
    })
}

/// The interval between reconciliations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IntervalWire {
    /// Whole months.
    pub months: i64,
    /// Days, used when `months` is 0.
    pub days: i64,
}

impl From<Interval> for IntervalWire {
    fn from(i: Interval) -> Self {
        Self {
            months: i.months,
            days: i.days,
        }
    }
}

/// A postponed reconciliation.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PostponeWire {
    /// The statement date.
    pub date: Timestamp,
    /// The ending balance, raw sign (debits positive).
    pub balance: Amount,
}

/// Params naming an account.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AccountParams {
    /// The account.
    pub account: AccountRef,
}

/// An account's reconcile information.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InfoResult {
    /// The account.
    pub account: Id,
    /// When it was last reconciled.
    pub last_date: Option<Timestamp>,
    /// The interval between its last two reconciliations.
    pub last_interval: Option<IntervalWire>,
    /// A postponed reconciliation's statement date.
    pub postpone_date: Option<Timestamp>,
    /// A postponed reconciliation's ending balance, raw sign.
    pub postpone_balance: Option<Amount>,
    /// Reconciliations include sub-accounts.
    pub include_subaccounts: bool,
}

fn info_result(account: Guid, i: &ReconcileInfo) -> InfoResult {
    InfoResult {
        account: account.into(),
        last_date: i.last_date.map(Into::into),
        last_interval: i.last_interval.map(Into::into),
        postpone_date: i.postpone_date.map(Into::into),
        postpone_balance: i.postpone_balance.map(Into::into),
        include_subaccounts: i.include_children,
    }
}

fn account_info(book: &Book, account: Guid) -> Result<InfoResult, RpcError> {
    let acc = book.account(account).ok_or_else(|| RpcError::not_found("account", account))?;
    Ok(info_result(account, &rec::reconcile_info(acc)))
}

fn info(ctx: &mut Context, p: AccountParams) -> Result<InfoResult, RpcError> {
    let book = ctx.book()?;
    account_info(book, p.account.resolve(book)?)
}

/// Read a field that may be absent (leave alone), `null` (remove) or a value (set).
#[allow(clippy::option_option)] // The three cases are the point: absent, null and a value mean different things.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// Params of `reconcile.setInfo`. An absent field is left alone; `null` removes it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetInfoParams {
    /// The account.
    pub account: AccountRef,
    /// The last reconcile date.
    #[serde(default, deserialize_with = "present")]
    pub last_date: Option<Option<Timestamp>>,
    /// The interval between reconciliations.
    #[serde(default, deserialize_with = "present")]
    pub last_interval: Option<Option<IntervalWire>>,
    /// A postponed reconciliation; `null` clears it.
    #[serde(default, deserialize_with = "present")]
    pub postpone: Option<Option<PostponeWire>>,
    /// Whether reconciliations include sub-accounts.
    #[serde(default)]
    pub include_subaccounts: Option<bool>,
}

fn set_info(ctx: &mut Context, p: SetInfoParams) -> Result<InfoResult, RpcError> {
    let account = p.account.resolve(ctx.book()?)?;
    ctx.mutate(|book, _| {
        let acc = rec::account_mut(book, account)?;
        if let Some(date) = p.last_date {
            rec::set_last_date(acc, date.map(|d| d.0));
        }
        if let Some(interval) = p.last_interval {
            rec::set_last_interval(
                acc,
                interval.map(|i| Interval {
                    months: i.months,
                    days: i.days,
                }),
            )?;
        }
        if let Some(postpone) = p.postpone {
            rec::set_postponed(
                acc,
                postpone.map(|q| Postponed {
                    date: q.date.0,
                    balance: q.balance.0,
                }),
            );
        }
        if let Some(include) = p.include_subaccounts {
            rec::set_include_children(acc, include);
        }
        Ok(())
    })?;
    account_info(ctx.book()?, account)
}

/// Params of `reconcile.start`. Anything absent gets GnuCash's default.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StartParams {
    /// The account to reconcile.
    pub account: AccountRef,
    /// The statement date. Defaults to the last reconcile date plus the last interval, no later than today, or the
    /// postponed statement's date.
    #[serde(default)]
    pub statement_date: Option<Day>,
    /// The statement's ending balance, raw sign (debits positive). Defaults to the postponed balance, else the
    /// account's balance at the end of the statement date.
    #[serde(default)]
    pub ending_balance: Option<Amount>,
    /// Reconcile the sub-accounts too. Defaults to the account's stored choice; refused when a sub-account holds
    /// another commodity.
    #[serde(default)]
    pub include_subaccounts: Option<bool>,
    /// A fixed offset from UTC in minutes to use instead of the server's local time zone, for GnuCash's local-time
    /// rules (the statement date ends at 23:59:59 local time).
    #[serde(default)]
    pub utc_offset_minutes: Option<i32>,
}

/// Params of `reconcile.status`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StatusParams {
    /// The account reconciled.
    pub account: AccountRef,
    /// The statement date.
    pub statement_date: Day,
    /// The statement's ending balance, raw sign.
    pub ending_balance: Amount,
    /// Reconcile the sub-accounts too. Defaults to the account's stored choice.
    #[serde(default)]
    pub include_subaccounts: Option<bool>,
    /// A fixed offset from UTC in minutes to use instead of the server's local time zone.
    #[serde(default)]
    pub utc_offset_minutes: Option<i32>,
    /// The ticked splits. Defaults to the ticks the window starts with: cleared splits posted by the statement date.
    #[serde(default)]
    pub ticked: Option<Vec<Id>>,
    /// Splits to tick.
    #[serde(default)]
    pub mark: Vec<Id>,
    /// Splits to untick.
    #[serde(default)]
    pub unmark: Vec<Id>,
}

/// Params of `reconcile.finish` and `reconcile.postpone`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FinishParams {
    /// The account reconciled.
    pub account: AccountRef,
    /// The statement date.
    pub statement_date: Day,
    /// The statement's ending balance, raw sign.
    pub ending_balance: Amount,
    /// Reconcile the sub-accounts too. Defaults to the account's stored choice.
    #[serde(default)]
    pub include_subaccounts: Option<bool>,
    /// A fixed offset from UTC in minutes to use instead of the server's local time zone.
    #[serde(default)]
    pub utc_offset_minutes: Option<i32>,
    /// The ticked splits.
    pub ticked: Vec<Id>,
    /// `reconcile.finish` only: finish even though the difference is not zero (GnuCash's "finish anyway").
    #[serde(default)]
    pub force: bool,
}

/// A split the reconciliation lists.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CandidateWire {
    /// The split.
    pub split: Id,
    /// Its transaction.
    pub transaction: Id,
    /// Its account.
    pub account: Id,
    /// When its transaction was posted.
    pub date_posted: Timestamp,
    /// The transaction's description.
    pub description: Option<String>,
    /// The split's memo.
    pub memo: Option<String>,
    /// The amount, raw sign (debits positive).
    pub amount: Amount,
    /// `n` or `c`.
    pub state: State,
    /// Ticked in this reconciliation.
    pub ticked: bool,
}

/// A reconciliation in progress: the statement, the splits to tick and the window's figures. Pass the statement
/// fields and `ticked` back to the next call.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileView {
    /// The account reconciled.
    pub account: Id,
    /// The statement date, local to the caller's offset.
    pub statement_date: Day,
    /// The statement date as an instant: the last second of the day, as GnuCash stores it.
    pub statement_time: Timestamp,
    /// The ending balance, raw sign.
    pub ending_balance: Amount,
    /// The sub-accounts are reconciled too.
    pub include_subaccounts: bool,
    /// The defaults came from a postponed reconciliation.
    pub resumed_postponed: bool,
    /// When the account was last reconciled.
    pub last_reconcile_date: Option<Timestamp>,
    /// The reconciled balance before this statement.
    pub starting_balance: Amount,
    /// The cleared balance (every split not `n`).
    pub cleared_balance: Amount,
    /// The sum of the ticked splits.
    pub ticked_balance: Amount,
    /// The starting balance plus the ticked splits.
    pub reconciled_balance: Amount,
    /// The ending balance minus the reconciled balance; `reconcile.finish` needs it to be zero.
    pub difference: Amount,
    /// The ticked splits.
    pub ticked: Vec<Id>,
    /// Every split that can be ticked.
    pub splits: Vec<CandidateWire>,
}

/// The zone the statement is read in: a fixed offset the caller gives, else the local time zone.
fn zone(minutes: Option<i32>, local: &TimeZone) -> Result<TimeZone, RpcError> {
    match minutes {
        None => Ok(local.clone()),
        Some(m) if (-18 * 60..=18 * 60).contains(&m) => TimeZone::fixed(m * 60).map_err(|e| RpcError::invalid_params(e.to_string())),
        Some(m) => Err(RpcError::invalid_params(format!("utcOffsetMinutes must be within ±1080, got {m}"))),
    }
}

fn view(book: &Book, prepared: &rec::Prepared, ticked: &[Guid]) -> Result<ReconcileView, RpcError> {
    let st = &prepared.statement;
    let candidates = rec::candidates(book, st);
    let b: Balances = rec::balances(book, st, &candidates, ticked)?;
    let account = book.account(st.account).ok_or_else(|| RpcError::not_found("account", st.account))?;
    let ticked: Vec<Guid> = candidates.iter().filter(|c| ticked.contains(&c.split)).map(|c| c.split).collect();
    Ok(ReconcileView {
        account: st.account.into(),
        statement_date: st
            .zone
            .local_date(st.date)
            .map(Day)
            .ok_or_else(|| RpcError::invalid("statement date out of range"))?,
        statement_time: st.date.into(),
        ending_balance: st.ending_balance.into(),
        include_subaccounts: st.include_children,
        resumed_postponed: prepared.resumed_postponed,
        last_reconcile_date: rec::reconcile_info(account).last_date.map(Into::into),
        starting_balance: b.starting.into(),
        cleared_balance: b.cleared.into(),
        ticked_balance: b.ticked.into(),
        reconciled_balance: b.reconciled.into(),
        difference: b.difference.into(),
        splits: candidates.iter().map(|c| candidate(book, c, &ticked)).collect(),
        ticked: ticked.into_iter().map(Into::into).collect(),
    })
}

fn candidate(book: &Book, c: &Candidate, ticked: &[Guid]) -> CandidateWire {
    let t = book.transaction(c.transaction);
    CandidateWire {
        split: c.split.into(),
        transaction: c.transaction.into(),
        account: c.account.into(),
        date_posted: c.date_posted.into(),
        description: t.and_then(|t| t.description.clone()),
        memo: t.and_then(|t| t.split(c.split)).and_then(|s| s.memo.clone()),
        amount: c.amount.into(),
        state: c.state.into(),
        ticked: ticked.contains(&c.split),
    }
}

fn start(ctx: &mut Context, p: StartParams) -> Result<ReconcileView, RpcError> {
    let zone = zone(p.utc_offset_minutes, &ctx.zone())?;
    ctx.query(|book, env| {
        let request = Request {
            account: p.account.resolve(book)?,
            date: p.statement_date.map(|d| d.0),
            ending_balance: p.ending_balance.map(|a| a.0),
            include_children: p.include_subaccounts,
            zone,
        };
        let prepared = rec::prepare(book, env.now(), &request)?;
        let ticks = rec::default_ticks(&rec::candidates(book, &prepared.statement));
        view(book, &prepared, &ticks)
    })
}

fn prepared(book: &Book, now: red_ale_core::Time64, request: &Request) -> Result<rec::Prepared, RpcError> {
    Ok(rec::prepare(book, now, request)?)
}

fn status(ctx: &mut Context, p: StatusParams) -> Result<ReconcileView, RpcError> {
    let zone = zone(p.utc_offset_minutes, &ctx.zone())?;
    ctx.query(|book, env| {
        let request = Request {
            account: p.account.resolve(book)?,
            date: Some(p.statement_date.0),
            ending_balance: Some(p.ending_balance.0),
            include_children: p.include_subaccounts,
            zone,
        };
        let prepared = prepared(book, env.now(), &request)?;
        let base = match &p.ticked {
            Some(t) => t.iter().map(|i| i.0).collect(),
            None => rec::default_ticks(&rec::candidates(book, &prepared.statement)),
        };
        let unmark: Vec<Guid> = p.unmark.iter().map(|i| i.0).collect();
        let ticks: Vec<Guid> = base
            .into_iter()
            .chain(p.mark.iter().map(|i| i.0))
            .filter(|g| !unmark.contains(g))
            .collect();
        view(book, &prepared, &ticks)
    })
}

fn statement(book: &Book, now: red_ale_core::Time64, p: &FinishParams, local: &TimeZone) -> Result<Statement, RpcError> {
    let request = Request {
        account: p.account.resolve(book)?,
        date: Some(p.statement_date.0),
        ending_balance: Some(p.ending_balance.0),
        include_children: p.include_subaccounts,
        zone: zone(p.utc_offset_minutes, local)?,
    };
    Ok(prepared(book, now, &request)?.statement)
}

/// A split whose reconcile state changed.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StateChangeWire {
    /// The split.
    pub split: Id,
    /// Its transaction.
    pub transaction: Id,
    /// The state before.
    pub from: State,
    /// The state after.
    pub to: State,
}

impl From<&StateChange> for StateChangeWire {
    fn from(c: &StateChange) -> Self {
        Self {
            split: c.split.into(),
            transaction: c.transaction.into(),
            from: c.from.into(),
            to: c.to.into(),
        }
    }
}

/// What `reconcile.finish` did.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinishResult {
    /// The splits now reconciled.
    pub reconciled: Vec<StateChangeWire>,
    /// The statement date, now the last reconcile date and the splits' reconcile date.
    pub statement_time: Timestamp,
    /// The interval recorded for proposing the next statement date.
    pub last_interval: Option<IntervalWire>,
    /// The difference at Finish (zero unless forced).
    pub difference: Amount,
}

/// What `reconcile.postpone` did.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostponeResult {
    /// Splits whose state changed: ticked ones to `c`, unticked ones posted by the statement date to `n`.
    pub changed: Vec<StateChangeWire>,
    /// The statement date kept for next time.
    pub statement_time: Timestamp,
    /// The interval recorded for proposing the next statement date.
    pub last_interval: Option<IntervalWire>,
}

fn ids(v: &[Id]) -> Vec<Guid> {
    v.iter().map(|i| i.0).collect()
}

fn finish(ctx: &mut Context, p: FinishParams) -> Result<FinishResult, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let st = statement(book, env.now(), &p, env.zone())?;
        let done = rec::finish(book, &st, &ids(&p.ticked), p.force)?;
        Ok(FinishResult {
            reconciled: done.reconciled.iter().map(Into::into).collect(),
            statement_time: st.date.into(),
            last_interval: done.interval.map(Into::into),
            difference: done.balances.difference.into(),
        })
    })
}

fn postpone(ctx: &mut Context, p: FinishParams) -> Result<PostponeResult, RpcError> {
    if p.force {
        return Err(RpcError::invalid_params("force applies only to reconcile.finish"));
    }
    ctx.mutate_rpc(|book, env| {
        let st = statement(book, env.now(), &p, env.zone())?;
        let done = rec::postpone(book, &st, &ids(&p.ticked))?;
        Ok(PostponeResult {
            changed: done.changed.iter().map(Into::into).collect(),
            statement_time: st.date.into(),
            last_interval: done.interval.map(Into::into),
        })
    })
}
