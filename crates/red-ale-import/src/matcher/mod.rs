//! The generic import matcher: what each imported transaction does to the Book, decided as GnuCash's does.
//!
//! Translated from GnuCash's generic import backend and the non-GUI parts of its matcher window
//! (`5.17:gnucash/import-export/import-backend.cpp`, GPL-2.0-or-later, Copyright (C) 2002 Benoit Grégoire, Christian
//! Stimming, Copyright (c) 2006 David Hampton; `5.17:gnucash/import-export/import-main-matcher.cpp`, the same and
//! Copyright (C) 2012 Robert Fewell). Each function cites the lines it follows. Spec: `design/api/import.md`,
//! "The matcher".
//!
//! | Module | GnuCash |
//! |--------|---------|
//! | [`tokens`] | `tokenize_string`, `TransactionGetTokens` |
//! | [`score`] | `split_find_match` |
//! | [`trans_info`] | `GNCImportTransInfo`: its constructor, destination lookup, dest amount, `init_matches` |
//! | [`batch`] | `gnc_gen_trans_list_add_trans_internal`, the candidate query, `perform_matching`, `resolve_conflicts` |
//! | [`process`] | `gnc_import_process_trans_item`, `process_reconcile`, `update_desc_and_notes`, `matchmap_store_destination`, `on_matcher_ok_clicked` |
//!
//! # Use
//!
//! [`match_batch`] takes the imported transactions in the order the importer builds them and returns one
//! [`TransInfo`] per transaction: its candidates with their scores, the selected match, the Import action, the
//! destination account and the balancing split's value and amount. The caller may then change an action, a match or
//! the destination ([`TransInfo::set_action`], [`TransInfo::select_match`], [`TransInfo::set_destination`],
//! [`TransInfo::set_dest_amount`]), and [`process::apply_batch`] applies them to a `&mut Book` inside one Mutation.
//!
//! # Order
//!
//! GnuCash prepends each arriving transaction to its list and walks that list to match, resolve conflicts and apply,
//! so it works in reverse order of arrival. [`match_batch`] and [`process::apply_batch`] do the same internally;
//! their inputs and outputs are in arrival order, each [`TransInfo`] keeping its [`TransInfo::index`].
//!
//! # Deviations from GnuCash
//!
//! Only those `design/api/import.md` lists, each documented where it is made: amounts compared exactly
//! ([`score`]); folded text compared bytewise ([`score`]); learning at apply, not when an account is picked
//! ([`process`]); no imported transaction to destroy, as it was never in the Book.

pub mod batch;
pub mod process;
pub mod score;
pub mod tokens;
pub mod trans_info;

pub use batch::{ImportedTransaction, MatchBatch, OnlineIds, match_batch};
pub use process::{ApplyOptions, Learned, ProcessWarning, Processed, append_text, apply_batch, process_trans_item, set_append_text};
pub use score::split_find_match;
pub use tokens::transaction_tokens;
pub use trans_info::{DestSource, LastSplitInfo, TransInfo};

use red_ale_core::{Guid, Numeric};

/// What applying an imported transaction does (`GNCImportAction`, `5.17:gnucash/import-export/import-backend.h:60-68`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportAction {
    /// Leave it out.
    Skip,
    /// Add it as a new transaction.
    Add,
    /// Mark the matched split cleared.
    Clear,
    /// Update the matched transaction from it, then clear it ("update and clear").
    Update,
}

impl ImportAction {
    /// The name `design/api/import.md` uses: `skip`, `add`, `clear`, `update`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skip => "skip",
            Self::Add => "add",
            Self::Clear => "clear",
            Self::Update => "update",
        }
    }
}

/// A Candidate match with its Match score (`GNCImportMatchInfo`, `5.17:gnucash/import-export/import-backend.h:40-47`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchInfo {
    /// The existing split, in the imported split's account.
    pub split: Guid,
    /// Its transaction.
    pub transaction: Guid,
    /// The Match score.
    pub probability: i32,
    /// True when the amount and date tests did not both score their best: *update* is then offered.
    pub update_proposed: bool,
}

/// The Import settings (`GNCImportSettings`, `5.17:gnucash/import-export/import-settings.cpp:47-110`), with GnuCash's
/// defaults (`5.17:gnucash/gschemas/org.gnucash.GnuCash.dialogs.import.generic.gschema.xml.in:3-47`). GnuCash reads
/// the thresholds as doubles and truncates them to integers; red-ale's Configuration holds them as integers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSettings {
    /// `enable-skip`: *skip* may be chosen for a middling best score.
    pub enable_skip: bool,
    /// `enable-update`: *update* may be chosen.
    pub enable_update: bool,
    /// `use-bayes`: find and learn destinations with the Bayesian Import map, else the plain one.
    pub use_bayes: bool,
    /// `match-threshold`: the least score a candidate needs to be kept (`display_threshold`).
    pub match_threshold: i32,
    /// `match-date-threshold`: days within which dates add 2.
    pub match_date_threshold: i32,
    /// `match-date-not-threshold`: days beyond which dates take 5 off.
    pub match_date_not_threshold: i32,
    /// `auto-add-threshold`: at or below this best score, *add*.
    pub auto_add_threshold: i32,
    /// `auto-clear-threshold`: at or above this best score, *clear* or *update*.
    pub auto_clear_threshold: i32,
    /// `atm-fee-threshold`: amounts this far apart still add 2. GnuCash keeps it as a double; red-ale exactly.
    pub atm_fee_threshold: Numeric,
    /// The candidate window in days either side of the batch (`match_date_hardlimit`): 42, as the CSV importer sets
    /// it (`5.17:gnucash/import-export/csv-imp/assistant-csv-trans-import.cpp:671-672`). Not a preference.
    pub match_date_hardlimit: i32,
}

impl Default for ImportSettings {
    fn default() -> Self {
        Self {
            enable_skip: true,
            enable_update: true,
            use_bayes: true,
            match_threshold: 1,
            match_date_threshold: 4,
            match_date_not_threshold: 14,
            auto_add_threshold: 3,
            auto_clear_threshold: 6,
            atm_fee_threshold: Numeric::new(200, 100),
            match_date_hardlimit: 42,
        }
    }
}

#[cfg(test)]
mod tests;
