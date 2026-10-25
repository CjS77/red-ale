//! A Session: one open book file with its lock, journal and read-only state.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use red_ale_core::book_options::{GNUCASH_FEATURES, OBSOLETE_BOOK_CURRENCY, auto_readonly_days, features, uses_trading_accounts};
use red_ale_core::{Book, DomainError, Env, Guid, Time64, Transaction};
use red_ale_xml::{XmlResidue, read_book, write_book};

use crate::error::ServiceError;
use crate::files::{Retention, remove_old_files, write_book_file};
use crate::journal::{Journal, RecordKind, record_lines};
use crate::lock::BookLock;

/// Book features v0.1 can edit safely: their data lives in slots or files red-ale keeps unchanged. Credit notes and
/// the num-field source are not implemented, so books declaring them open read-only.
pub const SUPPORTED_FEATURES: [&str; 8] = [
    "Extra data in addresses, jobs or invoice entries",
    "Account GUID based Bayesian data",
    "Account GUID based bayesian with flat KVP",
    "ISO-8601 formatted date strings in SQLite3 databases.",
    "Register sort and filter settings stored in .gcm file",
    "Use natural signs in budget amounts",
    "Show extra account columns in the Budget View",
    "Use a dedicated opening balance account identified by an 'equity-type' slot",
];

/// Session settings, from red-ale's configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// How long backups and old journals are kept.
    pub retention: Retention,
    /// Whether new books are gzip-compressed (GnuCash's default is yes).
    pub compress_new_books: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { retention: Retention::default(), compress_new_books: true }
    }
}

/// Why a session is read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOnlyReason {
    /// The caller asked for it.
    Requested,
    /// The book declares Book features v0.1 does not implement.
    UnsupportedFeatures(Vec<String>),
    /// The book balances transactions through trading accounts.
    TradingAccounts,
    /// The book makes transactions older than this many days read-only.
    AutoReadOnly(i64),
    /// The file holds content red-ale can keep but not edit safely.
    UnrecognisedContent(Vec<String>),
}

impl ReadOnlyReason {
    /// A stable machine-readable name.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::UnsupportedFeatures(_) => "unsupported-features",
            Self::TradingAccounts => "trading-accounts",
            Self::AutoReadOnly(_) => "auto-read-only",
            Self::UnrecognisedContent(_) => "unrecognised-content",
        }
    }
}

impl fmt::Display for ReadOnlyReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Requested => write!(f, "opened read-only on request"),
            Self::UnsupportedFeatures(list) => write!(f, "the book uses features red-ale v0.1 does not support: {}", list.join("; ")),
            Self::TradingAccounts => write!(f, "the book uses trading accounts, which red-ale v0.1 does not support"),
            Self::AutoReadOnly(days) => write!(f, "the book makes transactions older than {days} days read-only, which red-ale v0.1 does not support"),
            Self::UnrecognisedContent(list) => write!(f, "the book contains data red-ale cannot edit safely: {}", list.join("; ")),
        }
    }
}

/// The read-only triggers a book sets off.
pub fn read_only_triggers(book: &Book, residue: &XmlResidue) -> Vec<ReadOnlyReason> {
    let unsupported: Vec<String> = features(book)
        .into_iter()
        .filter(|(name, _)| name != OBSOLETE_BOOK_CURRENCY && !SUPPORTED_FEATURES.contains(&name.as_str()))
        .map(|(name, description)| if description.is_empty() { name } else { description })
        .collect();
    let mut reasons = Vec::new();
    if !unsupported.is_empty() {
        reasons.push(ReadOnlyReason::UnsupportedFeatures(unsupported));
    }
    if uses_trading_accounts(book) {
        reasons.push(ReadOnlyReason::TradingAccounts);
    }
    let days = auto_readonly_days(book);
    if days != 0 {
        reasons.push(ReadOnlyReason::AutoReadOnly(days));
    }
    if !residue.notices.unrecognised.is_empty() {
        reasons.push(ReadOnlyReason::UnrecognisedContent(residue.notices.unrecognised.clone()));
    }
    reasons
}

/// GnuCash's known feature names, for reporting.
pub fn gnucash_feature_names() -> impl Iterator<Item = &'static str> {
    GNUCASH_FEATURES.iter().map(|(n, _)| *n)
}

