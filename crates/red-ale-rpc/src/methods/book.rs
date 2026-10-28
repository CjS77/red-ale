//! Methods: opening, creating, saving and closing a book, and what the book declares about itself.

use red_ale_core::book_options::{GNUCASH_FEATURES, features};
use red_ale_service::session::SUPPORTED_FEATURES;
use red_ale_service::backend::Format;
use red_ale_service::{ReadOnlyReason, Session};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::{RpcError, codes};
use crate::wire::{Id, NoParams};

/// The book methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "book.open",
            "Open an XML or SQLite book. Some books open read-only (see readOnlyReasons).",
            Effect::Read,
            open,
        ),
        method("book.create", "Create a new, empty XML or SQLite book and open it.", Effect::Write, create),
        method(
            "book.info",
            "Describe the open book: path, read-only state, features and counts.",
            Effect::Read,
            info,
        ),
        method(
            "book.save",
            "Save the open book, backing up the previous file first (for an SQLite book, before the session's first save).",
            Effect::Write,
            save,
        ),
        method("book.close", "Close the open book and release its lock.", Effect::Write, close),
        method(
            "book.replayJournal",
            "Replay a GnuCash .log journal onto the open book: create, update or delete transactions by GUID.",
            Effect::Write,
            replay_journal,
        ),
    ]
}

/// Params of `book.open`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OpenParams {
    /// A path, or an `xml://`, `sqlite3://` or `file://` URI.
    pub path: String,
    /// Open without taking the lock; nothing can be changed.
    #[serde(default)]
    pub read_only: bool,
}

/// A book file's format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BookFormat {
    /// GnuCash's XML, gzipped or plain.
    Xml,
    /// An SQLite database.
    Sqlite,
}

impl From<BookFormat> for Format {
    fn from(f: BookFormat) -> Self {
        match f {
            BookFormat::Xml => Self::Xml,
            BookFormat::Sqlite => Self::Sqlite,
        }
    }
}

impl From<Format> for BookFormat {
    fn from(f: Format) -> Self {
        match f {
            Format::Xml => Self::Xml,
            Format::Sqlite => Self::Sqlite,
        }
    }
}

/// Params of `book.create`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateParams {
    /// A path, or an `xml://` or `sqlite3://` URI, that does not exist yet.
    pub path: String,
    /// XML only: write it gzip-compressed (GnuCash's default). Defaults to the server's setting.
    #[serde(default)]
    pub compressed: Option<bool>,
    /// `xml` or `sqlite`. Defaults to the one a `sqlite3://` or `xml://` URI names, else `xml`.
    #[serde(default)]
    pub format: Option<BookFormat>,
}

/// Params of `book.save`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveParams {
    /// XML only: change the file's compression; by default it is kept.
    #[serde(default)]
    pub compressed: Option<bool>,
}

/// Params of `book.close`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CloseParams {
    /// Save unsaved changes first.
    #[serde(default)]
    pub save: bool,
    /// Discard unsaved changes. Without `save` or `discard`, closing a book with unsaved changes is refused.
    #[serde(default)]
    pub discard: bool,
}

/// Params of `book.replayJournal`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReplayParams {
    /// The journal: a `<book>.<timestamp>.log` file.
    pub path: String,
    /// Skip records that cannot be applied (for example an unbalanced transaction) instead of refusing the replay.
    #[serde(default)]
    pub skip_invalid: bool,
}

/// What `book.replayJournal` did.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReplayResult {
    /// Records read.
    pub records: usize,
    /// Transactions created.
    pub created: Vec<Id>,
    /// Transactions updated.
    pub updated: Vec<Id>,
    /// Transactions deleted.
    pub deleted: Vec<Id>,
    /// Deletions of transactions the book does not have.
    pub not_found: Vec<Id>,
    /// Records skipped, with why.
    pub skipped_records: Vec<String>,
    /// Lines that could not be read, as `line N: reason`.
    pub skipped_lines: Vec<String>,
}

