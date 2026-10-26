//! The import pipeline every format shares: the Import proposal a preview builds, what applying it would do to the
//! Book ([`review`]), and applying it with the caller's changes as one Mutation ([`apply`]).
//!
//! Spec: `design/api/import.md`, "The pipeline". GnuCash's importers show their transactions in a window where the
//! user changes actions, matches and accounts, then apply them row by row; red-ale has no window, so a preview
//! returns the [`Proposal`] and `import.apply` takes it back by ID with [`Change`]s. Nothing here does I/O or holds
//! proposals: the service keeps them, by ID, against the Book revision they were made at.
//!
//! Each format builds its transactions its own way ([`crate::csv::build`] for CSV) and hands them to the matcher
//! ([`crate::matcher::match_batch`]); the proposal keeps the matcher's [`TransInfo`] for each, so applying runs the
//! matcher's own [`crate::matcher::apply_batch`].

pub mod apply;
pub mod review;

pub use apply::{Applied, ApplyChoices, Change, ImbalanceSplit, LearnedEntries, apply};
pub use review::{ImbalanceInfo, NewAccount, Review, TransactionReview, review};

use red_ale_core::{Date, Guid};

pub use crate::csv::accounts::ProposedAccount;
pub use crate::csv::prices::ProposedPrice;
use crate::matcher::{ImportSettings, TransInfo};

/// The format of an import (`design/api/import.md`, "Formats"). The formats still to come add variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// GnuCash's Import Transactions from CSV.
    CsvTransactions,
    /// GnuCash's Import Accounts from CSV.
    CsvAccounts,
    /// GnuCash's Import Prices from CSV.
    CsvPrices,
}

impl Format {
    /// The name `design/api/import.md` gives it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CsvTransactions => "csvTransactions",
            Self::CsvAccounts => "csvAccounts",
            Self::CsvPrices => "csvPrices",
        }
    }
}

/// Where in the import files something came from. Lines are numbered from 1; for CSV a line is a record, which a
/// quoted field may continue over several lines of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    /// The file's index among the import files.
    pub file: usize,
    /// The first line.
    pub first_line: usize,
    /// The last line.
    pub last_line: usize,
}

/// A row that can't be imported, with GnuCash's message. None of its records are in the proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowError {
    /// The file's index.
    pub file: usize,
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1, when the error is a column's.
    pub column: Option<usize>,
    /// What is wrong.
    pub message: String,
}

/// A line the reader skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoredLine {
    /// The file's index.
    pub file: usize,
    /// The line, from 1.
    pub line: usize,
    /// Why: `skipStartLines`, `skipEndLines` or `skipAltLines` for CSV; `headerRows` or `empty` for CSV accounts.
    pub reason: &'static str,
}

/// Something the caller should know that does not stop the import (`design/api/import.md`, "Warning").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// A stable code: `imbalance`, `read-only-by-date`, `missing-rate`, …
    pub code: &'static str,
    /// What happened, in words.
    pub message: String,
    /// The transaction's index, when the warning is about one.
    pub index: Option<usize>,
}

impl Warning {
    /// A warning about one transaction.
    pub fn about(index: usize, code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), index: Some(index) }
    }
}

/// One imported transaction as the matcher left it, and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedTransaction {
    /// The matcher's state for it: the transaction, its candidates, action and destination.
    pub info: TransInfo,
    /// Its lines.
    pub source: Source,
}

/// How an account name in a CSV file was resolved (`design/api/import.md`, "CSV transactions").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameSource {
    /// The caller's `accountNames`: GnuCash's account match page.
    Param,
    /// The `csv-account-map` Import map.
    Map,
    /// The account's full name.
    FullName,
}

impl NameSource {
    /// The name `design/api/import.md` gives it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Param => "param",
            Self::Map => "map",
            Self::FullName => "fullName",
        }
    }
}

/// One account name a CSV file uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountName {
    /// The name as the file writes it.
    pub name: String,
    /// The account it resolved to, if any.
    pub account: Option<Guid>,
    /// How.
    pub source: Option<NameSource>,
    /// What the map or the full name gave before the caller's choice: the account whose `csv-account-map` entry for
    /// this name GnuCash deletes when the user maps it elsewhere (`assistant-csv-trans-import.cpp:1916-1919`).
    pub previous: Option<Guid>,
}

/// What a CSV proposal keeps beyond the transactions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CsvState {
    /// Every account name the file uses, bytewise.
    pub account_names: Vec<AccountName>,
}

/// An Import proposal: everything a preview found, ready to apply with the caller's changes.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// The format.
    pub format: Format,
    /// The Import settings the matcher ran with; applying uses the same.
    pub settings: ImportSettings,
    /// The auto read-only threshold date at the preview, or `None` with no threshold.
    pub threshold: Option<Date>,
    /// Lines the reader skipped.
    pub ignored_lines: Vec<IgnoredLine>,
    /// Rows that can't be imported.
    pub errors: Vec<RowError>,
    /// Warnings about the whole import.
    pub warnings: Vec<Warning>,
    /// The transactions, by index.
    pub transactions: Vec<ProposedTransaction>,
    /// CSV's own state.
    pub csv: Option<CsvState>,
    /// The accounts of a CSV account import, by index.
    pub accounts: Vec<ProposedAccount>,
    /// The prices of a CSV price import, by index.
    pub prices: Vec<ProposedPrice>,
}

impl Proposal {
    /// A proposal of `format` with nothing in it yet.
    pub fn empty(format: Format, settings: ImportSettings, threshold: Option<Date>) -> Self {
        Self {
            format,
            settings,
            threshold,
            ignored_lines: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            transactions: Vec::new(),
            csv: None,
            accounts: Vec::new(),
            prices: Vec::new(),
        }
    }

    /// The transaction with this index.
    pub fn transaction(&self, index: usize) -> Option<&ProposedTransaction> {
        self.transactions.iter().find(|t| t.info.index == index)
    }
}
