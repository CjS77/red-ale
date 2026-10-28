//! Methods: scheduled transactions, their Schedules and template transactions, and GnuCash's Schedule from
//! transaction and loan assistant (`design/api/scheduled-transactions.md`, "Methods", "Schedule from transaction"
//! and "The loan assistant"). The rules live in `red_ale_core::ops::scheduled`; this module translates the wire.

mod assistants;
mod info;

use red_ale_core::formula::NumberFormat;
use red_ale_core::ops::scheduled::{self as ops, Edited, End, Fields, RecurrenceSpec, SplitSpec, TemplateSpec, template_transactions};
use red_ale_core::{Book, Guid, TemplateSplit};
use red_ale_service::since_last_run::number_format;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use info::{ScheduledInfo, TemplateSplitInfo, TemplateTransactionInfo, WarningInfo};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, CommodityId, Day, EndWire, Id, RecurrenceParam};

/// The scheduled transaction methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "scheduledTransaction.create",
            "Create a scheduled transaction with its template transactions.",
            Effect::Write,
            create,
        ),
        method(
            "scheduledTransaction.get",
            "One scheduled transaction with its template transactions.",
            Effect::Read,
            get,
        ),
        method(
            "scheduledTransaction.list",
            "Scheduled transactions in book order.",
            Effect::Read,
            list,
        ),
        method(
            "scheduledTransaction.update",
            "Change a scheduled transaction's fields, Schedule, end or template transactions.",
            Effect::Write,
            update,
        ),
        method(
            "scheduledTransaction.delete",
            "Delete a scheduled transaction with its template account and template transactions.",
            Effect::Destructive,
            delete,
        ),
        method(
            "scheduledTransaction.occurrences",
            "The next occurrence dates of a scheduled transaction, or of a Schedule not yet saved.",
            Effect::Read,
            occurrences,
        ),
        method(
            "scheduledTransaction.fromTransaction",
            "Make a scheduled transaction from a transaction, as GnuCash's Schedule from transaction does.",
            Effect::Write,
            assistants::from_transaction,
        ),
        method(
            "scheduledTransaction.loan",
            "Make a loan's repayment scheduled transaction, as GnuCash's mortgage and loan assistant does.",
            Effect::Write,
            assistants::loan,
        ),
    ]
}

/// A template split as the caller gives it.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TemplateSplitParam {
    /// An existing template split of this scheduled transaction to keep (`update` only).
    #[serde(default)]
    pub guid: Option<Id>,
    /// The real account the created split goes to: not the root, a placeholder or a template account.
    pub account: AccountRef,
    /// The memo.
    #[serde(default)]
    pub memo: Option<String>,
    /// The action.
    #[serde(default)]
    pub action: Option<String>,
    /// The debit Formula; empty or absent means no debit.
    #[serde(default)]
    pub debit_formula: Option<String>,
    /// The credit Formula; empty or absent means no credit.
    #[serde(default)]
    pub credit_formula: Option<String>,
}

/// A template transaction as the caller gives it.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TemplateTransactionParam {
    /// An existing template transaction of this scheduled transaction to keep (`update` only).
    #[serde(default)]
    pub guid: Option<Id>,
    /// The description.
    #[serde(default)]
    pub description: Option<String>,
    /// The num.
    #[serde(default)]
    pub num: Option<String>,
    /// The notes.
    #[serde(default)]
    pub notes: Option<String>,
    /// The currency; the first split's account commodity when that is a currency.
    #[serde(default)]
    pub currency: Option<CommodityId>,
    /// The template splits, in order.
    pub splits: Vec<TemplateSplitParam>,
}

/// Params of `scheduledTransaction.create`; absent fields take the editor's defaults.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateParams {
    /// The name, not empty.
    pub name: String,
    /// The Schedule: at least one Recurrence, each starting on `startDate` unless it says otherwise.
    pub schedule: Vec<RecurrenceParam>,
    /// The template transactions; default none.
    #[serde(default)]
    pub template_transactions: Vec<TemplateTransactionParam>,
    /// Since Last Run leaves out a disabled one; default true.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// The first possible occurrence; default today.
    #[serde(default)]
    pub start_date: Option<Day>,
    /// The end; default never. Setting one kind of end clears the other.
    #[serde(default)]
    pub end: Option<EndWire>,
    /// Create due instances without asking; default false. Refused with variables other than `i`, with more than
    /// one commodity, or with no template split.
    #[serde(default)]
    pub auto_create: Option<bool>,
    /// And say so; default true with `autoCreate`, refused without it.
    #[serde(default)]
    pub notify: Option<bool>,
    /// Days ahead to create; default 0.
    #[serde(default)]
    #[schemars(range(min = 0))]
    pub advance_create_days: Option<i32>,
    /// Days ahead to remind, after that; default 0.
    #[serde(default)]
    #[schemars(range(min = 0))]
    pub advance_remind_days: Option<i32>,
}

