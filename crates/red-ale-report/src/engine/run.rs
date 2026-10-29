//! Running a template (`design/reports.md` §2, §6, §7): declare its options, apply the caller's values, render with
//! the query API, and finish the draft into a report result.

use std::rc::Rc;
use std::sync::Arc;

use red_ale_core::formula::NumberFormat;
use red_ale_core::ops::period::{AccountingPeriod, FiscalYearEnd, book_fiscal_year_end, period_bounds};
use red_ale_core::time::PeriodBounds;
use red_ale_core::{Book, Guid, Time64, TimeZone};
use red_ale_locale::{Conventions, Locale};
use rhai::{AST, Dynamic, Engine};
use serde_json::Value;

use super::context::ContextHandle;
use super::convert::{from_json, to_json};
use super::errors::{Failure, from_eval, from_parse};
use super::log::Log;
use super::options::{OptionBuilder, Resolve, resolve};
use super::registry::{Registry, TemplateInfo, TemplateKind, call};
use super::resolver::Defined;
use super::setup::{Setup, engine};
use crate::error::{ReportError, ResultError};
use crate::limits::Limit;
use crate::options::{ChoiceKey, GENERAL, OPTIONS_SUMMARY, OptionDb, OptionValue, ReportOption, ValueContext};
use crate::query::{BookHandle, Snapshot};
use crate::result::{Block, DisplayContext, OptionsSummary, ReportDraft, ReportIdentity, ReportResult, RunInfo, Section};
use crate::settings::ReportSettings;
use crate::wire::Id;

/// The style sheet the General "Stylesheet" option offers when the host names none.
const DEFAULT_STYLE_SHEET: &str = "Default";

/// What a run needs from the host. Nothing here is read from the environment or the clock.
#[derive(Debug, Clone, Copy)]
pub struct RunContext<'a> {
    /// The Book to read: the open one, or, to describe options with no book open, an empty Book.
    pub book: &'a Book,
    /// The time the report is run for: the session's clock, or the caller's `now`.
    pub now: Time64,
    /// The local time zone.
    pub zone: &'a TimeZone,
    /// The report settings of the Configuration.
    pub settings: &'a ReportSettings,
    /// The Locale numbers and dates follow, and unset settings default from.
    pub locale: Locale,
    /// The accounting period setting; its bounds are taken at `now`.
    pub accounting_period: &'a AccountingPeriod,
    /// The Configuration's fiscal year end. When it is `None` the Book's `fy_end` slot applies, as `period.dates`
    /// decides it.
    pub fiscal_year_end: Option<FiscalYearEnd>,
    /// The style sheets the General "Stylesheet" option offers, by name.
    pub style_sheets: &'a [&'a str],
    /// The export the run is for, one of the template's export types (`csv`), which the script reads as
    /// `ctx.export_type()`, as GnuCash passes `export-type` to a renderer (`5.17:gnucash/report/report-core.scm:722`);
    /// `None` for a plain run, whose result then has no export.
    pub export: Option<&'a str>,
}

/// A template's engine and compiled script, over one snapshot.
struct Run<'r> {
    template: &'r TemplateInfo,
    engine: Engine,
    ast: AST,
    snap: Rc<Snapshot>,
    log: Log,
    params: Dynamic,
    registry: &'r Registry,
}

impl Registry {
    /// A template's options with their defaults, GnuCash's three General options included: what `report.options`
    /// describes. Defaults that depend on the Book are computed from `ctx.book`.
    pub fn options(&self, template: &TemplateInfo, ctx: &RunContext<'_>) -> Result<OptionDb, ReportError> {
        Run::start(self, template, ctx)?.declare(ctx)
    }

    /// Run a template: declare its options, apply each set of `changes` in turn (a saved configuration's, then the
    /// caller's; each `{ section: { name: value } }` in wire form), render, and finish the result. An invalid value
    /// is `report-option` and nothing runs. The Book is read through a snapshot and never changes.
    pub fn run(
        &self,
        template: &TemplateInfo,
        changes: &[&Value],
        configuration: Option<Guid>,
        ctx: &RunContext<'_>,
    ) -> Result<ReportResult, ReportError> {
        let run = Run::start(self, template, ctx)?;
        let mut db = run.declare(ctx)?;
        let values = ValueContext {
            book: Some(ctx.book),
            zone: ctx.zone,
        };
        changes.iter().try_for_each(|c| db.apply(c, &values))?;
        let rendered = run.render(&db, ctx)?;
        run.finish(&rendered, &db, configuration, ctx)
    }
}

/// The monetary decimal point and group separator GnuCash's expression parser reads numbers in, with
/// `gnc_localeconv`'s defaults (`5.17:libgnucash/app-utils/gnc-exp-parser.c:362-381`): what a scheduled transaction's
/// formulas are read in when a report asks for the cash flow.
fn formula_format(conv: &Conventions) -> NumberFormat {
    let first = |s: &str, default: char| s.chars().next().unwrap_or(default);
    NumberFormat {
        decimal_point: first(conv.mon_decimal_point, '.'),
        group_separator: first(conv.mon_thousands_sep, ','),
    }
}

