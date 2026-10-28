//! `report.list`, `report.options`, `report.run` and `report.export`: the read methods over the template registry.

use red_ale_core::{Book, Env};
use red_ale_locale::Locale;
use red_ale_report::engine::{EXPORT_CSV, RunContext};
use red_ale_report::error::OptionError;
use red_ale_report::options::{GENERAL, OptionInfo, STYLESHEET};
use red_ale_report::render::{self, DEFAULT_STYLE_SHEET, RenderContext, StyleSheet};
use red_ale_report::{ReportError, ReportResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Found, On, ReportInfo, ReportOptions, ReportRef, Reports};
use crate::dispatch::Context;
use crate::error::RpcError;
use crate::wire::{NoParams, Timestamp};

/// A user script that did not load, or a file the loader left out of the report directory.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoadProblem {
    /// The script: `./` and its path in the report directory, `.` for the directory itself.
    pub source: String,
    /// `report-load`, or `report-limit` for a script past a limit when it compiles.
    pub code: String,
    /// What went wrong.
    pub message: String,
    /// The line, from 1, when known.
    pub line: Option<u32>,
}

/// The result of `report.list`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListResult {
    /// Every template and saved configuration, sorted by name (bytewise), then GUID.
    pub reports: Vec<ReportInfo>,
    /// User scripts that did not load, so one bad file does not hide the others.
    pub problems: Vec<LoadProblem>,
}

pub(super) fn list(ctx: &mut Context, _: NoParams) -> Result<ListResult, RpcError> {
    let reports = Reports::load(ctx)?;
    let templates = reports.registry.templates().into_iter().map(Found::Template);
    let mut all: Vec<ReportInfo> = templates
        .chain(reports.saved.iter().map(Found::Saved))
        .map(|f| reports.info(f))
        .collect();
    all.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.guid.cmp(&b.guid)));
    let unread = reports.unread.iter().map(|p| LoadProblem {
        source: if p.path == "." { p.path.clone() } else { format!("./{}", p.path) },
        code: "report-load".to_owned(),
        message: p.message.clone(),
        line: None,
    });
    let problems = unread.chain(reports.registry.problems().iter().map(problem)).collect();
    Ok(ListResult { reports: all, problems })
}

fn problem(e: &ReportError) -> LoadProblem {
    let (source, line) = match e {
        ReportError::Load(l) => (l.source.clone(), l.line),
        ReportError::Limit(l) => (l.source.clone().unwrap_or_default(), None),
        ReportError::Script(s) => (s.source.clone(), s.line),
        _ => (String::new(), None),
    };
    let message = match e {
        ReportError::Load(l) => l.message.clone(),
        other => other.to_string(),
    };
    LoadProblem {
        source,
        code: e.kind().name().to_owned(),
        message,
        line,
    }
}

/// Params of `report.options`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OptionsParams {
    /// The report.
    pub report: ReportRef,
}

/// The result of `report.options`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptionsResult {
    /// The report.
    pub report: ReportInfo,
    /// Every option in GnuCash's order (sections by name, options by key), with its default and configured value.
    pub options: Vec<OptionInfo>,
    /// The JSON Schema (2020-12) of this report's `options` param.
    pub params_schema: Value,
}

pub(super) fn options(ctx: &mut Context, p: OptionsParams) -> Result<OptionsResult, RpcError> {
    let reports = Reports::load(ctx)?;
    ctx.query_any(|book, env| {
        let found = reports.find(&p.report.0)?;
        let (template, saved) = reports.resolve(found)?;
        let now = env.now();
        let db = reports.configured(
            template,
            saved,
            None,
            &On {
                book,
                now,
                zone: env.zone(),
            },
        )?;
        Ok(OptionsResult {
            report: reports.info(found),
            options: db.describe(),
            params_schema: db.params_schema(),
        })
    })
}

/// Params of `report.run`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RunParams {
    /// The report.
    pub report: ReportRef,
    /// Changes over the template's defaults, or over the saved configuration's values.
    #[serde(default)]
    pub options: Option<ReportOptions>,
    /// The time to run the report for; by default the session's clock. Relative dates and "today" follow it.
    #[serde(default)]
    pub now: Option<Timestamp>,
    /// The Locale to format numbers and dates in for this run: a supported POSIX locale name (`fr_FR`, `C`) or
    /// `environment`; by default the Configuration's `locale` setting.
    #[serde(default)]
    pub locale: Option<String>,
}

pub(super) fn run(ctx: &mut Context, p: RunParams) -> Result<ReportResult, RpcError> {
    let reports = Reports::load(ctx)?;
    let locale = reports.locale(p.locale.as_deref())?;
    ctx.query(|book, env| {
        reports.run(
            &p.report,
            p.options.as_ref(),
            &RunAt {
                now: p.now,
                locale,
                export: None,
            },
            book,
            env,
        )
    })
}

/// Params of `report.export`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportParams {
    /// The report.
    pub report: ReportRef,
    /// Changes over the template's defaults, or over the saved configuration's values.
    #[serde(default)]
    pub options: Option<ReportOptions>,
    /// The time to run the report for; by default the session's clock.
    #[serde(default)]
    pub now: Option<Timestamp>,
    /// The style sheet to render through, instead of the one the report's General/"Stylesheet" option names.
    #[serde(default)]
    pub style_sheet: Option<String>,
    /// The Locale to format numbers and dates in for this run, as in `report.run`.
    #[serde(default)]
    pub locale: Option<String>,
}

