//! red-ale's CSV exports: the account tree and transactions, as GnuCash 5.17's CSV export assistant writes them
//! (`design/api/export.md`).
//!
//! The exporters are translated from `5.17:gnucash/import-export/csv-exp/`: the line writer from
//! `csv-export-helpers.cpp`, the account tree from `csv-tree-export.cpp` and the transactions from
//! `csv-transactions-export.cpp`. This crate is that bounded context. It does no I/O and only reads the Book: the
//! text is returned, and the caller writes it where it wants.
//!
//! - [`line`]: one CSV line, quoted as GnuCash quotes it.
//! - [`format`]: how numbers and dates are printed; the one place the Locale plugs in.
//! - [`tree`]: the account tree.
//! - [`transactions`]: the transactions of some accounts, in the simple or complex layout.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod format;
pub mod line;
pub mod transactions;
pub mod tree;

use red_ale_core::{DomainError, DomainResult};

pub use format::Formats;
pub use transactions::{Layout, TransactionsCsv, TransactionsRequest, export_transactions};
pub use tree::{TreeCsv, export_tree};

/// How fields are joined and quoted: the separator and "Use quotes" choices of GnuCash's assistant
/// (`5.17:gnucash/import-export/csv-exp/assistant-csv-export.c:203-315`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvOptions {
    separator: String,
    quotes: bool,
}

impl CsvOptions {
    /// Options with a separator, which may be any non-empty string as GnuCash's custom separator allows, and whether
    /// every field is quoted. An empty separator is `invalid`.
    pub fn new(separator: impl Into<String>, quotes: bool) -> DomainResult<Self> {
        let separator = separator.into();
        if separator.is_empty() {
            return Err(DomainError::invalid("the separator may not be empty"));
        }
        Ok(Self { separator, quotes })
    }

    /// The separator.
    pub fn separator(&self) -> &str {
        &self.separator
    }

    /// Whether every field is quoted.
    pub fn quotes(&self) -> bool {
        self.quotes
    }

    /// Append one line of `fields` to `out` (see [`line::add_line`]).
    pub fn add_line<S: AsRef<str>>(&self, out: &mut String, fields: &[S]) {
        line::add_line(out, fields, self.quotes, &self.separator);
    }
}

impl Default for CsvOptions {
    /// A comma and no forced quotes: the assistant's defaults.
    fn default() -> Self {
        Self { separator: ",".to_owned(), quotes: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_separator_is_refused() {
        assert!(matches!(CsvOptions::new("", false), Err(DomainError::Invalid(_))));
        let options = CsvOptions::new("::", true).unwrap();
        assert_eq!((options.separator(), options.quotes()), ("::", true));
        assert_eq!(CsvOptions::default(), CsvOptions::new(",", false).unwrap());
    }
}
