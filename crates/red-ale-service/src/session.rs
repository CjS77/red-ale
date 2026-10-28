//! A Session: one open book file with its lock, journal and read-only state.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use red_ale_core::book_options::{GNUCASH_FEATURES, OBSOLETE_BOOK_CURRENCY, features};
use red_ale_core::diff::{Change, changes};
use red_ale_core::ops::commit;
use red_ale_core::{Book, DomainError, Env, Guid, Time64, Transaction};
use red_ale_xml::XmlResidue;

use crate::backend::{self, Access, Backend, Format, Opened, SaveOptions};
use crate::error::ServiceError;
use crate::files::{Retention, remove_old_files};
use crate::proposals::{ProposalKind, ProposalStore};
use crate::since_last_run::HeldRun;
use crate::journal::{Journal, RecordKind, record_lines};
use crate::replay::{ReplayReport, replay};

/// Book features red-ale can edit safely: their data lives in slots or files red-ale keeps unchanged, or red-ale
/// follows them (`Number Field Source`, `Credit Notes`).
pub const SUPPORTED_FEATURES: [&str; 10] = [
    "Credit Notes",
    "Number Field Source",
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
    /// red-ale's own configuration (the accounting period), shared by every clone of these settings.
    pub config: crate::config::ConfigStore,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            retention: Retention::default(),
            compress_new_books: true,
            config: crate::config::ConfigStore::in_memory(),
        }
    }
}

/// Why a session is read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOnlyReason {
    /// The caller asked for it.
    Requested,
    /// The book declares Book features v0.1 does not implement.
    UnsupportedFeatures(Vec<String>),
    /// The file holds content red-ale can keep but not edit safely.
    UnrecognisedContent(Vec<String>),
}

impl ReadOnlyReason {
    /// A stable machine-readable name.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::UnsupportedFeatures(_) => "unsupported-features",
            Self::UnrecognisedContent(_) => "unrecognised-content",
        }
    }
}

impl fmt::Display for ReadOnlyReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Requested => write!(f, "opened read-only on request"),
            Self::UnsupportedFeatures(list) => write!(f, "the book uses features red-ale v0.1 does not support: {}", list.join("; ")),
            Self::UnrecognisedContent(list) => write!(f, "the book contains data red-ale cannot edit safely: {}", list.join("; ")),
        }
    }
}

/// The read-only triggers a book sets off: the Book features it declares that red-ale does not support, then what its
/// Backend found (`findings`).
pub fn read_only_triggers(book: &Book, findings: &[ReadOnlyReason]) -> Vec<ReadOnlyReason> {
    let declared = features(book);
    let unsupported: Vec<String> = declared
        .iter()
        .filter(|(name, _)| name != OBSOLETE_BOOK_CURRENCY && !SUPPORTED_FEATURES.contains(&name.as_str()))
        .map(|(name, description)| {
            if description.is_empty() {
                name.clone()
            } else {
                description.clone()
            }
        })
        .collect();
    let mut reasons = Vec::new();
    if !unsupported.is_empty() {
        reasons.push(ReadOnlyReason::UnsupportedFeatures(unsupported));
    }
    reasons.extend_from_slice(findings);
    reasons
}

/// GnuCash's known feature names, for reporting.
pub fn gnucash_feature_names() -> impl Iterator<Item = &'static str> {
    GNUCASH_FEATURES.iter().map(|(n, _)| *n)
}

/// The real [`Env`]: the system clock, random GUIDs and the process's local time zone ([`crate::zone::local_zone`]).
#[derive(Debug, Default)]
pub struct SystemEnv;

impl Env for SystemEnv {
    fn zone(&self) -> &red_ale_core::TimeZone {
        crate::zone::local_zone()
    }

    fn now(&mut self) -> Time64 {
        Time64(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX)),
        )
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
    } else if ["sqlite3://", "postgres://", "postgresql://", "mysql://"]
        .iter()
        .any(|s| lower.starts_with(s))
    {
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
        std::env::current_dir()
            .map(|d| d.join(&path))
            .map_err(|e| ServiceError::Io { path, source: e })
    }
}

/// An open book.
#[derive(Debug)]
pub struct Session {
    backend: Box<dyn Backend>,
    book: Book,
    /// The Book as read or last saved, which a Backend saves the difference from.
    saved: Book,
    read_only: Vec<ReadOnlyReason>,
    journal: Journal,
    dirty: bool,
    settings: Settings,
    env: Box<dyn EnvDebug>,
    revision: u64,
    proposals: ProposalStore<red_ale_import::pipeline::Proposal>,
    runs: ProposalStore<HeldRun>,
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
    /// XML only: whether the file was written gzip-compressed.
    pub compressed: Option<bool>,
}

