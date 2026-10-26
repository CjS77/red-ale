//! The Import proposals a session holds (`design/milestone3.md`, "Decided", item 3): by ID, each with the Book
//! revision it was made at, until it is applied, discarded, pushed out by [`KEPT`] newer ones, or the book closes
//! (the store lives in the session, so closing drops it).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use red_ale_import::pipeline::Proposal;

/// How many proposals are held; a newer one pushes out the oldest.
pub const KEPT: usize = 16;

/// The number of the next proposal, for the whole process: an ID is never reused, so one made before the book was
/// opened (or in another session) is never mistaken for one of this session's.
static NEXT: AtomicU64 = AtomicU64::new(1);

/// One held proposal.
#[derive(Debug, Clone)]
pub struct Held {
    /// Its ID.
    pub id: String,
    /// The Book revision it was made at.
    pub revision: u64,
    /// The proposal.
    pub proposal: Arc<Proposal>,
}

/// The held proposals, oldest first.
#[derive(Debug, Default)]
pub struct ProposalStore {
    held: VecDeque<Held>,
}

impl ProposalStore {
    /// Hold a proposal made at `revision`, pushing out the oldest beyond [`KEPT`]. Returns its ID.
    pub fn insert(&mut self, revision: u64, proposal: Arc<Proposal>) -> String {
        let id = format!("imp-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        self.held.push_back(Held { id: id.clone(), revision, proposal });
        while self.held.len() > KEPT {
            self.held.pop_front();
        }
        id
    }

    /// The proposal with this ID.
    pub fn get(&self, id: &str) -> Option<&Held> {
        self.held.iter().find(|h| h.id == id)
    }

    /// Drop the proposal with this ID; false when none is held.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.held.len();
        self.held.retain(|h| h.id != id);
        self.held.len() != before
    }
}

#[cfg(test)]
mod tests {
    use red_ale_import::matcher::ImportSettings;
    use red_ale_import::pipeline::Format;

    use super::*;

    fn proposal() -> Arc<Proposal> {
        Arc::new(Proposal {
            format: Format::CsvTransactions,
            settings: ImportSettings::default(),
            threshold: None,
            ignored_lines: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            transactions: Vec::new(),
            csv: None,
            qif: None,
        })
    }

    #[test]
    fn sixteen_are_kept_and_ids_are_never_reused() {
        let mut store = ProposalStore::default();
        let ids: Vec<String> = (0..=KEPT).map(|r| store.insert(r as u64, proposal())).collect();
        assert!(store.get(&ids[0]).is_none(), "the oldest was pushed out by the seventeenth");
        assert!(ids[1..].iter().all(|id| store.get(id).is_some()));
        assert_eq!(store.get(&ids[KEPT]).map(|h| h.revision), Some(KEPT as u64));
        assert!(store.remove(&ids[3]) && !store.remove(&ids[3]));
        let mut other = ProposalStore::default();
        assert!(!ids.contains(&other.insert(0, proposal())), "another session's IDs are new");
    }
}
