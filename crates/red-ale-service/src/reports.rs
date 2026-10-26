//! What the Configuration keeps for reports (`design/reports.md` §4, §9), and the loader for the report directory
//! (§2, "The module resolver").
//!
//! GnuCash keeps saved report configurations and style sheets in files outside the book; red-ale keeps them under the
//! Configuration's `reports` key, with the report settings, the report directory and the limits:
//!
//! ```json
//! { "reports": { "directory": "reports", "currency": "CURRENCY::USD", "dateFormat": "locale",
//!                "limits": { "operations": 1000000000, "seconds": 120 },
//!                "saved": [{ "guid": "…", "name": "…", "template": "…", "options": {} }],
//!                "styleSheets": [{ "name": "Footer", "template": "Footer", "options": {} }] } }
//! ```
//!
//! This module reads and writes those keys as plain values; what an option or a setting means is the report
//! context's business (`red-ale-report`), which this crate does not depend on. Keys red-ale does not know, at any
//! level, are kept when an entry is rewritten.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use red_ale_core::Guid;
use serde_json::{Map, Value};

use crate::ServiceError;
use crate::config::{Config, ConfigStore, invalid};

const REPORTS: &str = "reports";
const DIRECTORY: &str = "directory";
const LIMITS: &str = "limits";
const SAVED: &str = "saved";
const STYLE_SHEETS: &str = "styleSheets";

/// The most script files the loader reads from the report directory (`design/reports.md` §2).
pub const MAX_SCRIPTS: usize = 1_000;

/// The largest script file the loader reads, in bytes.
pub const MAX_SCRIPT_BYTES: u64 = 1 << 20;

/// A saved report configuration: a template's options saved under a new name and GUID, GnuCash's custom report
/// (`5.17:gnucash/report/report-core.scm:126-145`).
#[derive(Debug, Clone, PartialEq)]
pub struct SavedReport {
    /// Its own GUID.
    pub guid: Guid,
    /// Its name, unique among saved configurations.
    pub name: String,
    /// The GUID of the report template it configures.
    pub template: Guid,
    /// The options that differ from the template's defaults, by section and name, in wire form.
    pub options: Map<String, Value>,
    /// Fields red-ale does not know, kept as read.
    pub other: Map<String, Value>,
}

/// A style sheet entry: changes to a predefined style sheet, or a user style sheet of a template.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleSheetRecord {
    /// The style sheet's name.
    pub name: String,
    /// The style sheet template's name.
    pub template: String,
    /// Option values by section and name, in wire form, including any red-ale ignores (images).
    pub options: Map<String, Value>,
    /// Fields red-ale does not know, kept as read.
    pub other: Map<String, Value>,
}

impl Config {
    /// The `reports` object: report settings, directory, limits, saved configurations and style sheets. Empty when
    /// unset.
    pub fn reports(&self) -> Result<Map<String, Value>, ServiceError> {
        match self.get(REPORTS) {
            None | Some(Value::Null) => Ok(Map::new()),
            Some(Value::Object(map)) => Ok(map.clone()),
            Some(other) => Err(invalid(format!("{REPORTS} must be an object, not {other}"))),
        }
    }

    fn reports_field(&self, key: &str) -> Result<Option<Value>, ServiceError> {
        Ok(self.reports()?.remove(key).filter(|v| !v.is_null()))
    }

    /// The report directory user report scripts load from, as written; none when unset.
    pub fn report_directory(&self) -> Result<Option<PathBuf>, ServiceError> {
        match self.reports_field(DIRECTORY)? {
            None => Ok(None),
            Some(Value::String(s)) if !s.is_empty() => Ok(Some(PathBuf::from(s))),
            Some(other) => Err(invalid(format!("{REPORTS}.{DIRECTORY} must be a path, not {other}"))),
        }
    }

    /// The report limits set, by their names under `reports.limits` (`operations`, `seconds`, …), in file order.
    pub fn report_limits(&self) -> Result<Vec<(String, u64)>, ServiceError> {
        let Some(limits) = self.reports_field(LIMITS)? else { return Ok(Vec::new()) };
        let limits = limits.as_object().ok_or_else(|| invalid(format!("{REPORTS}.{LIMITS} must be an object, not {limits}")))?;
        limits
            .iter()
            .map(|(name, max)| {
                let bad = || invalid(format!("{REPORTS}.{LIMITS}.{name} must be a whole number, not {max}"));
                max.as_u64().map(|m| (name.clone(), m)).ok_or_else(bad)
            })
            .collect()
    }

