//! Turning Rhai's errors into report errors (`design/reports.md` §2, "Limits and their errors"): a passed limit is
//! `report-limit` naming it, a script that does not compile is `report-load` (or `internal-error` for a built-in
//! script), and anything else a script does wrong is `report-script`, located in the innermost script it happened in.

use rhai::{EvalAltResult, LexError, ParseError, ParseErrorType, Position};

use super::numbers::NUMBER_SIZE;
use super::registry::USER_PREFIX;
use crate::error::{LimitError, LoadError, ReportError, ScriptError};
use crate::limits::{Limit, Limits};
use crate::result::LogEntry;

/// Where a failure happened: the template run, the script it started in, the limits and the log so far.
pub(crate) struct Failure<'a> {
    /// The template's GUID; empty while loading.
    pub report: &'a str,
    /// The script the call started in.
    pub source: &'a str,
    /// The run's limits.
    pub limits: &'a Limits,
    /// The report log up to the failure.
    pub log: Vec<LogEntry>,
}

fn line_column(pos: Position) -> (Option<u32>, Option<u32>) {
    let part = |n: Option<usize>| n.and_then(|n| u32::try_from(n).ok());
    (part(pos.line()), part(pos.position()))
}

/// The error a run failed with.
pub(crate) fn from_eval(err: &EvalAltResult, f: Failure<'_>) -> ReportError {
    let (inner, source) = innermost(err, f.source);
    if let Some(limit) = eval_limit(inner) {
        return limit_error(limit, Some(source), &f);
    }
    if let EvalAltResult::ErrorParsing(kind, pos) = inner {
        return from_parse(&ParseError(Box::new(kind.clone()), *pos), source, &f);
    }
    let (line, column) = line_column(inner.position());
    ReportError::Script(ScriptError { report: f.report.to_owned(), source: source.to_owned(), line, column, message: message(inner), log: f.log })
}

/// The error of a script that does not compile.
pub(crate) fn from_parse(err: &ParseError, source: &str, f: &Failure<'_>) -> ReportError {
    if let Some(limit) = parse_limit(err.err_type()) {
        return limit_error(limit, Some(source), f);
    }
    let (line, column) = line_column(err.position());
    if source.starts_with(USER_PREFIX) {
        ReportError::Load(LoadError { source: source.to_owned(), line, column, message: err.err_type().to_string() })
    } else {
        ReportError::Internal(format!("built-in script {source} does not compile: {err}"))
    }
}

fn limit_error(limit: Limit, source: Option<&str>, f: &Failure<'_>) -> ReportError {
    ReportError::Limit(LimitError {
        report: f.report.to_owned(),
        source: source.map(str::to_owned),
        limit,
        max: f.limits.get(limit),
        log: f.log.clone(),
    })
}

/// The error inside the calls and modules it passed through, and the script it happened in.
fn innermost<'e>(err: &'e EvalAltResult, source: &'e str) -> (&'e EvalAltResult, &'e str) {
    match err {
        EvalAltResult::ErrorInFunctionCall(_, src, inner, _) => innermost(inner, if src.is_empty() { source } else { src }),
        EvalAltResult::ErrorInModule(name, inner, _) => innermost(inner, name),
        other => (other, source),
    }
}

fn eval_limit(err: &EvalAltResult) -> Option<Limit> {
    match err {
        EvalAltResult::ErrorTooManyOperations(_) => Some(Limit::Operations),
        EvalAltResult::ErrorTerminated(..) => Some(Limit::Time),
        EvalAltResult::ErrorStackOverflow(_) => Some(Limit::CallDepth),
        EvalAltResult::ErrorTooManyModules(_) => Some(Limit::Modules),
        EvalAltResult::ErrorTooManyVariables(_) => Some(Limit::Variables),
        EvalAltResult::ErrorDataTooLarge(what, _) if what == NUMBER_SIZE => Some(Limit::NumberSize),
        EvalAltResult::ErrorDataTooLarge(what, _) => Some(data_limit(what)),
        _ => None,
    }
}

/// Which data-size limit Rhai's description names.
fn data_limit(what: &str) -> Limit {
    if what.starts_with("Size of array") {
        Limit::ArraySize
    } else if what.contains("object map") {
        Limit::MapSize
    } else {
        Limit::StringSize
    }
}

fn parse_limit(err: &ParseErrorType) -> Option<Limit> {
    match err {
        ParseErrorType::ExprTooDeep => Some(Limit::ExpressionDepth),
        ParseErrorType::TooManyFunctions => Some(Limit::Functions),
        ParseErrorType::LiteralTooLarge(what, _) => Some(data_limit(what)),
        ParseErrorType::BadInput(LexError::StringTooLong(_)) => Some(Limit::StringSize),
        _ => None,
    }
}

/// What went wrong, without Rhai's position suffix: a thrown value as itself, anything else as Rhai describes it.
fn message(err: &EvalAltResult) -> String {
    if let EvalAltResult::ErrorRuntime(value, _) = err {
        if !value.is_unit() {
            return value.to_string();
        }
    }
    let text = err.to_string();
    let suffix = format!(" ({})", err.position());
    match text.strip_suffix(&suffix) {
        Some(stripped) if !err.position().is_none() => stripped.to_owned(),
        _ => text,
    }
}
