//! Methods: reports (`design/api/reports.md`): listing, describing, running and exporting reports, saved report
//! configurations and style sheets.
//!
//! Reports only read the Book. `report.run` and `report.export` reach it through [`Context::query`] alone, never a
//! Mutation, so they work on books open read-only, never make a Book dirty and never write the journal. Saved
//! configurations and style sheets live in red-ale's Configuration; the methods that change them target
//! `configuration` and need no Book.
//!
//! Every call reads the Configuration and the report directory afresh, so edits to either are seen without a
//! restart (`design/reports.md` §2). The template registry is rebuilt only when what it is built from changed: see
//! [`registry`].

mod configurations;
mod import;
mod run;
mod style_sheets;

use std::borrow::Cow;
use std::sync::{Arc, Mutex, PoisonError};

use red_ale_core::ops::period::{AccountingPeriod, FiscalYearEnd};
use red_ale_core::{Book, FixedEnv, Guid, Time64, TimeZone};
use red_ale_locale::Locale;
use red_ale_report::ReportError;
use red_ale_report::engine::{Registry, RunContext, ScriptSource, TemplateInfo, TemplateKind};
use red_ale_report::library::Library;
use red_ale_report::limits::{Limit, Limits};
use red_ale_report::options::{GENERAL, OptionDb, STYLESHEET, ValueContext};
use red_ale_report::render::{StyleSheetEntry, StyleSheets};
use red_ale_report::settings::ReportSettings;
use red_ale_service::config::Config;
use red_ale_service::reports::{SavedReport, ScriptProblem, StyleSheetRecord, load_report_scripts};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::{RpcError, codes};
use crate::wire::Id;

/// The report methods.
pub fn methods() -> Vec<Method> {
    let reads = [
        method(
            "report.list",
            "Every report template and saved report configuration, and the user scripts that did not load.",
            Effect::Read,
            run::list,
        ),
        method(
            "report.options",
            "A report's options with their defaults and values, and the JSON Schema of its options param.",
            Effect::Read,
            run::options,
        ),
        method("report.run", "Run a report on the open book: the report result, with exact figures.", Effect::Read, run::run),
        method(
            "report.export",
            "Run a report and render it through a style sheet as one self-contained HTML document.",
            Effect::Read,
            run::export,
        ),
        method(
            "report.exportCsv",
            "Run a net or category chart and return its data as CSV, as GnuCash's CSV export of those charts writes it.",
            Effect::Read,
            run::export_csv,
        ),
        method("report.styleSheets", "Style sheet templates and style sheets, with their options.", Effect::Read, style_sheets::list),
    ];
    let configuration = [
        method(
            "report.saveConfiguration",
            "Save a report's options under a new name in red-ale's configuration.",
            Effect::Write,
            configurations::save,
        ),
        method("report.updateConfiguration", "Change options of a saved report configuration.", Effect::Write, configurations::update),
        method("report.renameConfiguration", "Rename a saved report configuration.", Effect::Write, configurations::rename),
        method(
            "report.importConfigurations",
            "Import GnuCash's saved report configurations from the text of its saved-reports-2.8 file, read as data, never run.",
            Effect::Write,
            import::configurations,
        ),
        method(
            "report.importStyleSheets",
            "Import GnuCash's style sheets from the text of its stylesheets-2.0 file, read as data, never run.",
            Effect::Write,
            import::style_sheets,
        ),
        method("report.deleteConfiguration", "Delete a saved report configuration.", Effect::Destructive, configurations::delete),
        method(
            "report.createStyleSheet",
            "Create a style sheet from a style sheet template in red-ale's configuration.",
            Effect::Write,
            style_sheets::create,
        ),
        method("report.updateStyleSheet", "Change a style sheet's options.", Effect::Write, style_sheets::update),
        method(
            "report.deleteStyleSheet",
            "Delete a user style sheet, or reset a predefined one to its template's defaults.",
            Effect::Destructive,
            style_sheets::delete,
        ),
    ];
    reads.into_iter().chain(configuration.into_iter().map(Method::on_configuration)).collect()
}

/// A report: a GUID (a template or a saved configuration), or a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReportRef(pub String);

impl JsonSchema for ReportRef {
    fn schema_name() -> Cow<'static, str> {
        "ReportRef".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "A report: the GUID of a report template or saved report configuration, or a name. A name that several reports have is refused with conflict."
        })
    }
}

/// Report option values to change, by section and option name, in wire form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReportOptions(pub Value);

