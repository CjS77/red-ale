//! Book sessions: opening, locking, atomic use cases, the journal, saving and backups.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod error;
pub mod files;
pub mod journal;
pub mod lock;
pub mod replay;
pub mod session;

pub use error::ServiceError;
pub use session::{ReadOnlyReason, SaveReport, Session, Settings};
