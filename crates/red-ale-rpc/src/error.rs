//! JSON-RPC errors and red-ale's stable error codes.
//!
//! Codes are part of the published language: a code, once released, keeps its meaning. `design/api.md` lists them.

use red_ale_core::DomainError;
use red_ale_core::ops::scheduled::{AccountProblem, ScheduledError};
use red_ale_report::{ReportError, ReportErrorKind};
use red_ale_service::ServiceError;
use serde::Serialize;
use serde_json::{Value, json};

/// Stable error codes. The JSON-RPC 2.0 range (-32768 to -32000) is used only for the protocol errors it defines.
pub mod codes {
    /// The request is not valid JSON.
    pub const PARSE_ERROR: i32 = -32700;
    /// The JSON is not a valid request object.
    pub const INVALID_REQUEST: i32 = -32600;
    /// No such method.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// The params do not match the method's schema.
    pub const INVALID_PARAMS: i32 = -32602;
    /// A bug in red-ale.
    pub const INTERNAL_ERROR: i32 = -32603;

    /// No book is open.
    pub const NO_BOOK: i32 = 1000;
    /// A book is already open in this server.
    pub const BOOK_ALREADY_OPEN: i32 = 1001;
    /// Another program holds the book's lock.
    pub const BOOK_LOCKED: i32 = 1002;
    /// The lock file could not be created.
    pub const CANNOT_LOCK: i32 = 1003;
    /// The book is an SQL book, which v0.1 refuses.
    pub const SQL_BOOK: i32 = 1004;
    /// The URI scheme is not supported.
    pub const UNSUPPORTED_URI: i32 = 1005;
    /// The book is open read-only.
    pub const READ_ONLY: i32 = 1006;
    /// The file to create exists.
    pub const FILE_EXISTS: i32 = 1007;
    /// A file operation failed.
    pub const IO_ERROR: i32 = 1008;
    /// The file is not a readable XML book.
    pub const FILE_FORMAT: i32 = 1009;
    /// The written book did not read back identically; nothing was saved.
    pub const SAVE_VERIFICATION: i32 = 1010;
    /// The backup could not be made; nothing was saved.
    pub const BACKUP_FAILED: i32 = 1011;
    /// The book has unsaved changes.
    pub const UNSAVED_CHANGES: i32 = 1012;
    /// Another program wrote the SQL book or broke its lock since red-ale opened or last saved it; nothing was
    /// written.
    pub const BOOK_CHANGED: i32 = 1013;

    /// An entity named in the request does not exist.
    pub const NOT_FOUND: i32 = 2000;
    /// The request is not valid for the book as it stands.
    pub const INVALID: i32 = 2001;
    /// A transaction would not balance.
    pub const UNBALANCED: i32 = 2002;
    /// The entity exists but may not be changed this way.
    pub const CONFLICT: i32 = 2003;
    /// Exact arithmetic overflowed or was undefined.
    pub const ARITHMETIC: i32 = 2004;

    /// A report script failed at run time.
    pub const REPORT_SCRIPT: i32 = 3000;
    /// A report script passed a limit.
    pub const REPORT_LIMIT: i32 = 3001;
    /// A report option value is not valid.
    pub const REPORT_OPTION: i32 = 3002;
    /// A report script returned an invalid report result.
    pub const REPORT_RESULT: i32 = 3003;
    /// A user report script does not load.
    pub const REPORT_LOAD: i32 = 3004;

    /// An import file can't be read as its format at all.
    pub const IMPORT_FILE: i32 = 4000;

    /// A method needs the network and the Configuration has it off; nothing was sent.
    pub const NETWORK_DISABLED: i32 = 5000;

    /// A scheduled transaction's formula does not parse, names a function red-ale does not compute, or can't be
    /// evaluated for an instance.
    pub const FORMULA: i32 = 6000;