/// Why a book is read-only.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ReadOnlyInfo {
    /// `requested`, `unsupported-features`, `unrecognised-content` or `schema-version`.
    pub code: &'static str,
    /// Explanation.
    pub message: String,
}

/// A `versions` row of an SQLite book: a table, or `Gnucash` and `Gnucash-Resave`, with its version.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    /// The table name.
    pub table: String,
    /// Its version.
    pub version: i64,
}

/// A Book feature the book declares.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FeatureInfo {
    /// The name GnuCash stores.
    pub name: String,
    /// The description stored with it.
    pub description: String,
    /// GnuCash 5.17 knows it.
    pub known: bool,
    /// red-ale can edit a book that uses it.
    pub supported: bool,
}

/// How many of each entity the book holds.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct Counts {
    pub commodities: usize,
    pub accounts: usize,
    pub transactions: usize,
    pub prices: usize,
    pub lots: usize,
    pub customers: usize,
    pub vendors: usize,
    pub employees: usize,
    pub jobs: usize,
    pub invoices: usize,
    pub orders: usize,
    pub tax_tables: usize,
    pub bill_terms: usize,
}

/// The result of `book.open`, `book.create` and `book.info`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BookInfo {
    /// The book's file.
    pub path: String,
    /// The book's GUID.
    pub guid: Id,
    /// The root account's GUID.
    pub root_account: Id,
    /// Nothing can be changed.
    pub read_only: bool,
    /// Why, when read-only.
    pub read_only_reasons: Vec<ReadOnlyInfo>,
    /// There are unsaved changes.
    pub dirty: bool,
    /// The file is gzip-compressed; always false for an SQLite book.
    pub compressed: bool,
    /// The file's format: `xml` or `sqlite`.
    pub format: BookFormat,
    /// SQLite books only: the `versions` table as read, in its order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versions: Option<Vec<VersionInfo>>,
    /// The transaction journal being written, if any.
    pub journal: Option<String>,
    /// Book features the book declares.
    pub features: Vec<FeatureInfo>,
    /// Entity counts.
    pub counts: Counts,
}

/// What `book.save` did.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    /// The backup of the previous file.
    pub backup: Option<String>,
    /// Old backups and journals removed by retention.
    pub removed: Vec<String>,
    /// The file was written gzip-compressed; always false for an SQLite book.
    pub compressed: bool,
}

/// What `book.close` did.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CloseResult {
    /// The book was saved before closing.
    pub saved: bool,
    /// Unsaved changes were discarded.
    pub discarded: bool,
}

fn reason_info(r: &ReadOnlyReason) -> ReadOnlyInfo {
    ReadOnlyInfo {
        code: r.code(),
        message: r.to_string(),
    }
}

/// Describe a session.
pub fn describe(session: &Session) -> BookInfo {
    let book = session.book();
    let b = &book.business;
    BookInfo {
        path: session.path().display().to_string(),
        guid: book.guid.into(),
        root_account: book.root.into(),
        read_only: session.is_read_only(),
        read_only_reasons: session.read_only_reasons().iter().map(reason_info).collect(),
        dirty: session.is_dirty(),
        compressed: session.compressed().unwrap_or(false),
        format: session.format().into(),
        versions: session
            .versions()
            .map(|rows| rows.iter().map(|(table, version)| VersionInfo { table: table.clone(), version: *version }).collect()),
        journal: session.journal_path().map(|p| p.display().to_string()),
        features: features(book)
            .into_iter()
            .map(|(name, description)| FeatureInfo {
                known: GNUCASH_FEATURES.iter().any(|(n, _)| *n == name),
                supported: SUPPORTED_FEATURES.contains(&name.as_str()),
                name,
                description,
            })
            .collect(),
        counts: Counts {
            commodities: book.commodities.len(),
            accounts: book.accounts.len(),
            transactions: book.transactions.len(),
            prices: book.prices.len(),
            lots: book.lots.len(),
            customers: b.customers.len(),
            vendors: b.vendors.len(),
            employees: b.employees.len(),
            jobs: b.jobs.len(),
            invoices: b.invoices.len(),
            orders: b.orders.len(),
            tax_tables: b.tax_tables.len(),
            bill_terms: b.bill_terms.len(),
        },
    }
}

