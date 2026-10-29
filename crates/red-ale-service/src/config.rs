//! red-ale's own configuration: settings GnuCash keeps as user preferences rather than in the book, such as the
//! accounting period (`5.17:gnucash/gschemas/org.gnucash.GnuCash.window.pages.account.tree.gschema.xml.in`).
//!
//! The configuration is a JSON object in a file named by [`Settings`](crate::Settings), or, with no file, held in
//! memory for the life of the process. The binary uses [`default_path`] unless given another file, or `--no-config`
//! for memory (`design/api/configuration.md`). Keys red-ale does not know are kept when it writes the file.
//!
//! ```json
//! { "accountingPeriod": { "start": "year", "end": "2024-12-31", "fiscalYearEnd": "06-30" } }
//! ```

use std::ffi::OsString;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use red_ale_core::DomainError;
use red_ale_core::ops::period::{AccountingPeriod, FiscalYearEnd, PeriodBound};
use serde_json::{Map, Value};

use crate::ServiceError;

pub mod schema;
pub mod settings;

pub use settings::{Applied, Problem, SettingError, SettingPath};

const PERIOD: &str = "accountingPeriod";
const DOC_LINKS: &str = "docLinks";
const PATH_HEAD: &str = "pathHead";

/// The configuration document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Config {
    raw: Map<String, Value>,
}

pub(crate) fn invalid(message: impl Into<String>) -> ServiceError {
    ServiceError::Domain(DomainError::invalid(message))
}

impl Config {
    /// The value under a top-level key.
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.raw.get(key)
    }

    /// Change the object under a top-level key, made an object first if it is missing or is not one. Other keys
    /// keep their place.
    pub(crate) fn update_object(&mut self, key: &str, f: impl FnOnce(&mut Map<String, Value>)) {
        let entry = self.raw.entry(key).or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        if let Value::Object(map) = entry {
            f(map);
        }
    }

    /// Parse a configuration file's text.
    pub fn parse(text: &str) -> Result<Self, ServiceError> {
        match serde_json::from_str(text) {
            Ok(Value::Object(raw)) => Ok(Self { raw }),
            Ok(_) => Err(invalid("the configuration must be a JSON object")),
            Err(e) => Err(invalid(format!("the configuration is not valid JSON: {e}"))),
        }
    }

    /// The text to write.
    pub fn to_text(&self) -> String {
        let mut text = serde_json::to_string_pretty(&self.raw).unwrap_or_else(|_| "{}".to_owned());
        text.push('\n');
        text
    }

    fn period_field(&self, key: &str) -> Option<&Value> {
        self.raw.get(PERIOD).and_then(|p| p.get(key))
    }

    fn period_text(&self, key: &str) -> Result<Option<&str>, ServiceError> {
        match self.period_field(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(other) => Err(invalid(format!("{PERIOD}.{key} must be a string, not {other}"))),
        }
    }

    /// The accounting period; GnuCash's default (this calendar year) when unset.
    pub fn accounting_period(&self) -> Result<AccountingPeriod, ServiceError> {
        let default = AccountingPeriod::default();
        let bound = |key: &str, d: PeriodBound| -> Result<PeriodBound, ServiceError> {
            self.period_text(key)?.map_or(Ok(d), |s| s.parse().map_err(ServiceError::Domain))
        };
        Ok(AccountingPeriod {
            start: bound("start", default.start)?,
            end: bound("end", default.end)?,
        })
    }

    /// The configured fiscal year end, if any.
    pub fn fiscal_year_end(&self) -> Result<Option<FiscalYearEnd>, ServiceError> {
        self.period_text("fiscalYearEnd")?
            .map(str::parse)
            .transpose()
            .map_err(ServiceError::Domain)
    }

    /// Set the accounting period and the fiscal year end (`None` removes it).
    pub fn set_accounting_period(&mut self, period: &AccountingPeriod, fiscal_year_end: Option<FiscalYearEnd>) {
        let entry = self.raw.entry(PERIOD).or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        if let Value::Object(p) = entry {
            p.insert("start".into(), Value::String(period.start.to_string()));
            p.insert("end".into(), Value::String(period.end.to_string()));
            match fiscal_year_end {
                Some(fy) => p.insert("fiscalYearEnd".into(), Value::String(fy.to_string())),
                None => p.remove("fiscalYearEnd"),
            };
        }
    }
}

