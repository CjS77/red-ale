//! Imports as use cases on a session: preview a file into an Import proposal held by ID, apply a held proposal with
//! the caller's changes as one Mutation, or discard it (`design/api/import.md`, "The pipeline";
//! `design/milestone3.md`, "Decided", item 3).
//!
//! The formats' readers, the matcher and the proposal are `red_ale_import`'s; this module reads the files and the
//! Configuration, holds the proposals against the Book revision, and runs the Mutation.

mod accounts;
mod business;
mod csv;
pub mod file;
mod prices;
pub mod qif;
pub mod settings;

use std::sync::Arc;

use red_ale_import::pipeline::{self, Applied, ApplyChoices, Change, Proposal, Review, Warning};

pub use accounts::AccountsParams;
pub use business::{DocumentsParams, OwnersParams};
pub use csv::CsvParams;
pub use file::{FileContent, ImportFile};
pub use prices::PricesParams;
pub use qif::QifParams;
pub use settings::{LayoutChoice, PriceLayoutChoice};

use crate::config::{Config, ConfigStore};
use crate::{ServiceError, Session};

/// What to import, by format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewParams {
    /// GnuCash's Import Transactions from CSV.
    CsvTransactions(CsvParams),
    /// GnuCash's Import Bills & Invoices.
    CsvDocuments(DocumentsParams),
    /// GnuCash's Import Customers & Vendors.
    CsvOwners(OwnersParams),
    /// GnuCash's Import Accounts from CSV.
    CsvAccounts(AccountsParams),
    /// GnuCash's Import Prices from CSV.
    CsvPrices(PricesParams),
    /// GnuCash's QIF import.
    Qif(QifParams),
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
        PreviewParams::CsvDocuments(p) => {
            let choices = business::Choices::read(config)?;
            session.query(|book, env| business::propose_documents(book, env, files, *p, &choices))?
        }
        PreviewParams::CsvOwners(p) => {
            let choices = business::Choices::read(config)?;
            session.query(|book, env| business::propose_owners(book, env, files, *p, &choices, config))?
        }
        PreviewParams::Qif(p) => session.query(|book, env| qif::propose(book, env, config, files, p))?,
        PreviewParams::CsvAccounts(p) => {
            let settings = config.import_settings()?;
            session.query(|book, _| accounts::propose(book, files, p, settings))?
        }
        PreviewParams::CsvPrices(p) => {
            let choices = prices::Choices::read(config, &p.layout)?;
            session.query(|book, env| prices::propose(book, env, files, p, &choices))?
        }
    };
    let review = session.query(|book, env| {
        let now = env.now();
        pipeline::review(book, now, env.zone(), &proposal)
    });
    let revision = session.revision();
    let proposal = Arc::new(proposal);
    let id = session.proposals().insert(revision, Arc::clone(&proposal));
    Ok(Previewed {
        id,
        revision,
        proposal,
        review,
    })
}

/// Apply a held proposal with the caller's changes as one Mutation. A proposal made at another Book revision is
/// refused; a failed apply leaves the Book as it was and the proposal held. A QIF import then saves its account map
/// in the Configuration `store` holds; a failure to save is a warning, as it is in GnuCash
/// (`5.17:gnucash/import-export/qif-imp/assistant-qif-import.c:3533-3535`).
pub fn apply(
    session: &mut Session,
    store: &ConfigStore,
    id: &str,
    changes: &[Change],
    choices: ApplyChoices,
) -> Result<Applied, ServiceError> {
    if let Some(reason) = session.read_only_reasons().first() {
        return Err(ServiceError::ReadOnly {
            reason: reason.to_string(),
        });
    }
    let revision = session.revision();
    let held = session.proposals().current(id, revision)?;
    let mut applied = session.mutate(|book, env| pipeline::apply(book, env, &held.proposal, changes, choices))?;
    session.proposals().remove(id);
    if let Some(Err(e)) = applied.qif_account_map.as_ref().map(|map| qif::save_map(store, map)) {
        applied.warnings.push(Warning {
            code: "qif-account-map",
            message: format!("the QIF account map was not saved: {e}"),
            index: None,
        });
    }
    Ok(applied)
}

/// Drop a held proposal.
pub fn discard(session: &mut Session, id: &str) -> Result<(), ServiceError> {
    session.proposals().discard(id)
}