/// The real [`Env`]: the system clock and random GUIDs.
#[derive(Debug, Default)]
pub struct SystemEnv;

impl Env for SystemEnv {
    fn now(&mut self) -> Time64 {
        Time64(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX)))
    }

    fn new_guid(&mut self) -> Guid {
        let mut bytes = [0_u8; 16];
        // Random GUIDs, as GnuCash's `guid_replace` makes them. A failing system RNG is not recoverable.
        if getrandom::fill(&mut bytes).is_err() {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
            bytes = (nanos ^ u128::from(std::process::id()) << 64).to_be_bytes();
        }
        Guid::from_bytes(bytes)
    }
}

/// Turn a path or `xml://` / `file://` URI into a path, refusing SQL books and other schemes before anything is
/// opened.
pub fn resolve_path(spec: &str) -> Result<PathBuf, ServiceError> {
    let lower = spec.to_ascii_lowercase();
    let path = if lower.starts_with("xml://") || lower.starts_with("file://") {
        &spec[spec.find("://").map_or(0, |i| i + 3)..]
    } else if ["sqlite3://", "postgres://", "postgresql://", "mysql://"].iter().any(|s| lower.starts_with(s)) {
        return Err(ServiceError::SqlBook { path: spec.to_owned() });
    } else if spec.contains("://") {
        return Err(ServiceError::UnsupportedUri { uri: spec.to_owned() });
    } else {
        spec
    };
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir().map(|d| d.join(&path)).map_err(|e| ServiceError::Io { path, source: e })
    }
}

/// True when the file starts with SQLite's header.
fn is_sqlite(bytes: &[u8]) -> bool {
    bytes.starts_with(b"SQLite format 3\0")
}

/// An open book.
#[derive(Debug)]
pub struct Session {
    path: PathBuf,
    book: Book,
    residue: XmlResidue,
    lock: Option<BookLock>,
    read_only: Vec<ReadOnlyReason>,
    journal: Journal,
    dirty: bool,
    settings: Settings,
    env: Box<dyn EnvDebug>,
}

/// An [`Env`] that can be stored in a session.
pub trait EnvDebug: Env + fmt::Debug + Send {}
impl<T: Env + fmt::Debug + Send> EnvDebug for T {}

/// What a save did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReport {
    /// The backup of the previous file, if there was one.
    pub backup: Option<PathBuf>,
    /// Old backups and journals deleted by retention.
    pub removed: Vec<PathBuf>,
    /// Whether the file was written gzip-compressed.
    pub compressed: bool,
}

impl Session {
    /// Open an existing book. Unless `read_only`, take the lock first; release it again if the book turns out to be
    /// read-only.
    pub fn open(spec: &str, read_only: bool, settings: Settings) -> Result<Self, ServiceError> {
        let path = resolve_path(spec)?;
        let bytes = std::fs::read(&path).map_err(|e| ServiceError::Io { path: path.clone(), source: e })?;
        if is_sqlite(&bytes) {
            return Err(ServiceError::SqlBook { path: path.display().to_string() });
        }
        let lock = if read_only { None } else { Some(BookLock::acquire(&path)?) };
        // Re-read under the lock so no other writer can have changed the file in between.
        let bytes = if lock.is_some() { std::fs::read(&path).map_err(|e| ServiceError::Io { path: path.clone(), source: e })? } else { bytes };
        let (book, residue) = read_book(&bytes).map_err(|e| ServiceError::Xml { path: path.clone(), source: e })?;
        let mut reasons = read_only_triggers(&book, &residue);
        if read_only {
            reasons.insert(0, ReadOnlyReason::Requested);
        }
        let lock = match lock {
            Some(held) if !reasons.is_empty() => {
                held.release()?;
                None
            }
            other => other,
        };
        Ok(Self { journal: Journal::new(&path), path, book, residue, lock, read_only: reasons, dirty: false, settings, env: Box::new(SystemEnv) })
    }

