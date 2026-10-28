//! An entity's Slots into `slots` rows, as GnuCash writes them (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:580-653`;
//! `design/sql-fidelity.md` §4, "Slots").
//!
//! A Frame's slots go in key byte order, depth first, list items in list order, so `id`s ascend in that order. Each
//! frame and list row gets a new GUID, which owns its children; a child's name is its parent's path, `/`, then its key,
//! and list items have the key `""`. Every row fills the columns not of its type as GnuCash does: `int64_val` 0,
//! `timespec_val` `1970-01-01 00:00:00`, `numeric_val_num` 0, `numeric_val_denom` 1, and NULL in the others.

use red_ale_core::{Frame, Guid, SlotValue};

use super::{date, guid, int, text, time};
use crate::cell::Cell;
use crate::error::SqlError;

/// One `slots` row, without its `id`: the cells from `obj_guid` to `gdate_val`.
pub type SlotCells = Vec<Cell>;

/// `slot_type`, `KvpValue::Type` (`5.17:libgnucash/engine/kvp-value.hpp:55-67`).
fn slot_type(v: &SlotValue) -> i64 {
    match v {
        SlotValue::Integer(_) => 1,
        SlotValue::Double(_) => 2,
        SlotValue::Numeric(_) => 3,
        SlotValue::String(_) => 4,
        SlotValue::Guid(_) => 5,
        SlotValue::Timespec(_) => 6,
        SlotValue::List(_) => 8,
        SlotValue::Frame(_) => 9,
        SlotValue::Date(_) => 10,
    }
}

/// The REAL a double slot is written as: the double its text names. A double read from the file has the text of its
/// eight bytes, so this gives them back.
fn double(text: &str) -> Result<f64, SqlError> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|d| !d.is_nan())
        .ok_or_else(|| SqlError::Unstorable(format!("the double slot {text:?} is not a number SQLite can store")))
}

/// Builds the rows of one entity's Slots.
struct Writer<'a> {
    new_guid: &'a mut dyn FnMut() -> Guid,
    rows: Vec<SlotCells>,
}

impl Writer<'_> {
    fn frame(&mut self, owner: Guid, prefix: Option<&str>, frame: &Frame) -> Result<(), SqlError> {
        frame.iter().try_for_each(|(key, value)| {
            let name = prefix.map_or_else(|| key.clone(), |p| format!("{p}/{key}"));
            self.value(owner, &name, value)
        })
    }

    fn value(&mut self, owner: Guid, name: &str, value: &SlotValue) -> Result<(), SqlError> {
        let mut cells = vec![
            guid(owner),
            text(name),
            int(slot_type(value)),
            int(0),
            Cell::Null,
            Cell::Null,
            text("1970-01-01 00:00:00"),
            Cell::Null,
            int(0),
            int(1),
            Cell::Null,
        ];
        let container = match value {
            SlotValue::Integer(i) => {
                cells[3] = int(*i);
                None
            }
            SlotValue::Double(d) => {
                cells[5] = Cell::Real(double(d)?);
                None
            }
            SlotValue::Numeric(n) => {
                cells[8] = int(n.num());
                cells[9] = int(n.denom());
                None
            }
            SlotValue::String(s) => {
                cells[4] = text(s);
                None
            }
            SlotValue::Guid(g) => {
                cells[7] = guid(*g);
                None
            }
            SlotValue::Timespec(t) => {
                cells[6] = time(*t);
                None
            }
            SlotValue::Date(d) => {
                cells[10] = date(Some(*d));
                None
            }
            SlotValue::List(_) | SlotValue::Frame(_) => {
                let g = (self.new_guid)();
                cells[7] = guid(g);
                Some(g)
            }
        };
        self.rows.push(cells);
        match (value, container) {
            (SlotValue::Frame(f), Some(g)) => self.frame(g, Some(name), f),
            (SlotValue::List(items), Some(g)) => {
                let item_name = format!("{name}/");
                items.iter().try_for_each(|item| self.value(g, &item_name, item))
            }
            _ => Ok(()),
        }
    }
}

/// The rows of `owner`'s Slots, in GnuCash's order, with a new GUID from `new_guid` for each frame and list.
pub fn rows(owner: Guid, frame: &Frame, new_guid: &mut dyn FnMut() -> Guid) -> Result<Vec<SlotCells>, SqlError> {
    let mut w = Writer {
        new_guid,
        rows: Vec::new(),
    };
    w.frame(owner, None, frame)?;
    Ok(w.rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Row;
    use crate::rows::slot;
    use crate::slots::{Owner, build};

    #[test]
    fn rows_read_back_as_the_frame() {
        let mut inner = Frame::new();
        inner.insert_key("k", SlotValue::Integer(4));
        let mut frame = Frame::new();
        frame.insert_key("a/b", SlotValue::String("x".into()));
        frame.insert_key("Zupper", SlotValue::Double("0.100000000000000006".into()));
        frame.insert_key(
            "l",
            SlotValue::List(vec![SlotValue::Integer(3), SlotValue::Frame(inner), SlotValue::List(Vec::new())]),
        );
        frame.insert_key("f", SlotValue::Frame(Frame::new()));
        let owner = Guid::from_bytes([1; 16]);
        let mut n = 1_u8;
        let mut next = || {
            n += 1;
            Guid::from_bytes([n; 16])
        };
        let cells = rows(owner, &frame, &mut next).unwrap();
        let names: Vec<_> = cells.iter().map(|c| c[1].clone()).collect();
        assert_eq!(names, ["Zupper", "a/b", "f", "l", "l/", "l/", "l//k", "l/"].map(text));
        let mut findings = Vec::new();
        let decoded: Vec<_> = cells
            .into_iter()
            .enumerate()
            .filter_map(|(i, mut c)| {
                c.insert(0, int(i64::try_from(i).unwrap()));
                slot(
                    &Row {
                        rowid: i64::try_from(i).unwrap(),
                        cells: c,
                    },
                    &mut findings,
                )
            })
            .collect();
        let built = build(
            &decoded,
            &|g| if g == owner { Owner::Entity } else { Owner::Unknown },
            &mut findings,
        );
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(built.frames[&owner], frame);
        assert_eq!(built.rows[&owner], 8);
    }
}
