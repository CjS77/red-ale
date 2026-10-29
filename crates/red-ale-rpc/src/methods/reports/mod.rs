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
use red_ale_docs::{ReportOption, ReportTemplate};
use red_ale_locale::Locale;
use red_ale_report::ReportError;
use red_ale_report::engine::{Configurations, Registry, RunContext, ScriptSource, TemplateInfo, TemplateKind};
use red_ale_report::library::{BuiltinConfiguration, Library, builtin_configurations};
use red_ale_report::limits::{Limit, Limits};
use red_ale_report::options::{GENERAL, OptionDb, OptionInfo, STYLESHEET, ValueContext};
use red_ale_report::render::{StyleSheetEntry, StyleSheets};
use red_ale_report::settings::ReportSettings;
use red_ale_service::Settings;
use red_ale_service::config::Config;
use red_ale_service::reports::{SavedReport, ScriptProblem, StyleSheetRecord, load_report_scripts};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

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
        method(
            "report.run",
            "Run a report on the open book: the report result, with exact figures.",
            Effect::Read,
            run::run,
        ),
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
        method(
            "report.styleSheets",
            "Style sheet templates and style sheets, with their options.",
            Effect::Read,
            style_sheets::list,
        ),
    ];
    let configuration = [
        method(
            "report.saveConfiguration",
            "Save a report's options under a new name in red-ale's configuration.",
            Effect::Write,
            configurations::save,
        ),
        method(
            "report.updateConfiguration",
            "Change options of a saved report configuration.",
            Effect::Write,
            configurations::update,
        ),
        method(
            "report.renameConfiguration",
            "Rename a saved report configuration.",
            Effect::Write,
            configurations::rename,
        ),
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
        method(
            "report.deleteConfiguration",
            "Delete a saved report configuration.",
            Effect::Destructive,
            configurations::delete,
        ),
        method(
            "report.createStyleSheet",
            "Create a style sheet from a style sheet template in red-ale's configuration.",
            Effect::Write,
            style_sheets::create,
        ),
        method(
            "report.updateStyleSheet",
            "Change a style sheet's options.",
            Effect::Write,
            style_sheets::update,
        ),
        method(
            "report.deleteStyleSheet",
            "Delete a user style sheet, or reset a predefined one to its template's defaults.",
            Effect::Destructive,
            style_sheets::delete,
        ),
    ];
    reads
        .into_iter()
        .chain(configuration.into_iter().map(Method::on_configuration))
        .collect()
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
    /// `true` for a built-in saved configuration (v0.6), which cannot be changed, renamed or deleted, only copied
    /// with `report.saveConfiguration`; absent otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
}

impl ReportInfo {
    fn template(t: &TemplateInfo) -> Self {
        Self {
            guid: Id(t.guid.0),
            name: t.name.clone(),
            kind: if t.kind == TemplateKind::Builtin {
                ReportKind::Builtin
            } else {
                ReportKind::User
            },
            template: None,
            script: Some(t.script.clone()),
            menu_path: t.menu_path.clone(),
            menu_name: t.menu_name.clone(),
            menu_tip: t.menu_tip.clone(),
            in_menu: t.in_menu,
            export_types: t.export_types.clone(),
            read_only: false,
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
            read_only: false,
        }
    }

    /// A built-in saved configuration: a saved configuration with the menu fields of the GnuCash menu item it stands
    /// for, and read-only.
    fn builtin_saved(b: &BuiltinSaved, template: Option<&TemplateInfo>) -> Self {
        Self {
            menu_path: b.config.menu_path.clone(),
            menu_name: b.config.menu_name.clone(),
            menu_tip: b.config.menu_tip.clone(),
            in_menu: b.config.in_menu,
            read_only: true,
            ..Self::saved(&b.saved, template)
        }
    }
}

/// A built-in saved configuration: its record, as a Configuration entry reads, and its menu fields.
#[derive(Debug, Clone)]
struct BuiltinSaved {
    saved: SavedReport,
    config: BuiltinConfiguration,
}