impl<'r> Run<'r> {
    fn start(registry: &'r Registry, template: &'r TemplateInfo, ctx: &RunContext<'_>) -> Result<Self, ReportError> {
        let limits = registry.limits();
        let conventions = ctx.locale.conventions();
        let snap = Snapshot::new(ctx.book, ctx.now, ctx.zone.clone(), formula_format(&conventions));
        let log = Log::new(limits.get_usize(Limit::StringSize));
        let defined = Defined::default();
        let engine = engine(&Setup {
            snap: Rc::clone(&snap),
            sources: Arc::clone(registry.sources()),
            limits,
            log: log.clone(),
            conventions,
            defined: defined.clone(),
        });
        let ast = registry.compile(&engine, &template.script, &defined).map_err(|e| {
            let report = template.guid.0.to_string();
            from_parse(
                &e,
                &template.script,
                &Failure {
                    report: &report,
                    source: &template.script,
                    limits: &limits,
                    log: Vec::new(),
                },
            )
        })?;
        let params = from_json(&template.params).map_err(|e| ReportError::Internal(format!("params of {}: {e}", template.name)))?;
        Ok(Self {
            template,
            engine,
            ast,
            snap,
            log,
            params,
            registry,
        })
    }

    fn book(&self) -> BookHandle {
        BookHandle(Rc::clone(&self.snap))
    }

    fn failed(&self, e: &rhai::EvalAltResult) -> ReportError {
        let limits = self.registry.limits();
        let report = self.template.guid.0.to_string();
        from_eval(
            e,
            Failure {
                report: &report,
                source: &self.template.script,
                limits: &limits,
                log: self.log.entries(),
            },
        )
    }

    /// `options(params, o, book)`, then the General options every template gets.
    fn declare(&self, ctx: &RunContext<'_>) -> Result<OptionDb, ReportError> {
        let builder = OptionBuilder::new(self.book(), ctx.settings.report_currency(ctx.book, &ctx.locale));
        let args = (self.params.clone(), builder.clone(), self.book());
        call(&self.engine, &self.ast, "options", args)
            .map(drop)
            .map_err(|e| self.failed(&e))?;
        let mut db = builder.into_db();
        let sheets = if ctx.style_sheets.is_empty() {
            &[DEFAULT_STYLE_SHEET][..]
        } else {
            ctx.style_sheets
        };
        db.add_general_options(&self.template.name, sheets)?;
        Ok(db)
    }

    /// `render(params, book, opts, ctx)`.
    fn render(&self, db: &OptionDb, ctx: &RunContext<'_>) -> Result<Dynamic, ReportError> {
        let period = accounting_period(ctx);
        let opts = resolve(
            db,
            &Resolve {
                book: &self.book(),
                now: ctx.now,
                zone: ctx.zone,
                period: &period,
            },
        )?;
        let context = ContextHandle::new(Rc::clone(&self.snap), ctx.zone.clone(), ctx, period);
        let args = (self.params.clone(), self.book(), opts, context);
        call(&self.engine, &self.ast, "render", args).map_err(|e| self.failed(&e))
    }

    /// The draft as a result: the run's identity, time, options and log, every display form, and the options summary
    /// when the General option asks for it (`gnc:report-render-html`, `5.17:gnucash/report/report-core.scm:764-780`).
    fn finish(
        &self,
        rendered: &Dynamic,
        db: &OptionDb,
        configuration: Option<Guid>,
        ctx: &RunContext<'_>,
    ) -> Result<ReportResult, ReportError> {
        let display = DisplayContext {
            book: ctx.book,
            zone: ctx.zone,
            date_format: ctx.settings.date_format,
            locale: ctx.locale,
        };
        let run = RunInfo {
            report: ReportIdentity {
                guid: self.template.guid,
                name: self.template.name.clone(),
                configuration: configuration.map(Id),
            },
            now: ctx.now,
            options: db.values(),
            log: self.log.entries(),
        };
        let finished = to_json(rendered)
            .and_then(|json| ReportDraft::from_json(&json))
            .and_then(|draft| draft.finish(run, &display));
        let mut result = finished.map_err(|e| self.invalid(e))?;
        if ctx.export.is_none() {
            result.export = None;
        }
        let summary = db.get(GENERAL, OPTIONS_SUMMARY).map(ReportOption::value);
        if summary.is_some_and(|v| *v != OptionValue::Key(ChoiceKey::from("never"))) {
            let rows = db.summary(&display);
            result.sections.push(Section {
                title: None,
                blocks: vec![Block::OptionsSummary(OptionsSummary { rows })],
            });
        }
        Ok(result)
    }

    /// A `report-result` error from a user template; a built-in template's invalid result is red-ale's bug.
    fn invalid(&self, mut e: ResultError) -> ReportError {
        if self.template.kind == TemplateKind::Builtin {
            let name = &self.template.name;
            return ReportError::Internal(format!(
                "built-in template {name} returned an invalid result at {}: {}",
                e.path, e.reason
            ));
        }
        e.report = self.template.guid.0.to_string();
        e.log = self.log.entries();
        ReportError::Result(e)
    }
}

/// The accounting period containing now's local date, as GnuCash bounds it: local 00:00:00 of its first day to
/// 23:59:59 of its last (`ops::period::period_bounds`). An error is kept until something needs the period.
fn accounting_period(ctx: &RunContext<'_>) -> Result<PeriodBounds, String> {
    let today = ctx
        .zone
        .local_date(ctx.now)
        .ok_or_else(|| "now is outside the supported range".to_owned())?;
    let fy = ctx.fiscal_year_end.or_else(|| book_fiscal_year_end(ctx.book));
    period_bounds(ctx.accounting_period, today, fy, ctx.zone).map_err(|e| e.to_string())
}