impl Config {
    /// The path head relative document links start from (GnuCash's `general/assoc-head` preference), if set.
    pub fn doc_link_path_head(&self) -> Result<Option<&str>, ServiceError> {
        match self.raw.get(DOC_LINKS).and_then(|d| d.get(PATH_HEAD)) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(other) => Err(invalid(format!("{DOC_LINKS}.{PATH_HEAD} must be a string, not {other}"))),
        }
    }

    /// Set the path head, or remove it with `None`.
    pub fn set_doc_link_path_head(&mut self, head: Option<&str>) {
        let entry = self.raw.entry(DOC_LINKS).or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        if let Value::Object(d) = entry {
            match head {
                Some(h) => d.insert(PATH_HEAD.into(), Value::String(h.to_owned())),
                None => d.remove(PATH_HEAD),
            };
        }
    }
}

/// The default Configuration file, shared by every red-ale process started without `--config`:
/// `$XDG_CONFIG_HOME/red-ale/config.json` (else `~/.config/red-ale/config.json`) on Linux and other Unixes,
/// `~/Library/Application Support/red-ale/config.json` on macOS, `%APPDATA%\red-ale\config.json` on Windows. `None`
/// when the environment names no home directory.
pub fn default_path() -> Option<PathBuf> {
    default_path_for(std::env::consts::OS, |name| std::env::var_os(name))
}

fn default_path_for(os: &str, var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let dir = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = match os {
        "windows" => dir("APPDATA"),
        "macos" => dir("HOME").map(|home| home.join("Library").join("Application Support")),
        // The XDG Base Directory spec ignores a relative XDG_CONFIG_HOME.
        _ => dir("XDG_CONFIG_HOME")
            .filter(|d| d.is_absolute())
            .or_else(|| dir("HOME").map(|home| home.join(".config"))),
    }?;
    Some(base.join("red-ale").join("config.json"))
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Held from loading the Configuration to saving it ([`ConfigStore::lock`]).
#[must_use = "the lock is held only while the guard lives"]
#[derive(Debug)]
pub struct ConfigLock {
    _file: Option<File>,
}

/// Where the configuration lives: a file, or memory shared by every clone of the store.
#[derive(Clone, Default)]
pub struct ConfigStore {
    path: Option<PathBuf>,
    memory: Arc<Mutex<Config>>,
}

impl fmt::Debug for ConfigStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigStore").field("path", &self.path).finish_non_exhaustive()
    }
}

/// Two stores are equal when they keep the configuration in the same place.
impl PartialEq for ConfigStore {
    fn eq(&self, other: &Self) -> bool {
        match (&self.path, &other.path) {
            (Some(a), Some(b)) => a == b,
            (None, None) => Arc::ptr_eq(&self.memory, &other.memory),
            _ => false,
        }
    }
}

impl Eq for ConfigStore {}