/// Params of `scheduledTransaction.update`; absent fields are left alone.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateParams {
    /// The scheduled transaction.
    pub scheduled_transaction: Id,
    /// The new name.
    #[serde(default)]
    pub name: Option<String>,
    /// A new Schedule.
    #[serde(default)]
    pub schedule: Option<Vec<RecurrenceParam>>,
    /// New template transactions, replacing every one; those carrying a GUID keep it.
    #[serde(default)]
    pub template_transactions: Option<Vec<TemplateTransactionParam>>,
    /// Since Last Run leaves out a disabled one; default true.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// The first possible occurrence; default today.
    #[serde(default)]
    pub start_date: Option<Day>,
    /// The end; default never. Setting one kind of end clears the other.
    #[serde(default)]
    pub end: Option<EndWire>,
    /// Create due instances without asking; default false. Refused with variables other than `i`, with more than
    /// one commodity, or with no template split.
    #[serde(default)]
    pub auto_create: Option<bool>,
    /// And say so; default true with `autoCreate`, refused without it.
    #[serde(default)]
    pub notify: Option<bool>,
    /// Days ahead to create; default 0.
    #[serde(default)]
    #[schemars(range(min = 0))]
    pub advance_create_days: Option<i32>,
    /// Days ahead to remind, after that; default 0.
    #[serde(default)]
    #[schemars(range(min = 0))]
    pub advance_remind_days: Option<i32>,
}

/// Names one scheduled transaction.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ScheduledKey {
    /// The scheduled transaction.
    pub scheduled_transaction: Id,
}

/// Params of `scheduledTransaction.list`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListParams {
    /// Only those enabled, or only those disabled.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Only those whose template splits name this account.
    #[serde(default)]
    pub account: Option<AccountRef>,
}

/// The result of `scheduledTransaction.list`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledList {
    /// In book order, without their template transactions.
    pub scheduled_transactions: Vec<ScheduledInfo>,
}

/// The result of `scheduledTransaction.delete`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Deleted {
    /// Always true.
    pub deleted: bool,
}

const fn ten() -> usize {
    10
}

/// Params of `scheduledTransaction.occurrences`: a scheduled transaction, or a Schedule not yet saved.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OccurrencesParams {
    /// Its occurrences from its current state.
    #[serde(default)]
    pub scheduled_transaction: Option<Id>,
    /// Or a Schedule not yet saved, as never run.
    #[serde(default)]
    pub schedule: Option<Vec<RecurrenceParam>>,
    /// The unsaved Schedule's start date; default today.
    #[serde(default)]
    pub start_date: Option<Day>,
    /// The unsaved Schedule's end; default never.
    #[serde(default)]
    pub end: Option<EndWire>,
    /// How many, 1 to 1000.
    #[serde(default = "ten")]
    #[schemars(range(min = 1, max = 1000))]
    pub count: usize,
    /// Only occurrences after this date.
    #[serde(default)]
    pub after: Option<Day>,
}

/// One occurrence.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OccurrenceInfo {
    /// The date.
    pub date: Day,
    /// Its `i`.
    pub instance_count: i32,
}

/// The result of `scheduledTransaction.occurrences`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OccurrenceList {
    /// Ascending.
    pub occurrences: Vec<OccurrenceInfo>,
}

/// The Formulas' number format: the Configuration's Locale's monetary separators.
pub(crate) fn format(ctx: &Context) -> Result<NumberFormat, RpcError> {
    Ok(number_format(&ctx.settings.config.load()?.locale()?))
}

/// A scheduled transaction as written, with the editor's confirmations.
pub(crate) fn edited(book: &Book, e: &Edited, format: NumberFormat) -> Result<ScheduledInfo, RpcError> {
    Ok(ScheduledInfo::new(book, ops::get(book, e.guid)?, true, format)?.with_warnings(&e.warnings))
}

pub(crate) fn schedule(params: Vec<RecurrenceParam>) -> Vec<RecurrenceSpec> {
    params
        .into_iter()
        .map(|r| RecurrenceSpec {
            mult: r.mult,
            period_type: r.period_type.to_core(),
            start: r.start.map(|d| d.0),
            weekend_adjust: r.weekend_adjust.to_core(),
        })
        .collect()
}

fn templates(book: &Book, params: Vec<TemplateTransactionParam>) -> Result<Vec<TemplateSpec>, RpcError> {
    params
        .into_iter()
        .map(|t| {
            Ok(TemplateSpec {
                guid: t.guid.map(|g| g.0),
                description: t.description,
                num: t.num,
                notes: t.notes,
                currency: t.currency.map(|c| c.0),
                splits: t.splits.into_iter().map(|s| split(book, s)).collect::<Result<_, RpcError>>()?,
            })
        })
        .collect()
}