impl JsonSchema for ReportOptions {
    fn schema_name() -> Cow<'static, str> {
        "ReportOptions".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "object",
            "description": "Report options to change, by section and name: {\"General\": {\"Start Date\": {\"absolute\": \"2024-01-01\"}}}. Only the options given change; null returns one to its default where a method allows it. report.options gives each report's own schema (paramsSchema).",
            "additionalProperties": { "type": "object" }
        })
    }
}

/// Where a report is defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ReportKind {
    /// A built-in template, compiled into red-ale.
    Builtin,
    /// A template from a user script in the report directory.
    User,
    /// A saved report configuration.
    Saved,
}

/// A report template or saved report configuration, as `report.list` lists it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportInfo {
    /// The template's GUID (GnuCash's, for a built-in report), or the saved configuration's.
    pub guid: Id,
    /// The template's name, or the saved configuration's.
    pub name: String,
    /// `builtin`, `user` or `saved`.
    pub kind: ReportKind,
    /// For a saved configuration: the GUID of the template it configures.
    pub template: Option<Id>,
    /// The script defining the template: `standard/balance-sheet.rhai`, or `./` and a user script's path in the
    /// report directory. `null` for a saved configuration whose template is not loaded.
    pub script: Option<String>,
    /// GnuCash's menu path. Empty for a saved configuration, which GnuCash lists in its own dialog.
    pub menu_path: Vec<String>,
    /// The menu entry when it differs from the name.
    pub menu_name: Option<String>,
    /// The menu tip.
    pub menu_tip: Option<String>,
    /// Whether the template shows in menus; `false` for a saved configuration.
    pub in_menu: bool,
    /// The exports offered besides HTML: `["csv"]` for the net and category charts (`report.exportCsv`), and for a
    /// saved configuration of one.
    pub export_types: Vec<String>,
}

impl ReportInfo {
    fn template(t: &TemplateInfo) -> Self {
        Self {
            guid: Id(t.guid.0),
            name: t.name.clone(),
            kind: if t.kind == TemplateKind::Builtin { ReportKind::Builtin } else { ReportKind::User },
            template: None,
            script: Some(t.script.clone()),
            menu_path: t.menu_path.clone(),
            menu_name: t.menu_name.clone(),
            menu_tip: t.menu_tip.clone(),
            in_menu: t.in_menu,
            export_types: t.export_types.clone(),
        }
    }

    fn saved(s: &SavedReport, template: Option<&TemplateInfo>) -> Self {
        Self {
            guid: Id(s.guid),
            name: s.name.clone(),
            kind: ReportKind::Saved,
            template: Some(Id(s.template)),
            script: template.map(|t| t.script.clone()),
            menu_path: Vec::new(),
            menu_name: None,
            menu_tip: None,
            in_menu: false,
            export_types: template.map(|t| t.export_types.clone()).unwrap_or_default(),
        }
    }
}