impl ConfigStore {
    /// A configuration held in memory.
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// A configuration kept in a JSON file, read at every use so edits by hand are seen.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
            memory: Arc::default(),
        }
    }

    /// The file, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The configuration as it stands. A missing file reads as an empty configuration.
    pub fn load(&self) -> Result<Config, ServiceError> {
        match &self.path {
            None => Ok(self.memory.lock().unwrap_or_else(PoisonError::into_inner).clone()),
            Some(path) => match std::fs::read_to_string(path) {
                Ok(text) => Config::parse(&text),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
                Err(e) => Err(ServiceError::Io {
                    path: path.clone(),
                    source: e,
                }),
            },
        }
    }

    /// Take the lock every change holds from its [`load`](Self::load) to its [`save`](Self::save), so two
    /// processes changing one file at once don't lose either change. A file store locks `<file>.lock` beside it,
    /// creating the directory if needed, and waits for any other holder; the lock goes when the guard is dropped.
    /// A memory store needs none: one process's changes already come one at a time through the dispatcher.
    pub fn lock(&self) -> Result<ConfigLock, ServiceError> {
        let Some(path) = &self.path else {
            return Ok(ConfigLock { _file: None });
        };
        let lock_path = with_suffix(path, ".lock");
        let io = |source| ServiceError::Io {
            path: lock_path.clone(),
            source,
        };
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(io)?;
        file.lock().map_err(io)?;
        Ok(ConfigLock { _file: Some(file) })
    }

    /// Replace the configuration, under the lock taken before it was loaded. A file is written beside itself and
    /// renamed into place, so it is never half written; its directory is made if it doesn't exist.
    pub fn save(&self, _lock: &ConfigLock, config: &Config) -> Result<(), ServiceError> {
        let Some(path) = &self.path else {
            *self.memory.lock().unwrap_or_else(PoisonError::into_inner) = config.clone();
            return Ok(());
        };
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|source| ServiceError::Io {
                path: dir.to_owned(),
                source,
            })?;
        }
        crate::files::replace_private_file(path, config.to_text().as_bytes())
    }

    /// The largest request the JSON-RPC server accepts (`jsonRpc.maxRequestBytes`).
    pub fn jsonrpc_max_request_bytes(&self) -> Result<u32, ServiceError> {
        let bytes = self.load()?.checked("jsonRpc.maxRequestBytes")?;
        bytes
            .as_u64()
            .and_then(|b| u32::try_from(b).ok())
            .ok_or_else(|| invalid("jsonRpc.maxRequestBytes must be a whole number of bytes"))
    }

    /// The largest response the JSON-RPC server sends (`jsonRpc.maxResponseBytes`).
    pub fn jsonrpc_max_response_bytes(&self) -> Result<u32, ServiceError> {
        let bytes = self.load()?.checked("jsonRpc.maxResponseBytes")?;
        bytes
            .as_u64()
            .and_then(|b| u32::try_from(b).ok())
            .ok_or_else(|| invalid("jsonRpc.maxResponseBytes must be a whole number of bytes"))
    }

    /// The largest request the MCP server accepts (`mcp.maxRequestBytes`).
    pub fn mcp_max_request_bytes(&self) -> Result<usize, ServiceError> {
        let bytes = self.load()?.checked("mcp.maxRequestBytes")?;
        bytes
            .as_u64()
            .and_then(|b| usize::try_from(b).ok())
            .ok_or_else(|| invalid("mcp.maxRequestBytes must be a whole number of bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use red_ale_core::ops::period::RelativePeriod;

    #[test]
    fn defaults_round_trip_and_unknown_keys_survive() {
        let mut c = Config::parse(r#"{"other": {"x": 1}, "accountingPeriod": {"note": "kept"}}"#).unwrap();
        assert_eq!(c.accounting_period().unwrap(), AccountingPeriod::default());
        let p = AccountingPeriod {
            start: PeriodBound::Relative(RelativePeriod::FiscalYear),
            end: "2024-12-31".parse().unwrap(),
        };
        c.set_accounting_period(&p, Some("06-30".parse().unwrap()));
        let again = Config::parse(&c.to_text()).unwrap();
        assert_eq!(again.accounting_period().unwrap(), p);
        assert_eq!(again.fiscal_year_end().unwrap().unwrap().to_string(), "06-30");
        assert!(again.to_text().contains("\"other\"") && again.to_text().contains("\"note\""));
        assert!(Config::parse("[]").is_err());
        assert!(
            Config::parse(r#"{"accountingPeriod": {"start": "soon"}}"#)
                .unwrap()
                .accounting_period()
                .is_err()
        );
    }

    #[test]
    fn the_doc_link_path_head_is_kept_beside_other_settings() {
        let mut c = Config::parse(r#"{"docLinks": {"other": 1}}"#).unwrap();
        assert_eq!(c.doc_link_path_head().unwrap(), None);
        c.set_doc_link_path_head(Some("file:///srv/docs/"));
        let again = Config::parse(&c.to_text()).unwrap();
        assert_eq!(again.doc_link_path_head().unwrap(), Some("file:///srv/docs/"));
        assert!(again.to_text().contains("\"other\""));
        c.set_doc_link_path_head(None);
        assert_eq!(c.doc_link_path_head().unwrap(), None);
        assert!(
            Config::parse(r#"{"docLinks": {"pathHead": 3}}"#)
                .unwrap()
                .doc_link_path_head()
                .is_err()
        );
    }

    #[test]
    fn stores_share_memory_or_use_a_file() {
        let memory = ConfigStore::in_memory();
        let clone = memory.clone();
        let mut c = Config::default();
        c.set_accounting_period(&AccountingPeriod::default(), "12-31".parse().ok());
        memory.save(&memory.lock().unwrap(), &c).unwrap();
        assert_eq!(clone.load().unwrap(), c);
        assert_eq!(memory, clone);
        assert_ne!(memory, ConfigStore::in_memory());
        let dir = tempfile::tempdir().unwrap();
        let file = ConfigStore::at(dir.path().join("red-ale.json"));
        assert_eq!(file.load().unwrap(), Config::default());
        file.save(&file.lock().unwrap(), &c).unwrap();
        assert_eq!(ConfigStore::at(dir.path().join("red-ale.json")).load().unwrap(), c);
    }

    #[test]
    fn the_default_path_follows_each_platform() {
        fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
            move |name| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| OsString::from(*v))
        }
        let linux = [("HOME", "/home/me")];
        assert_eq!(
            default_path_for("linux", env(&linux)),
            Some(PathBuf::from("/home/me/.config/red-ale/config.json"))
        );
        let xdg = [("HOME", "/home/me"), ("XDG_CONFIG_HOME", "/xdg")];
        assert_eq!(
            default_path_for("linux", env(&xdg)),
            Some(PathBuf::from("/xdg/red-ale/config.json"))
        );
        let relative = [("HOME", "/home/me"), ("XDG_CONFIG_HOME", "rel")];
        assert_eq!(
            default_path_for("freebsd", env(&relative)),
            Some(PathBuf::from("/home/me/.config/red-ale/config.json"))
        );
        assert_eq!(
            default_path_for("macos", env(&linux)),
            Some(PathBuf::from("/home/me/Library/Application Support/red-ale/config.json"))
        );
        let windows = [("APPDATA", "C:/Users/me/AppData/Roaming")];
        assert_eq!(
            default_path_for("windows", env(&windows)),
            Some(PathBuf::from("C:/Users/me/AppData/Roaming").join("red-ale").join("config.json"))
        );
        assert_eq!(default_path_for("linux", env(&[("HOME", "")])), None);
        assert_eq!(default_path_for("windows", env(&linux)), None);
    }

    /// The first save makes the file's directory.
    #[test]
    fn a_first_save_makes_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::at(dir.path().join("a/b/config.json"));
        let mut c = Config::default();
        c.set_doc_link_path_head(Some("file:///docs/"));
        store.save(&store.lock().unwrap(), &c).unwrap();
        assert_eq!(store.load().unwrap(), c);
    }

    /// Changes by many writers at once, each through its own store over the same file as separate processes would
    /// have, are all kept: each holds the lock from its load to its save.
    #[test]
    fn concurrent_changes_are_all_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let writers: Vec<_> = (0..8)
            .map(|i| {
                let store = ConfigStore::at(&path);
                std::thread::spawn(move || {
                    (0..10).for_each(|j| {
                        let lock = store.lock().unwrap();
                        let mut c = store.load().unwrap();
                        c.raw.insert(format!("custom.w{i}.n{j}"), Value::Bool(true));
                        store.save(&lock, &c).unwrap();
                    });
                })
            })
            .collect();
        writers.into_iter().for_each(|w| w.join().unwrap());
        let kept = ConfigStore::at(&path).load().unwrap();
        assert_eq!(kept.raw.len(), 80);
    }
}
