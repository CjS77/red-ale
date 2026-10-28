//! Since Last Run as use cases on a session: propose the due instances of every enabled scheduled transaction into a
//! Since Last Run proposal held by ID, apply a held proposal with the caller's choices as one Mutation, or discard it
//! (`design/api/scheduled-transactions.md`, "Since Last Run").
//!
//! The proposal and the apply are `red_ale_core::ops::scheduled`'s; this module finds "today" on the session's clock and
//! the Formulas' number format in the Configuration's Locale, holds the proposals against the Book revision, and runs
//! the Mutation.

use std::sync::Arc;

use red_ale_core::formula::NumberFormat;
use red_ale_core::ops::scheduled::{self, Applied, ApplyOptions, Choice, Proposal, ScheduledError};
use red_ale_locale::Locale;

use crate::backend::Format;
use crate::config::{Config, invalid};
use crate::{ServiceError, Session};

/// A held Since Last Run proposal, with the number format its Formulas were read in, so the apply reads them alike
/// even if the Configuration's Locale changes in between.
#[derive(Debug)]
pub struct HeldRun {
    /// The proposal.
    pub proposal: Proposal,
    /// How its Formulas write numbers.
    pub format: NumberFormat,
}

/// A proposal made and held.
#[derive(Debug, Clone)]
pub struct Proposed {
    /// Its ID, for `scheduledTransaction.apply` and `scheduledTransaction.discard`.
    pub id: String,
    /// The Book revision it was made at.
    pub revision: u64,
    /// The proposal.
    pub run: Arc<HeldRun>,
}

/// Propose the instances due on the local day of the session's clock and hold the proposal. Changes nothing in the
/// Book, so it works on a read-only book.
pub fn propose(session: &mut Session, config: &Config) -> Result<Proposed, ServiceError> {
    let format = number_format(&config.locale()?);
    let today = session
        .query(|_, env| env.today())
        .ok_or_else(|| invalid("the session's clock is outside the calendar"))?;
    let proposal = session
        .query(|book, _| scheduled::since_last_run(book, today, format))
        .map_err(ScheduledError::from)?;
    let revision = session.revision();
    let run = Arc::new(HeldRun { proposal, format });
    let id = session.runs().insert(revision, Arc::clone(&run));
    Ok(Proposed { id, revision, run })
}

/// Apply a held proposal with the caller's `choices` as one Mutation; with `auto_create_only`, only the instances of
/// auto-create scheduled transactions. A proposal made at another Book revision is refused; a failed apply leaves the
/// Book as it was and the proposal held. An SQLite book can't hold Deferred instances
/// (`5.17:libgnucash/backend/sql/gnc-schedxaction-sql.cpp:51-82`), so a new postponement there is refused.
pub fn apply(session: &mut Session, id: &str, choices: &[Choice], auto_create_only: bool) -> Result<Applied, ServiceError> {
    if let Some(reason) = session.read_only_reasons().first() {
        return Err(ServiceError::ReadOnly {
            reason: reason.to_string(),
        });
    }
    let revision = session.revision();
    let held = session.runs().current(id, revision)?;
    let options = ApplyOptions {
        auto_create_only,
        can_postpone: session.format() != Format::Sqlite,
        format: held.proposal.format,
    };
    let applied = session.try_mutate(|book, env| {
        scheduled::apply(book, env, &held.proposal.proposal, choices, options).map_err(ServiceError::from)
    })?;
    session.runs().remove(id);
    Ok(applied)
}

/// Drop a held proposal.
pub fn discard(session: &mut Session, id: &str) -> Result<(), ServiceError> {
    session.runs().discard(id)
}

/// The Locale's monetary decimal point and group separator, with `gnc_localeconv`'s defaults, which GnuCash's
/// expression parser reads numbers in (`5.17:libgnucash/app-utils/gnc-exp-parser.c:362-381`).
pub fn number_format(locale: &Locale) -> NumberFormat {
    let monetary = locale.monetary();
    let first = |s: &str, default: char| s.chars().next().unwrap_or(default);
    NumberFormat {
        decimal_point: first(monetary.mon_decimal_point, '.'),
        group_separator: first(monetary.mon_thousands_sep, ','),
    }
}
