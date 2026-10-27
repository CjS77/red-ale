//! red-ale's own configuration: settings GnuCash keeps as user preferences rather than in the book, such as the
//! accounting period (`5.17:gnucash/gschemas/org.gnucash.GnuCash.window.pages.account.tree.gschema.xml.in`).
//!
//! The configuration is a JSON object in a file named by [`Settings`](crate::Settings), or, with no file, held in
//! memory for the life of the process. Keys red-ale does not know are kept when it writes the file.
//!
//! ```json
//! { "accountingPeriod": { "start": "year", "end": "2024-12-31", "fiscalYearEnd": "06-30" } }
//! ```

use std::fmt;
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

    /// Replace the configuration. A file is written beside itself and renamed into place, so it is never half
    /// written.
    pub fn save(&self, config: &Config) -> Result<(), ServiceError> {
        let Some(path) = &self.path else {
            *self.memory.lock().unwrap_or_else(PoisonError::into_inner) = config.clone();
            return Ok(());
        };
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
        memory.save(&c).unwrap();
        assert_eq!(clone.load().unwrap(), c);
        assert_eq!(memory, clone);
        assert_ne!(memory, ConfigStore::in_memory());
        let dir = tempfile::tempdir().unwrap();
        let file = ConfigStore::at(dir.path().join("red-ale.json"));
        assert_eq!(file.load().unwrap(), Config::default());
        file.save(&c).unwrap();
        assert_eq!(ConfigStore::at(dir.path().join("red-ale.json")).load().unwrap(), c);
    }
}
