//! Methods: the accounting period, a red-ale setting rather than book data (GnuCash keeps it in user preferences).
//! See `design/api/accounts.md`.

use red_ale_core::ops::period::{self, AccountingPeriod, FiscalYearEnd, PeriodBound, RelativePeriod};
use red_ale_core::{Date, Env};
use red_ale_service::session::SystemEnv;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{Day, NoParams};

/// The period methods.
pub fn methods() -> Vec<Method> {
    vec![
        method("period.get", "The configured accounting period and fiscal year end.", Effect::Read, get),
        method("period.set", "Set the accounting period or the fiscal year end in red-ale's configuration.", Effect::Write, set),
        method("period.dates", "The first and last days of the accounting period containing today or a given date.", Effect::Read, dates),
    ]
}

fn bound(s: &str) -> Result<PeriodBound, RpcError> {
    s.parse().map_err(|e: red_ale_core::DomainError| RpcError::invalid_params(e.to_string()))
}

fn fiscal(s: &str) -> Result<FiscalYearEnd, RpcError> {
    s.parse().map_err(|e: red_ale_core::DomainError| RpcError::invalid_params(e.to_string()))
}

/// The accounting period setting.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PeriodInfo {
    /// Start: a relative choice (`today`, `month`, `previous-month`, `quarter`, `previous-quarter`, `year`,
    /// `previous-year`, `fiscal-year`, `previous-fiscal-year`) or a `YYYY-MM-DD` date.
    pub start: String,
    /// End, in the same form; a relative choice means the period's last day.
    pub end: String,
    /// The fiscal year end in use (`MM-DD`), if any.
    pub fiscal_year_end: Option<String>,
    /// Where it comes from: `config`, or `book` (the open book's `fy_end` slot).
    pub fiscal_year_end_source: Option<String>,
    /// The configuration file, if the setting is kept in one; otherwise it lasts as long as the server.
    pub config_file: Option<String>,
    /// Every relative choice, in GnuCash's order.
    pub choices: Vec<String>,
}

struct Current {
    period: AccountingPeriod,
    configured_fy: Option<FiscalYearEnd>,
    book_fy: Option<FiscalYearEnd>,
}

impl Current {
    fn fy(&self) -> Option<FiscalYearEnd> {
        self.configured_fy.or(self.book_fy)
    }
}

fn current(ctx: &mut Context) -> Result<Current, RpcError> {
    let config = ctx.settings.config.load()?;
    let book_fy = if ctx.has_session() { period::book_fiscal_year_end(ctx.book()?) } else { None };
    Ok(Current { period: config.accounting_period()?, configured_fy: config.fiscal_year_end()?, book_fy })
}

fn describe(ctx: &Context, c: &Current) -> PeriodInfo {
    let source = match (c.configured_fy, c.book_fy) {
        (Some(_), _) => Some("config"),
        (None, Some(_)) => Some("book"),
        (None, None) => None,
    };
    PeriodInfo {
        start: c.period.start.to_string(),
        end: c.period.end.to_string(),
        fiscal_year_end: c.fy().map(|f| f.to_string()),
        fiscal_year_end_source: source.map(ToOwned::to_owned),
        config_file: ctx.settings.config.path().map(|p| p.display().to_string()),
        choices: RelativePeriod::ALL.iter().map(|p| p.as_str().to_owned()).collect(),
    }
}

fn get(ctx: &mut Context, _: NoParams) -> Result<PeriodInfo, RpcError> {
    let c = current(ctx)?;
    Ok(describe(ctx, &c))
}

/// Params of `period.set`. Absent fields keep their setting.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetParams {
    /// Start: a relative choice or a `YYYY-MM-DD` date.
    #[serde(default)]
    pub start: Option<String>,
    /// End: a relative choice or a `YYYY-MM-DD` date.
    #[serde(default)]
    pub end: Option<String>,
    /// Fiscal year end, `MM-DD`, for the fiscal-year choices.
    #[serde(default)]
    pub fiscal_year_end: Option<String>,
    /// Remove the configured fiscal year end (the open book's `fy_end` slot, if any, then applies).
    #[serde(default)]
    pub clear_fiscal_year_end: bool,
}

fn set(ctx: &mut Context, p: SetParams) -> Result<PeriodInfo, RpcError> {
    let mut c = current(ctx)?;
    if let Some(s) = p.start.as_deref() {
        c.period.start = bound(s)?;
    }
    if let Some(e) = p.end.as_deref() {
        c.period.end = bound(e)?;
    }
    if p.clear_fiscal_year_end {
        c.configured_fy = None;
    }
    if let Some(f) = p.fiscal_year_end.as_deref() {
        c.configured_fy = Some(fiscal(f)?);
    }
    period::check_period(&c.period, c.fy())?;
    let mut config = ctx.settings.config.load()?;
    config.set_accounting_period(&c.period, c.configured_fy);
    ctx.settings.config.save(&config)?;
    Ok(describe(ctx, &c))
}

/// Params of `period.dates`. The optional fields override the configured setting for this call only.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DatesParams {
    /// The day the period must contain; by default today (UTC) by the server's clock.
    #[serde(default)]
    pub date: Option<Day>,
    /// Start, instead of the configured one.
    #[serde(default)]
    pub start: Option<String>,
    /// End, instead of the configured one.
    #[serde(default)]
    pub end: Option<String>,
    /// Fiscal year end, `MM-DD`, instead of the configured one.
    #[serde(default)]
    pub fiscal_year_end: Option<String>,
}

/// The result of `period.dates`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DatesResult {
    /// The day used as today.
    pub today: Day,
    /// First day of the period (GnuCash starts it at 00:00:00 local time).
    pub start: Day,
    /// Last day of the period (GnuCash ends it at 23:59:59 local time).
    pub end: Day,
}

fn today(ctx: &mut Context) -> Result<Date, RpcError> {
    let now = if ctx.has_session() { ctx.query(|_, env| Ok(env.now()))? } else { SystemEnv.now() };
    now.utc_date().ok_or_else(|| RpcError::internal("the clock is out of range"))
}

fn dates(ctx: &mut Context, p: DatesParams) -> Result<DatesResult, RpcError> {
    let mut c = current(ctx)?;
    if let Some(s) = p.start.as_deref() {
        c.period.start = bound(s)?;
    }
    if let Some(e) = p.end.as_deref() {
        c.period.end = bound(e)?;
    }
    if let Some(f) = p.fiscal_year_end.as_deref() {
        c.configured_fy = Some(fiscal(f)?);
    }
    let day = match p.date {
        Some(d) => d.0,
        None => today(ctx)?,
    };
    let (start, end) = period::period_dates(&c.period, day, c.fy())?;
    Ok(DatesResult { today: day.into(), start: start.into(), end: end.into() })
}
