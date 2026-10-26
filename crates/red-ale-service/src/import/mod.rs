//! Imports as use cases on a session: preview a file into an Import proposal held by ID, apply a held proposal with
//! the caller's changes as one Mutation, or discard it (`design/api/import.md`, "The pipeline";
//! `design/milestone3.md`, "Decided", item 3).
//!
//! The formats' readers, the matcher and the proposal are `red_ale_import`'s; this module reads the files and the
//! Configuration, holds the proposals against the Book revision, and runs the Mutation.

mod csv;
pub mod file;
pub mod settings;
mod store;

use std::sync::Arc;

use red_ale_import::pipeline::{self, Applied, ApplyChoices, Change, Proposal, Review};

pub use csv::CsvParams;
pub use file::{FileContent, ImportFile};
pub use settings::LayoutChoice;
pub use store::{KEPT, ProposalStore};

use crate::config::Config;
use crate::{ServiceError, Session};

/// What to import, by format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewParams {
    /// GnuCash's Import Transactions from CSV.
    CsvTransactions(CsvParams),
}

/// A proposal made and held.
#[derive(Debug, Clone)]
pub struct Previewed {
    /// Its ID, for `import.apply` and `import.discard`.
    pub id: String,
    /// The Book revision it was made at.
    pub revision: u64,
    /// The proposal.
    pub proposal: Arc<Proposal>,
    /// What applying it as it stands would do.
    pub review: Review,
}

/// Read the files, build what they hold, run the matcher, and hold the proposal. Changes nothing in the Book, so it
/// works on a read-only book.
pub fn preview(session: &mut Session, config: &Config, files: &[ImportFile], params: &PreviewParams) -> Result<Previewed, ServiceError> {
    let proposal = match params {
        PreviewParams::CsvTransactions(p) => {
            let choices = csv::Choices::read(config, &p.layout)?;
            session.query(|book, env| csv::propose(book, env, files, p, &choices))?
        }
    };
    let review = session.query(|book, env| {
        let now = env.now();
        pipeline::review(book, now, env.zone(), &proposal)
    });
    let revision = session.revision();
    let proposal = Arc::new(proposal);
    let id = session.proposals().insert(revision, Arc::clone(&proposal));
    Ok(Previewed { id, revision, proposal, review })
}

/// Apply a held proposal with the caller's changes as one Mutation. A proposal made at another Book revision is
/// refused; a failed apply leaves the Book as it was and the proposal held.
pub fn apply(session: &mut Session, id: &str, changes: &[Change], choices: ApplyChoices) -> Result<Applied, ServiceError> {
    if let Some(reason) = session.read_only_reasons().first() {
        return Err(ServiceError::ReadOnly { reason: reason.to_string() });
    }
    let held = session.proposals().get(id).cloned().ok_or_else(|| ServiceError::ProposalNotFound { id: id.to_owned() })?;
    if held.revision != session.revision() {
        return Err(ServiceError::StaleProposal { id: id.to_owned(), proposal_revision: held.revision, book_revision: session.revision() });
    }
    let applied = session.mutate(|book, env| pipeline::apply(book, env, &held.proposal, changes, choices))?;
    session.proposals().remove(id);
    Ok(applied)
}

/// Drop a held proposal.
pub fn discard(session: &mut Session, id: &str) -> Result<(), ServiceError> {
    if session.proposals().remove(id) { Ok(()) } else { Err(ServiceError::ProposalNotFound { id: id.to_owned() }) }
}
