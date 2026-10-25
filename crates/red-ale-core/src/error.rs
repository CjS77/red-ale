//! Errors from domain operations.

use thiserror::Error;

use crate::{Date, NumericError};

/// Why a domain operation was refused. Every variant leaves the Book unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    /// An entity named by the caller does not exist.
    #[error("{kind} {id} not found")]
    NotFound {
        /// What kind of entity.
        kind: &'static str,
        /// The identifier given.
        id: String,
    },
    /// The request is not valid for the Book as it stands.
    #[error("{0}")]
    Invalid(String),
    /// A transaction's split values do not sum to zero.
    #[error("transaction is unbalanced by {imbalance} {currency}")]
    Unbalanced {
        /// The imbalance as `num/denom`.
        imbalance: String,
        /// The transaction currency's mnemonic.
        currency: String,
    },
    /// The entity exists but may not be changed this way.
    #[error("{0}")]
    Conflict(String),
    /// The transaction is posted before the book's auto read-only threshold, so it may not be changed.
    #[error("the transaction is posted on {date}, before the book's read-only threshold {threshold}")]
    ReadOnlyByDate {
        /// The transaction's posted date.
        date: Date,
        /// The threshold date.
        threshold: Date,
    },
    /// A posted date before the book's auto read-only threshold.
    #[error("the date {date} is before the book's read-only threshold {threshold}")]
    BeforeReadOnlyThreshold {
        /// The date asked for.
        date: Date,
        /// The threshold date.
        threshold: Date,
    },
    /// Arithmetic failed.
    #[error("arithmetic error: {0}")]
    Numeric(#[from] NumericError),
}

impl DomainError {
    /// A `NotFound` for an identifier.
    pub fn not_found(kind: &'static str, id: &impl std::fmt::Display) -> Self {
        Self::NotFound { kind, id: id.to_string() }
    }

    /// An `Invalid` with a message.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    /// A `Conflict` with a message.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }
}

/// Result of a domain operation.
pub type DomainResult<T> = Result<T, DomainError>;
