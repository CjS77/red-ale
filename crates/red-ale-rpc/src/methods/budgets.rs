//! Methods: Budgets, their amounts and notes, estimating amounts from history, and the natural-signs scrub
//! (`design/api/budgets.md`). The rules live in `red_ale_core::ops::budgets`.

use std::collections::BTreeSet;

use red_ale_core::ops::budgets::{self as ops, BudgetFields, Cell, DefaultFrom, Estimate, SignPolicy};
use red_ale_core::{Book, Budget, Guid, Numeric};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, Day, Id, NoParams, RecurrenceWire};

/// The budget methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "budget.create",
            "Create a budget; the first sets the Book feature for natural signs.",
            Effect::Write,
            create,
        ),
        method("budget.get", "One budget with its periods, amounts and notes.", Effect::Read, get),
        method(
            "budget.list",
            "Every budget in book order, and which is the default.",
            Effect::Read,
            list,
        ),
        method(
            "budget.update",
            "Change a budget's name, description, number of periods or recurrence.",
            Effect::Write,
            update,
        ),
        method(
            "budget.copy",
            "Copy a budget as GnuCash's Copy Budget does (notes are not copied).",
            Effect::Write,
            copy,
        ),
        method(
            "budget.delete",
            "Delete a budget; the last unsets the Book feature for natural signs.",
            Effect::Destructive,
            delete,
        ),
        method(
            "budget.setAmounts",
            "Set or unset many budget amounts, atomically.",
            Effect::Write,
            set_amounts,
        ),
        method(
            "budget.setNotes",
            "Set or delete many budget notes, atomically.",
            Effect::Write,
            set_notes,
        ),
        method(
            "budget.estimate",
            "Overwrite accounts' budget amounts with figures from past transactions, as GnuCash's Estimate dialog does.",
            Effect::Destructive,
            estimate,
        ),
        method(
            "budget.scrubSigns",
            "Run GnuCash's natural-signs scrub: guess each budget's old sign convention and flip amounts to match.",
            Effect::Destructive,
            scrub_signs,
        ),
    ]
}

/// Params of `budget.create`; absent fields take GnuCash's defaults.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateParams {
    /// The name; `Unnamed Budget` when absent.
    #[serde(default)]
    pub name: Option<String>,
    /// The description; empty when absent.
    #[serde(default)]
    pub description: Option<String>,
    /// The number of periods, 1 to 60; 12 when absent.
    #[serde(default)]
    #[schemars(range(min = 1, max = ops::MAX_NUM_PERIODS))]
    pub num_periods: Option<u32>,
    /// The periods' recurrence: never `once`, weekend adjustment `none`. Monthly from the first of the current month
    /// when absent.
    #[serde(default)]
    pub recurrence: Option<RecurrenceWire>,
}

impl CreateParams {
    fn fields(self) -> Result<BudgetFields, RpcError> {
        Ok(BudgetFields {
            name: self.name,
            description: self.description,
            num_periods: self.num_periods,
            recurrence: self.recurrence.map(RecurrenceWire::to_core).transpose()?,
        })
    }
}

/// Params of `budget.update`; absent fields are left alone.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateParams {
    /// The budget.
    pub budget: Id,
    /// The new name.
    #[serde(default)]
    pub name: Option<String>,
    /// The new description.
    #[serde(default)]
    pub description: Option<String>,
    /// The new number of periods, 1 to 60. Lowering it hides amounts and notes beyond it but keeps them.
    #[serde(default)]
    #[schemars(range(min = 1, max = ops::MAX_NUM_PERIODS))]
    pub num_periods: Option<u32>,
    /// The new recurrence: never `once`, weekend adjustment `none`. No amount moves.
    #[serde(default)]
    pub recurrence: Option<RecurrenceWire>,
}

/// Names one budget.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BudgetKey {
    /// The budget.
    pub budget: Id,
}

/// Params of `budget.get`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetParams {
    /// The budget.
    pub budget: Id,
    /// List amounts and notes of these accounts only.
    #[serde(default)]
    pub accounts: Option<Vec<AccountRef>>,
}

/// One Budget period.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PeriodInfo {
    /// 0-based.
    pub index: u32,
    /// The period's first local day.
    pub start_date: Day,
    /// The period's last local day.
    pub end_date: Day,
}