/// What a report reference names.
#[derive(Debug, Clone, Copy)]
enum Found<'a> {
    Template(&'a TemplateInfo),
    Saved(&'a SavedReport),
}

/// The report state of one call: the Configuration as it stands, the registry over the built-in library and the
/// report directory, and what the Configuration holds for reports.
struct Reports {
    config: Config,
    registry: Arc<Registry>,
    /// Files the loader left out of the report directory.
    unread: Vec<ScriptProblem>,
    saved: Vec<SavedReport>,
    settings: ReportSettings,
    period: AccountingPeriod,
    fiscal_year_end: Option<FiscalYearEnd>,
    sheets: StyleSheets,
}

impl Reports {
    fn load(ctx: &Context) -> Result<Self, RpcError> {
        let config = ctx.settings.config.load()?;
        let settings = settings(&config)?;
        let (user, unread) = user_scripts(ctx, &config)?;
        let registry = registry(user, limits(&config)?)?;
        Ok(Self {
            saved: config.saved_reports()?,
            period: config.accounting_period()?,
            fiscal_year_end: config.fiscal_year_end()?,
            sheets: style_sheets(&config.style_sheets()?)?,
            config,
            registry,
            unread,
            settings,
        })
    }

    /// The Locale of a run: `chosen`, else the Configuration's `locale` setting (`C` unless set). A name no locale
    /// has is invalid params.
    fn locale(&self, chosen: Option<&str>) -> Result<Locale, RpcError> {
        match chosen {
            Some(name) => red_ale_service::locale::resolve(name).map_err(|e| RpcError::invalid_params(e.to_string())),
            None => Ok(self.config.locale()?),
        }
    }

    /// The template or saved configuration `reference` names: a GUID, else a name exactly one has
    /// (`design/reports.md` §11).
    fn find(&self, reference: &str) -> Result<Found<'_>, RpcError> {
        if let Ok(guid) = Guid::parse(reference.trim()) {
            if let Some(t) = self.registry.template(guid) {
                return Ok(Found::Template(t));
            }
            if let Some(s) = self.saved.iter().find(|s| s.guid == guid) {
                return Ok(Found::Saved(s));
            }
        }
        let templates = self.registry.named(reference).into_iter().map(Found::Template);
        let mut matches: Vec<Found<'_>> = templates.chain(self.saved.iter().filter(|s| s.name == reference).map(Found::Saved)).collect();
        match matches.len() {
            0 => Err(RpcError::not_found("report", reference)),
            1 => Ok(matches.remove(0)),
            n => {
                let guids: Vec<String> = matches.iter().map(|m| self.info(*m).guid.0.to_string()).collect();
                let message = format!("{n} reports are named {reference:?}; name one by its GUID");
                Err(RpcError::new(codes::CONFLICT, message).with_data(json!({ "matches": guids })))
            }
        }
    }

    /// The saved configuration `reference` names, by GUID or name.
    fn configuration(&self, reference: &str) -> Result<&SavedReport, RpcError> {
        let guid = Guid::parse(reference.trim()).ok();
        let by_guid = guid.and_then(|g| self.saved.iter().find(|s| s.guid == g));
        by_guid.or_else(|| self.saved.iter().find(|s| s.name == reference)).ok_or_else(|| RpcError::not_found("configuration", reference))
    }

    /// The template a saved configuration configures. One that is not loaded (its user script was removed or no
    /// longer loads) is `not-found`; `report.list` names the script under `problems`.
    fn template_of(&self, s: &SavedReport) -> Result<&TemplateInfo, RpcError> {
        self.registry.template(s.template).ok_or_else(|| {
            let message = format!("the template {} of saved configuration {:?} is not loaded", s.template, s.name);
            RpcError::new(codes::NOT_FOUND, message).with_data(json!({ "kind": "report", "id": s.template.to_string() }))
        })
    }

    /// The template to run and the saved configuration whose values apply first.
    fn resolve<'a>(&'a self, found: Found<'a>) -> Result<(&'a TemplateInfo, Option<&'a SavedReport>), RpcError> {
        match found {
            Found::Template(t) => Ok((t, None)),
            Found::Saved(s) => Ok((self.template_of(s)?, Some(s))),
        }
    }

    fn info(&self, found: Found<'_>) -> ReportInfo {
        match found {
            Found::Template(t) => ReportInfo::template(t),
            Found::Saved(s) => ReportInfo::saved(s, self.registry.template(s.template)),
        }
    }

    /// The choices of a report's General/"Stylesheet" option: every style sheet, and the one a saved configuration
    /// names if it no longer exists, so the configuration still applies and renders with the base look
    /// (`design/reports.md` §8).
    fn style_sheet_choices<'a>(&'a self, saved: Option<&'a SavedReport>) -> Vec<&'a str> {
        let mut choices = self.sheets.names();
        let named = saved.and_then(|s| s.options.get(GENERAL)).and_then(|g| g.get(STYLESHEET)).and_then(Value::as_str);
        if let Some(name) = named.filter(|n| !choices.contains(n)) {
            choices.push(name);
        }
        choices
    }

    /// A template's options with a saved configuration's values, then `changes`, applied. Defaults that depend on
    /// the Book come from `book`, or from an empty Book; without a Book, accounts can be named only by GUID.
    fn configured(
        &self,
        template: &TemplateInfo,
        saved: Option<&SavedReport>,
        changes: Option<&Value>,
        on: &On<'_>,
    ) -> Result<OptionDb, RpcError> {
        let mut db = self.defaults(template, &self.style_sheet_choices(saved), on)?;
        let values = ValueContext { book: on.book, zone: on.zone };
        let saved = saved.map(|s| Value::Object(s.options.clone()));
        saved.iter().chain(changes).try_for_each(|c| db.apply(c, &values)).map_err(ReportError::from)?;
        Ok(db)
    }

    /// A template's options at their defaults, its General/"Stylesheet" option offering `style_sheets`. Defaults that
    /// depend on the Book come from `on.book`, or from an empty Book.
    fn defaults(&self, template: &TemplateInfo, style_sheets: &[&str], on: &On<'_>) -> Result<OptionDb, RpcError> {
        let empty;
        let book = if let Some(book) = on.book {
            book
        } else {
            empty = Book::new_empty(&mut FixedEnv::new(Time64(0), 0));
            &empty
        };
        Ok(self.registry.options(template, &self.run_context(book, on.now, on.zone, style_sheets, self.locale(None)?))?)
    }

    /// What a run reads besides the options: the Book, now, the zone, the Configuration's settings and the Locale.
    fn run_context<'a>(
        &'a self,
        book: &'a Book,
        now: Time64,
        zone: &'a TimeZone,
        style_sheets: &'a [&'a str],
        locale: Locale,
    ) -> RunContext<'a> {
        RunContext {
            book,
            now,
            zone,
            settings: &self.settings,
            locale,
            accounting_period: &self.period,
            fiscal_year_end: self.fiscal_year_end,
            style_sheets,
            export: None,
        }
    }
}

