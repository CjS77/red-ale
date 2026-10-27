//! Loading the files, as the assistant's first pages do: each file read and its fields parsed
//! (`gnc_ui_qif_import_load_file_complete`, `5.17:gnucash/import-export/qif-imp/assistant-qif-import.c:1860-1960`),
//! the date format asked for when the dates are ambiguous (`:2042-2083`, `:2175-2187`), and a default account given
//! to a file read under no `!Account` (`:2222-2350`).
//!
//! red-ale also turns the records GnuCash would import wrongly into error rows, leaving them out
//! (`design/api/import.md`, QIF deviations): a number GnuCash reads as 0, a day that does not exist, an investment
//! transaction with no action GnuCash knows.

use std::collections::BTreeSet;

use super::super::dialog_utils::{action, default_equity_account, fix_from_acct};
use super::super::file::{
    FieldError, FieldKind, FieldProblem, ParseFieldsReport, ReadError, ReadOptions, ReadWarningKind, RecordKind, check_from_acct,
    parse_fields, read_file, reparse_dates, reparse_price_dates,
};
use super::super::objects::{Field, QifFile, QifXtn, TickerMap};
use super::super::parse::{Action, DateFormat};
use super::super::to_gnc::xtn::qif_date;
use super::{QifError, QifFileInput};
use crate::pipeline::{IgnoredLine, RowError, Warning};

/// The files as loaded, before any choice of accounts.
#[derive(Debug, Default)]
pub(super) struct Loaded {
    /// The files, with the error rows left out.
    pub files: Vec<QifFile>,
    /// The securities of every file.
    pub ticker_map: TickerMap,
    /// Lines the reader skipped.
    pub ignored: Vec<IgnoredLine>,
    /// Records left out.
    pub errors: Vec<RowError>,
    /// Field problems GnuCash replaces and number formats it guessed.
    pub warnings: Vec<Warning>,
    /// The date formats to choose between, when the dates are ambiguous and no format was given.
    pub date_formats: Vec<DateFormat>,
}

/// Read and parse every file; with the dates decided, leave out the error rows and give each file its account.
pub(super) fn load(inputs: &[QifFileInput], date_format: Option<DateFormat>) -> Result<Loaded, QifError> {
    let mut loaded = Loaded::default();
    let reports: Vec<ParseFieldsReport> = inputs
        .iter()
        .enumerate()
        .map(|(i, input)| read_one(i, input, &mut loaded))
        .collect::<Result<_, _>>()?;
    let ambiguous: Vec<(usize, Vec<DateFormat>)> = reports
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.date_formats().map(|f| (i, f)))
        .collect();
    if !ambiguous.is_empty() {
        let Some(format) = date_format else {
            loaded.date_formats = common_formats(&ambiguous);
            loaded
                .errors
                .extend(reports.iter().enumerate().flat_map(|(i, r)| unreadable_rows(i, &r.errors)));
            return if loaded.date_formats.is_empty() {
                Err(no_common_format(&ambiguous))
            } else {
                Ok(loaded)
            };
        };
        ambiguous
            .iter()
            .try_for_each(|(i, formats)| reparse(&mut loaded.files, *i, formats, format))?;
    }
    reports
        .iter()
        .enumerate()
        .for_each(|(i, r)| loaded.warnings.extend(field_warnings(i, r)));
    let files = std::mem::take(&mut loaded.files);
    loaded.files = files
        .into_iter()
        .enumerate()
        .map(|(i, f)| without_errors(i, f, &reports[i], &mut loaded.errors))
        .collect();
    loaded.files.iter_mut().zip(inputs).for_each(|(f, input)| default_account(f, input));
    Ok(loaded)
}

/// `qif-file:read-file` then `qif-file:parse-fields` for one file.
fn read_one(index: usize, input: &QifFileInput, loaded: &mut Loaded) -> Result<ParseFieldsReport, QifError> {
    let options = ReadOptions {
        path: input.path.clone(),
        default_equity_account: default_equity_account(),
    };
    let read = read_file(&input.text, &options).map_err(|e| read_error(index, &e))?;
    read.securities.into_iter().for_each(|s| loaded.ticker_map.add_ticker(s));
    loaded.ignored.extend(read.warnings.iter().map(|w| IgnoredLine {
        file: index,
        line: w.line,
        reason: warning_reason(&w.kind),
    }));
    let mut file = read.file;
    let report = parse_fields(&mut file).map_err(|e| QifError::File {
        file: index,
        line: e.report.errors.iter().rev().find_map(|f| f.line),
        reason: "format",
        message: e.to_string(),
    })?;
    loaded.files.push(file);
    Ok(report)
}

fn read_error(file: usize, e: &ReadError) -> QifError {
    let line = match e {
        ReadError::NotQif { line, .. } | ReadError::Category { line, .. } => *line,
    };
    QifError::File {
        file,
        line: Some(line),
        reason: "qif",
        message: e.to_string(),
    }
}

/// The reason code of a line the reader skipped.
fn warning_reason(kind: &ReadWarningKind) -> &'static str {
    match kind {
        ReadWarningKind::UnknownOption(_) => "unknown-option",
        ReadWarningKind::DateRequired => "date-required",
        ReadWarningKind::IgnoringClassLine(_) => "class-line",
        ReadWarningKind::IgnoringCategoryLine(_) => "category-line",
        ReadWarningKind::IgnoringSecurityLine(_) => "security-line",
        ReadWarningKind::UnparsablePriceLine(_) => "price-line",
    }
}

/// The formats every ambiguous file allows, in GnuCash's order.
fn common_formats(ambiguous: &[(usize, Vec<DateFormat>)]) -> Vec<DateFormat> {
    let first = ambiguous.first().map(|(_, f)| f.clone()).unwrap_or_default();
    first
        .into_iter()
        .filter(|f| ambiguous.iter().all(|(_, formats)| formats.contains(f)))
        .collect()
}

