//! Methods: opening, creating, saving and closing a book, and what the book declares about itself.

use red_ale_core::book_options::{GNUCASH_FEATURES, features};
use red_ale_service::session::SUPPORTED_FEATURES;
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
            "Open an XML or SQLite book. SQLite books open read-only for now, and some XML books do too (see readOnlyReasons).",
            Effect::Read,
            open,
        ),
        method("book.create", "Create a new, empty XML book and open it.", Effect::Write, create),
        method(
            "book.info",
            "Describe the open book: path, read-only state, features and counts.",
            Effect::Read,
            info,
        ),
        method(
            "book.save",
            "Save the open book, backing up the previous file first.",
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

/// Params of `book.create`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateParams {
    /// A path or `xml://` URI that does not exist yet.
    pub path: String,
    /// Write it gzip-compressed (GnuCash's default). Defaults to the server's setting.
    #[serde(default)]
    pub compressed: Option<bool>,
}

/// Params of `book.save`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveParams {
    /// Change the file's compression; by default it is kept.
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
    /// `requested`, `unsupported-features`, `unrecognised-content`, `schema-version` or `writing-not-supported`.
    pub code: &'static str,
    /// Explanation.
    pub message: String,
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
    /// The file is gzip-compressed.
    pub compressed: bool,
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
    /// The file was written gzip-compressed.
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

fn create(ctx: &mut Context, p: CreateParams) -> Result<BookInfo, RpcError> {
    ctx.require_no_session()?;
    let mut settings = ctx.settings.clone();
    if let Some(c) = p.compressed {
        settings.compress_new_books = c;
    }
    let session = Session::create(&p.path, settings)?;
    Ok(describe(ctx.install(session)?))
}

fn info(ctx: &mut Context, _: NoParams) -> Result<BookInfo, RpcError> {
    Ok(describe(ctx.session()?))
}

fn save(ctx: &mut Context, p: SaveParams) -> Result<SaveResult, RpcError> {
    let report = ctx.session()?.save(p.compressed)?;
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