/// What option values are read against.
struct On<'a> {
    /// The open Book, if any.
    book: Option<&'a Book>,
    /// The time to run for.
    now: Time64,
    /// The local time zone.
    zone: &'a TimeZone,
}

/// The report settings of the Configuration's `reports` key, with GnuCash's defaults for what is unset
/// (`design/reports.md` §4).
pub(crate) fn settings(config: &Config) -> Result<ReportSettings, RpcError> {
    let reports = Value::Object(config.reports()?);
    serde_json::from_value(reports).map_err(|e| RpcError::invalid(format!("the Configuration's report settings: {e}")))
}

/// The limits of `reports.limits` over the defaults; `seconds` is the run time limit.
fn limits(config: &Config) -> Result<Limits, RpcError> {
    config.report_limits()?.into_iter().try_fold(Limits::default(), |limits, (name, max)| {
        let limit = if name == "seconds" { Ok(Limit::Time) } else { name.parse::<Limit>() };
        limit.map(|l| limits.with(l, max)).map_err(|e| RpcError::invalid(format!("the Configuration's reports.limits: {e}")))
    })
}

/// The scripts of the report directory, and the files left out of it.
fn user_scripts(ctx: &Context, config: &Config) -> Result<(Vec<ScriptSource>, Vec<ScriptProblem>), RpcError> {
    let Some(dir) = config.report_directory()? else { return Ok((Vec::new(), Vec::new())) };
    let found = load_report_scripts(&ctx.settings.config.resolve(&dir));
    Ok((found.scripts.into_iter().map(|s| ScriptSource { path: s.path, text: s.text }).collect(), found.problems))
}

/// The style sheets reports can name: the predefined ones with the Configuration's entries over them.
fn style_sheets(records: &[StyleSheetRecord]) -> Result<StyleSheets, RpcError> {
    let entries: Vec<StyleSheetEntry> = records.iter().map(entry).collect();
    StyleSheets::from_configuration(&entries).map_err(|e| RpcError::invalid(format!("the Configuration's reports.styleSheets: {e}")))
}

fn entry(r: &StyleSheetRecord) -> StyleSheetEntry {
    StyleSheetEntry { name: r.name.clone(), template: r.template.clone(), options: r.options.clone() }
}

/// What the cached registry was built from.
type Built = (Vec<ScriptSource>, Limits, Arc<Registry>);

/// The last registry built. The built-in library is compiled in, so the user scripts' paths and texts and the limits
/// are all a registry depends on. Each call reads the report directory and the Configuration again; the registry is
/// reused when both give what it was built from, and rebuilt when a script was added, removed or edited, or the
/// limits changed. One entry is kept, for the whole process: every dispatcher compares the same contents.
static CACHE: Mutex<Option<Built>> = Mutex::new(None);

/// The registry of the built-in library and `user`, from the cache when it matches. A built-in script that does not
/// load is a bug: `internal-error`.
fn registry(user: Vec<ScriptSource>, limits: Limits) -> Result<Arc<Registry>, RpcError> {
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((scripts, built_limits, registry)) = cache.as_ref()
        && *scripts == user
        && *built_limits == limits
    {
        return Ok(Arc::clone(registry));
    }
    let registry = Arc::new(Registry::load(&Library::builtin(), &user, limits)?);
    *cache = Some((user, limits, Arc::clone(&registry)));
    Ok(registry)
}

/// Name a new saved configuration or style sheet.
fn name(name: &str, what: &str) -> Result<String, RpcError> {
    if name.trim().is_empty() { Err(RpcError::invalid_params(format!("a {what} needs a name"))) } else { Ok(name.to_owned()) }
}

/// A name already taken by a `what`, whose errors have `data.kind` `kind`.
fn taken(kind: &str, what: &str, name: &str) -> RpcError {
    RpcError::new(codes::CONFLICT, format!("a {what} is already called {name:?}")).with_data(json!({ "kind": kind, "name": name }))
}
