//! Methods: the data-integrity scrubs and Check & Repair. Each is one atomic mutation and returns every repair it
//! made. See `design/api/scrub.md`.

use red_ale_core::ops::scrub::{self, Repair, Scope};
use red_ale_core::{Book, DomainResult, Env};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, CommodityId, Id};

/// The scrub methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "scrub.imbalance",
            "Balance unbalanced transactions with an Imbalance-<CUR> split, after fixing orphans and split amounts.",
            Effect::Destructive,
            imbalance,
        ),
        method(
            "scrub.orphans",
            "Put splits with no account into an Orphan-<CUR> account, balancing each transaction changed.",
            Effect::Destructive,
            orphans,
        ),
        method(
            "scrub.currency",
            "Give transactions whose currency is not a currency the most common currency of their splits.",
            Effect::Destructive,
            currency,
        ),
        method(
            "scrub.checkAndRepair",
            "GnuCash's Check & Repair: orphans, then imbalances, then (if asked) lots.",
            Effect::Destructive,
            check_and_repair,
        ),
    ]
}

/// Params of the scrubs. Give a transaction, or an account (with or without its sub-accounts); give neither for the
/// whole book.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ScrubParams {
    /// Scrub this transaction only.
    #[serde(default)]
    pub transaction: Option<Id>,
    /// Scrub the transactions with a split in this account.
    #[serde(default)]
    pub account: Option<AccountRef>,
    /// With `account`: its sub-accounts too.
    #[serde(default)]
    pub subaccounts: bool,
}

/// Params of `scrub.checkAndRepair`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CheckParams {
    /// Check this transaction only (the register's Check & Repair).
    #[serde(default)]
    pub transaction: Option<Id>,
    /// Check the transactions with a split in this account.
    #[serde(default)]
    pub account: Option<AccountRef>,
    /// With `account`: its sub-accounts too.
    #[serde(default)]
    pub subaccounts: bool,
    /// Also run the lot scrub on each account checked, sub-accounts first. Off by default, as in GnuCash 5.17, which
    /// runs it only when the `GNC_AUTO_SCRUB_LOTS` environment variable is set.
    #[serde(default)]
    pub lots: bool,
}

/// One change a scrub made.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[allow(missing_docs)] // Each variant's fields are named for what they hold.
pub enum RepairWire {
    /// A balancing or orphan account was created under the root.
    AccountCreated { account: Id, name: String, commodity: CommodityId },
    /// A split with no account was put in an Orphan-<CUR> account.
    OrphanAssigned { transaction: Id, split: Id, account: Id },
    /// An invalid value or amount was set to zero.
    InvalidNumberZeroed { transaction: Id, split: Id, field: String },
    /// A split's amount was set to its value, its account holding the transaction currency.
    AmountSetToValue {
        transaction: Id,
        split: Id,
        from: Amount,
        to: Amount,
    },
    /// A transaction whose currency was not a currency got one.
    CurrencySet {
        transaction: Id,
        from: CommodityId,
        to: CommodityId,
    },
    /// A split's value was re-expressed in the transaction's new currency.
    ValueChanged {
        transaction: Id,
        split: Id,
        from: Amount,
        to: Amount,
    },
    /// A balancing split was added.
    BalanceSplitAdded {
        transaction: Id,
        split: Id,
        account: Id,
        value: Amount,
    },
    /// A balancing split's value was changed.
    BalanceSplitChanged {
        transaction: Id,
        split: Id,
        account: Id,
        from: Amount,
        to: Amount,
    },
    /// A balancing split that came to zero, with no memo or action, was deleted.
    BalanceSplitRemoved {
        transaction: Id,
        split: Id,
        account: Id,
        value: Amount,
    },
    /// The lot scrub changed an account's lots or splits.
    LotsRepaired { account: Id },
    /// A trading split was removed before the transaction's trading splits were rebuilt.
    TradingSplitRemoved {
        transaction: Id,
        split: Id,
        account: Id,
        value: Amount,
        amount: Amount,
    },
    /// A trading split was added.
    TradingSplitAdded {
        transaction: Id,
        split: Id,
        account: Id,
        value: Amount,
        amount: Amount,
    },
}

