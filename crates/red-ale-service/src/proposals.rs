//! The proposals a session holds: Import proposals (`design/milestone3.md`, "Decided", item 3) and Since Last Run
//! proposals (`design/api/scheduled-transactions.md`, "Since Last Run"), each kind in its own store so neither pushes
//! out the other. A proposal is held by ID, with the Book revision it was made at, until it is applied, discarded,
//! pushed out by [`KEPT`] newer ones of its kind, or the book closes (the stores live in the session, so closing
//! drops them).

use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ServiceError;

/// How many proposals of one kind are held; a newer one pushes out the oldest.
pub const KEPT: usize = 16;

/// The number of the next proposal, for the whole process: an ID is never reused, so one made before the book was
/// opened (or in another session) is never mistaken for one of this session's.
static NEXT: AtomicU64 = AtomicU64::new(1);

/// The kinds of proposal a session holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalKind {
    /// An Import proposal, from `import.preview`.
    Import,
    /// A Since Last Run proposal, from `scheduledTransaction.sinceLastRun`.
    SinceLastRun,
}

impl ProposalKind {
    /// The `data.kind` of a `not-found` error for it.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Import => "importProposal",
            Self::SinceLastRun => "sinceLastRunProposal",
        }
    }

    /// What the caller does to get a new one.
    pub fn again(self) -> &'static str {
        match self {
            Self::Import => "preview the file again",
            Self::SinceLastRun => "run Since Last Run again",
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::Import => "imp",
            Self::SinceLastRun => "slr",
        }
    }
}

impl fmt::Display for ProposalKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Import => "import proposal",
            Self::SinceLastRun => "Since Last Run proposal",
        })
    }
}

/// One held proposal.
#[derive(Debug)]
pub struct Held<P> {
    /// Its ID.
    pub id: String,
    /// The Book revision it was made at.
    pub revision: u64,
    /// The proposal.
    pub proposal: Arc<P>,
}

impl<P> Clone for Held<P> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            revision: self.revision,
            proposal: Arc::clone(&self.proposal),
        }
    }
}

/// The held proposals of one kind, oldest first.
#[derive(Debug)]
pub struct ProposalStore<P> {
    kind: ProposalKind,
    held: VecDeque<Held<P>>,
}

impl<P> ProposalStore<P> {
    /// An empty store for proposals of `kind`.
    pub fn new(kind: ProposalKind) -> Self {
        Self {
            kind,
            held: VecDeque::new(),
        }
    }

    /// Hold a proposal made at `revision`, pushing out the oldest beyond [`KEPT`]. Returns its ID.
    pub fn insert(&mut self, revision: u64, proposal: Arc<P>) -> String {
        let id = format!("{}-{}", self.kind.prefix(), NEXT.fetch_add(1, Ordering::Relaxed));
        self.held.push_back(Held {
            id: id.clone(),
            revision,
            proposal,
        });
        while self.held.len() > KEPT {
            self.held.pop_front();
        }
        id
    }

    /// The proposal with this ID.
    pub fn get(&self, id: &str) -> Option<&Held<P>> {
        self.held.iter().find(|h| h.id == id)
    }

    /// The proposal with this ID, if it was made at `revision`: not held is `ProposalNotFound`, made at another
    /// revision `StaleProposal`.
    pub fn current(&self, id: &str, revision: u64) -> Result<Held<P>, ServiceError> {
        let held = self.get(id).cloned().ok_or_else(|| self.not_found(id))?;
        if held.revision == revision {
            Ok(held)
        } else {
            Err(ServiceError::StaleProposal {
                kind: self.kind,
                id: id.to_owned(),
                proposal_revision: held.revision,
                book_revision: revision,
            })
        }
    }

    /// Drop the proposal with this ID; false when none is held.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.held.len();
        self.held.retain(|h| h.id != id);
        self.held.len() != before
    }

    /// Drop the proposal with this ID; `ProposalNotFound` when none is held.
    pub fn discard(&mut self, id: &str) -> Result<(), ServiceError> {
        if self.remove(id) { Ok(()) } else { Err(self.not_found(id)) }
    }

    fn not_found(&self, id: &str) -> ServiceError {
        ServiceError::ProposalNotFound {
            kind: self.kind,
            id: id.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixteen_are_kept_and_ids_are_never_reused() {
        let mut store = ProposalStore::new(ProposalKind::Import);
        let ids: Vec<String> = (0..=KEPT).map(|r| store.insert(r as u64, Arc::new(r))).collect();
        assert!(store.get(&ids[0]).is_none(), "the oldest was pushed out by the seventeenth");
        assert!(ids[1..].iter().all(|id| store.get(id).is_some()));
        assert_eq!(store.get(&ids[KEPT]).map(|h| h.revision), Some(KEPT as u64));
        assert!(store.remove(&ids[3]) && !store.remove(&ids[3]));
        let mut other = ProposalStore::new(ProposalKind::Import);
        assert!(!ids.contains(&other.insert(0, Arc::new(0))), "another session's IDs are new");
    }

    #[test]
    fn kinds_are_named_apart_and_stale_ones_refused() {
        let mut store = ProposalStore::new(ProposalKind::SinceLastRun);
        let id = store.insert(4, Arc::new(()));
        assert!(id.starts_with("slr-"), "{id}");
        assert!(store.current(&id, 4).is_ok());
        assert!(matches!(
            store.current(&id, 5),
            Err(ServiceError::StaleProposal {
                kind: ProposalKind::SinceLastRun,
                proposal_revision: 4,
                book_revision: 5,
                ..
            })
        ));
        assert!(store.discard(&id).is_ok());
        assert!(matches!(
            store.discard(&id),
            Err(ServiceError::ProposalNotFound {
                kind: ProposalKind::SinceLastRun,
                ..
            })
        ));
    }
}