    /// The saved report configurations, in the order they were added.
    pub fn saved_reports(&self) -> Result<Vec<SavedReport>, ServiceError> {
        entries(self.reports_field(SAVED)?, SAVED)?.into_iter().enumerate().map(|(i, e)| saved(e, i)).collect()
    }

    /// Replace the saved report configurations.
    pub fn set_saved_reports(&mut self, saved: &[SavedReport]) {
        let list = saved.iter().map(|s| {
            let mut entry = Map::new();
            entry.insert("guid".into(), Value::String(s.guid.to_string()));
            entry.insert("name".into(), Value::String(s.name.clone()));
            entry.insert("template".into(), Value::String(s.template.to_string()));
            entry.insert("options".into(), Value::Object(s.options.clone()));
            entry.extend(s.other.clone());
            Value::Object(entry)
        });
        let list = Value::Array(list.collect());
        self.update_object(REPORTS, |r| {
            r.insert(SAVED.into(), list);
        });
    }

    /// The style sheet entries, in the order they were added.
    pub fn style_sheets(&self) -> Result<Vec<StyleSheetRecord>, ServiceError> {
        entries(self.reports_field(STYLE_SHEETS)?, STYLE_SHEETS)?.into_iter().enumerate().map(|(i, e)| style_sheet(e, i)).collect()
    }

    /// Replace the style sheet entries.
    pub fn set_style_sheets(&mut self, sheets: &[StyleSheetRecord]) {
        let list = sheets.iter().map(|s| {
            let mut entry = Map::new();
            entry.insert("name".into(), Value::String(s.name.clone()));
            entry.insert("template".into(), Value::String(s.template.clone()));
            entry.insert("options".into(), Value::Object(s.options.clone()));
            entry.extend(s.other.clone());
            Value::Object(entry)
        });
        let list = Value::Array(list.collect());
        self.update_object(REPORTS, |r| {
            r.insert(STYLE_SHEETS.into(), list);
        });
    }
}

/// The objects of the list under `reports.<key>`.
fn entries(list: Option<Value>, key: &str) -> Result<Vec<Map<String, Value>>, ServiceError> {
    let Some(list) = list else { return Ok(Vec::new()) };
    let Value::Array(items) = list else { return Err(invalid(format!("{REPORTS}.{key} must be a list, not {list}"))) };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| match item {
            Value::Object(map) => Ok(map),
            other => Err(invalid(format!("{REPORTS}.{key}[{i}] must be an object, not {other}"))),
        })
        .collect()
}

/// Take the string field `field` out of entry `i` of `reports.<key>`.
fn text(entry: &mut Map<String, Value>, field: &str, key: &str, i: usize) -> Result<String, ServiceError> {
    match entry.remove(field) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s),
        other => Err(invalid(format!("{REPORTS}.{key}[{i}].{field} must be a non-empty string, not {}", other.unwrap_or_default()))),
    }
}

fn guid(entry: &mut Map<String, Value>, field: &str, i: usize) -> Result<Guid, ServiceError> {
    let s = text(entry, field, SAVED, i)?;
    Guid::parse(&s).map_err(|e| invalid(format!("{REPORTS}.{SAVED}[{i}].{field}: {e}")))
}

fn options(entry: &mut Map<String, Value>, key: &str, i: usize) -> Result<Map<String, Value>, ServiceError> {
    match entry.remove("options") {
        None | Some(Value::Null) => Ok(Map::new()),
        Some(Value::Object(map)) => Ok(map),
        Some(other) => Err(invalid(format!("{REPORTS}.{key}[{i}].options must be an object, not {other}"))),
    }
}

fn saved(mut entry: Map<String, Value>, i: usize) -> Result<SavedReport, ServiceError> {
    let (guid, template) = (guid(&mut entry, "guid", i)?, guid(&mut entry, "template", i)?);
    let name = text(&mut entry, "name", SAVED, i)?;
    let options = options(&mut entry, SAVED, i)?;
    Ok(SavedReport { guid, name, template, options, other: entry })
}

fn style_sheet(mut entry: Map<String, Value>, i: usize) -> Result<StyleSheetRecord, ServiceError> {
    let name = text(&mut entry, "name", STYLE_SHEETS, i)?;
    let template = text(&mut entry, "template", STYLE_SHEETS, i)?;
    let options = options(&mut entry, STYLE_SHEETS, i)?;
    Ok(StyleSheetRecord { name, template, options, other: entry })
}