impl From<BuiltinConfiguration> for BuiltinSaved {
    fn from(config: BuiltinConfiguration) -> Self {
        let saved = SavedReport {
            guid: config.guid.0,
            name: config.name.clone(),
            template: config.template.0,
            options: config.options.clone(),
            other: Map::new(),
        };
        Self { saved, config }
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
    /// The Configuration's saved configurations, all of them, as they are written back.
    saved: Vec<SavedReport>,
    /// The built-in saved configurations, which the Configuration never stores.
    builtin: Vec<BuiltinSaved>,
    settings: ReportSettings,
    period: AccountingPeriod,
    fiscal_year_end: Option<FiscalYearEnd>,
    sheets: StyleSheets,
}

impl Reports {
    fn load(ctx: &Context) -> Result<Self, RpcError> {
        let config = ctx.settings.config.load()?;
        let (user, unread) = user_scripts(ctx, &config)?;
        Self::from_config(config, user, unread)
    }

    /// The built-in library alone, under the default Configuration.
    fn builtin() -> Result<Self, RpcError> {
        Self::from_config(Settings::default().config.load()?, Vec::new(), Vec::new())
    }

    fn from_config(config: Config, user: Vec<ScriptSource>, unread: Vec<ScriptProblem>) -> Result<Self, RpcError> {
        let settings = settings(&config)?;
        let registry = registry(user, limits(&config)?)?;
        Ok(Self {
            saved: config.saved_reports()?,
            builtin: builtin_configurations()?.into_iter().map(BuiltinSaved::from).collect(),
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
    /// (`design/reports.md` §11). Built-in saved configurations are named as the Configuration's are: their names are
    /// not reserved, so a name a built-in one shares with another report is ambiguous, and the GUID always works.
    fn find(&self, reference: &str) -> Result<Found<'_>, RpcError> {
        if let Ok(guid) = Guid::parse(reference.trim()) {
            if let Some(t) = self.registry.template(guid) {
                return Ok(Found::Template(t));
            }
            if let Some(s) = self.all_saved().find(|s| s.guid == guid) {
                return Ok(Found::Saved(s));
            }
        }
        let templates = self.registry.named(reference).into_iter().map(Found::Template);
        let mut matches: Vec<Found<'_>> = templates
            .chain(self.all_saved().filter(|s| s.name == reference).map(Found::Saved))
            .collect();
        match matches.len() {
            0 => Err(RpcError::not_found("report", reference)),
            1 => Ok(matches.remove(0)),
            _ => Err(ambiguous(reference, matches.iter().map(|m| self.info(*m).guid))),
        }
    }

    /// The saved configuration `reference` names, by GUID or name, for a method that changes it. A built-in one is
    /// refused (`conflict`): it can only be copied.
    fn configuration(&self, reference: &str) -> Result<&SavedReport, RpcError> {
        let guid = Guid::parse(reference.trim()).ok();
        let found = match guid.and_then(|g| self.all_saved().find(|s| s.guid == g)) {
            Some(s) => s,
            None => self.saved_named(reference)?,
        };
        match self.builtin_saved(found.guid) {
            Some(b) => Err(read_only(&b.saved)),
            None => Ok(found),
        }
    }

    /// The one saved configuration named `name`.
    fn saved_named(&self, name: &str) -> Result<&SavedReport, RpcError> {
        let matches: Vec<&SavedReport> = self.all_saved().filter(|s| s.name == name).collect();
        match matches.as_slice() {
            [] => Err(RpcError::not_found("configuration", name)),
            [one] => Ok(one),
            _ => Err(ambiguous(name, matches.iter().map(|s| Id(s.guid)))),
        }
    }

    /// The built-in saved configuration with GUID `guid`.
    fn builtin_saved(&self, guid: Guid) -> Option<&BuiltinSaved> {
        self.builtin.iter().find(|b| b.saved.guid == guid)
    }

    /// Every saved configuration a report reference can name: the built-in ones, then the Configuration's, less any
    /// that reuses a built-in one's GUID ([`Reports::shadowed`]).
    fn all_saved(&self) -> impl Iterator<Item = &SavedReport> {
        let builtin = self.builtin.iter().map(|b| &b.saved);
        builtin.chain(self.saved.iter().filter(|s| self.builtin_saved(s.guid).is_none()))
    }

    /// The Configuration's saved configurations that reuse a built-in one's GUID. The built-in one wins, as a
    /// built-in template wins over a user script reusing its GUID (`design/reports.md` §11); they stay in the
    /// Configuration untouched, and `report.list` names them under `problems`.
    fn shadowed(&self) -> impl Iterator<Item = &SavedReport> {
        self.saved.iter().filter(|s| self.builtin_saved(s.guid).is_some())
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
            Found::Saved(s) => {
                let template = self.registry.template(s.template);
                match self.builtin_saved(s.guid) {
                    Some(b) => ReportInfo::builtin_saved(b, template),
                    None => ReportInfo::saved(s, template),
                }
            }
        }
    }

    /// The choices of a report's General/"Stylesheet" option: every style sheet, and the one a saved configuration
    /// names if it no longer exists, so the configuration still applies and renders with the base look
    /// (`design/reports.md` §8).
    fn style_sheet_choices<'a>(&'a self, saved: Option<&'a SavedReport>) -> Vec<&'a str> {
        let mut choices = self.sheets.names();
        let named = saved
            .and_then(|s| s.options.get(GENERAL))
            .and_then(|g| g.get(STYLESHEET))
            .and_then(Value::as_str);
        if let Some(name) = named.filter(|n| !choices.contains(n)) {
            choices.push(name);
        }
        choices
    }

    /// A template's options with a saved configuration's values, then `changes`, applied, and the children `changes`
    /// name checked. Defaults that depend on the Book come from `book`, or from an empty Book; without a Book,
    /// accounts can be named only by GUID.
    fn configured(
        &self,
        template: &TemplateInfo,
        saved: Option<&SavedReport>,
        changes: Option<&Value>,
        on: &On<'_>,
    ) -> Result<OptionDb, RpcError> {
        let style_sheets = self.style_sheet_choices(saved);
        let empty;
        let book = if let Some(book) = on.book {
            book
        } else {
            empty = empty_book();
            &empty
        };
        let run = self.run_context(book, on.now, on.zone, &style_sheets, self.locale(None)?);
        let mut db = self.registry.options(template, &run)?;
        let values = ValueContext {
            book: on.book,
            zone: on.zone,
        };
        let saved = saved.map(|s| Value::Object(s.options.clone()));
        saved
            .iter()
            .chain(changes)
            .try_for_each(|c| db.apply(c, &values))
            .map_err(ReportError::from)?;
        if let Some(changes) = changes {
            self.registry
                .check_children(&mut db, changes, &run, &values)
                .map_err(ReportError::from)?;
        }
        Ok(db)
    }

    /// A template's options at their defaults, its General/"Stylesheet" option offering `style_sheets`. Defaults that
    /// depend on the Book come from `on.book`, or from an empty Book.
    fn defaults(&self, template: &TemplateInfo, style_sheets: &[&str], on: &On<'_>) -> Result<OptionDb, RpcError> {
        let empty;
        let book = if let Some(book) = on.book {
            book
        } else {
            empty = empty_book();
            &empty
        };
        Ok(self
            .registry
            .options(template, &self.run_context(book, on.now, on.zone, style_sheets, self.locale(None)?))?)
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
            configurations: self,
        }
    }
}

impl Configurations for Reports {
    fn configuration(&self, guid: Guid) -> Option<(Guid, &Map<String, Value>)> {
        self.all_saved().find(|s| s.guid == guid).map(|s| (s.template, &s.options))
    }
}

/// Every built-in report template as the Manual's report Pages show it: its options at their defaults, described as
/// `report.options` describes them with no book open, under the default Configuration, in UTC at the epoch; then
/// every built-in saved configuration, its options with its values. None when the library does not load; a report
/// whose options can't be described has none listed. Either is a bug in a built-in script, which the report tests
/// catch.
pub(crate) fn builtin_templates() -> Vec<ReportTemplate> {
    let Ok(reports) = Reports::builtin() else {
        return Vec::new();
    };
    let zone = TimeZone::utc();
    let on = On {
        book: None,
        now: Time64(0),
        zone: &zone,
    };
    let sheets = reports.style_sheet_choices(None);
    let templates = reports.registry.templates().into_iter().filter(|t| t.kind == TemplateKind::Builtin);
    let templates = templates.map(|t| ReportTemplate {
        guid: t.guid.0.to_string(),
        name: t.name.clone(),
        script: t.script.clone(),
        menu_tip: t.menu_tip.clone(),
        configures: None,
        options: reports
            .defaults(t, &sheets, &on)
            .map(|db| db.describe().into_iter().map(|o| manual_option(o, false)).collect())
            .unwrap_or_default(),
    });
    let configurations = reports.builtin.iter().filter_map(|b| reports.builtin_page(b, &on));
    templates.chain(configurations).collect()
}

impl Reports {
    /// A built-in saved configuration as the Manual's report Pages show it, its values as the defaults; `None` when
    /// its template is not loaded.
    fn builtin_page(&self, b: &BuiltinSaved, on: &On<'_>) -> Option<ReportTemplate> {
        let template = self.registry.template(b.saved.template)?;
        Some(ReportTemplate {
            guid: b.saved.guid.to_string(),
            name: b.saved.name.clone(),
            script: template.script.clone(),
            menu_tip: b.config.menu_tip.clone(),
            configures: Some(template.name.clone()),
            options: self
                .configured(template, Some(&b.saved), None, on)
                .map(|db| db.describe().into_iter().map(|o| manual_option(o, true)).collect())
                .unwrap_or_default(),
        })
    }
}

/// An option as the Manual lists it: its default, or for a saved configuration its configured value.
fn manual_option(o: OptionInfo, configured: bool) -> ReportOption {
    ReportOption {
        section: o.section,
        name: o.name,
        option_type: o.option_type,
        help: o.help,
        default: if configured { o.value } else { o.default },
    }
}

/// The Book options are described against when none is open.
fn empty_book() -> Book {
    Book::new_empty(&mut FixedEnv::new(Time64(0), 0))
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
    config
        .report_limits()?
        .into_iter()
        .try_fold(Limits::default(), |limits, (name, max)| {
            let limit = if name == "seconds" {
                Ok(Limit::Time)
            } else {
                name.parse::<Limit>()
            };
            limit
                .map(|l| limits.with(l, max))
                .map_err(|e| RpcError::invalid(format!("the Configuration's reports.limits: {e}")))
        })
}

/// The scripts of the report directory, and the files left out of it.
fn user_scripts(ctx: &Context, config: &Config) -> Result<(Vec<ScriptSource>, Vec<ScriptProblem>), RpcError> {
    let Some(dir) = config.report_directory()? else {
        return Ok((Vec::new(), Vec::new()));
    };
    let found = load_report_scripts(&ctx.settings.config.resolve(&dir));
    Ok((
        found
            .scripts
            .into_iter()
            .map(|s| ScriptSource {
                path: s.path,
                text: s.text,
            })
            .collect(),
        found.problems,
    ))
}

/// The style sheets reports can name: the predefined ones with the Configuration's entries over them.
fn style_sheets(records: &[StyleSheetRecord]) -> Result<StyleSheets, RpcError> {
    let entries: Vec<StyleSheetEntry> = records.iter().map(entry).collect();
    StyleSheets::from_configuration(&entries).map_err(|e| RpcError::invalid(format!("the Configuration's reports.styleSheets: {e}")))
}

fn entry(r: &StyleSheetRecord) -> StyleSheetEntry {
    StyleSheetEntry {
        name: r.name.clone(),
        template: r.template.clone(),
        options: r.options.clone(),
    }
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
    if name.trim().is_empty() {
        Err(RpcError::invalid_params(format!("a {what} needs a name")))
    } else {
        Ok(name.to_owned())
    }
}

/// A name several reports have: `conflict`, with their GUIDs in `data.matches`, as `gnucash-cli` refuses a name that
/// matches several reports (`5.17:gnucash/report/report-core.scm:914-929`).
fn ambiguous(name: &str, guids: impl Iterator<Item = Id>) -> RpcError {
    let guids: Vec<String> = guids.map(|g| g.0.to_string()).collect();
    let message = format!("{} reports are named {name:?}; name one by its GUID", guids.len());
    RpcError::new(codes::CONFLICT, message).with_data(json!({ "matches": guids }))
}

/// A change to a built-in saved configuration: `conflict`, since it exists but may only be copied.
fn read_only(s: &SavedReport) -> RpcError {
    let message = format!(
        "{:?} is a built-in saved report configuration and cannot be changed; save a copy with report.saveConfiguration",
        s.name
    );
    let data = json!({ "kind": "configuration", "reason": "built-in", "guid": s.guid.to_string(), "name": s.name });
    RpcError::new(codes::CONFLICT, message).with_data(data)
}

/// A name already taken by a `what`, whose errors have `data.kind` `kind`.
fn taken(kind: &str, what: &str, name: &str) -> RpcError {
    RpcError::new(codes::CONFLICT, format!("a {what} is already called {name:?}")).with_data(json!({ "kind": kind, "name": name }))
}
