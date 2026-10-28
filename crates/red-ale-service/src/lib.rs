//! Book sessions: opening, locking, atomic use cases, the journal, saving and backups.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod backend;
pub mod config;
pub mod error;
pub mod export;
pub mod files;
pub mod import;
pub mod journal;
pub mod locale;
pub mod lock;
pub mod proposals;
pub mod replay;
pub mod reports;
pub mod session;
pub mod since_last_run;
pub mod zone;

pub use backend::Format;
pub use error::ServiceError;
pub use session::{ReadOnlyReason, SaveReport, Session, Settings};