fn open(ctx: &mut Context, p: OpenParams) -> Result<BookInfo, RpcError> {
    ctx.require_no_session()?;
    let session = Session::open(&p.path, p.read_only, ctx.settings.clone())?;
    Ok(describe(ctx.install(session)?))
}

/// The format `book.create` makes: the one asked for, which a `sqlite3://` or `xml://` URI must agree with, else the
/// URI's, else XML (`design/sql-fidelity.md` §7).
fn create_format(p: &CreateParams) -> Result<Format, RpcError> {
    let scheme = Format::of_scheme(&p.path);
    let format = match (p.format.map(Format::from), scheme) {
        (Some(asked), Some(named)) if asked != named => {
            return Err(RpcError::new(codes::INVALID_PARAMS, format!("format {} contradicts the path's URI scheme", asked.name())));
        }
        (asked, named) => asked.or(named).unwrap_or(Format::Xml),
    };
    if format == Format::Sqlite && p.compressed.is_some() {
        return Err(RpcError::new(codes::INVALID_PARAMS, "compressed applies to XML books only"));
    }
    Ok(format)
}

fn create(ctx: &mut Context, p: CreateParams) -> Result<BookInfo, RpcError> {
    ctx.require_no_session()?;
    let format = create_format(&p)?;
    let mut settings = ctx.settings.clone();
    if let Some(c) = p.compressed {
        settings.compress_new_books = c;
    }
    let session = Session::create_as(&p.path, format, settings)?;
    Ok(describe(ctx.install(session)?))
}

fn info(ctx: &mut Context, _: NoParams) -> Result<BookInfo, RpcError> {
    Ok(describe(ctx.session()?))
}

fn save(ctx: &mut Context, p: SaveParams) -> Result<SaveResult, RpcError> {
    let session = ctx.session()?;
    if session.format() == Format::Sqlite && p.compressed.is_some() {
        return Err(RpcError::new(codes::INVALID_PARAMS, "compressed applies to XML books only"));
    }
    let report = session.save(p.compressed)?;
    let show = |p: &std::path::PathBuf| p.display().to_string();
    Ok(SaveResult {
        backup: report.backup.as_ref().map(show),
        removed: report.removed.iter().map(show).collect(),
        compressed: report.compressed.unwrap_or(false),
    })
}

fn close(ctx: &mut Context, p: CloseParams) -> Result<CloseResult, RpcError> {
    let dirty = ctx.session()?.is_dirty();
    if dirty && !p.save && !p.discard {
        return Err(RpcError::new(
            codes::UNSAVED_CHANGES,
            "the book has unsaved changes; pass save or discard",
        ));
    }
    let save = dirty && p.save;
    // Save while the session is still held, so a failed save keeps the book open with its changes.
    if save {
        ctx.session()?.save(None)?;
    }
    ctx.take_session()?.close(false)?;
    Ok(CloseResult {
        saved: save,
        discarded: dirty && !p.save,
    })
}

fn replay_journal(ctx: &mut Context, p: ReplayParams) -> Result<ReplayResult, RpcError> {
    let r = ctx.session()?.replay_journal(std::path::Path::new(&p.path), p.skip_invalid)?;
    let ids = |v: &[red_ale_core::Guid]| v.iter().copied().map(Id).collect();
    Ok(ReplayResult {
        records: r.records,
        created: ids(&r.created),
        updated: ids(&r.updated),
        deleted: ids(&r.deleted),
        not_found: ids(&r.not_found),
        skipped_records: r.skipped_records,
        skipped_lines: r.skipped_lines.iter().map(|l| format!("line {}: {}", l.line, l.reason)).collect(),
    })
}