impl Session {
    /// Open an existing book. Unless `read_only`, take the lock first; release it again if the book turns out to be
    /// read-only.
    pub fn open(spec: &str, read_only: bool, settings: Settings) -> Result<Self, ServiceError> {
        let path = resolve_path(spec)?;
        let format = Format::of(&path)?;
        let access = if read_only { Access::ReadOnly } else { Access::ReadWrite };
        let rule = |book: &Book, findings: &[ReadOnlyReason]| {
            let mut reasons = read_only_triggers(book, findings);
            if read_only {
                reasons.insert(0, ReadOnlyReason::Requested);
            }
            reasons
        };
        let opened = backend::open(&path, format, access, &rule)?;
        Ok(Self::from_opened(opened, settings, Box::new(SystemEnv)))
    }

    /// Create a new, empty book file, taking its lock. Fails if the file exists.
    pub fn create(spec: &str, settings: Settings) -> Result<Self, ServiceError> {
        let path = resolve_path(spec)?;
        if path.exists() {
            return Err(ServiceError::Exists { path });
        }
        let mut env = SystemEnv;
        let opened = backend::create(&path, Format::Xml, &settings, &mut env)?;
        let mut session = Self::from_opened(opened, settings, Box::new(env));
        session.dirty = true;
        session.save(None)?;
        Ok(session)
    }

    /// A session over a book a Backend has opened, with nothing unsaved.
    fn from_opened(opened: Opened, settings: Settings, env: Box<dyn EnvDebug>) -> Self {
        let Opened { book, read_only, backend } = opened;
        Self {
            journal: Journal::new(backend.path()),
            backend,
            saved: book.clone(),
            book,
            read_only,
            dirty: false,
            settings,
            env,
            revision: 0,
            proposals: ProposalStore::new(ProposalKind::Import),
            runs: ProposalStore::new(ProposalKind::SinceLastRun),
        }
    }

    /// Replace the environment (clock and GUID source), for deterministic tests.
    pub fn set_env(&mut self, env: impl EnvDebug + 'static) {
        self.env = Box::new(env);
    }

    /// [`Session::set_env`] for an environment that is already boxed.
    pub fn set_env_boxed(&mut self, env: Box<dyn EnvDebug>) {
        self.env = env;
    }

    /// The book's path.
    pub fn path(&self) -> &Path {
        self.backend.path()
    }

    /// The book file's format.
    pub fn format(&self) -> Format {
        self.backend.format()
    }

    /// The book as it stands.
    pub fn book(&self) -> &Book {
        &self.book
    }

    /// XML only: the file-format Residue.
    pub fn xml_residue(&self) -> Option<&XmlResidue> {
        self.backend.xml_residue()
    }

