//! The errors of report methods (`design/reports.md` §2, "Limits and their errors"; `design/api.md`, error codes
//! 3000 to 3004). Each kind carries the `data` the API documents, so `red-ale-rpc` maps an error to its code and data
//! without knowing the details.

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use crate::limits::Limit;
use crate::result::LogEntry;

/// The kind of a [`ReportError`], with its stable API code and name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportErrorKind {
    /// `report-script` (3000): the script failed at run time.
    Script,
    /// `report-limit` (3001): the script passed a limit.
    Limit,
    /// `report-option` (3002): an option value is not valid.
    Option,
    /// `report-result` (3003): the script returned something that is not a valid report result.
    Result,
    /// `report-load` (3004): a user script does not load.
    Load,
    /// `internal-error` (-32603): a bug in red-ale, such as a built-in script that does not compile.
    Internal,
}

impl ReportErrorKind {
    /// The JSON-RPC error code.
    pub const fn code(self) -> i32 {
        match self {
            Self::Script => 3000,
            Self::Limit => 3001,
            Self::Option => 3002,
            Self::Result => 3003,
            Self::Load => 3004,
            Self::Internal => -32603,
        }
    }

    /// The error's name in the API.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Script => "report-script",
            Self::Limit => "report-limit",
            Self::Option => "report-option",
            Self::Result => "report-result",
            Self::Load => "report-load",
            Self::Internal => "internal-error",
        }
    }
}

/// A script failed at run time: `throw`, a type error, division by zero, an unknown function, variable or module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptError {
    /// The template's GUID.
    pub report: String,
    /// The script the failure is in.
    pub source: String,
    /// Line, from 1, when known.
    pub line: Option<u32>,
    /// Column, from 1, when known.
    pub column: Option<u32>,
    /// What went wrong.
    pub message: String,
    /// The report log up to the failure.
    pub log: Vec<LogEntry>,
}

/// A script passed a limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitError {
    /// The template's GUID; empty when the limit was passed while a script compiled, before any template was known.
    pub report: String,
    /// The script the limit was passed in, when known.
    pub source: Option<String>,
    /// The limit passed.
    pub limit: Limit,
    /// Its maximum.
    pub max: u64,
    /// The report log up to the failure.
    pub log: Vec<LogEntry>,
}

/// An option value is not valid, or an option declaration is malformed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionError {
    /// The option's section.
    pub section: String,
    /// The option's name.
    pub name: String,
    /// Why the value is refused.
    pub reason: String,
}

impl OptionError {
    /// An error about the option `section`/`name`.
    pub fn new(section: impl Into<String>, name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self { section: section.into(), name: name.into(), reason: reason.into() }
    }
}

/// A template returned something that is not a valid report result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultError {
    /// The template's GUID; empty until the host fills it in.
    pub report: String,
    /// Where in the result: a JSON path such as `sections[0].blocks[2].rows[3].cells`.
    pub path: String,
    /// What is wrong there.
    pub reason: String,
    /// The report log.
    pub log: Vec<LogEntry>,
}

impl ResultError {
    /// An error at `path`, with no report or log yet.
    pub fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self { report: String::new(), path: path.into(), reason: reason.into(), log: Vec::new() }
    }
}

/// A user script does not compile, or its `templates()` is missing, malformed or repeats a GUID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadError {
    /// The script.
    pub source: String,
    /// Line, from 1, when known.
    pub line: Option<u32>,
    /// Column, from 1, when known.
    pub column: Option<u32>,
    /// What went wrong.
    pub message: String,
}

/// An error from running, configuring or loading a report.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReportError {
    /// `report-script`.
    #[error("report script failed in {}: {}", .0.source, .0.message)]
    Script(ScriptError),
    /// `report-limit`.
    #[error("report passed the {} limit of {}", .0.limit, .0.max)]
    Limit(LimitError),
    /// `report-option`.
    #[error("option {}/{}: {}", .0.section, .0.name, .0.reason)]
    Option(OptionError),
    /// `report-result`.
    #[error("invalid report result at {}: {}", .0.path, .0.reason)]
    Result(ResultError),
    /// `report-load`.
    #[error("report script {} does not load: {}", .0.source, .0.message)]
    Load(LoadError),
    /// `internal-error`: a bug in red-ale, such as a built-in script that does not compile or returns an invalid
    /// result (`design/reports.md` §2). The tests of every built-in script catch these before a release.
    #[error("internal error in the report engine: {0}")]
    Internal(String),
}

impl ReportError {
    /// The error's kind.
    pub const fn kind(&self) -> ReportErrorKind {
        match self {
            Self::Script(_) => ReportErrorKind::Script,
            Self::Limit(_) => ReportErrorKind::Limit,
            Self::Option(_) => ReportErrorKind::Option,
            Self::Result(_) => ReportErrorKind::Result,
            Self::Load(_) => ReportErrorKind::Load,
            Self::Internal(_) => ReportErrorKind::Internal,
        }
    }

    /// The `data` member of the JSON-RPC error.
    pub fn data(&self) -> Value {
        let data = match self {
            Self::Script(e) => serde_json::to_value(e),
            Self::Limit(e) => serde_json::to_value(e),
            Self::Option(e) => serde_json::to_value(e),
            Self::Result(e) => serde_json::to_value(e),
            Self::Load(e) => serde_json::to_value(e),
            Self::Internal(_) => Ok(Value::Null),
        };
        data.unwrap_or(Value::Null)
    }
}

impl From<OptionError> for ReportError {
    fn from(e: OptionError) -> Self {
        Self::Option(e)
    }
}

impl From<ResultError> for ReportError {
    fn from(e: ResultError) -> Self {
        Self::Result(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kinds_carry_the_api_codes_and_data() {
        let e = ReportError::from(OptionError::new("General", "Start Date", "not one of the choices"));
        assert_eq!((e.kind().code(), e.kind().name()), (3002, "report-option"));
        assert_eq!(e.data(), json!({ "section": "General", "name": "Start Date", "reason": "not one of the choices" }));
        let report = "c4173ac99b2b448289bf4d11c731af13".to_owned();
        let limit = ReportError::Limit(LimitError { report, source: None, limit: Limit::NumberSize, max: 4096, log: vec![] });
        assert_eq!(limit.data()["limit"], json!("number-size"));
        assert_eq!(limit.kind().code(), 3001);
        assert_eq!(limit.to_string(), "report passed the number-size limit of 4096");
        let result = ReportError::from(ResultError::new("sections[0]", "missing field `blocks`"));
        assert_eq!(result.data()["path"], json!("sections[0]"));
        let load = ReportError::Load(LoadError { source: "mine.rhai".into(), line: Some(3), column: None, message: "x".into() });
        assert_eq!((load.kind().code(), load.data()["line"].clone()), (3004, json!(3)));
        let script = ReportError::Script(ScriptError {
            report: String::new(),
            source: "a.rhai".into(),
            line: None,
            column: None,
            message: "boom".into(),
            log: vec![],
        });
        assert_eq!(script.kind().name(), "report-script");
    }
}
