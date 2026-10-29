//! Errors from domain operations.

use thiserror::Error;

use crate::{Date, Guid, NumericError};

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
    /// The book has Budgets but lacks the Book feature `Use natural signs in budget amounts`, so their sign
    /// convention is unknown until the natural-signs scrub runs (`design/api/budgets.md`, "Natural signs").
    #[error("the book's budgets predate natural signs: run the budget sign scrub before editing budgets")]
    NaturalSigns,
    /// A transaction and a template transaction would share a GUID, which GnuCash keeps in one collection, where
    /// one would hide the other.
    #[error(
        "a transaction and a template transaction share the GUID {guid}; GnuCash would find only one of them, so the \
         book can't hold both"
    )]
    SharedGuid {
        /// The GUID both have.
        guid: Guid,
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
