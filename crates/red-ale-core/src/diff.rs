//! What changed between two versions of an entity map.
//!
//! A Mutation edits a clone of the Book, so most entries of the old and new maps are the same `Arc` at the same index.
//! Walking both maps in order while their keys line up finds the changes there with a pointer comparison per entry;
//! lookups by GUID are needed only past the first place the keys stop lining up, where an entry was removed or
//! moved. `design/milestone3.md` ("Decided") measured this at about a hundred times faster than looking every entry
//! up, which made it most of a Mutation's fixed cost.

use std::sync::Arc;

use indexmap::IndexMap;

use crate::Guid;

/// How one entry differs between an old and a new map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change<'a, T> {
    /// In the new map only.
    Added(&'a T),
    /// In both, with different contents: the old, then the new.
    Changed(&'a T, &'a T),
    /// In the old map only.
    Removed(&'a T),
}

impl<'a, T> Change<'a, T> {
    /// The entry as it is now, unless it was removed.
    #[must_use]
    pub fn after(&self) -> Option<&'a T> {
        match self {
            Self::Added(t) | Self::Changed(_, t) => Some(t),
            Self::Removed(_) => None,
        }
    }
}

/// Every entry added, changed or removed between `old` and `new`: additions and changes in `new`'s order, then
/// removals in `old`'s order.
#[must_use]
pub fn changes<'a, T: PartialEq>(old: &'a IndexMap<Guid, Arc<T>>, new: &'a IndexMap<Guid, Arc<T>>) -> Vec<Change<'a, T>> {
    let aligned = old.keys().zip(new.keys()).take_while(|(o, n)| o == n).count();
    let in_step = old.values().zip(new.values()).take(aligned).filter_map(|(o, n)| changed(o, n));
    let rest = new.iter().skip(aligned).filter_map(|(guid, n)| match old.get(guid) {
        None => Some(Change::Added(&**n)),
        Some(o) => changed(o, n),
    });
    // Entries before `aligned` are in both maps, so only the old map's tail can hold removals.
    let removed = old
        .iter()
        .skip(aligned)
        .filter(|(guid, _)| !new.contains_key(*guid))
        .map(|(_, o)| Change::Removed(&**o));
    in_step.chain(rest).chain(removed).collect()
}

fn changed<'a, T: PartialEq>(old: &'a Arc<T>, new: &'a Arc<T>) -> Option<Change<'a, T>> {
    (!Arc::ptr_eq(old, new) && **old != **new).then(|| Change::Changed(&**old, &**new))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn map(entries: &[(u8, u32)]) -> IndexMap<Guid, Arc<u32>> {
        entries.iter().map(|&(k, v)| (key(k), Arc::new(v))).collect()
    }

    fn key(k: u8) -> Guid {
        Guid::from_bytes([k; 16])
    }

    /// The result by looking every entry up, as Mutations did before.
    fn by_lookup<'a>(old: &'a IndexMap<Guid, Arc<u32>>, new: &'a IndexMap<Guid, Arc<u32>>) -> Vec<Change<'a, u32>> {
        let changed = new.iter().filter_map(|(g, n)| match old.get(g) {
            None => Some(Change::Added(&**n)),
            Some(o) => changed(o, n),
        });
        let removed = old
            .iter()
            .filter(|(g, _)| !new.contains_key(*g))
            .map(|(_, o)| Change::Removed(&**o));
        changed.chain(removed).collect()
    }

    #[test]
    fn a_shared_entry_is_unchanged_and_an_edit_is_a_change() {
        let old = map(&[(1, 10), (2, 20)]);
        let mut new = old.clone();
        new.insert(key(2), Arc::new(21));
        assert_eq!(changes(&old, &new), vec![Change::Changed(&20, &21)]);
    }

    #[test]
    fn an_equal_copy_is_not_a_change() {
        let old = map(&[(1, 10)]);
        let new = map(&[(1, 10)]);
        assert!(changes(&old, &new).is_empty());
    }

    #[test]
    fn appends_removals_and_moves() {
        let old = map(&[(1, 10), (2, 20), (3, 30)]);
        let mut new = old.clone();
        new.swap_remove(&key(1));
        new.insert(key(4), Arc::new(40));
        assert_eq!(changes(&old, &new), vec![Change::Added(&40), Change::Removed(&10)]);
    }

    proptest! {
        /// Whatever happens to the map, the walk finds what looking every entry up finds, in the same order.
        #[test]
        fn the_walk_finds_what_lookups_find(
            old in proptest::collection::vec((0u8..12, 0u32..4), 0..12),
            edits in proptest::collection::vec((0u8..3, 0u8..14, 0u32..4), 0..8),
        ) {
            let old = map(&old);
            let mut new = old.clone();
            edits.iter().for_each(|&(op, k, v)| match op {
                0 => { new.insert(key(k), Arc::new(v)); }
                1 => { new.swap_remove(&key(k)); }
                _ => { new.shift_remove(&key(k)); }
            });
            prop_assert_eq!(changes(&old, &new), by_lookup(&old, &new));
        }
    }
}