/// A set budget amount, in natural signs.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AmountInfo {
    /// The account's GUID.
    pub account: Id,
    /// The account's full name.
    pub account_name: String,
    /// 0-based period.
    pub period: u32,
    /// The amount as stored.
    pub amount: Amount,
}

/// A budget note.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoteInfo {
    /// The account's GUID.
    pub account: Id,
    /// The account's full name.
    pub account_name: String,
    /// 0-based period.
    pub period: u32,
    /// The note.
    pub note: String,
}

/// How many amount and note Slots a budget keeps without listing them.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SlotCounts {
    /// Amounts.
    pub amounts: usize,
    /// Notes.
    pub notes: usize,
}

/// A budget.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BudgetInfo {
    /// Identity.
    pub guid: Id,
    /// The name.
    pub name: String,
    /// The description.
    pub description: String,
    /// The number of periods.
    pub num_periods: u32,
    /// The recurrence that gives the periods' dates, as stored.
    pub recurrence: RecurrenceWire,
    /// True for the default budget.
    pub is_default: bool,
    /// Each period below `numPeriods`; absent in `budget.list`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub periods: Option<Vec<PeriodInfo>>,
    /// Every set amount of an account in the tree below `numPeriods`, by account full name then period; absent in
    /// `budget.list`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amounts: Option<Vec<AmountInfo>>,
    /// Every note likewise; absent in `budget.list`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<Vec<NoteInfo>>,
    /// Amounts and notes kept at or beyond `numPeriods`.
    pub hidden: SlotCounts,
    /// Amounts and notes kept for GUIDs that name no account.
    pub orphaned: SlotCounts,
}

/// Where the default budget comes from.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DefaultFromWire {
    /// The Book option names it.
    Option,
    /// The option is unset or stale: the first budget in book order.
    First,
}

/// The result of `budget.list`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BudgetList {
    /// Every budget, in book order, without periods, amounts and notes.
    pub budgets: Vec<BudgetInfo>,
    /// The default budget's GUID, or null when the book has none.
    pub default: Option<Id>,
    /// Where the default comes from, or null.
    pub default_from: Option<DefaultFromWire>,
    /// Whether the book has the feature `Use natural signs in budget amounts`.
    pub natural_signs: bool,
    /// True when the book has budgets and lacks the feature: edits are refused until `budget.scrubSigns`.
    pub needs_sign_scrub: bool,
}

/// One cell's amount for `budget.setAmounts`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AmountCell {
    /// The account.
    pub account: AccountRef,
    /// 0-based period, below the budget's number of periods.
    pub period: u32,
    /// The amount in natural signs (an expense budget positive, an income budget negative), stored as given; null
    /// unsets the cell, `"0"` sets a zero.
    pub amount: Option<Amount>,
}

/// Params of `budget.setAmounts`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetAmountsParams {
    /// The budget.
    pub budget: Id,
    /// The cells; each at most once.
    pub amounts: Vec<AmountCell>,
}

/// The result of `budget.setAmounts`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SetAmountsResult {
    /// The budget.
    pub budget: Id,
    /// Cells set.
    pub set: usize,
    /// Set cells unset.
    pub unset: usize,
}

/// One cell's note for `budget.setNotes`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NoteCell {
    /// The account.
    pub account: AccountRef,
    /// 0-based period, below the budget's number of periods.
    pub period: u32,
    /// The note; empty or null deletes it.
    pub note: Option<String>,
}

/// Params of `budget.setNotes`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetNotesParams {
    /// The budget.
    pub budget: Id,
    /// The cells; each at most once.
    pub notes: Vec<NoteCell>,
}

/// The result of `budget.setNotes`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SetNotesResult {
    /// The budget.
    pub budget: Id,
    /// Notes set.
    pub set: usize,
    /// Notes deleted.
    pub deleted: usize,
}

/// The result of `budget.delete`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    /// Always true.
    pub deleted: bool,
    /// The last budget went, so the feature `Use natural signs in budget amounts` was unset.
    pub feature_unset: bool,
}

const fn one() -> u8 {
    1
}

/// Params of `budget.estimate`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EstimateParams {
    /// The budget.
    pub budget: Id,
    /// The accounts to estimate, at least one; each from its own subtree.
    pub accounts: Vec<AccountRef>,
    /// The start of history period 0; history periods follow the budget's recurrence from here.
    pub start_date: Day,
    /// Significant digits to round to, 1 to 12.
    #[serde(default = "one")]
    #[schemars(range(min = 1, max = 12))]
    pub significant_digits: u8,
    /// Store the average over every history period in every period.
    #[serde(default)]
    pub use_average: bool,
}

