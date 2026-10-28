//! Methods: Since Last Run (`design/api/scheduled-transactions.md`, "Since Last Run").
//! `scheduledTransaction.sinceLastRun` proposes the due instances of every enabled scheduled transaction and holds the
//! Since Last Run proposal by ID; `scheduledTransaction.discard` drops it. The proposal is `red_ale_core`'s, the held
//! proposals `red_ale_service`'s; this module translates the wire.

use red_ale_core::ops::scheduled::{BuiltTransaction, Instance, InstanceState, Proposal, ScheduleInstances, Summary};
use red_ale_core::ops::scrub::IMBALANCE;
use red_ale_core::Book;
use red_ale_service::since_last_run::{self, Proposed};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{Amount, CommodityId, Day, Id, NoParams};

/// The Since Last Run methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "scheduledTransaction.sinceLastRun",
            "Propose the due instances of every enabled scheduled transaction: the Since Last Run proposal, held by ID. Changes nothing.",
            Effect::Read,
            since_last_run,
        ),
        method("scheduledTransaction.discard", "Drop a held Since Last Run proposal.", Effect::Read, discard),
    ]
}

fn since_last_run(ctx: &mut Context, _: NoParams) -> Result<RunProposal, RpcError> {
    let config = ctx.settings.config.load()?;
    let session = ctx.session()?;
    let proposed = since_last_run::propose(session, &config)?;
    Ok(RunProposal::new(session.book(), &proposed))
}

fn discard(ctx: &mut Context, p: RunDiscardParams) -> Result<RunDiscarded, RpcError> {
    since_last_run::discard(ctx.session()?, &p.proposal)?;
    Ok(RunDiscarded { discarded: true })
}

/// An instance's state (`GncSxInstanceState`, `5.17:libgnucash/app-utils/gnc-sx-instance-model.h:52-60`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename = "InstanceState")]
pub enum InstanceStateWire {
    /// Create its transactions, then advance the schedule.
    ToCreate,
    /// Nothing: reminders are not recorded, so it is proposed again next time.
    Reminder,
    /// Save it as a Deferred instance, then advance the schedule.
    Postponed,
    /// Advance the schedule without creating anything.
    Ignored,
    /// Created: only in results; choosing it is `invalid`.
    Created,
}

impl From<InstanceState> for InstanceStateWire {
    fn from(s: InstanceState) -> Self {
        match s {
            InstanceState::ToCreate => Self::ToCreate,
            InstanceState::Reminder => Self::Reminder,
            InstanceState::Postponed => Self::Postponed,
            InstanceState::Ignored => Self::Ignored,
            InstanceState::Created => Self::Created,
        }
    }
}

/// The params of `scheduledTransaction.discard`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RunDiscardParams {
    /// The proposal's ID.
    pub proposal: String,
}

/// The result of `scheduledTransaction.discard`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RunDiscarded {
    /// Always true.
    pub discarded: bool,
}

/// Counts of a proposal's instances.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    /// Proposed to create.
    pub to_create: usize,
    /// Proposed as reminders.
    pub reminders: usize,
    /// Postponed before.
    pub postponed: usize,
    /// Of auto-create scheduled transactions.
    pub auto_create: usize,
    /// To create, with a variable still unbound.
    pub need_variables: usize,
    /// Dated before the read-only threshold.
    pub read_only_by_date: usize,
    /// With errors.
    pub errors: usize,
}

impl From<Summary> for RunSummary {
    fn from(s: Summary) -> Self {
        Self {
            to_create: s.to_create,
            reminders: s.reminders,
            postponed: s.postponed,
            auto_create: s.auto_create,
            need_variables: s.need_variables,
            read_only_by_date: s.read_only_by_date,
            errors: s.errors,
        }
    }
}

/// A Since Last Run proposal (the result of `scheduledTransaction.sinceLastRun`).
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunProposal {
    /// The proposal's ID, for `scheduledTransaction.apply` and `scheduledTransaction.discard`.
    pub id: String,
    /// The Book revision it was made at.
    pub book_revision: u64,
    /// The day it was made for: the local day of the session's clock.
    pub today: Day,
    /// The auto read-only threshold date, or null with no threshold.
    pub threshold_date: Option<Day>,
    /// Counts of its instances.
    pub summary: RunSummary,
    /// One entry per enabled scheduled transaction with at least one instance, in book order.
    pub schedules: Vec<RunSchedule>,
}

impl RunProposal {
    fn new(book: &Book, proposed: &Proposed) -> Self {
        let p: &Proposal = &proposed.run.proposal;
        Self {
            id: proposed.id.clone(),
            book_revision: proposed.revision,
            today: Day(p.today),
            threshold_date: p.threshold.map(Day),
            summary: p.summary().into(),
            schedules: p.schedules.iter().map(|s| RunSchedule::new(book, s)).collect(),
        }
    }
}

/// The instances of one scheduled transaction.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunSchedule {
    /// The scheduled transaction.
    pub scheduled_transaction: Id,
    /// Its name.
    pub name: String,
    /// It creates its instances without asking.
    pub auto_create: bool,
    /// And says so.
    pub notify: bool,
    /// Its instances: postponed first, then to create, then reminders, each group in date order.
    pub instances: Vec<InstanceWire>,
}