impl ConfigStore {
    /// A path from the Configuration: a relative one is taken from the Configuration file's directory, or from the
    /// working directory when the Configuration is held in memory.
    pub fn resolve(&self, path: &Path) -> PathBuf {
        match self.path().and_then(Path::parent) {
            Some(base) if path.is_relative() => base.join(path),
            _ => path.to_owned(),
        }
    }
}

/// A user report script read from the report directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptFile {
    /// The path below the report directory, with `/` between parts: `mine/report.rhai`.
    pub path: String,
    /// The script's text.
    pub text: String,
}

/// A file below the report directory that was left out, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptProblem {
    /// The path below the report directory, `.` for the directory itself.
    pub path: String,
    /// Why it was left out.
    pub message: String,
}

/// What the loader found in the report directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportScripts {
    /// Every `*.rhai` file below the directory, sorted by path.
    pub scripts: Vec<ScriptFile>,
    /// Files and directories left out.
    pub problems: Vec<ScriptProblem>,
}

/// Read every `*.rhai` file below `dir`, at most [`MAX_SCRIPTS`] files of at most [`MAX_SCRIPT_BYTES`] each
/// (`design/reports.md` §2). A symbolic link is followed only when it resolves inside the directory, so no script can
/// come from elsewhere; one that leads outside, a file too large or not UTF-8, and anything unreadable are left out
/// and listed as problems, so one bad file does not hide the others. Nothing here fails.
pub fn load_report_scripts(dir: &Path) -> ReportScripts {
    let message = match dir.canonicalize() {
        Ok(root) if root.is_dir() => {
            let visited = BTreeSet::from([root.clone()]);
            let mut walk = Walk { root: root.clone(), visited, found: ReportScripts::default(), full: false };
            walk.directory(&root, "");
            return walk.found;
        }
        Ok(_) => format!("the report directory {} is not a directory", dir.display()),
        Err(e) => format!("the report directory {} cannot be read: {e}", dir.display()),
    };
    ReportScripts { scripts: Vec::new(), problems: vec![ScriptProblem { path: ".".to_owned(), message }] }
}

struct Walk {
    /// The report directory, every link resolved.
    root: PathBuf,
    /// Directories already read, so a link back up does not loop.
    visited: BTreeSet<PathBuf>,
    found: ReportScripts,
    /// [`MAX_SCRIPTS`] were read; the rest are left out.
    full: bool,
}

impl Walk {
    fn problem(&mut self, path: &str, message: String) {
        self.found.problems.push(ScriptProblem { path: path.to_owned(), message });
    }

    /// Read the directory `resolved`, which is at `below` in the report directory.
    fn directory(&mut self, resolved: &Path, below: &str) {
        let mut entries: Vec<(PathBuf, std::ffi::OsString)> = match std::fs::read_dir(resolved) {
            Ok(entries) => entries.flatten().map(|e| (e.path(), e.file_name())).collect(),
            Err(e) => return self.problem(if below.is_empty() { "." } else { below }, format!("cannot be read: {e}")),
        };
        entries.sort_by(|a, b| a.1.cmp(&b.1));
        entries.into_iter().for_each(|(path, name)| self.entry(&path, &name.to_string_lossy(), below));
    }

    fn entry(&mut self, path: &Path, name: &str, parent: &str) {
        let below = if parent.is_empty() { name.to_owned() } else { format!("{parent}/{name}") };
        let resolved = match path.canonicalize() {
            Ok(resolved) => resolved,
            Err(e) => return self.problem(&below, format!("cannot be read: {e}")),
        };
        if !resolved.starts_with(&self.root) {
            return self.problem(&below, "a link that leads outside the report directory is not followed".to_owned());
        }
        if resolved.is_dir() {
            if self.visited.insert(resolved.clone()) {
                self.directory(&resolved, &below);
            }
        } else if Path::new(name).extension().is_some_and(|x| x == "rhai") {
            self.script(&resolved, below);
        }
    }

    fn script(&mut self, resolved: &Path, below: String) {
        if self.full {
            return;
        }
        if self.found.scripts.len() == MAX_SCRIPTS {
            self.full = true;
            return self.problem(&below, format!("more than {MAX_SCRIPTS} scripts; this one and the rest are left out"));
        }
        match read_script(resolved) {
            Ok(text) => self.found.scripts.push(ScriptFile { path: below, text }),
            Err(message) => self.problem(&below, message),
        }
    }
}

