//! The QIF reader, translated from GnuCash's Scheme (`5.17:gnucash/import-export/qif-imp/`).
//!
//! One module per `.scm` file, each keeping its source's copyright lines and citing the lines it follows:
//!
//! - [`objects`]: the records (`qif-objects.scm`);
//! - [`file`]: reading a file into records and parsing their fields (`qif-file.scm`);
//! - [`parse`]: the field parsers (`qif-parse.scm`);
//! - [`string`]: the string helpers they use (`string.scm`);
//! - [`numeric_string`]: `gnc-numeric-from-string`, which price lines are read with
//!   (`5.17:libgnucash/engine/gnc-numeric.cpp`).
//!
//! `qif-utils.scm` holds only the progress dialog's pause and cancel state and a log function; the reader returns
//! its warnings instead, so nothing of it is translated.
//!
//! Converting the records to GnuCash transactions (`qif-to-gnc.scm`), finding duplicates (`qif-merge-groups.scm`)
//! and the QIF account map (`qif-guess-map.scm`, `qif-dialog-utils.scm`) build on these records.

pub mod file;
pub mod numeric_string;
pub mod objects;
pub mod parse;
pub mod string;

pub use file::{
    FieldError, FieldKind, FieldProblem, Format, FormatGuess, ParseFieldsError, ParseFieldsReport, ReadError, ReadOptions, ReadResult,
    ReadWarning, ReadWarningKind, RecordKind, check_from_acct, parse_fields, read_file, reparse_dates,
};
pub use objects::{Field, QifAcct, QifCat, QifClass, QifFile, QifMapEntry, QifPrice, QifSplit, QifStockSymbol, QifXtn, TickerMap};
pub use parse::{Action, Cleared, DateFormat, NumberFormat, QifDate};
