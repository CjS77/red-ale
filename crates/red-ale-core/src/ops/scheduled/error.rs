//! Why a scheduled transaction's instance can't be proposed, built or applied.

use std::fmt;

use thiserror::Error;

use crate::formula::FormulaError;
use crate::{DomainError, Guid, RecurrenceError};

/// The template split a problem was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Site {
    /// The scheduled transaction.
    pub scheduled: Guid,
    /// The template transaction.
    pub template_transaction: Guid,
    /// The template split.
    pub split: Guid,
}

/// The side of a template split a Formula is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// `debit-formula`.
    Debit,
    /// `credit-formula`.
    Credit,
}

impl Side {
    /// `debit` or `credit`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debit => "debit",
            Self::Credit => "credit",
        }
    }
}

/// What is wrong with a template split's account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountProblem {
    /// The account the `sched-xaction` Frame names is not in the Book, or it names none: GnuCash's "Unknown account for
    /// guid [%s], cancelling SX [%s] creation." (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1060-1085`).
    Missing(Option<Guid>),
    /// A placeholder account, which red-ale's commit rules refuse a split in where GnuCash creates one.
    Placeholder(Guid),
    /// The root account.
    Root(Guid),
    /// An account with no commodity.
    NoCommodity(Guid),
}

impl AccountProblem {
    /// The reason the API names it by: `missing-account`, `placeholder`, `root` or `no-commodity`.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Missing(_) => "missing-account",
            Self::Placeholder(_) => "placeholder",
            Self::Root(_) => "root",
            Self::NoCommodity(_) => "no-commodity",
        }
    }
}

impl fmt::Display for AccountProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(Some(g)) => write!(f, "names account {g}, which is not in the book"),
            Self::Missing(None) => f.write_str("names no account"),
            Self::Placeholder(g) => write!(f, "is in placeholder account {g}"),
            Self::Root(g) => write!(f, "is in the root account {g}"),
            Self::NoCommodity(g) => write!(f, "is in account {g}, which has no commodity"),
        }
    }
}

/// Why a choice in a Since Last Run apply is refused; [`Refusal::reason`] is the API's `data.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// No instance has this index.
    Index,
    /// `created` is a result, never a choice.
    CreatedState,
    /// Two choices for one instance.
    DuplicateChoice,
    /// A `toCreate` instance lacks a value for a variable.
    UnboundVariable,
    /// `i` can't be set.
    NotEditable,
    /// A value for a name that is not one of the instance's variables.
    UnknownVariable,
    /// A reminder before another state in one schedule (`5.17:libgnucash/app-utils/gnc-sx-instance-model.h:138-158`).
    ReminderFirst,
    /// A `toCreate` instance before the read-only threshold that the caller did not choose.
    ReadOnlyByDate,
    /// Postponing in a book that can't hold Deferred instances.
    SqlDeferredInstances,
    /// With auto-create only, a choice for a scheduled transaction that is not auto-create.
    NotAutoCreate,
    /// An exchange rate of zero for a split whose value is not zero.
    ZeroRate,
    /// A transaction currency that is not a currency.
    NotACurrency,
}

impl Refusal {
    /// The API's `data.reason`.
    pub fn reason(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::CreatedState => "created-state",
            Self::DuplicateChoice => "duplicate-choice",
            Self::UnboundVariable => "unbound-variable",
            Self::NotEditable => "not-editable",
            Self::UnknownVariable => "unknown-variable",
            Self::ReminderFirst => "reminder-first",
            Self::ReadOnlyByDate => "read-only-by-date",
            Self::SqlDeferredInstances => "sql-deferred-instances",
            Self::NotAutoCreate => "not-auto-create",
            Self::ZeroRate => "zero-rate",
            Self::NotACurrency => "not-a-currency",
        }
    }
}

/// A Formula that does not evaluate, and where it is.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the {} formula {formula:?} of scheduled transaction {}: {error}", side.as_str(), site.scheduled)]
pub struct FormulaFailure {
    /// Where.
    pub site: Site,
    /// Which side.
    pub side: Side,
    /// The formula's text.
    pub formula: String,
    /// Why.
    pub error: FormulaError,
    /// The instance's index in a Since Last Run proposal, if any.
    pub index: Option<usize>,
}

/// Why an instance can't be proposed, built or applied. Each leaves the Book unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduledError {
    /// A template split's account can't take the split.
    #[error("a template split of scheduled transaction {} {problem}", site.scheduled)]
    Account {
        /// Where.
        site: Site,
        /// What.
        problem: AccountProblem,
        /// The instance's index in a Since Last Run proposal, if any.
        index: Option<usize>,
    },
    /// A Formula does not evaluate.
    #[error("{0}")]
    Formula(Box<FormulaFailure>),
    /// A choice or instance is refused.
    #[error("{message}")]
    Refused {
        /// Why.
        reason: Refusal,
        /// The instances concerned, by index.
        indexes: Vec<usize>,
        /// What to tell the caller.
        message: String,
    },
    /// The schedule can't be computed.
    #[error(transparent)]
    Recurrence(#[from] RecurrenceError),
    /// A domain rule refused it.
    #[error(transparent)]
    Domain(#[from] DomainError),
}

impl ScheduledError {
    /// A refusal concerning `indexes`.
    pub fn refused(reason: Refusal, indexes: Vec<usize>, message: impl Into<String>) -> Self {
        Self::Refused {
            reason,
            indexes,
            message: message.into(),
        }
    }

    /// The same error, naming the instance at `at` in a Since Last Run proposal.
    #[must_use]
    pub fn at(mut self, at: usize) -> Self {
        match &mut self {
            Self::Account { index, .. } => *index = Some(at),
            Self::Formula(f) => f.index = Some(at),
            _ => {}
        }
        self
    }
}

impl ScheduledError {
    /// A Formula's failure.
    pub fn formula(at: Site, side: Side, formula: &str, error: FormulaError) -> Self {
        Self::Formula(Box::new(FormulaFailure {
            site: at,
            side,
            formula: formula.to_owned(),
            error,
            index: None,
        }))
    }
}

impl From<crate::NumericError> for ScheduledError {
    fn from(e: crate::NumericError) -> Self {
        Self::Domain(e.into())
    }
}