fn no_common_format(ambiguous: &[(usize, Vec<DateFormat>)]) -> QifError {
    let (file, _) = ambiguous.last().cloned().unwrap_or_default();
    let message = "the files' dates are ambiguous, and no one date format fits them all: import the files one at a time".to_owned();
    QifError::File {
        file,
        line: None,
        reason: "date-format",
        message,
    }
}

/// `qif-file:reparse-dates` with the caller's format, for transactions and prices.
fn reparse(files: &mut [QifFile], index: usize, formats: &[DateFormat], format: DateFormat) -> Result<(), QifError> {
    let fail = |line: Option<usize>, message: String| QifError::File {
        file: index,
        line,
        reason: "date-format",
        message,
    };
    if !formats.contains(&format) {
        let names: Vec<&str> = formats.iter().map(|f| f.symbol()).collect();
        return Err(fail(
            None,
            format!(
                "dateFormat {} does not fit the dates; these do: {}",
                format.symbol(),
                names.join(", ")
            ),
        ));
    }
    let Some(file) = files.get_mut(index) else { return Ok(()) };
    let failed = |errors: Vec<FieldError>| {
        let line = errors.first().and_then(|e| e.line);
        fail(line, errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))
    };
    reparse_dates(file, format).map_err(failed)?;
    reparse_price_dates(file, format).map_err(failed)
}

/// A number GnuCash reads as 0 and an action it does not know make their record an error row.
fn is_row_error(e: &FieldError) -> bool {
    matches!(e.problem, FieldProblem::Unreadable(_))
        || (e.record == RecordKind::Transaction && e.field == FieldKind::Action && matches!(e.problem, FieldProblem::Message(_)))
}

fn unreadable_rows(file: usize, errors: &[FieldError]) -> impl Iterator<Item = RowError> + '_ {
    errors
        .iter()
        .filter(|e| matches!(e.record, RecordKind::Transaction | RecordKind::Price) && is_row_error(e))
        .filter_map(move |e| {
            e.line.map(|line| RowError {
                file,
                line,
                column: None,
                message: e.problem.to_string(),
            })
        })
}

/// The field problems that do not make an error row, as warnings, and the number formats guessed.
fn field_warnings(file: usize, report: &ParseFieldsReport) -> Vec<Warning> {
    let problems = report
        .errors
        .iter()
        .filter(|e| !(matches!(e.record, RecordKind::Transaction | RecordKind::Price) && is_row_error(e)))
        .filter(|e| !matches!(e.problem, FieldProblem::Ambiguous(_)))
        .map(|e| Warning {
            code: "qif-field",
            message: format!("file {file}: {e}"),
            index: None,
        });
    let guesses = report.guesses.iter().map(|g| {
        let candidates: Vec<String> = g.candidates.iter().map(ToString::to_string).collect();
        let message = format!(
            "file {file}: {} read as {}; it also fits {}",
            g.field,
            g.chosen,
            candidates.join(", ")
        );
        Warning {
            code: "number-format",
            message,
            index: None,
        }
    });
    problems.chain(guesses).collect()
}

/// The file without its error rows, which are added to `errors`.
fn without_errors(index: usize, mut file: QifFile, report: &ParseFieldsReport, errors: &mut Vec<RowError>) -> QifFile {
    let row = |line: usize, message: String| RowError {
        file: index,
        line,
        column: None,
        message,
    };
    let bad = |record: RecordKind| -> BTreeSet<usize> {
        report
            .errors
            .iter()
            .filter(|e| e.record == record && is_row_error(e))
            .filter_map(|e| e.line)
            .collect()
    };
    let (bad_xtns, bad_prices) = (bad(RecordKind::Transaction), bad(RecordKind::Price));
    errors.extend(unreadable_rows(index, &report.errors));
    errors.extend(
        file.xtns
            .iter()
            .filter(|x| !bad_xtns.contains(&x.line))
            .filter_map(|x| xtn_problem(x).map(|m| row(x.line, m))),
    );
    file.xtns.retain(|x| !bad_xtns.contains(&x.line) && xtn_problem(x).is_none());
    errors.extend(
        file.prices
            .iter()
            .filter(|p| !bad_prices.contains(&p.line) && !date_ok(&p.date))
            .map(|p| row(p.line, no_such_day(&p.date))),
    );
    file.prices.retain(|p| !bad_prices.contains(&p.line) && date_ok(&p.date));
    file
}

/// Why GnuCash would import the transaction wrongly: a day that does not exist, or an investment transaction with no
/// action.
fn xtn_problem(x: &QifXtn) -> Option<String> {
    if let Some(date) = &x.date
        && !date_ok(date)
    {
        return Some(no_such_day(date));
    }
    let investment = x.security_name.is_some();
    (investment && matches!(action(x), None | Some(Action::Unrecognized))).then(|| "Missing QIF investment action.".to_owned())
}

fn date_ok(date: &Field<super::super::parse::QifDate>) -> bool {
    date.value().is_some_and(|d| qif_date(d).is_some())
}

fn no_such_day(date: &Field<super::super::parse::QifDate>) -> String {
    match date.value() {
        Some(d) => format!("Invalid date: there is no day {}-{:02}-{:02}.", d.year, d.month, d.day),
        None => format!("Invalid date: {:?}.", date.text().unwrap_or_default()),
    }
}

/// The account for a file's transactions read under no `!Account`: the caller's, else from the file name.
fn default_account(file: &mut QifFile, input: &QifFileInput) {
    if !check_from_acct(file) {
        let name = input.default_account.clone().unwrap_or_else(|| file.path_to_accountname());
        fix_from_acct(file, &name);
    }
}
