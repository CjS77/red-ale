//! The CSV account import's preview: GnuCash's account tree export read against the open Book
//! (`design/api/import.md`, "CSV accounts").

use red_ale_core::Book;
use red_ale_import::csv::accounts::{self, Separator};
use red_ale_import::matcher::ImportSettings;
use red_ale_import::pipeline::{Format, Proposal};

use super::file::{ImportFile, decode};
use crate::ServiceError;
use crate::config::invalid;

/// The params of a `csvAccounts` preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccountsParams {
    /// The separator between fields.
    pub separator: Separator,
    /// Header records to leave out; `None` detects one as GnuCash does.
    pub header_rows: Option<usize>,
}

/// Read the file and build the proposal. The file is UTF-8 unless it names another encoding.
pub(super) fn propose(book: &Book, files: &[ImportFile], params: &AccountsParams, settings: ImportSettings) -> Result<Proposal, ServiceError> {
    let [file] = files else { return Err(invalid(format!("csvAccounts takes exactly one file, not {}", files.len()))) };
    let text = decode(0, file, "utf-8")?;
    let read = accounts::read(&text, params.separator, params.header_rows, 0);
    if read.rows.is_empty() && read.errors.is_empty() {
        let message = "the file has no account rows".to_owned();
        return Err(ServiceError::ImportFile { file: 0, line: None, reason: "columns", message });
    }
    let (records, mut errors) = accounts::propose(book, &read.rows, 0);
    errors.extend(read.errors);
    errors.sort_by_key(|e| e.line);
    Ok(Proposal { ignored_lines: read.ignored, errors, warnings: read.warnings, accounts: records, ..Proposal::empty(Format::CsvAccounts, settings, None) })
}
