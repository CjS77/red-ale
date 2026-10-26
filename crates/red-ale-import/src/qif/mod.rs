//! The QIF reader, translated from GnuCash's Scheme (`5.17:gnucash/import-export/qif-imp/`).
//!
//! One module per `.scm` file, each keeping its source's copyright lines and citing the lines it follows:
//!
//! - [`objects`]: the records (`qif-objects.scm`);
//! - [`file`]: reading a file into records and parsing their fields (`qif-file.scm`);
//! - [`parse`]: the field parsers (`qif-parse.scm`);
//! - [`string`]: the string helpers they use (`string.scm`);
//! - [`numeric_string`]: `gnc-numeric-from-string`, which price lines are read with
//!   (`5.17:libgnucash/engine/gnc-numeric.cpp`);
//! - [`guess_map`]: the QIF account map, its guesses and its saved form (`qif-guess-map.scm`);
//! - [`dialog_utils`]: the account, category and payee displays and the security choices, without the dialogs
//!   (`qif-dialog-utils.scm`);
//! - [`to_gnc`]: converting the records to accounts, transactions and prices (`qif-to-gnc.scm`);
//! - [`merge_groups`]: QIF's own duplicate finder (`qif-merge-groups.scm`);
//! - [`import`]: the assistant's steps, without its pages, as one preview and one apply.
//!
//! `qif-utils.scm` holds only the progress dialog's pause and cancel state and a log function; the reader returns
//! its warnings instead, so nothing of it is translated.

pub mod dialog_utils;
pub mod file;
pub mod guess_map;
pub mod import;
pub mod merge_groups;
pub mod numeric_string;
pub mod objects;
pub mod parse;
pub mod string;
pub mod to_gnc;

pub use file::{
    FieldError, FieldKind, FieldProblem, Format, FormatGuess, ParseFieldsError, ParseFieldsReport, ReadError, ReadOptions, ReadResult,
    ReadWarning, ReadWarningKind, RecordKind, check_from_acct, parse_fields, read_file, reparse_dates, reparse_price_dates,
};
pub use objects::{Field, QifAcct, QifCat, QifClass, QifFile, QifMapEntry, QifPrice, QifSplit, QifStockSymbol, QifXtn, TickerMap};
pub use parse::{Action, Cleared, DateFormat, NumberFormat, QifDate};