    /// XML only: whether the file is gzip-compressed.
    pub fn compressed(&self) -> Option<bool> {
        self.backend.compressed()
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

    /// The Book revision: how many Mutations this session has applied. An Import proposal or Since Last Run proposal
    /// records the revision it was made at, and is applied only at that revision (`design/milestone3.md`, "Decided").
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The Import proposals held for this session's Book, dropped with it.
    pub(crate) fn proposals(&mut self) -> &mut ProposalStore<red_ale_import::pipeline::Proposal> {
        &mut self.proposals
    }

    /// The Since Last Run proposals held for this session's Book, dropped with it; apart from the Import proposals.
    pub(crate) fn runs(&mut self) -> &mut ProposalStore<HeldRun> {
        &mut self.runs
    }

    /// The current time from the session's environment.
    pub fn now(&mut self) -> Time64 {
        self.env.now()
    }

    /// The local time zone of the session's environment.
    pub fn zone(&self) -> &red_ale_core::TimeZone {
        self.env.zone()
    }

    /// Read-only access to the book with the environment, for queries that need the clock.
    pub fn query<T>(&mut self, f: impl FnOnce(&Book, &mut dyn Env) -> T) -> T {
        f(&self.book, &mut *self.env)
    }

    /// Apply a Mutation atomically: `f` edits a copy of the book; every transaction it added or changed is then
    /// committed as GnuCash commits it (trading splits, when the book uses them); the copy replaces the book only if
    /// all this succeeds and every changed transaction is balanced and refers to existing accounts. Transaction changes
    /// are journaled.
    pub fn mutate<T>(&mut self, f: impl FnOnce(&mut Book, &mut dyn Env) -> Result<T, DomainError>) -> Result<T, ServiceError> {
        self.try_mutate(|book, env| f(book, env).map_err(ServiceError::from))
    }

    /// [`Session::mutate`] for callers with their own error type, such as one that also resolves wire references.
    pub fn try_mutate<T, E: From<ServiceError>>(&mut self, f: impl FnOnce(&mut Book, &mut dyn Env) -> Result<T, E>) -> Result<T, E> {
        if let Some(reason) = self.read_only.first() {
            return Err(ServiceError::ReadOnly {
                reason: reason.to_string(),
            }
            .into());
        }
        let mut next = self.book.clone();
        let out = f(&mut next, &mut *self.env)?;
        commit::commit_changed(&self.book, &mut next, &mut *self.env).map_err(ServiceError::from)?;
        let records = {
            let changes = transaction_changes(&self.book, &next);
            changes
                .iter()
                .filter_map(Change::after)
                .try_for_each(|t| check_transaction(&next, t))?;
            self.backend.check_storable(&self.book, &next)?;
            journal_records(&self.book, &next, &changes, self.env.now())
        };
        records
            .iter()
            .try_for_each(|lines| self.journal.write_record(lines))
            .map_err(|e| self.io_error(e))?;
        self.book = next;
        self.dirty = true;
        self.revision += 1;
        Ok(out)
    }

    /// Save the book. `compress` overrides the file's current compression. The Backend writes the book and checks
    /// what it wrote before the old file is touched; a mismatch refuses the save.
    pub fn save(&mut self, compress: Option<bool>) -> Result<SaveReport, ServiceError> {
        if let Some(reason) = self.read_only.first() {
            return Err(ServiceError::ReadOnly {
                reason: reason.to_string(),
            });
        }
        let prepared = if self.dirty { self.backend.prepare_save(&self.book) } else { None };
        let book = prepared.as_ref().unwrap_or(&self.book);
        let options = SaveOptions { compress };
        let saved = self.backend.save(&self.saved, book, &options, &mut *self.env)?;
        // What a format adds on saving is not a Mutation and touches nothing an Import proposal reads, so the
        // revision stays.
        if let Some(prepared) = prepared {
            self.book = prepared;
        }
        self.saved = self.book.clone();
        self.dirty = false;
        self.journal.rotate().map_err(|e| self.io_error(e))?;
        let removed = remove_old_files(self.backend.path(), self.settings.retention, self.journal.current_path());
        Ok(SaveReport {
            backup: saved.backup,
            removed,
            compressed: saved.compressed,
        })
    }

    /// Replay a journal onto the book as one atomic Mutation (see [`crate::replay`]). The journal this session is
    /// writing is refused, as GnuCash refuses it.
    pub fn replay_journal(&mut self, journal: &Path, skip_invalid: bool) -> Result<ReplayReport, ServiceError> {
        let journal = resolve_path(&journal.display().to_string())?;
        if self.journal.current_path().is_some_and(|current| current == journal) {
            return Err(ServiceError::CurrentJournal { path: journal });
        }
        let text = std::fs::read(&journal).map_err(|e| ServiceError::Io {
            path: journal.clone(),
            source: e,
        })?;
        let (records, skipped_lines) = crate::journal::parse(&String::from_utf8_lossy(&text)).map_err(|source| ServiceError::Journal {
            path: journal.clone(),
            source,
        })?;
        let mut report = self.mutate(|book, env| replay(book, env, &records, skip_invalid))?;
        report.skipped_lines = skipped_lines;
        Ok(report)
    }

    /// Close the session, releasing the lock. Unsaved changes are discarded unless `save` is set.
    pub fn close(mut self, save: bool) -> Result<(), ServiceError> {
        if save && self.dirty {
            self.save(None)?;
        }
        self.journal.rotate().map_err(|e| self.io_error(e))?;
        self.backend.release_lock()
    }

    fn io_error(&self, source: std::io::Error) -> ServiceError {
        ServiceError::Io {
            path: self.path().to_owned(),
            source,
        }
    }
}

/// How a transaction differs between two books.
type TxnChange<'a> = Change<'a, Transaction>;

/// The journal records for a set of changes: `B` then `C` for an addition or edit, `B` then `D` for a deletion.
fn journal_records(old: &Book, new: &Book, changes: &[TxnChange<'_>], now: Time64) -> Vec<Vec<String>> {
    changes
        .iter()
        .flat_map(|change| match change {
            TxnChange::Added(t) => [Vec::new(), record_lines(RecordKind::Commit, t, new, now)],
            TxnChange::Changed(o, t) => [
                record_lines(RecordKind::Begin, o, old, now),
                record_lines(RecordKind::Commit, t, new, now),
            ],
            TxnChange::Removed(o) => [
                record_lines(RecordKind::Begin, o, old, now),
                record_lines(RecordKind::Delete, o, old, now),
            ],
        })
        .collect()
}

fn transaction_changes<'a>(old: &'a Book, new: &'a Book) -> Vec<TxnChange<'a>> {
    changes(&old.transactions, &new.transactions)
}

/// The invariants every changed transaction must meet.
fn check_transaction(book: &Book, t: &Transaction) -> Result<(), ServiceError> {
    if t.splits.is_empty() {
        return Err(DomainError::invalid(format!("transaction {} has no splits", t.guid)).into());
    }
    if let Some(s) = t.splits.iter().find(|s| s.account.is_some_and(|a| book.account(a).is_none())) {
        return Err(DomainError::invalid(format!("split {} refers to a missing account", s.guid)).into());
    }
    commit::check_committed(book, t).map_err(ServiceError::from)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use red_ale_core::SlotValue;

    use super::*;
    use crate::backend::Saved;

    /// What the test Backend was asked to do.
    #[derive(Debug, Default)]
    struct Seen {
        checks: usize,
        /// The `saved` and `book` arguments of each save.
        saves: Vec<(Book, Book)>,
    }

    /// A Backend that writes nothing: it can refuse every Mutation, and add a slot when preparing a save.
    #[derive(Debug)]
    struct TestBackend {
        path: PathBuf,
        refuse: bool,
        prepare: bool,
        seen: Arc<Mutex<Seen>>,
    }

    impl Backend for TestBackend {
        fn format(&self) -> Format {
            Format::Xml
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn holds_lock(&self) -> bool {
            true
        }

        fn release_lock(&mut self) -> Result<(), ServiceError> {
            Ok(())
        }

        fn check_storable(&self, _before: &Book, _after: &Book) -> Result<(), ServiceError> {
            self.seen.lock().unwrap().checks += 1;
            if self.refuse {
                Err(DomainError::invalid("not storable").into())
            } else {
                Ok(())
            }
        }

        fn prepare_save(&self, book: &Book) -> Option<Book> {
            self.prepare.then(|| {
                let mut prepared = book.clone();
                prepared.slots.set("test/prepared", SlotValue::String("yes".into()));
                prepared
            })
        }

        fn save(&mut self, saved: &Book, book: &Book, _: &SaveOptions, _: &mut dyn Env) -> Result<Saved, ServiceError> {
            self.seen.lock().unwrap().saves.push((saved.clone(), book.clone()));
            Ok(Saved {
                backup: None,
                compressed: None,
            })
        }

        fn compressed(&self) -> Option<bool> {
            None
        }

        fn xml_residue(&self) -> Option<&XmlResidue> {
            None
        }
    }

    fn session(dir: &Path, refuse: bool, prepare: bool) -> (Session, Arc<Mutex<Seen>>) {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let backend = TestBackend {
            path: dir.join("b.gnucash"),
            refuse,
            prepare,
            seen: Arc::clone(&seen),
        };
        let opened = Opened {
            book: Book::new_empty(&mut SystemEnv),
            read_only: Vec::new(),
            backend: Box::new(backend),
        };
        (Session::from_opened(opened, Settings::default(), Box::new(SystemEnv)), seen)
    }

    fn set_note(session: &mut Session, note: &str) -> Result<(), ServiceError> {
        session.mutate(|book, _| {
            book.slots.set("test/note", SlotValue::String(note.into()));
            Ok(())
        })
    }

    #[test]
    fn check_storable_runs_for_every_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let (mut session, seen) = session(dir.path(), false, false);
        ["a", "b", "c"].iter().for_each(|note| set_note(&mut session, note).unwrap());
        assert_eq!(seen.lock().unwrap().checks, 3);
        assert_eq!(session.revision(), 3);
    }

    #[test]
    fn a_refused_mutation_leaves_the_book_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let (mut session, seen) = session(dir.path(), true, false);
        let before = session.book().clone();
        let err = set_note(&mut session, "a").unwrap_err();
        assert!(matches!(err, ServiceError::Domain(_)), "{err}");
        assert_eq!(seen.lock().unwrap().checks, 1);
        assert_eq!(session.book(), &before);
        assert!(!session.is_dirty());
        assert_eq!(session.revision(), 0);
        assert!(session.journal_path().is_none());
    }

    #[test]
    fn a_save_gets_the_last_saved_book_and_keeps_what_prepare_save_adds() {
        let dir = tempfile::tempdir().unwrap();
        let (mut session, seen) = session(dir.path(), false, true);
        let read = session.book().clone();
        set_note(&mut session, "a").unwrap();
        session.save(None).unwrap();
        let first = session.book().clone();
        assert_eq!(first.slots.get_str("test/prepared"), Some("yes"));
        assert!(!session.is_dirty());
        set_note(&mut session, "b").unwrap();
        session.save(None).unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.saves.len(), 2);
        assert_eq!(seen.saves[0], (read, first.clone()));
        assert_eq!(seen.saves[1].0, first);
        assert_eq!(&seen.saves[1].1, session.book());
    }
}