    /// Create a new, empty book file, taking its lock. Fails if the file exists.
    pub fn create(spec: &str, settings: Settings) -> Result<Self, ServiceError> {
        let path = resolve_path(spec)?;
        if path.exists() {
            return Err(ServiceError::Exists { path });
        }
        let lock = BookLock::acquire(&path)?;
        let mut env = SystemEnv;
        let book = Book::new_empty(&mut env);
        let residue = XmlResidue::new_book(&book, settings.compress_new_books);
        let mut session =
            Self { journal: Journal::new(&path), path, book, residue, lock: Some(lock), read_only: Vec::new(), dirty: true, settings, env: Box::new(env) };
        session.save(None)?;
        Ok(session)
    }

    /// Replace the environment (clock and GUID source), for deterministic tests.
    pub fn set_env(&mut self, env: impl EnvDebug + 'static) {
        self.env = Box::new(env);
    }

    /// The book's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The book as it stands.
    pub fn book(&self) -> &Book {
        &self.book
    }

    /// The file-format residue.
    pub fn residue(&self) -> &XmlResidue {
        &self.residue
    }

    /// Why the session is read-only; empty when it is writable.
    pub fn read_only_reasons(&self) -> &[ReadOnlyReason] {
        &self.read_only
    }

    /// True when the session refuses mutations.
    pub fn is_read_only(&self) -> bool {
        !self.read_only.is_empty()
    }

    /// True when there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The path of the journal being written, if any.
    pub fn journal_path(&self) -> Option<&Path> {
        self.journal.current_path()
    }

    /// The current time from the session's environment.
    pub fn now(&mut self) -> Time64 {
        self.env.now()
    }

    /// Read-only access to the book with the environment, for queries that need the clock.
    pub fn query<T>(&mut self, f: impl FnOnce(&Book, &mut dyn Env) -> T) -> T {
        f(&self.book, &mut *self.env)
    }

    /// Apply a Mutation atomically: `f` edits a copy of the book; the copy replaces the book only if `f` succeeds and
    /// every changed transaction is balanced and refers to existing accounts. Transaction changes are journaled.
    pub fn mutate<T>(&mut self, f: impl FnOnce(&mut Book, &mut dyn Env) -> Result<T, DomainError>) -> Result<T, ServiceError> {
        if let Some(reason) = self.read_only.first() {
            return Err(ServiceError::ReadOnly { reason: reason.to_string() });
        }
        let mut next = self.book.clone();
        let out = f(&mut next, &mut *self.env)?;
        let records = {
            let changes = transaction_changes(&self.book, &next);
            changes.iter().filter_map(TxnChange::after).try_for_each(|t| check_transaction(&next, t))?;
            journal_records(&self.book, &next, &changes, self.env.now())
        };
        records
            .iter()
            .try_for_each(|lines| self.journal.write_record(lines))
            .map_err(|e| ServiceError::Io { path: self.path.clone(), source: e })?;
        self.book = next;
        self.dirty = true;
        Ok(out)
    }

    /// Save the book. `compress` overrides the file's current compression. The written bytes are read back and
    /// compared with the book before the old file is touched; a mismatch refuses the save.
    pub fn save(&mut self, compress: Option<bool>) -> Result<SaveReport, ServiceError> {
        if let Some(reason) = self.read_only.first() {
            return Err(ServiceError::ReadOnly { reason: reason.to_string() });
        }
        let compressed = compress.unwrap_or(self.residue.compressed);
        let bytes = write_book(&self.book, &self.residue, compressed).map_err(|e| ServiceError::Xml { path: self.path.clone(), source: e })?;
        let (reread, mut residue) = read_book(&bytes).map_err(|e| ServiceError::Xml { path: self.path.clone(), source: e })?;
        if let Some(difference) = first_difference(&self.book, &reread) {
            return Err(ServiceError::Verification { detail: difference });
        }
        let backup = write_book_file(&self.path, &bytes)?;
        residue.notices = self.residue.notices.clone();
        self.residue = residue;
        self.dirty = false;
        self.journal.rotate().map_err(|e| ServiceError::Io { path: self.path.clone(), source: e })?;
        let removed = remove_old_files(&self.path, self.settings.retention, self.journal.current_path());
        Ok(SaveReport { backup, removed, compressed })
    }

    /// Close the session, releasing the lock. Unsaved changes are discarded unless `save` is set.
    pub fn close(mut self, save: bool) -> Result<(), ServiceError> {
        if save && self.dirty {
            self.save(None)?;
        }
        self.journal.rotate().map_err(|e| ServiceError::Io { path: self.path.clone(), source: e })?;
        match self.lock.take() {
            Some(lock) => lock.release(),
            None => Ok(()),
        }
    }
}

