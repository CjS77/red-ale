//! Errors from domain operations.

use thiserror::Error;

use crate::NumericError;

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
