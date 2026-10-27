//! Slot rows into Frames (`design/sql-fidelity.md` §3, "Slots").
//!
//! A row with `slot_type` 9 (frame) or 8 (list) owns the rows whose `obj_guid` is its `guid_val`. A child's key is its
//! `name` less its parent's path and `/`; list items have the key `""` and keep their `rowid` order; top-level keys
//! are taken whole, so a key holding `/` stays one key (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:197-231`).
//! Rows that don't form such a tree are findings, since the Frame read would not be the rows. Rows owned by nothing
//! read are orphans: GnuCash skips them on load and never touches them (`design/notes/sql-backend.md` §9).

use std::collections::{BTreeMap, BTreeSet};

use red_ale_core::{Frame, Guid, SlotValue};

use crate::rows::{SlotCell, SlotRow};

/// What a GUID that owns slot rows is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// An entity the core gives Slots.
    Entity,
    /// An entity the core gives no Slots (a price): rows it owns could not be kept.
    NoSlots,
    /// Nothing read.
    Unknown,
}

/// The Frames built from the slot rows.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Built {
    /// Each entity's Slots.
    pub frames: BTreeMap<Guid, Frame>,
    /// How many rows each entity's Slots are stored as, its frames' and lists' rows included.
    pub rows: BTreeMap<Guid, usize>,
    /// The owners of rows that belong to nothing read.
    pub orphans: BTreeSet<Guid>,
}

struct Builder<'a> {
    rows: &'a [SlotRow],
    by_owner: BTreeMap<Guid, Vec<usize>>,
    containers: BTreeMap<Guid, usize>,
    visited: BTreeSet<Guid>,
    reached: BTreeSet<usize>,
    findings: &'a mut Vec<String>,
}

/// Build every entity's Slots from `rows`, in `rowid` order. `owner` says what each owning GUID is.
pub fn build(rows: &[SlotRow], owner: &dyn Fn(Guid) -> Owner, findings: &mut Vec<String>) -> Built {
    let mut b = Builder {
        rows,
        by_owner: BTreeMap::new(),
        containers: BTreeMap::new(),
        visited: BTreeSet::new(),
        reached: BTreeSet::new(),
        findings,
    };
    rows.iter().enumerate().for_each(|(i, r)| b.by_owner.entry(r.owner).or_default().push(i));
    b.index_containers();
    let mut built = Built::default();
    let owners: Vec<Guid> = b.by_owner.keys().copied().collect();
    owners.into_iter().for_each(|guid| match owner(guid) {
        Owner::Entity if b.containers.contains_key(&guid) => {
            b.finding(b.by_owner[&guid][0], format!("owner {guid} is both an entity and a frame or list"));
        }
        Owner::Entity => {
            let before = b.reached.len();
            let frame = b.frame(guid, None);
            built.rows.insert(guid, b.reached.len() - before);
            built.frames.insert(guid, frame);
        }
        Owner::NoSlots => b.finding(b.by_owner[&guid][0], format!("owned by {guid}, which has no Slots in red-ale")),
        Owner::Unknown if !b.containers.contains_key(&guid) => {
            built.orphans.insert(guid);
        }
        Owner::Unknown => {}
    });
    built
}