fn read_script(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot be read: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_SCRIPT_BYTES + 1).read_to_end(&mut bytes).map_err(|e| format!("cannot be read: {e}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_SCRIPT_BYTES {
        return Err(format!("larger than {MAX_SCRIPT_BYTES} bytes"));
    }
    String::from_utf8(bytes).map_err(|_| "not UTF-8 text".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_keys_read_and_write_keeping_what_red_ale_does_not_know() {
        let text = r#"{"reports": {"directory": "mine", "dateFormat": "iso", "limits": {"operations": 5, "seconds": 2},
            "saved": [{"guid": "6f1e0c9d2b7a4e3f8a5b1c2d3e4f5a6b", "name": "Two levels", "template": "c4173ac99b2b448289bf4d11c731af13",
                       "options": {"Accounts": {"Levels of Subaccounts": 2}}, "note": "kept"}],
            "styleSheets": [{"name": "Footer", "template": "Footer", "options": {"Images": {"Logo": "x.png"}}}], "other": 1}}"#;
        let mut c = Config::parse(text).unwrap();
        assert_eq!(c.report_directory().unwrap(), Some(PathBuf::from("mine")));
        assert_eq!(c.report_limits().unwrap(), [("operations".to_owned(), 5), ("seconds".to_owned(), 2)]);
        assert_eq!(c.reports().unwrap()["dateFormat"], "iso");
        let mut saved = c.saved_reports().unwrap();
        assert_eq!((saved[0].name.as_str(), saved[0].other["note"].as_str()), ("Two levels", Some("kept")));
        saved[0].name = "Renamed".into();
        c.set_saved_reports(&saved);
        let sheets = c.style_sheets().unwrap();
        c.set_style_sheets(&sheets);
        let again = Config::parse(&c.to_text()).unwrap();
        assert_eq!(again.saved_reports().unwrap()[0].name, "Renamed");
        assert_eq!(again.saved_reports().unwrap()[0].other["note"], "kept");
        assert_eq!(again.style_sheets().unwrap(), sheets);
        assert_eq!(again.reports().unwrap()["other"], 1);
        let empty = Config::default();
        assert_eq!((empty.report_directory().unwrap(), empty.saved_reports().unwrap()), (None, Vec::new()));
        let bad = |t: &str| Config::parse(t).unwrap();
        assert!(bad(r#"{"reports": []}"#).reports().is_err());
        assert!(bad(r#"{"reports": {"saved": [{"name": "x"}]}}"#).saved_reports().is_err());
        assert!(bad(r#"{"reports": {"limits": {"seconds": -1}}}"#).report_limits().is_err());
        assert!(bad(r#"{"reports": {"directory": 3}}"#).report_directory().is_err());
    }

    #[test]
    fn relative_paths_start_from_the_configuration_file() {
        let store = ConfigStore::at("/etc/red-ale/config.json");
        assert_eq!(store.resolve(Path::new("reports")), PathBuf::from("/etc/red-ale/reports"));
        assert_eq!(store.resolve(Path::new("/srv/reports")), PathBuf::from("/srv/reports"));
        assert_eq!(ConfigStore::in_memory().resolve(Path::new("reports")), PathBuf::from("reports"));
    }

    #[test]
    fn the_loader_reads_scripts_below_the_directory_only() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.rhai"), "fn x() {}").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("mine/deeper")).unwrap();
        std::fs::write(root.join("b.rhai"), "// b").unwrap();
        std::fs::write(root.join("notes.txt"), "not a script").unwrap();
        std::fs::write(root.join("mine/a.rhai"), "// a").unwrap();
        std::fs::write(root.join("mine/deeper/c.rhai"), "// c").unwrap();
        std::fs::write(root.join("big.rhai"), vec![b' '; usize::try_from(MAX_SCRIPT_BYTES).unwrap() + 1]).unwrap();
        std::fs::write(root.join("latin1.rhai"), [0xe9_u8]).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.rhai"), root.join("escape.rhai")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("away")).unwrap();
        std::os::unix::fs::symlink(root.join("mine/a.rhai"), root.join("inside.rhai")).unwrap();
        std::os::unix::fs::symlink(root, root.join("mine/loop")).unwrap();
        let found = load_report_scripts(root);
        let paths: Vec<&str> = found.scripts.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, ["b.rhai", "inside.rhai", "mine/a.rhai", "mine/deeper/c.rhai"]);
        assert_eq!(found.scripts[2].text, "// a");
        let problems: Vec<&str> = found.problems.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(problems, ["away", "big.rhai", "escape.rhai", "latin1.rhai"]);
        assert!(found.problems[0].message.contains("outside the report directory"));
        let missing = load_report_scripts(&root.join("nowhere"));
        assert_eq!((missing.scripts.len(), missing.problems[0].path.as_str()), (0, "."));
    }
}
