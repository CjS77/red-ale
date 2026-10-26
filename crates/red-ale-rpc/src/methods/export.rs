//! Methods: exports (`design/api/export.md`): the account tree and transactions as CSV text, and Export accounts.
//!
//! Exports only read. The CSV methods reach the Book through [`Context::query`] and return their text, writing no
//! file; `export.accounts` writes a new book file from the open Book and leaves the Book, its dirty state and its
//! journal as they were. All three work on a book open read-only. The rules live in `red_ale_export` and
//! `red_ale_service::export`; this module translates wire types.

use red_ale_export::{CsvOptions, Formats, Layout, TransactionsRequest, export_transactions, export_tree};
use red_ale_service::export::export_accounts;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Day};

/// The export methods.
pub fn methods() -> Vec<Method> {
    vec![
        method("export.csvTree", "The account tree as CSV text, as GnuCash's CSV export writes it. Writes no file.", Effect::Read, csv_tree),
        method(
            "export.csvTransactions",
            "Transactions of some accounts and dates as CSV text, in GnuCash's simple or complex layout. Writes no file.",
            Effect::Read,
            csv_transactions,
        ),
        method(
            "export.accounts",
            "Write a new book file holding the open book's account tree and commodities, and no transactions. Never overwrites a file.",
            Effect::Read,
            accounts,
        ),
    ]
}

fn comma() -> String {
    ",".to_owned()
}

/// Params of `export.csvTree`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CsvTreeParams {
    /// The field separator: any non-empty string.
    #[serde(default = "comma")]
    pub separator: String,
    /// Quote every field. Without it, a field is quoted only when it holds the separator, a quote or a line break.
    #[serde(default)]
    pub quotes: bool,
}

/// The result of `export.csvTree`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CsvTreeResult {
    /// The CSV text: UTF-8, every line ending in CRLF, the header first.
    pub csv: String,
    /// Account lines, the header not counted.
    pub rows: usize,
}

/// The layout of `export.csvTransactions`.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LayoutParam {
    /// One line per transaction, for its split in the exported account.
    Simple,
    /// One line per split, every split of the transaction.
    #[default]
    Complex,
}

/// Params of `export.csvTransactions`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CsvTransactionsParams {
    /// The accounts, at least one, in the order to process them; a transaction goes under the first one it has a split
    /// in. Sub-accounts are not included unless listed.
    pub accounts: Vec<AccountRef>,
    /// Posted on or after local 00:00:00 of this date; by default the earliest.
    #[serde(default)]
    pub from: Option<Day>,
    /// Posted on or before local 23:59:59 of this date; by default the latest.
    #[serde(default)]
    pub to: Option<Day>,
    /// `simple` or `complex` (the default).
    #[serde(default)]
    pub layout: LayoutParam,
    /// The field separator: any non-empty string.
    #[serde(default = "comma")]
    pub separator: String,
    /// Quote every field.
    #[serde(default)]
    pub quotes: bool,
    /// The Locale to format numbers and dates in: a supported POSIX locale name (`fr_FR`, `C`) or `environment`; by
    /// default the Configuration's `locale` setting.
    #[serde(default)]
    pub locale: Option<String>,
}

/// The result of `export.csvTransactions`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CsvTransactionsResult {
    /// The CSV text: UTF-8, every line ending in CRLF, the header first.
    pub csv: String,
    /// Transactions written.
    pub transactions: usize,
    /// Lines written, the header not counted.
    pub lines: usize,
}

/// Params of `export.accounts`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AccountsParams {
    /// The new book file: a path or `xml://` URI that does not exist. `.gnucash` is added when missing.
    pub path: String,
}

/// The result of `export.accounts`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountsResult {
    /// The file written, uncompressed.
    pub path: String,
    /// Account elements written, the root account's included.
    pub accounts: usize,
    /// Commodity elements written.
    pub commodities: usize,
}

fn csv_tree(ctx: &mut Context, p: CsvTreeParams) -> Result<CsvTreeResult, RpcError> {
    let options = CsvOptions::new(p.separator, p.quotes)?;
    let tree = export_tree(ctx.book()?, &options);
    Ok(CsvTreeResult { csv: tree.csv, rows: tree.rows })
}

fn csv_transactions(ctx: &mut Context, p: CsvTransactionsParams) -> Result<CsvTransactionsResult, RpcError> {
    let options = CsvOptions::new(p.separator, p.quotes)?;
    let config = ctx.settings.config.load()?;
    let date_format = super::reports::settings(&config)?.date_format;
    let locale = match p.locale.as_deref() {
        Some(name) => red_ale_service::locale::resolve(name).map_err(|e| RpcError::invalid_params(e.to_string()))?,
        None => config.locale()?,
    };
    let formats = Formats::new(date_format, locale);
    let layout = match p.layout {
        LayoutParam::Simple => Layout::Simple,
        LayoutParam::Complex => Layout::Complex,
    };
    ctx.query(|book, env| {
        let accounts = p.accounts.iter().map(|a| a.resolve(book)).collect::<Result<Vec<_>, _>>()?;
        let request = TransactionsRequest { accounts, from: p.from.map(|d| d.0), to: p.to.map(|d| d.0), layout };
        let out = export_transactions(book, env.zone(), &formats, &options, &request)?;
        Ok(CsvTransactionsResult { csv: out.csv, transactions: out.transactions, lines: out.lines })
    })
}

fn accounts(ctx: &mut Context, p: AccountsParams) -> Result<AccountsResult, RpcError> {
    let written = export_accounts(ctx.session()?, &p.path)?;
    Ok(AccountsResult { path: written.path.display().to_string(), accounts: written.counts.accounts, commodities: written.counts.commodities })
}