impl Builder<'_> {
    fn finding(&mut self, index: usize, what: impl std::fmt::Display) {
        let rowid = self.rows[index].rowid;
        self.findings.push(format!("slots row {rowid}: {what}"));
    }

    /// Each frame and list GUID with the row that declares it; a GUID declared twice is a finding.
    fn index_containers(&mut self) {
        for (i, row) in self.rows.iter().enumerate() {
            let (SlotCell::Frame(g) | SlotCell::List(g)) = row.cell else { continue };
            if self.containers.insert(g, i).is_some() {
                self.findings.push(format!("slots row {}: frame or list {g} is owned twice", row.rowid));
            }
        }
    }

    fn enter(&mut self, container: Guid, at: usize) -> bool {
        if self.visited.insert(container) {
            true
        } else {
            self.finding(at, format!("frame or list {container} is reached twice"));
            false
        }
    }

    fn children(&self, owner: Guid) -> Vec<usize> {
        self.by_owner.get(&owner).cloned().unwrap_or_default()
    }

    /// The Frame of the rows `owner` owns. `prefix` is the parent's path plus `/`, or `None` at the top level.
    fn frame(&mut self, owner: Guid, prefix: Option<&str>) -> Frame {
        let mut frame = Frame::new();
        for i in self.children(owner) {
            let name = self.rows[i].name.clone();
            let key = match prefix {
                None => Some(name.as_str()),
                Some(p) => name.strip_prefix(p),
            };
            let Some(key) = key.filter(|k| !k.is_empty()) else {
                self.finding(i, format!("name {name:?} is not its parent's path and a key"));
                continue;
            };
            let key = key.to_owned();
            let Some(value) = self.value(i) else { continue };
            if frame.insert_key(key.clone(), value).is_some() {
                self.finding(i, format!("key {key:?} appears twice in one frame"));
            }
        }
        frame
    }

    /// The items of a list, in `rowid` order; each item's name is the list's path plus `/`.
    fn list(&mut self, owner: Guid, prefix: &str) -> Vec<SlotValue> {
        let mut items = Vec::new();
        for i in self.children(owner) {
            if self.rows[i].name != prefix {
                let name = self.rows[i].name.clone();
                self.finding(i, format!("list item {name:?} is not named {prefix:?}"));
                continue;
            }
            items.extend(self.value(i));
        }
        items
    }

    fn value(&mut self, i: usize) -> Option<SlotValue> {
        self.reached.insert(i);
        let path = format!("{}/", self.rows[i].name);
        match self.rows[i].cell.clone() {
            SlotCell::Value(v) => Some(v),
            SlotCell::Frame(g) => self.enter(g, i).then(|| SlotValue::Frame(self.frame(g, Some(&path)))),
            SlotCell::List(g) => self.enter(g, i).then(|| SlotValue::List(self.list(g, &path))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guid(n: u8) -> Guid {
        Guid::from_bytes([n; 16])
    }

    fn row(rowid: i64, owner: u8, name: &str, cell: SlotCell) -> SlotRow {
        SlotRow {
            rowid,
            owner: guid(owner),
            name: name.into(),
            cell,
        }
    }

    fn int(i: i64) -> SlotCell {
        SlotCell::Value(SlotValue::Integer(i))
    }

    fn entity(owners: &'static [u8]) -> impl Fn(Guid) -> Owner {
        move |g| if owners.iter().any(|o| guid(*o) == g) { Owner::Entity } else { Owner::Unknown }
    }

    #[test]
    fn frames_lists_and_slash_keys_build_as_gnucash_reads_them() {
        let rows = [
            row(1, 1, "a/b", int(1)),
            row(2, 1, "f", SlotCell::Frame(guid(10))),
            row(3, 10, "f/x", int(2)),
            row(4, 1, "l", SlotCell::List(guid(11))),
            row(5, 11, "l/", int(3)),
            row(6, 11, "l/", SlotCell::Frame(guid(12))),
            row(7, 12, "l//k", int(4)),
            row(8, 11, "l/", SlotCell::List(guid(13))),
            row(9, 9, "orphan", int(5)),
        ];
        let mut findings = Vec::new();
        let built = build(&rows, &entity(&[1]), &mut findings);
        assert!(findings.is_empty(), "{findings:?}");
        let f = &built.frames[&guid(1)];
        assert_eq!(f.get_key("a/b"), Some(&SlotValue::Integer(1)));
        assert_eq!(f.get_in(&["f", "x"]), Some(&SlotValue::Integer(2)));
        let inner: Frame = [("k".to_owned(), SlotValue::Integer(4))].into_iter().collect();
        assert_eq!(
            f.get_key("l"),
            Some(&SlotValue::List(vec![SlotValue::Integer(3), SlotValue::Frame(inner), SlotValue::List(Vec::new())]))
        );
        assert_eq!(built.rows[&guid(1)], 8);
        assert_eq!(built.orphans, BTreeSet::from([guid(9)]));
    }

    #[test]
    fn rows_that_are_not_a_tree_are_findings() {
        let rows = [
            row(1, 1, "f", SlotCell::Frame(guid(10))),
            row(2, 10, "g/x", int(2)),
            row(3, 1, "dup", int(1)),
            row(4, 1, "dup", int(2)),
            row(5, 1, "again", SlotCell::Frame(guid(10))),
            row(6, 2, "price slot", int(1)),
        ];
        let mut findings = Vec::new();
        let owner = |g: Guid| match g {
            g if g == guid(1) => Owner::Entity,
            g if g == guid(2) => Owner::NoSlots,
            _ => Owner::Unknown,
        };
        build(&rows, &owner, &mut findings);
        assert_eq!(
            findings,
            [
                format!("slots row 5: frame or list {} is owned twice", guid(10)),
                "slots row 2: name \"g/x\" is not its parent's path and a key".to_owned(),
                "slots row 4: key \"dup\" appears twice in one frame".to_owned(),
                format!("slots row 5: frame or list {} is reached twice", guid(10)),
                format!("slots row 6: owned by {}, which has no Slots in red-ale", guid(2)),
            ]
        );
    }

    #[test]
    fn a_cycle_ends() {
        let rows = [row(1, 1, "a", SlotCell::Frame(guid(10))), row(2, 10, "a/b", SlotCell::Frame(guid(10)))];
        let mut findings = Vec::new();
        build(&rows, &entity(&[1]), &mut findings);
        assert!(findings.iter().any(|f| f.contains("owned twice")), "{findings:?}");
    }
}
