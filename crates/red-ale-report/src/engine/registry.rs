//! The template registry (`design/reports.md` §2, "The shape of a report script"; §11): every report template the
//! built-in library and the user's scripts declare, with the scripts' text to run them from.
//!
//! The registry is plain data (`Send + Sync`): it keeps each script's text and each template's record, never a Rhai
//! value, so the service may keep it between calls and share it between threads. Each run compiles what it needs into
//! an engine of its own.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use red_ale_core::{Book, FixedEnv, Guid, Time64};
use rhai::{AST, Array, CallFnOptions, Dynamic, Engine, Map, Scope};
use serde::Serialize;
use serde_json::Value;

use super::convert::to_json;
use super::errors::{Failure, from_eval, from_parse};
use super::log::Log;
use super::setup::{Setup, engine};
use crate::error::{LoadError, ReportError};
use crate::library::Library;
use crate::limits::{Limit, Limits};
use crate::query::Snapshot;
use crate::wire::Id;

/// The prefix of a user script's source name: `./mine/report.rhai` is `mine/report.rhai` in the report directory.
/// It keeps user scripts apart from built-in ones, whatever their paths.
pub(crate) const USER_PREFIX: &str = "./";

/// The directories of the built-in library, as GnuCash lays out its reports (`5.17:gnucash/report/reports/`).
pub(crate) const BUILTIN_DIRS: [&str; 3] = ["lib", "standard", "example"];

/// A script's text and origin.
#[derive(Debug, Clone)]
pub(crate) struct Script {
    pub text: Arc<str>,
    pub builtin: bool,
}

/// Every script by source name: `standard/balance-sheet.rhai` for a built-in one, `./mine/report.rhai` for a user's.
pub(crate) type Sources = BTreeMap<String, Script>;

/// A user script, as the service read it from the report directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptSource {
    /// The path relative to the report directory, with `/` between parts: `mine/report.rhai`.
    pub path: String,
    /// The script's text.
    pub text: String,
}

/// Where a template is defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TemplateKind {
    /// In a built-in script, compiled into red-ale.
    Builtin,
    /// In a user script from the report directory.
    User,
}

/// A report template, as its script's `templates()` declares it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    /// The template's GUID, GnuCash's for a built-in report.
    pub guid: Id,
    /// The template's name, GnuCash's English one for a built-in report.
    pub name: String,
    /// Built-in or user.
    pub kind: TemplateKind,
    /// The script's source name: `standard/balance-sheet.rhai`, or `./` and the path in the report directory.
    pub script: String,
    /// GnuCash's menu path (`"_Assets & Liabilities"`), for clients that build menus.
    pub menu_path: Vec<String>,
    /// The menu entry when it differs from the name.
    pub menu_name: Option<String>,
    /// The menu tip.
    pub menu_tip: Option<String>,
    /// Whether the template shows in menus.
    pub in_menu: bool,
    /// The values that tell one script's templates apart, passed to its `options` and `render` as `params`.
    #[serde(skip)]
    pub params: Value,
}

/// Why a report reference names no single template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// No template has that GUID or name.
    NotFound,
    /// Several templates have that name; their GUIDs, in registry order.
    Ambiguous(Vec<Guid>),
}

/// The templates of the built-in library and the user's scripts.
#[derive(Debug, Clone)]
pub struct Registry {
    sources: Arc<Sources>,
    templates: Vec<TemplateInfo>,
    problems: Vec<ReportError>,
    limits: Limits,
}

/// One script's templates, or why it has none.
type Declared = Result<Vec<TemplateInfo>, ReportError>;

impl Registry {
    /// Load the library's scripts and the user's. A built-in script that does not load is a bug, an
    /// `internal-error`; a user script that does not load is listed in [`Registry::problems`] (`report-load`, or
    /// `report-limit` for one past a compile-time limit) and the others still load. GUIDs are unique: the first
    /// definition wins, built-in scripts first, then user scripts by path (`design/reports.md` §11).
    pub fn load(library: &Library, user: &[ScriptSource], limits: Limits) -> Result<Self, ReportError> {
        let mut problems = Vec::new();
        let builtin = |(path, text): (&str, &str)| (path.to_owned(), Script { text: Arc::from(text), builtin: true });
        let mut sources: Sources = library.scripts().map(builtin).collect();
        let mut user: Vec<&ScriptSource> = user.iter().collect();
        user.sort_by(|a, b| a.path.cmp(&b.path));
        user.into_iter().for_each(|s| match user_name(&s.path) {
            Some(name) if !sources.contains_key(&name) => {
                sources.insert(name, Script { text: Arc::from(s.text.as_str()), builtin: false });
            }
            Some(name) => problems.push(load_error(&name, "a script with this path is already loaded")),
            None => problems.push(load_error(&s.path, "a report script's path is relative, ends in .rhai and has no . or .. parts")),
        });
        let sources = Arc::new(sources);
        let mut registry = Self { sources: Arc::clone(&sources), templates: Vec::new(), problems, limits };
        let names = sources.iter().filter(|(_, s)| s.builtin).chain(sources.iter().filter(|(_, s)| !s.builtin)).map(|(n, _)| n);
        names.for_each(|name| registry.declare(name));
        match registry.problems.iter().position(|p| matches!(p, ReportError::Internal(_))) {
            Some(i) => Err(registry.problems.swap_remove(i)),
            None => Ok(registry),
        }
    }