    /// Every code with its name, for the schema document.
    pub const ALL: [(i32, &str); 32] = [
        (PARSE_ERROR, "parse-error"),
        (INVALID_REQUEST, "invalid-request"),
        (METHOD_NOT_FOUND, "method-not-found"),
        (INVALID_PARAMS, "invalid-params"),
        (INTERNAL_ERROR, "internal-error"),
        (NO_BOOK, "no-book"),
        (BOOK_ALREADY_OPEN, "book-already-open"),
        (BOOK_LOCKED, "book-locked"),
        (CANNOT_LOCK, "cannot-lock"),
        (SQL_BOOK, "sql-book"),
        (UNSUPPORTED_URI, "unsupported-uri"),
        (READ_ONLY, "read-only"),
        (FILE_EXISTS, "file-exists"),
        (IO_ERROR, "io-error"),
        (FILE_FORMAT, "file-format"),
        (SAVE_VERIFICATION, "save-verification"),
        (BACKUP_FAILED, "backup-failed"),
        (UNSAVED_CHANGES, "unsaved-changes"),
        (BOOK_CHANGED, "book-changed"),
        (NOT_FOUND, "not-found"),
        (INVALID, "invalid"),
        (UNBALANCED, "unbalanced"),
        (CONFLICT, "conflict"),
        (ARITHMETIC, "arithmetic"),
        (REPORT_SCRIPT, "report-script"),
        (REPORT_LIMIT, "report-limit"),
        (REPORT_OPTION, "report-option"),
        (REPORT_RESULT, "report-result"),
        (REPORT_LOAD, "report-load"),
        (IMPORT_FILE, "import-file"),
        (NETWORK_DISABLED, "network-disabled"),
        (FORMULA, "formula"),
    ];
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[error("{message} (code {code})")]
pub struct RpcError {
    /// One of [`codes`].
    pub code: i32,
    /// A human-readable message.
    pub message: String,
    /// Structured detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    /// An error with a code and message.
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// The same error with structured detail.
    #[must_use]
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Invalid params.
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(codes::INVALID_PARAMS, message)
    }

    /// An entity was not found.
    pub fn not_found(kind: &str, id: impl std::fmt::Display) -> Self {
        Self::new(codes::NOT_FOUND, format!("{kind} {id} not found")).with_data(json!({ "kind": kind, "id": id.to_string() }))
    }

    /// The request is not valid for the book.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(codes::INVALID, message)
    }

    /// A bug.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(codes::INTERNAL_ERROR, message)
    }
}

impl From<DomainError> for RpcError {
    fn from(e: DomainError) -> Self {
        let message = e.to_string();
        match e {
            DomainError::NotFound { kind, id } => Self::new(codes::NOT_FOUND, message).with_data(json!({ "kind": kind, "id": id })),
            DomainError::Invalid(_) => Self::new(codes::INVALID, message),
            DomainError::Unbalanced { imbalance, currency } => {
                Self::new(codes::UNBALANCED, message).with_data(json!({ "imbalance": imbalance, "currency": currency }))
            }
            DomainError::Conflict(_) => Self::new(codes::CONFLICT, message),
            DomainError::Numeric(_) => Self::new(codes::ARITHMETIC, message),
            DomainError::NaturalSigns => Self::new(codes::CONFLICT, message).with_data(json!({ "reason": "natural-signs" })),
            DomainError::ReadOnlyByDate { date, threshold } => {
                Self::new(codes::CONFLICT, message).with_data(auto_read_only(date, threshold))
            }
            DomainError::BeforeReadOnlyThreshold { date, threshold } => {
                Self::new(codes::INVALID, message).with_data(auto_read_only(date, threshold))
            }
        }
    }
}

/// A report error keeps its kind's code and the `data` `design/api.md` documents for it; a bug in the engine or a
/// built-in script is `internal-error` with no data.
impl From<ReportError> for RpcError {
    fn from(e: ReportError) -> Self {
        let error = Self::new(e.kind().code(), e.to_string());
        if e.kind() == ReportErrorKind::Internal {
            error
        } else {
            error.with_data(e.data())
        }
    }
}

/// The data of a refusal by the book's auto read-only threshold (`design/api/book-options.md`).
fn auto_read_only(date: red_ale_core::Date, threshold: red_ale_core::Date) -> Value {
    json!({ "reason": "auto-read-only", "date": date.to_string(), "threshold": threshold.to_string() })
}