/// What a run takes besides the report and its options.
struct RunAt<'a> {
    /// The time to run for, else the session's clock.
    now: Option<Timestamp>,
    /// The Locale numbers and dates follow.
    locale: Locale,
    /// The export the run is for, which the template must offer.
    export: Option<&'a str>,
}

/// The result of `report.export`. Nothing is written to disk.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    /// The report's title.
    pub title: String,
    /// One self-contained HTML document: no script and no external resource.
    pub html: String,
}

pub(super) fn export(ctx: &mut Context, p: ExportParams) -> Result<ExportResult, RpcError> {
    let reports = Reports::load(ctx)?;
    let locale = reports.locale(p.locale.as_deref())?;
    ctx.query(|book, env| {
        let mut result = reports.run(
            &p.report,
            p.options.as_ref(),
            &RunAt {
                now: p.now,
                locale,
                export: None,
            },
            book,
            env,
        )?;
        let (name, sheet) = reports.style_sheet(&result, p.style_sheet.as_deref())?;
        if sheet.is_none() {
            result.log.push(render::missing_style_sheet(&name));
        }
        let page = RenderContext {
            zone: env.zone(),
            date_format: reports.settings.date_format,
            locale,
        };
        let html = render::html(&result, sheet, &page);
        Ok(ExportResult { title: result.title, html })
    })
}

/// Params of `report.exportCsv`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportCsvParams {
    /// The report: a net or category chart, or a saved configuration of one.
    pub report: ReportRef,
    /// Changes over the template's defaults, or over the saved configuration's values.
    #[serde(default)]
    pub options: Option<ReportOptions>,
    /// The time to run the report for; by default the session's clock.
    #[serde(default)]
    pub now: Option<Timestamp>,
}

/// The result of `report.exportCsv`. Nothing is written to disk.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportCsvResult {
    /// The report's title: empty for the charts, which GnuCash leaves untitled.
    pub title: String,
    /// The CSV text, as GnuCash's `gnc:lists->csv` writes it: rows joined by `\n`, no newline after the last.
    pub csv: String,
}

/// Run a chart report for CSV export, GnuCash's export thunk with export type `csv`
/// (`5.17:gnucash/report/reports/standard/net-charts.scm:480-482`). A run with nothing to export (no accounts, no
/// data) is `invalid`, GnuCash's "No exportable data" (`:456-457`).
pub(super) fn export_csv(ctx: &mut Context, p: ExportCsvParams) -> Result<ExportCsvResult, RpcError> {
    let reports = Reports::load(ctx)?;
    let locale = reports.locale(None)?;
    ctx.query(|book, env| {
        let at = RunAt {
            now: p.now,
            locale,
            export: Some(EXPORT_CSV),
        };
        let result = reports.run(&p.report, p.options.as_ref(), &at, book, env)?;
        if let Some(message) = &result.export_error {
            return Err(RpcError::invalid(message.clone()).with_data(json!({ "reason": "export-error", "message": message })));
        }
        let rows = result.export.as_ref().ok_or_else(|| {
            RpcError::invalid("No exportable data: the report has no accounts or no figures to export")
                .with_data(json!({ "reason": "no-exportable-data" }))
        })?;
        Ok(ExportCsvResult {
            title: result.title.clone(),
            csv: render::csv(rows),
        })
    })
}

impl Reports {
    /// Run a report: the template's options with a saved configuration's values, then the caller's, applied.
    fn run(
        &self,
        report: &ReportRef,
        options: Option<&ReportOptions>,
        at: &RunAt<'_>,
        book: &Book,
        env: &mut dyn Env,
    ) -> Result<ReportResult, RpcError> {
        let (template, saved) = self.resolve(self.find(&report.0)?)?;
        let now = at.now.map_or_else(|| env.now(), |t| t.0);
        let names = self.style_sheet_choices(saved);
        let saved_options = saved.map(|s| Value::Object(s.options.clone()));
        let changes: Vec<&Value> = saved_options.iter().chain(options.map(|o| &o.0)).collect();
        if let Some(export) = at.export.filter(|e| !template.export_types.iter().any(|t| t == e)) {
            let message = format!(
                "{} has no {export} export; only the net and category charts have one",
                template.name
            );
            return Err(RpcError::invalid(message).with_data(json!({ "reason": "no-export", "exportType": export })));
        }
        let run = RunContext {
            export: at.export,
            ..self.run_context(book, now, env.zone(), &names, at.locale)
        };
        Ok(self.registry.run(template, &changes, saved.map(|s| s.guid), &run)?)
    }

    /// The style sheet to render `result` through: `chosen`, which must exist, else the one its General/"Stylesheet"
    /// option names, which may not (`None`: the base look).
    fn style_sheet(&self, result: &ReportResult, chosen: Option<&str>) -> Result<(String, Option<&StyleSheet>), RpcError> {
        if let Some(name) = chosen {
            let missing = || ReportError::from(OptionError::new(GENERAL, STYLESHEET, format!("no style sheet is called {name:?}")));
            return self
                .sheets
                .get(name)
                .map(|s| (name.to_owned(), Some(s)))
                .ok_or_else(|| missing().into());
        }
        let named = result
            .options
            .get(GENERAL)
            .and_then(|g| g.get(STYLESHEET))
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT_STYLE_SHEET);
        Ok((named.to_owned(), self.sheets.get(named)))
    }
}