/// The result of `budget.estimate`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EstimateResult {
    /// The budget.
    pub budget: Id,
    /// Every amount written.
    pub amounts: Vec<AmountInfo>,
}

/// The sign convention the scrub guessed.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PolicyWire {
    /// Income and expense amounts were flipped.
    IncomeExpense,
    /// Nothing was flipped.
    None,
    /// Liability, equity and income amounts were flipped.
    CreditAccounts,
}

/// What the scrub did to one budget.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BudgetScrubInfo {
    /// The budget.
    pub budget: Id,
    /// The convention guessed.
    pub policy: PolicyWire,
    /// How many set amounts were negated, zeros included.
    pub flipped: usize,
}

/// The result of `budget.scrubSigns`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScrubResult {
    /// The feature was set.
    pub feature_set: bool,
    /// The feature was unset, as the book has no budgets.
    pub feature_unset: bool,
    /// Each budget scrubbed, in book order.
    pub budgets: Vec<BudgetScrubInfo>,
}

fn account_name(book: &Book, g: Guid) -> String {
    book.full_name(g).unwrap_or_default()
}

fn amount_info(book: &Book, cell: Cell, amount: Numeric) -> AmountInfo {
    AmountInfo {
        account: Id(cell.account),
        account_name: account_name(book, cell.account),
        period: cell.period,
        amount: Amount(amount),
    }
}

/// A budget as `budget.list` gives it: without periods, amounts and notes.
fn summary(book: &Book, b: &Budget) -> BudgetInfo {
    let kept = ops::kept_slots(book, b);
    BudgetInfo {
        guid: Id(b.guid),
        name: b.name.clone(),
        description: b.description.clone(),
        num_periods: b.num_periods,
        recurrence: b.recurrence.into(),
        is_default: ops::default_budget(book).is_some_and(|(g, _)| g == b.guid),
        periods: None,
        amounts: None,
        notes: None,
        hidden: SlotCounts {
            amounts: kept.hidden_amounts,
            notes: kept.hidden_notes,
        },
        orphaned: SlotCounts {
            amounts: kept.orphaned_amounts,
            notes: kept.orphaned_notes,
        },
    }
}

/// A budget with everything, its amounts and notes limited to `only` when given.
fn describe(book: &Book, guid: Guid, only: Option<&BTreeSet<Guid>>) -> Result<BudgetInfo, RpcError> {
    let b = ops::get(book, guid)?;
    let wanted = |c: &Cell| only.is_none_or(|set| set.contains(&c.account));
    let periods = ops::period_dates(&b.recurrence, b.num_periods)?
        .into_iter()
        .zip(0..)
        .map(|((first, last), index)| PeriodInfo {
            index,
            start_date: Day(first),
            end_date: Day(last),
        })
        .collect();
    let amounts = ops::amounts(book, b)
        .into_iter()
        .filter(|(c, _)| wanted(c))
        .map(|(c, n)| amount_info(book, c, n))
        .collect();
    let notes = ops::notes(book, b)
        .into_iter()
        .filter(|(c, _)| wanted(c))
        .map(|(c, note)| NoteInfo {
            account: Id(c.account),
            account_name: account_name(book, c.account),
            period: c.period,
            note: note.to_owned(),
        })
        .collect();
    Ok(BudgetInfo {
        periods: Some(periods),
        amounts: Some(amounts),
        notes: Some(notes),
        ..summary(book, b)
    })
}

fn create(ctx: &mut Context, p: CreateParams) -> Result<BudgetInfo, RpcError> {
    let fields = p.fields()?;
    ctx.mutate_rpc(|book, env| {
        let guid = ops::create(book, env, fields)?;
        describe(book, guid, None)
    })
}

fn get(ctx: &mut Context, p: GetParams) -> Result<BudgetInfo, RpcError> {
    ctx.query(|book, _| {
        let only = p
            .accounts
            .map(|refs| refs.iter().map(|r| r.resolve(book)).collect::<Result<BTreeSet<_>, _>>())
            .transpose()?;
        describe(book, p.budget.0, only.as_ref())
    })
}