impl From<ServiceError> for RpcError {
    fn from(e: ServiceError) -> Self {
        let message = e.to_string();
        let path = |p: &std::path::Path| json!({ "path": p.display().to_string() });
        match e {
            ServiceError::Domain(d) => d.into(),
            ServiceError::Locked { lock_file } => Self::new(codes::BOOK_LOCKED, message).with_data(path(&lock_file)),
            ServiceError::CannotLock { lock_file, .. } => Self::new(codes::CANNOT_LOCK, message).with_data(path(&lock_file)),
            ServiceError::Io { path: p, .. } => Self::new(codes::IO_ERROR, message).with_data(path(&p)),
            ServiceError::Backup { path: p, .. } => Self::new(codes::BACKUP_FAILED, message).with_data(path(&p)),
            ServiceError::SqlBook { path: p } => Self::new(codes::SQL_BOOK, message).with_data(json!({ "path": p })),
            ServiceError::SqlLocked { path: p, hostname, pid } => Self::new(codes::BOOK_LOCKED, message).with_data(json!({
                "path": p.display().to_string(), "holder": { "hostname": hostname, "pid": pid },
            })),
            ServiceError::BookChanged { path: p, detail } => {
                Self::new(codes::BOOK_CHANGED, message).with_data(json!({ "path": p.display().to_string(), "detail": detail }))
            }
            ServiceError::Busy { path: p } => Self::new(codes::BOOK_LOCKED, message).with_data(path(&p)),
            ServiceError::UnsupportedUri { uri } => Self::new(codes::UNSUPPORTED_URI, message).with_data(json!({ "uri": uri })),
            ServiceError::Xml { path: p, .. }
            | ServiceError::Sql { path: p, .. }
            | ServiceError::WrongFormat { path: p, .. }
            | ServiceError::Journal { path: p, .. } => Self::new(codes::FILE_FORMAT, message).with_data(path(&p)),
            ServiceError::ReadOnly { reason } => Self::new(codes::READ_ONLY, message).with_data(json!({ "reason": reason })),
            ServiceError::Exists { path: p } => Self::new(codes::FILE_EXISTS, message).with_data(path(&p)),
            ServiceError::Verification { detail } => Self::new(codes::SAVE_VERIFICATION, message).with_data(json!({ "detail": detail })),
            ServiceError::CurrentJournal { path: p } => Self::new(codes::INVALID, message).with_data(path(&p)),
            ServiceError::NoSession => Self::new(codes::NO_BOOK, message),
            ServiceError::AlreadyOpen { path: p } => Self::new(codes::BOOK_ALREADY_OPEN, message).with_data(path(&p)),
            ServiceError::ImportFile {
                file,
                line,
                reason,
                message: text,
            } => Self::new(codes::IMPORT_FILE, message).with_data(json!({ "file": file, "line": line, "reason": reason, "message": text })),
            ServiceError::ProposalNotFound { kind, id } => {
                Self::new(codes::NOT_FOUND, message).with_data(json!({ "kind": kind.wire(), "id": id }))
            }
            ServiceError::StaleProposal {
                proposal_revision,
                book_revision,
                ..
            } => Self::new(codes::CONFLICT, message).with_data(json!({
                "reason": "book-revision", "proposalRevision": proposal_revision, "bookRevision": book_revision,
            })),
            ServiceError::Scheduled(e) => e.into(),
        }
    }
}

/// A Since Last Run refusal is `invalid` with `data.reason` and the instances it names; a failing Formula is `formula`
/// with where it is (`design/api/scheduled-transactions.md`, "Evaluation" and "`scheduledTransaction.apply`").
impl From<ScheduledError> for RpcError {
    fn from(e: ScheduledError) -> Self {
        let message = e.to_string();
        match e {
            ScheduledError::Formula(f) => Self::new(codes::FORMULA, message).with_data(json!({
                "reason": f.error.reason(),
                "formula": f.formula,
                "position": Value::Null,
                "scheduledTransaction": f.site.scheduled.to_string(),
                "templateTransaction": f.site.template_transaction.to_string(),
                "split": f.site.split.to_string(),
                "side": f.side.as_str(),
                "index": f.index,
            })),
            ScheduledError::Account { site, problem, index } => {
                let account = match problem {
                    AccountProblem::Missing(a) => a,
                    AccountProblem::Placeholder(a) | AccountProblem::Root(a) | AccountProblem::NoCommodity(a) => Some(a),
                };
                Self::new(codes::INVALID, message).with_data(json!({
                    "reason": problem.reason(),
                    "account": account.map(|a| a.to_string()),
                    "scheduledTransaction": site.scheduled.to_string(),
                    "templateTransaction": site.template_transaction.to_string(),
                    "split": site.split.to_string(),
                    "index": index,
                }))
            }
            ScheduledError::Refused { reason, indexes, .. } => Self::new(codes::INVALID, message).with_data(json!({
                "reason": reason.reason(), "index": indexes.first(), "indexes": indexes,
            })),
            ScheduledError::Invalid { reason, .. } => Self::new(codes::INVALID, message).with_data(json!({ "reason": reason })),
            ScheduledError::Recurrence(_) => Self::new(codes::INVALID, message).with_data(json!({ "reason": "recurrence" })),
            ScheduledError::Domain(d) => d.into(),
        }
    }
}