fn split(book: &Book, s: TemplateSplitParam) -> Result<SplitSpec, RpcError> {
    Ok(SplitSpec {
        guid: s.guid.map(|g| g.0),
        account: s.account.resolve(book)?,
        memo: s.memo,
        action: s.action,
        debit_formula: s.debit_formula,
        credit_formula: s.credit_formula,
    })
}

/// The fields `create` and `update` share.
macro_rules! fields {
    ($p:expr) => {
        Fields {
            enabled: $p.enabled,
            start: $p.start_date.map(|d| d.0),
            end: $p.end.map(EndWire::to_core).transpose()?,
            auto_create: $p.auto_create,
            notify: $p.notify,
            advance_create_days: $p.advance_create_days,
            advance_remind_days: $p.advance_remind_days,
            ..Fields::default()
        }
    };
}

fn create(ctx: &mut Context, p: CreateParams) -> Result<ScheduledInfo, RpcError> {
    let format = format(ctx)?;
    let base = fields!(p);
    ctx.mutate_rpc(|book, env| {
        let f = Fields {
            name: Some(p.name),
            schedule: Some(schedule(p.schedule)),
            templates: Some(templates(book, p.template_transactions)?),
            ..base
        };
        let e = ops::create(book, env, f, format)?;
        edited(book, &e, format)
    })
}

fn get(ctx: &mut Context, p: ScheduledKey) -> Result<ScheduledInfo, RpcError> {
    let format = format(ctx)?;
    let book = ctx.book()?;
    ScheduledInfo::new(book, ops::get(book, p.scheduled_transaction.0)?, true, format)
}

/// Whether a template split of `sx` names `account` (`gnc_sx_get_sxes_referencing_account`,
/// `5.17:libgnucash/engine/SX-book.cpp:364-389`).
fn references(book: &Book, sx: &red_ale_core::ScheduledTransaction, account: Guid) -> bool {
    template_transactions(book, sx)
        .iter()
        .flat_map(|t| &t.splits)
        .any(|s| TemplateSplit::of(s).and_then(|f| f.account()) == Some(account))
}

fn list(ctx: &mut Context, p: ListParams) -> Result<ScheduledList, RpcError> {
    let format = format(ctx)?;
    let book = ctx.book()?;
    let account = p.account.map(|a| a.resolve(book)).transpose()?;
    let scheduled_transactions = book
        .scheduled
        .values()
        .filter(|sx| p.enabled.is_none_or(|e| sx.enabled == e))
        .filter(|sx| account.is_none_or(|a| references(book, sx, a)))
        .map(|sx| ScheduledInfo::new(book, sx, false, format))
        .collect::<Result<_, _>>()?;
    Ok(ScheduledList { scheduled_transactions })
}

fn update(ctx: &mut Context, p: UpdateParams) -> Result<ScheduledInfo, RpcError> {
    let format = format(ctx)?;
    let base = fields!(p);
    ctx.mutate_rpc(|book, env| {
        let f = Fields {
            name: p.name,
            schedule: p.schedule.map(schedule),
            templates: p.template_transactions.map(|t| templates(book, t)).transpose()?,
            ..base
        };
        let e = ops::update(book, env, p.scheduled_transaction.0, f, format)?;
        edited(book, &e, format)
    })
}

fn delete(ctx: &mut Context, p: ScheduledKey) -> Result<Deleted, RpcError> {
    ctx.mutate(|book, _| ops::delete(book, p.scheduled_transaction.0).map(|()| Deleted { deleted: true }))
}

fn occurrences(ctx: &mut Context, p: OccurrencesParams) -> Result<OccurrenceList, RpcError> {
    let after = p.after.map(|d| d.0);
    if !(1..=1000).contains(&p.count) {
        return Err(RpcError::invalid_params(format!("count must be 1 to 1000, not {}", p.count)));
    }
    ctx.query(|book, env| {
        let draft;
        let sx = match (p.scheduled_transaction, p.schedule) {
            (Some(guid), None) if p.start_date.is_none() && p.end.is_none() => ops::get(book, guid.0)?,
            (None, Some(recurrences)) => {
                let today = env.today().ok_or_else(|| RpcError::invalid("the clock is outside the calendar"))?;
                let end = p.end.map(EndWire::to_core).transpose()?.unwrap_or(End::Never);
                draft = ops::unsaved(p.start_date.map_or(today, |d| d.0), schedule(recurrences), end)?;
                &draft
            }
            _ => {
                return Err(RpcError::invalid_params(
                    "give scheduledTransaction, or a schedule with its startDate and end, not both",
                ));
            }
        };
        let occurrences = ops::occurrences(sx, p.count, after).map_err(|e| RpcError::invalid(e.to_string()))?;
        Ok(OccurrenceList {
            occurrences: occurrences
                .into_iter()
                .map(|o| OccurrenceInfo {
                    date: Day(o.date),
                    instance_count: o.instance_count,
                })
                .collect(),
        })
    })
}