    /// Add the templates `name` declares, or the problem it has.
    fn declare(&mut self, name: &str) {
        let builtin = self.sources.get(name).is_some_and(|s| s.builtin);
        match self.templates_of(name) {
            Ok(found) => found.into_iter().for_each(|t| self.add(t, builtin)),
            Err(e) => self.problems.push(e),
        }
    }

    fn add(&mut self, t: TemplateInfo, builtin: bool) {
        match self.templates.iter().find(|o| o.guid == t.guid) {
            None => self.templates.push(t),
            Some(first) if builtin => {
                let message = format!("built-in template {} ({}) repeats the GUID of {} in {}", t.name, t.guid.0, first.name, first.script);
                self.problems.push(ReportError::Internal(message));
            }
            Some(first) => {
                let message = format!("template {:?} repeats the GUID {} of {:?} in {}", t.name, t.guid.0, first.name, first.script);
                self.problems.push(load_error(&t.script, &message));
            }
        }
    }

    /// Compile `name` and read its `templates()`. A script without one is a module other scripts import.
    fn templates_of(&self, name: &str) -> Declared {
        let snap = Snapshot::new(&Book::new_empty(&mut FixedEnv::new(Time64(0), 0)), Time64(0));
        let log = Log::new(self.limits.get_usize(Limit::StringSize));
        let conventions = red_ale_locale::Locale::c().conventions();
        let setup = Setup { snap: Rc::clone(&snap), sources: Arc::clone(&self.sources), limits: self.limits, log: log.clone(), conventions };
        let engine = engine(&setup);
        let failure = |log: &Log| Failure { report: "", source: name, limits: &self.limits, log: log.entries() };
        let ast = self.compile(&engine, name).map_err(|e| self.internal_if_builtin(name, from_parse(&e, name, &failure(&log))))?;
        if !has_fn(&ast, "templates", 0) {
            return Ok(Vec::new());
        }
        let malformed = |message: String| self.malformed(name, &message);
        ["options", "render"].iter().zip([3, 4]).try_for_each(|(f, n)| {
            let missing = || malformed(format!("a script with templates() needs fn {f}() with {n} parameters"));
            if has_fn(&ast, f, n) { Ok(()) } else { Err(missing()) }
        })?;
        let declared = call(&engine, &ast, "templates", ())
            .map_err(|e| self.internal_if_builtin(name, as_load(from_eval(&e, failure(&log)))))?;
        let records = declared.into_array().map_err(|t| malformed(format!("templates() returns an array of maps, not {t}")))?;
        let builtin = self.is_builtin(name);
        records.iter().enumerate().map(|(i, r)| record(r, name, builtin).map_err(|e| malformed(format!("templates()[{i}]: {e}")))).collect()
    }

    pub(crate) fn compile(&self, engine: &Engine, name: &str) -> Result<AST, rhai::ParseError> {
        let text = self.sources.get(name).map_or_else(|| Arc::from(""), |s| Arc::clone(&s.text));
        let mut ast = engine.compile(&*text)?;
        ast.set_source(name);
        Ok(ast)
    }

    fn is_builtin(&self, name: &str) -> bool {
        self.sources.get(name).is_some_and(|s| s.builtin)
    }

    fn malformed(&self, name: &str, message: &str) -> ReportError {
        self.internal_if_builtin(name, load_error(name, message))
    }

    /// A built-in script's failure is red-ale's bug.
    fn internal_if_builtin(&self, name: &str, e: ReportError) -> ReportError {
        if self.is_builtin(name) && !matches!(e, ReportError::Internal(_)) {
            ReportError::Internal(format!("built-in script {name}: {e}"))
        } else {
            e
        }
    }