fn list(ctx: &mut Context, _: NoParams) -> Result<BudgetList, RpcError> {
    ctx.query(|book, _| {
        let default = ops::default_budget(book);
        Ok(BudgetList {
            budgets: book.budgets.values().map(|b| summary(book, b)).collect(),
            default: default.map(|(g, _)| Id(g)),
            default_from: default.map(|(_, from)| match from {
                DefaultFrom::Option => DefaultFromWire::Option,
                DefaultFrom::First => DefaultFromWire::First,
            }),
            natural_signs: ops::uses_natural_signs(book),
            needs_sign_scrub: ops::needs_sign_scrub(book),
        })
    })
}

fn update(ctx: &mut Context, p: UpdateParams) -> Result<BudgetInfo, RpcError> {
    let fields = CreateParams {
        name: p.name,
        description: p.description,
        num_periods: p.num_periods,
        recurrence: p.recurrence,
    }
    .fields()?;
    ctx.mutate_rpc(|book, _| {
        ops::update(book, p.budget.0, fields)?;
        describe(book, p.budget.0, None)
    })
}

fn copy(ctx: &mut Context, p: BudgetKey) -> Result<BudgetInfo, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let guid = ops::copy(book, env, p.budget.0)?;
        describe(book, guid, None)
    })
}

fn delete(ctx: &mut Context, p: BudgetKey) -> Result<DeleteResult, RpcError> {
    ctx.mutate(|book, _| {
        ops::delete(book, p.budget.0).map(|feature_unset| DeleteResult {
            deleted: true,
            feature_unset,
        })
    })
}

fn cell(book: &Book, account: &AccountRef, period: u32) -> Result<Cell, RpcError> {
    Ok(Cell {
        account: account.resolve(book)?,
        period,
    })
}

fn set_amounts(ctx: &mut Context, p: SetAmountsParams) -> Result<SetAmountsResult, RpcError> {
    ctx.mutate_rpc(|book, _| {
        let edits = p
            .amounts
            .iter()
            .map(|a| Ok((cell(book, &a.account, a.period)?, a.amount.map(|x| x.0))))
            .collect::<Result<Vec<_>, RpcError>>()?;
        let (set, unset) = ops::set_amounts(book, p.budget.0, &edits)?;
        Ok(SetAmountsResult {
            budget: p.budget,
            set,
            unset,
        })
    })
}

fn set_notes(ctx: &mut Context, p: SetNotesParams) -> Result<SetNotesResult, RpcError> {
    ctx.mutate_rpc(|book, _| {
        let edits = p
            .notes
            .into_iter()
            .map(|n| Ok((cell(book, &n.account, n.period)?, n.note)))
            .collect::<Result<Vec<_>, RpcError>>()?;
        let (set, deleted) = ops::set_notes(book, p.budget.0, &edits)?;
        Ok(SetNotesResult {
            budget: p.budget,
            set,
            deleted,
        })
    })
}

fn estimate(ctx: &mut Context, p: EstimateParams) -> Result<EstimateResult, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let accounts = p.accounts.iter().map(|a| a.resolve(book)).collect::<Result<Vec<_>, _>>()?;
        let e = Estimate {
            accounts,
            start: p.start_date.0,
            significant_digits: p.significant_digits,
            use_average: p.use_average,
        };
        let zone = env.zone().clone();
        let cells = ops::estimate(book, p.budget.0, &e, &zone)?;
        Ok(EstimateResult {
            budget: p.budget,
            amounts: cells.into_iter().map(|(c, n)| amount_info(book, c, n)).collect(),
        })
    })
}

fn scrub_signs(ctx: &mut Context, _: NoParams) -> Result<ScrubResult, RpcError> {
    ctx.mutate(|book, _| {
        let report = ops::scrub_signs(book);
        Ok(ScrubResult {
            feature_set: report.feature_set,
            feature_unset: report.feature_unset,
            budgets: report
                .budgets
                .iter()
                .map(|s| BudgetScrubInfo {
                    budget: Id(s.budget),
                    policy: match s.policy {
                        SignPolicy::IncomeExpense => PolicyWire::IncomeExpense,
                        SignPolicy::None => PolicyWire::None,
                        SignPolicy::CreditAccounts => PolicyWire::CreditAccounts,
                    },
                    flipped: s.flipped,
                })
                .collect(),
        })
    })
}
