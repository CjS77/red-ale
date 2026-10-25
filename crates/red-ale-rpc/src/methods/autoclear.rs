//! Methods: auto-clear, which finds the uncleared splits that bring an account's cleared balance to a given figure.
//! The search is `red_ale_core::ops::autoclear`. See `design/api/reconcile.md`, "Auto-clear".

use red_ale_core::ops::autoclear::{self, AutoClear, Outcome};
use red_ale_core::ops::transactions::posted_date;
use red_ale_core::{Book, Guid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, Day, Id};

/// The auto-clear methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "reconcile.findAutoClear",
            "Find the one set of uncleared splits that brings an account's cleared balance to a figure, as GnuCash's Auto-clear does.",
            Effect::Read,
            find,
        ),
        method(
            "reconcile.autoClear",
            "Auto-clear an account: mark cleared the one set of uncleared splits that brings its cleared balance to a figure.",
            Effect::Write,
            apply,
        ),
    ]
}

/// Params of `reconcile.findAutoClear` and `reconcile.autoClear`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AutoClearParams {
    /// The account; its sub-accounts are not included.
    pub account: AccountRef,
    /// The cleared balance wanted, raw sign (debits positive). Rounded to the account's SCU with banker's rounding.
    pub cleared_balance: Amount,
}

/// What the search found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum OutcomeWire {
    /// Exactly one set of splits.
    Found,
    /// The cleared balance is already the figure.
    AlreadyCleared,
    /// More than one set of splits gives the figure.
    Ambiguous,
    /// No set of splits gives the figure.
    Unreachable,
    /// The search stopped at its step limit.
    Limit,
}

impl OutcomeWire {
    fn of(o: &Outcome) -> Self {
        match o {
            Outcome::Found(_) => Self::Found,
            Outcome::AlreadyCleared => Self::AlreadyCleared,
            Outcome::Ambiguous => Self::Ambiguous,
            Outcome::Unreachable => Self::Unreachable,
            Outcome::Limit => Self::Limit,
        }
    }
}

/// A split auto-clear picked.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClearedSplit {
    /// The split.
    pub split: Id,
    /// Its transaction.
    pub transaction: Id,
    /// The transaction's posted date.
    pub date_posted: Option<Day>,
    /// The transaction's description.
    pub description: Option<String>,
    /// The split's memo.
    pub memo: Option<String>,
    /// The split's amount, in the account's commodity.
    pub amount: Amount,
}

/// The result of an auto-clear search.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AutoClearResult {
    /// The account.
    pub account: Id,
    /// The account's cleared balance before auto-clear: every split not `n`, raw sign.
    pub current_cleared_balance: Amount,
    /// The cleared balance asked for, rounded to the account's SCU.
    pub cleared_balance: Amount,
    /// What the search found.
    pub outcome: OutcomeWire,
    /// GnuCash's message for an outcome other than `found`.
    pub message: Option<String>,
    /// The splits found, in account order; empty unless `found`.
    pub splits: Vec<ClearedSplit>,
}

fn cleared_split(book: &Book, guid: Guid) -> Result<ClearedSplit, RpcError> {
    let t = book.transaction_of_split(guid).ok_or_else(|| RpcError::not_found("split", guid))?;
    let s = t.split(guid).ok_or_else(|| RpcError::not_found("split", guid))?;
    Ok(ClearedSplit {
        split: guid.into(),
        transaction: t.guid.into(),
        date_posted: posted_date(t).map(Day),
        description: t.description.clone(),
        memo: s.memo.clone(),
        amount: s.amount.into(),
    })
}

fn result(book: &Book, account: Guid, r: &AutoClear) -> Result<AutoClearResult, RpcError> {
    let splits = match &r.outcome {
        Outcome::Found(splits) => splits.iter().map(|s| cleared_split(book, *s)).collect::<Result<_, _>>()?,
        _ => Vec::new(),
    };
    Ok(AutoClearResult {
        account: account.into(),
        current_cleared_balance: r.current.into(),
        cleared_balance: r.target.into(),
        outcome: OutcomeWire::of(&r.outcome),
        message: r.outcome.message().map(str::to_owned),
        splits,
    })
}

fn find(ctx: &mut Context, p: AutoClearParams) -> Result<AutoClearResult, RpcError> {
    let book = ctx.book()?;
    let account = p.account.resolve(book)?;
    let r = autoclear::find(book, account, p.cleared_balance.0)?;
    result(book, account, &r)
}

fn apply(ctx: &mut Context, p: AutoClearParams) -> Result<AutoClearResult, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let account = p.account.resolve(book)?;
        let found = autoclear::find(book, account, p.cleared_balance.0)?;
        if !matches!(found.outcome, Outcome::Found(_)) {
            let message = found.outcome.message().unwrap_or_default();
            return Err(RpcError::invalid(message).with_data(json!({ "reason": OutcomeWire::of(&found.outcome) })));
        }
        autoclear::mark_cleared(book, env, &found)?;
        result(book, account, &found)
    })
}