    /// Every template, sorted by name (bytewise), then GUID, as `report.list` lists them.
    pub fn templates(&self) -> Vec<&TemplateInfo> {
        let mut all: Vec<&TemplateInfo> = self.templates.iter().collect();
        all.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.guid.cmp(&b.guid)));
        all
    }

    /// The user scripts that did not load, and the templates refused for repeating a GUID, in load order.
    pub fn problems(&self) -> &[ReportError] {
        &self.problems
    }

    /// The limits runs from this registry stay within.
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// The template with `guid`.
    pub fn template(&self, guid: Guid) -> Option<&TemplateInfo> {
        self.templates.iter().find(|t| t.guid.0 == guid)
    }

    /// The templates named `name`, in load order.
    pub fn named(&self, name: &str) -> Vec<&TemplateInfo> {
        self.templates.iter().filter(|t| t.name == name).collect()
    }

    /// The template a reference names: a GUID, else a name that exactly one template has. Saved report
    /// configurations are the service's; it looks among them too and reports a clash across both.
    pub fn find(&self, reference: &str) -> Result<&TemplateInfo, Lookup> {
        if let Some(t) = Guid::parse(reference.trim()).ok().and_then(|g| self.template(g)) {
            return Ok(t);
        }
        match self.named(reference).as_slice() {
            [] => Err(Lookup::NotFound),
            [one] => Ok(one),
            many => Err(Lookup::Ambiguous(many.iter().map(|t| t.guid.0).collect())),
        }
    }

    pub(crate) fn sources(&self) -> &Arc<Sources> {
        &self.sources
    }
}

/// A script that fails while declaring its templates does not load: `report-load`, located where it failed.
fn as_load(e: ReportError) -> ReportError {
    match e {
        ReportError::Script(s) => ReportError::Load(LoadError { source: s.source, line: s.line, column: s.column, message: s.message }),
        other => other,
    }
}

fn load_error(source: &str, message: &str) -> ReportError {
    ReportError::Load(LoadError { source: source.to_owned(), line: None, column: None, message: message.to_owned() })
}

/// The source name of a user script at `path`, if the path is a plain relative `.rhai` path.
fn user_name(path: &str) -> Option<String> {
    let plain = path.split('/').all(|c| !c.is_empty() && c != "." && c != ".." && !c.contains('\\'));
    let rhai = std::path::Path::new(path).extension().is_some_and(|e| e == "rhai") && path.len() > ".rhai".len();
    (plain && rhai).then(|| format!("{USER_PREFIX}{path}"))
}

fn has_fn(ast: &AST, name: &str, params: usize) -> bool {
    ast.iter_functions().any(|f| f.name == name && f.params.len() == params)
}

/// Call a script function, running the script's top level (its imports) first.
pub(crate) fn call(engine: &Engine, ast: &AST, name: &str, args: impl rhai::FuncArgs) -> Result<Dynamic, Box<rhai::EvalAltResult>> {
    engine.call_fn_with_options(CallFnOptions::new(), &mut Scope::new(), ast, name, args)
}

/// One record of `templates()`: `guid`, `name`, and optionally `menuPath`, `menuName`, `menuTip`, `inMenu` and
/// `params`.
fn record(r: &Dynamic, script: &str, builtin: bool) -> Result<TemplateInfo, String> {
    let map = r.read_lock::<Map>().ok_or_else(|| format!("a template is a map, not {}", r.type_name()))?;
    let known = ["guid", "name", "menuPath", "menuName", "menuTip", "inMenu", "params"];
    if let Some(key) = map.keys().find(|k| !known.contains(&k.as_str())) {
        return Err(format!("unknown field {key:?}"));
    }
    let text = |key: &str| -> Result<Option<String>, String> {
        let string = |v: &Dynamic| v.clone().into_string().map_err(|t| format!("{key} must be a string, not {t}"));
        map.get(key).filter(|v| !v.is_unit()).map(string).transpose()
    };
    let guid_text = text("guid")?.ok_or("guid is required")?;
    let lowercase_hex = guid_text.len() == 32 && guid_text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    let guid = Guid::parse(&guid_text)
        .ok()
        .filter(|_| lowercase_hex)
        .ok_or_else(|| format!("guid must be 32 lowercase hex digits, not {guid_text:?}"))?;
    let name = text("name")?.filter(|n| !n.is_empty()).ok_or("name is required")?;
    let menu_path = match map.get("menuPath") {
        None => Vec::new(),
        Some(v) => {
            let items = v.read_lock::<Array>().ok_or("menuPath must be an array of strings")?;
            items.iter().map(|p| p.clone().into_string().map_err(|t| format!("menuPath holds strings, not {t}"))).collect::<Result<_, _>>()?
        }
    };
    let in_menu = map.get("inMenu").map_or(Ok(true), |v| v.as_bool().map_err(|t| format!("inMenu must be true or false, not {t}")))?;
    let json = |p: &Dynamic| to_json(p).map_err(|e| format!("params: {}", e.reason));
    let params = map.get("params").map_or(Ok(Value::Object(serde_json::Map::new())), json)?;
    let kind = if builtin { TemplateKind::Builtin } else { TemplateKind::User };
    let (menu_name, menu_tip) = (text("menuName")?, text("menuTip")?);
    Ok(TemplateInfo { guid: Id(guid), name, kind, script: script.to_owned(), menu_path, menu_name, menu_tip, in_menu, params })
}