/// How a transaction differs between two books.
enum TxnChange<'a> {
    Added(&'a Transaction),
    Changed(&'a Transaction, &'a Transaction),
    Removed(&'a Transaction),
}

impl<'a> TxnChange<'a> {
    fn after(&self) -> Option<&'a Transaction> {
        match self {
            Self::Added(t) | Self::Changed(_, t) => Some(t),
            Self::Removed(_) => None,
        }
    }
}

/// The journal records for a set of changes: `B` then `C` for an addition or edit, `B` then `D` for a deletion.
fn journal_records(old: &Book, new: &Book, changes: &[TxnChange<'_>], now: Time64) -> Vec<Vec<String>> {
    changes
        .iter()
        .flat_map(|change| match change {
            TxnChange::Added(t) => [Vec::new(), record_lines(RecordKind::Commit, t, new, now)],
            TxnChange::Changed(o, t) => [record_lines(RecordKind::Begin, o, old, now), record_lines(RecordKind::Commit, t, new, now)],
            TxnChange::Removed(o) => [record_lines(RecordKind::Begin, o, old, now), record_lines(RecordKind::Delete, o, old, now)],
        })
        .collect()
}

fn transaction_changes<'a>(old: &'a Book, new: &'a Book) -> Vec<TxnChange<'a>> {
    let changed = new.transactions.values().filter_map(|t| match old.transactions.get(&t.guid) {
        None => Some(TxnChange::Added(t)),
        Some(o) if !Arc::ptr_eq(o, t) && **o != **t => Some(TxnChange::Changed(o, t)),
        Some(_) => None,
    });
    let removed = old.transactions.values().filter(|t| !new.transactions.contains_key(&t.guid)).map(|t| TxnChange::Removed(t));
    changed.chain(removed).collect()
}

/// The invariants every changed transaction must meet.
fn check_transaction(book: &Book, t: &Transaction) -> Result<(), ServiceError> {
    if t.splits.is_empty() {
        return Err(DomainError::invalid(format!("transaction {} has no splits", t.guid)).into());
    }
    if let Some(s) = t.splits.iter().find(|s| s.account.is_some_and(|a| book.account(a).is_none())) {
        return Err(DomainError::invalid(format!("split {} refers to a missing account", s.guid)).into());
    }
    let imbalance = t.imbalance().map_err(DomainError::from)?;
    if !imbalance.is_zero() {
        return Err(DomainError::Unbalanced { imbalance: imbalance.to_string(), currency: t.currency.mnemonic.clone() }.into());
    }
    Ok(())
}

/// The first difference between the book in memory and the book read back from the bytes about to be written.
fn first_difference(expected: &Book, actual: &Book) -> Option<String> {
    if expected == actual {
        return None;
    }
    let field = |name: &str| Some(format!("{name} differ after writing"));
    if expected.guid != actual.guid || expected.root != actual.root {
        return field("book identity");
    }
    if expected.slots != actual.slots {
        return field("book slots");
    }
    if let Some(c) = expected.commodities.iter().find(|(k, v)| actual.commodities.get(*k) != Some(*v) && red_ale_xml::entities::commodity_is_written(v)) {
        return Some(format!("commodity {} differs after writing", c.0));
    }
    if let Some((g, _)) = expected.accounts.iter().find(|(g, v)| actual.accounts.get(*g) != Some(*v)) {
        return Some(format!("account {g} differs after writing"));
    }
    if let Some((g, _)) = expected.transactions.iter().find(|(g, v)| actual.transactions.get(*g) != Some(*v)) {
        return Some(format!("transaction {g} differs after writing"));
    }
    if let Some((g, _)) = expected.prices.iter().find(|(g, v)| actual.prices.get(*g) != Some(*v)) {
        return Some(format!("price {g} differs after writing"));
    }
    if let Some((g, _)) = expected.lots.iter().find(|(g, v)| actual.lots.get(*g) != Some(*v)) {
        return Some(format!("lot {g} differs after writing"));
    }
    if expected.business != actual.business {
        return field("business objects");
    }
    // Remaining differences are currencies GnuCash does not write (unused, no quotes); they are not data.
    None
}