impl RunSchedule {
    fn new(book: &Book, s: &ScheduleInstances) -> Self {
        Self {
            scheduled_transaction: Id(s.scheduled),
            name: s.name.clone(),
            auto_create: s.auto_create,
            notify: s.notify,
            instances: s.instances.iter().map(|i| InstanceWire::new(book, i)).collect(),
        }
    }
}

/// One instance of a scheduled transaction, as Since Last Run proposes it.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename = "Instance")]
pub struct InstanceWire {
    /// Its position across the whole proposal: the key `scheduledTransaction.apply` changes it by.
    pub index: usize,
    /// The occurrence date.
    pub date: Day,
    /// The proposed state: `postponed`, `toCreate` or `reminder`.
    pub state: InstanceStateWire,
    /// Its `i`.
    pub instance_count: i32,
    /// Every variable, sorted by name; `i` has its value and is not editable.
    pub variables: Vec<VariableWire>,
    /// Dated before the threshold date: creating it needs the caller's explicit choice.
    pub read_only_by_date: bool,
    /// What creating it as it stands would fail on, each as the error `scheduledTransaction.apply` would return.
    pub errors: Vec<InstanceError>,
    /// When every variable has a value and nothing fails: the transactions creating it would make. Otherwise null.
    pub transactions: Option<Vec<TransactionPreview>>,
}

impl InstanceWire {
    fn new(book: &Book, i: &Instance) -> Self {
        Self {
            index: i.index,
            date: Day(i.date),
            state: i.state.into(),
            instance_count: i.instance_count(),
            variables: i
                .variables
                .iter()
                .map(|v| VariableWire {
                    name: v.name.clone(),
                    value: v.value.map(Amount),
                    editable: v.editable,
                })
                .collect(),
            read_only_by_date: i.read_only_by_date,
            errors: i.errors.iter().cloned().map(|e| InstanceError::from(RpcError::from(e))).collect(),
            transactions: i.transactions.as_ref().map(|ts| ts.iter().map(|t| TransactionPreview::new(book, t)).collect()),
        }
    }
}

/// A variable of an instance.
#[derive(Debug, Serialize, JsonSchema)]
pub struct VariableWire {
    /// Its name.
    pub name: String,
    /// Its value; null while unbound.
    pub value: Option<Amount>,
    /// False for `i`, the Instance count.
    pub editable: bool,
}

/// A problem found in a proposal: `code`, `message` and `data` as the error object of the apply it would refuse.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstanceError {
    /// The error code: `formula` (6000) or `invalid` (2001).
    pub code: i32,
    /// What is wrong.
    pub message: String,
    /// Where, with `reason`.
    pub data: serde_json::Value,
}

impl From<RpcError> for InstanceError {
    fn from(e: RpcError) -> Self {
        Self {
            code: e.code,
            message: e.message,
            data: e.data.unwrap_or_default(),
        }
    }
}

/// A transaction creating an instance would make.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransactionPreview {
    /// The template transaction it comes from.
    pub template_transaction: Id,
    /// Its currency.
    pub currency: CommodityId,
    /// The template's description.
    pub description: Option<String>,
    /// The template's num.
    pub num: Option<String>,
    /// One split per template split, in template order.
    pub splits: Vec<SplitPreview>,
    /// The `Imbalance-<CUR>` split the commit would add when rounding leaves the values unbalanced, or null.
    pub imbalance: Option<ImbalancePreview>,
}

impl TransactionPreview {
    fn new(book: &Book, t: &BuiltTransaction) -> Self {
        Self {
            template_transaction: Id(t.template),
            currency: CommodityId(t.currency.clone()),
            description: t.description.clone(),
            num: t.num.clone(),
            splits: t
                .splits
                .iter()
                .map(|s| SplitPreview {
                    account: Id(s.account),
                    account_name: book.full_name(s.account).unwrap_or_default(),
                    value: s.value.into(),
                    amount: s.amount.into(),
                    memo: s.memo.clone(),
                    action: s.action.clone(),
                })
                .collect(),
            imbalance: t.imbalance.map(|value| ImbalancePreview {
                value: value.into(),
                currency: CommodityId(t.currency.clone()),
                account: format!("{IMBALANCE}-{}", t.currency.mnemonic),
            }),
        }
    }
}

/// A split creating an instance would make.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SplitPreview {
    /// The account.
    pub account: Id,
    /// Its full name.
    pub account_name: String,
    /// Value in the transaction currency.
    pub value: Amount,
    /// Amount in the account's commodity.
    pub amount: Amount,
    /// The template split's memo.
    pub memo: Option<String>,
    /// The template split's action.
    pub action: Option<String>,
}

/// The Imbalance split creating an instance would add.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ImbalancePreview {
    /// Its value.
    pub value: Amount,
    /// The transaction currency.
    pub currency: CommodityId,
    /// The full name of the account it would go to, made if the Book has none.
    pub account: String,
}
