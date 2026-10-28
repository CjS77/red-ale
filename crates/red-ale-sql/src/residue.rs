//! The SQL Residue: what the Backend keeps beside the Book so a later save can write only the rows that changed
//! (`design/sql-fidelity.md` §1, "The SQL Residue").
//!
//! Nothing here is needed to read: it is what writing needs that the core does not model. Unchanged rows are never
//! rewritten, so their compact timestamps, NULL-versus-empty strings, slot `id`s and frame GUIDs need no keeping, and
//! a double's eight bytes and its `%.18g` text are in one-to-one correspondence ([`crate::double`]).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use indexmap::IndexMap;
use red_ale_core::{CommodityRef, Guid};

/// The owner GUIDs of rows that belong to nothing read, by table. GnuCash skips them on load and never touches them,
/// and neither does red-ale, except to delete those an inserted entity would adopt (`design/sql-fidelity.md` §4).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Orphans {
    /// `slots.obj_guid` of top-level slot rows owned by no entity.
    pub slots: BTreeSet<Guid>,
    /// `splits.tx_guid` of splits whose transaction has no row.
    pub splits: BTreeSet<Guid>,
    /// `recurrences.obj_guid` owned by no budget or scheduled transaction.
    pub recurrences: BTreeSet<Guid>,
    /// `budget_amounts.budget_guid` of no budget.
    pub budget_amounts: BTreeSet<Guid>,
    /// `taxtable_entries.taxtable` of no tax table.
    pub taxtable_entries: BTreeSet<Guid>,
}

/// What an SQLite book holds that the Book does not.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SqlResidue {
    /// Each commodity row's GUID, in `rowid` order: rows refer to commodities by GUID, which the core does not model.
    pub commodities: IndexMap<CommodityRef, Guid>,
    /// The `books` row's template root, which the Book holds only while it has template accounts (as in XML, where
    /// GnuCash writes it only then).
    pub template_root: Guid,
    /// How many slot rows each entity's Slots are stored as, frames' and lists' rows included: a save that rewrites
    /// an entity's Slots must delete exactly these.
    pub slot_rows: BTreeMap<Guid, usize>,
    /// Rows that belong to nothing read.
    pub orphans: Orphans,
    /// The `versions` rows as read, in `rowid` order, for `book.info`.
    pub versions: Vec<(String, i64)>,
}