impl From<Repair> for RepairWire {
    fn from(r: Repair) -> Self {
        match r {
            Repair::AccountCreated { account, name, commodity } => Self::AccountCreated {
                account: account.into(),
                name,
                commodity: commodity.into(),
            },
            Repair::OrphanAssigned {
                transaction,
                split,
                account,
            } => Self::OrphanAssigned {
                transaction: transaction.into(),
                split: split.into(),
                account: account.into(),
            },
            Repair::InvalidNumberZeroed { transaction, split, field } => Self::InvalidNumberZeroed {
                transaction: transaction.into(),
                split: split.into(),
                field: field.to_owned(),
            },
            Repair::AmountSetToValue {
                transaction,
                split,
                from,
                to,
            } => Self::AmountSetToValue {
                transaction: transaction.into(),
                split: split.into(),
                from: from.into(),
                to: to.into(),
            },
            Repair::CurrencySet { transaction, from, to } => Self::CurrencySet {
                transaction: transaction.into(),
                from: from.into(),
                to: to.into(),
            },
            Repair::ValueChanged {
                transaction,
                split,
                from,
                to,
            } => Self::ValueChanged {
                transaction: transaction.into(),
                split: split.into(),
                from: from.into(),
                to: to.into(),
            },
            Repair::BalanceSplitAdded {
                transaction,
                split,
                account,
                value,
            } => Self::BalanceSplitAdded {
                transaction: transaction.into(),
                split: split.into(),
                account: account.into(),
                value: value.into(),
            },
            Repair::BalanceSplitChanged {
                transaction,
                split,
                account,
                from,
                to,
            } => Self::BalanceSplitChanged {
                transaction: transaction.into(),
                split: split.into(),
                account: account.into(),
                from: from.into(),
                to: to.into(),
            },
            Repair::BalanceSplitRemoved {
                transaction,
                split,
                account,
                value,
            } => Self::BalanceSplitRemoved {
                transaction: transaction.into(),
                split: split.into(),
                account: account.into(),
                value: value.into(),
            },
            Repair::LotsRepaired { account } => Self::LotsRepaired { account: account.into() },
            Repair::TradingSplitRemoved {
                transaction,
                split,
                account,
                value,
                amount,
            } => Self::TradingSplitRemoved {
                transaction: transaction.into(),
                split: split.into(),
                account: account.into(),
                value: value.into(),
                amount: amount.into(),
            },
            Repair::TradingSplitAdded {
                transaction,
                split,
                account,
                value,
                amount,
            } => Self::TradingSplitAdded {
                transaction: transaction.into(),
                split: split.into(),
                account: account.into(),
                value: value.into(),
                amount: amount.into(),
            },
        }
    }
}

/// What a scrub did.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScrubResult {
    /// Every change, in the order made. Empty when there was nothing to repair.
    pub repairs: Vec<RepairWire>,
}

fn scope(book: &Book, transaction: Option<Id>, account: Option<&AccountRef>, subaccounts: bool) -> Result<Scope, RpcError> {
    match (transaction, account) {
        (Some(_), Some(_)) => Err(RpcError::invalid_params("give a transaction or an account, not both")),
        (Some(t), None) if subaccounts => Err(RpcError::invalid_params(format!(
            "subaccounts applies to an account, not to transaction {}",
            t.0
        ))),
        (Some(t), None) => Ok(Scope::Transaction(t.0)),
        (None, Some(a)) if subaccounts => Ok(Scope::Tree(a.resolve(book)?)),
        (None, Some(a)) => Ok(Scope::Account(a.resolve(book)?)),
        (None, None) if subaccounts => Err(RpcError::invalid_params("subaccounts needs an account")),
        (None, None) => Ok(Scope::Book),
    }
}

type ScrubFn = fn(&mut Book, &mut dyn Env, Scope) -> DomainResult<Vec<Repair>>;

fn run(ctx: &mut Context, p: &ScrubParams, f: ScrubFn) -> Result<ScrubResult, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let scope = scope(book, p.transaction, p.account.as_ref(), p.subaccounts)?;
        Ok(ScrubResult {
            repairs: f(book, env, scope)?.into_iter().map(Into::into).collect(),
        })
    })
}

fn imbalance(ctx: &mut Context, p: ScrubParams) -> Result<ScrubResult, RpcError> {
    run(ctx, &p, scrub::scrub_imbalance)
}

fn orphans(ctx: &mut Context, p: ScrubParams) -> Result<ScrubResult, RpcError> {
    run(ctx, &p, scrub::scrub_orphans)
}

fn currency(ctx: &mut Context, p: ScrubParams) -> Result<ScrubResult, RpcError> {
    run(ctx, &p, scrub::scrub_currency)
}

fn check_and_repair(ctx: &mut Context, p: CheckParams) -> Result<ScrubResult, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let scope = scope(book, p.transaction, p.account.as_ref(), p.subaccounts)?;
        Ok(ScrubResult {
            repairs: scrub::check_and_repair(book, env, scope, p.lots)?
                .into_iter()
                .map(Into::into)
                .collect(),
        })
    })
}
