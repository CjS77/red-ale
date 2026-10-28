//! A scheduled transaction on the wire (`design/api/scheduled-transactions.md`, "The model on the wire").

use red_ale_core::formula::NumberFormat;
use red_ale_core::ops::scheduled::{TemporalState, Warning, next_instance, template_transactions, variables};
use red_ale_core::{Book, ScheduledTransaction, Split, TemplateSplit, Transaction};
use schemars::JsonSchema;
use serde::Serialize;

use crate::error::RpcError;
use crate::wire::{Amount, CommodityId, Day, DeferredInstanceWire, EndWire, Id, RecurrenceWire};

/// A scheduled transaction, with every field GnuCash's file holds.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename = "ScheduledTransactionInfo")]
pub struct ScheduledInfo {
    /// Identity.
    pub guid: Id,
    /// The name; names need not be unique.
    pub name: String,
    /// Disabled ones are left out of Since Last Run and the forecasts.
    pub enabled: bool,
    /// The first possible occurrence.
    pub start_date: Day,
    /// The end, as stored.
    pub end: EndWire,
    /// The last occurrence acted on (created, ignored or postponed), or null if never run.
    pub last_occurrence: Option<Day>,
    /// The Instance count: the value of `i` for the next instance.
    pub instance_count: i32,
    /// Create due instances without asking.
    pub auto_create: bool,
    /// And say so.
    pub notify: bool,
    /// Days ahead to create.
    pub advance_create_days: i32,
    /// Days ahead to remind, after that.
    pub advance_remind_days: i32,
    /// The Schedule, as stored.
    pub schedule: Vec<RecurrenceWire>,
    /// The next occurrence after the last (or the first), or null when it has ended or never occurs.
    pub next_occurrence: Option<Day>,
    /// The state saved before each postponed instance, in file order.
    pub deferred_instances: Vec<DeferredInstanceWire>,
    /// The template account's GUID, or null when there is none.
    pub template_account: Option<Id>,
    /// The template transactions, in the template account's split order; absent in `scheduledTransaction.list`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_transactions: Option<Vec<TemplateTransactionInfo>>,
    /// Every variable the formulas name, `i` and the exchange-rate variables, sorted bytewise.
    pub variables: Vec<String>,
    /// What GnuCash's editor would have asked to confirm; only from methods that write.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<WarningInfo>>,
}

/// A template transaction.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename = "TemplateTransaction")]
pub struct TemplateTransactionInfo {
    /// Identity.
    pub guid: Id,
    /// The description.
    pub description: Option<String>,
    /// The num.
    pub num: Option<String>,
    /// The notes.
    pub notes: Option<String>,
    /// The currency it is written in.
    pub currency: CommodityId,
    /// The template splits, in order.
    pub splits: Vec<TemplateSplitInfo>,
}

/// A template split.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename = "TemplateSplit")]
pub struct TemplateSplitInfo {
    /// Identity.
    pub guid: Id,
    /// The real account the created split goes to, or null when the split names none.
    pub account: Option<Id>,
    /// Its full name, or null when it is not in the Book.
    pub account_name: Option<String>,
    /// The memo.
    pub memo: Option<String>,
    /// The action.
    pub action: Option<String>,
    /// The debit Formula; empty or null means no debit.
    pub debit_formula: Option<String>,
    /// The credit Formula; empty or null means no credit.
    pub credit_formula: Option<String>,
    /// The debit Formula's value when it names no variable, as stored (read only).
    pub debit_numeric: Option<Amount>,
    /// The credit Formula's value likewise.
    pub credit_numeric: Option<Amount>,
}

/// One of the editor's confirmations, reported instead of asked.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WarningInfo {
    /// `duplicate-name` or `never-runs`.
    pub code: &'static str,
    /// What the editor would have asked.
    pub message: String,
}

impl From<&Warning> for WarningInfo {
    fn from(w: &Warning) -> Self {
        Self {
            code: w.code(),
            message: w.to_string(),
        }
    }
}

impl ScheduledInfo {
    /// `sx` as `get` gives it, with its template transactions when `full`.
    pub fn new(book: &Book, sx: &ScheduledTransaction, full: bool, format: NumberFormat) -> Result<Self, RpcError> {
        let next = next_instance(sx, Some(&TemporalState::of(sx))).map_err(|e| RpcError::invalid(e.to_string()))?;
        Ok(Self {
            guid: Id(sx.guid),
            name: sx.name.clone(),
            enabled: sx.enabled,
            start_date: Day(sx.start),
            end: EndWire::of(sx),
            last_occurrence: sx.last.map(Day),
            instance_count: sx.instance_count,
            auto_create: sx.auto_create,
            notify: sx.auto_create_notify,
            advance_create_days: sx.advance_create_days,
            advance_remind_days: sx.advance_remind_days,
            schedule: sx.schedule.iter().copied().map(RecurrenceWire::from).collect(),
            next_occurrence: next.map(Day),
            deferred_instances: sx.deferred.iter().map(DeferredInstanceWire::from).collect(),
            template_account: sx.template_account.map(Id),
            template_transactions: full.then(|| template_transactions(book, sx).into_iter().map(|t| template(book, t)).collect()),
            variables: variables(book, sx, format).0.into_iter().collect(),
            warnings: None,
        })
    }

    /// With the editor's confirmations.
    #[must_use]
    pub fn with_warnings(mut self, warnings: &[Warning]) -> Self {
        self.warnings = Some(warnings.iter().map(WarningInfo::from).collect());
        self
    }
}

fn template(book: &Book, t: &Transaction) -> TemplateTransactionInfo {
    TemplateTransactionInfo {
        guid: Id(t.guid),
        description: t.description.clone(),
        num: t.num.clone(),
        notes: t.notes().map(str::to_owned),
        currency: CommodityId(t.currency.clone()),
        splits: t.splits.iter().map(|s| split(book, s)).collect(),
    }
}

fn split(book: &Book, s: &Split) -> TemplateSplitInfo {
    let frame = TemplateSplit::of(s);
    let account = frame.and_then(|f| f.account());
    TemplateSplitInfo {
        guid: Id(s.guid),
        account: account.map(Id),
        account_name: account.and_then(|a| book.full_name(a)),
        memo: s.memo.clone(),
        action: s.action.clone(),
        debit_formula: frame.and_then(|f| f.debit_formula()).map(str::to_owned),
        credit_formula: frame.and_then(|f| f.credit_formula()).map(str::to_owned),
        debit_numeric: frame.and_then(|f| f.debit_numeric()).map(Amount),
        credit_numeric: frame.and_then(|f| f.credit_numeric()).map(Amount),
    }
}
