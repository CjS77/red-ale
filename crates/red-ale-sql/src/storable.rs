//! What an SQLite book can't hold (`design/sql-fidelity.md` §8): a Mutation that would need it is refused, with the
//! reason, rather than losing it at the save.
//!
//! Only what the Mutation changed is checked: anything read from the file was storable, and a nested list read from it
//! is kept as read.

use red_ale_core::diff::{Change, changes};
use red_ale_core::{Book, Frame, SlotValue, Time64};

use crate::cell::Cell;
use crate::encode::{self, Refs, business as biz, time_in_range};
use crate::save::btree_changes;

/// One added or changed entity: its row, its Slots before and after, and its times.
struct Item<'a> {
    what: String,
    row: Vec<Cell>,
    slots: Option<(Option<&'a Frame>, &'a Frame)>,
    times: Vec<Time64>,
}

fn after<'a, T>(c: &Change<'a, T>) -> Option<(Option<&'a T>, &'a T)> {
    match c {
        Change::Added(n) => Some((None, n)),
        Change::Changed(o, n) => Some((Some(o), n)),
        Change::Removed(_) => None,
    }
}

/// True when a value is a list holding a list or a frame, anywhere inside it: GnuCash writes such rows but does not
/// read them back as written (`design/notes/sql-backend.md` §9, experiments S1 and E1b).
fn nests_in_list(v: &SlotValue) -> bool {
    match v {
        SlotValue::List(items) => items.iter().any(|i| matches!(i, SlotValue::List(_) | SlotValue::Frame(_)) || nests_in_list(i)),
        SlotValue::Frame(f) => f.iter().any(|(_, v)| nests_in_list(v)),
        _ => false,
    }
}

/// The first key of `new` whose value changed and holds a list nested in a list.
fn new_nesting(old: Option<&Frame>, new: &Frame, prefix: &str) -> Option<String> {
    new.iter().find_map(|(key, value)| {
        let before = old.and_then(|o| o.get_key(key));
        let path = format!("{prefix}{key}");
        match (before, value) {
            (Some(b), v) if b == v => None,
            (Some(SlotValue::Frame(b)), SlotValue::Frame(f)) => new_nesting(Some(b), f, &format!("{path}/")),
            (None, SlotValue::Frame(f)) => new_nesting(None, f, &format!("{path}/")),
            (_, v) => nests_in_list(v).then_some(path),
        }
    })
}

/// Every time in a Frame.
fn slot_times(frame: &Frame, out: &mut Vec<Time64>) {
    fn value(v: &SlotValue, out: &mut Vec<Time64>) {
        match v {
            SlotValue::Timespec(t) => out.push(*t),
            SlotValue::Frame(f) => slot_times(f, out),
            SlotValue::List(items) => items.iter().for_each(|i| value(i, out)),
            _ => {}
        }
    }
    frame.iter().for_each(|(_, v)| value(v, out));
}

/// Why this item can't be stored, if it can't.
fn refusal(item: &Item<'_>) -> Option<String> {
    let what = &item.what;
    let mut times = item.times.clone();
    if let Some((old, new)) = item.slots {
        if let Some(path) = new_nesting(old, new, "") {
            return Some(format!(
                "{what}: the slot {path:?} holds a list inside a list, which GnuCash does not read back from an SQLite book"
            ));
        }
        slot_times(new, &mut times);
        let mut n = 0_u128;
        let rows = encode::slots::rows(red_ale_core::Guid::NULL, new, &mut || {
            n += 1;
            red_ale_core::Guid::from_bytes(n.to_be_bytes())
        });
        match rows {
            Err(e) => return Some(format!("{what}: {e}")),
            Ok(rows) if rows.iter().flatten().any(has_nul) => {
                return Some(format!("{what}: a slot holds a NUL character, which GnuCash would cut short"));
            }
            Ok(_) => {}
        }
    }
    if item.row.iter().any(has_nul) {
        return Some(format!("{what}: a text holds a NUL character, which GnuCash would cut short"));
    }
    times
        .iter()
        .find(|t| !time_in_range(**t))
        .map(|t| {
            let t = t.0;
            format!("{what}: the time {t} is outside the range GnuCash can store in an SQLite book, which it would read back as 1970")
        })
}

/// An entity with no times of its own.
fn no_times<T>(_: &T) -> Vec<Time64> {
    Vec::new()
}

fn has_nul(c: &Cell) -> bool {
    matches!(c, Cell::Text(t) if t.contains(&0))
}

/// Why `after` can't be stored in an SQLite book, if it can't: checks what changed from `before`.
pub(crate) fn unstorable(before: &Book, after_book: &Book) -> Option<String> {
    if let Some(sx) = changes(&before.scheduled, &after_book.scheduled).iter().filter_map(Change::after).find(|s| !s.deferred.is_empty()) {
        return Some(format!(
            "scheduled transaction {} would have postponed instances, which an SQLite book has no place for \
             (GnuCash's SQL backend drops them)",
            sx.guid
        ));
    }
    items(before, after_book).iter().find_map(refusal)
}

/// Every added or changed entity, with what the checks need.
fn items<'a>(before: &'a Book, book: &'a Book) -> Vec<Item<'a>> {
    let none = indexmap::IndexMap::new();
    let refs = Refs { commodities: &none };
    let mut out = Vec::new();
    let mut push = |what: String, row: Vec<Cell>, slots: Option<(Option<&'a Frame>, &'a Frame)>, times: Vec<Time64>| {
        out.push(Item { what, row, slots, times });
    };
    if before.slots != book.slots {
        push("the book".into(), Vec::new(), Some((Some(&before.slots), &book.slots)), Vec::new());
    }
    btree_changes(&before.commodities, &book.commodities).iter().filter_map(after).for_each(|(o, n)| {
        let cells = encode::commodity(red_ale_core::Guid::NULL, n);
        push(format!("commodity {}", n.id), cells, Some((o.map(|o| &o.slots), &n.slots)), Vec::new());
    });
    let accounts =
        changes(&before.accounts, &book.accounts).into_iter().chain(changes(&before.templates.accounts, &book.templates.accounts));
    accounts.filter_map(|c| after(&c)).for_each(|(o, n)| {
        push(format!("account {}", n.guid), encode::account(n, refs), Some((o.map(|o| &o.slots), &n.slots)), Vec::new());
    });
    let templates = changes(&before.templates.transactions, &book.templates.transactions);
    let transactions = changes(&before.transactions, &book.transactions).into_iter().chain(templates);
    transactions.filter_map(|c| after(&c)).for_each(|(o, n)| {
        let slots = Some((o.map(|o| &o.slots), &n.slots));
        push(format!("transaction {}", n.guid), encode::transaction(n, refs), slots, vec![n.date_posted, n.date_entered]);
        n.splits.iter().for_each(|s| {
            let old = o.and_then(|o| o.split(s.guid)).map(|s| &s.slots);
            push(format!("split {}", s.guid), encode::split(n.guid, s), Some((old, &s.slots)), s.reconcile_date.into_iter().collect());
        });
    });
    changes(&before.prices, &book.prices).iter().filter_map(after).for_each(|(_, n)| {
        push(format!("price {}", n.guid), encode::price(n, refs), None, vec![n.time]);
    });
    changes(&before.lots, &book.lots).iter().filter_map(after).for_each(|(o, n)| {
        push(format!("lot {}", n.guid), Vec::new(), Some((o.map(|o| &o.slots), &n.slots)), Vec::new());
    });
    changes(&before.budgets, &book.budgets).iter().filter_map(after).for_each(|(o, n)| {
        push(format!("budget {}", n.guid), encode::budget(n), Some((o.map(|o| &o.slots), &n.slots)), Vec::new());
    });
    changes(&before.scheduled, &book.scheduled).iter().filter_map(after).for_each(|(o, n)| {
        push(format!("scheduled transaction {}", n.guid), encode::scheduled(n), Some((o.map(|o| &o.slots), &n.slots)), Vec::new());
    });
    let (b, a) = (&before.business, &book.business);
    macro_rules! business {
        ($field:ident, $what:literal, $encode:expr, $times:expr) => {
            btree_changes(&b.$field, &a.$field).iter().filter_map(after).for_each(|(o, n)| {
                push(format!("{} {}", $what, n.guid), $encode(n), Some((o.map(|o| &o.slots), &n.slots)), $times(n));
            });
        };
    }
    business!(bill_terms, "bill term", biz::bill_term, no_times);
    business!(customers, "customer", |x| biz::customer(x, refs), no_times);
    business!(employees, "employee", |x| biz::employee(x, refs), no_times);
    business!(entries, "entry", biz::entry, |x: &red_ale_core::business::Entry| vec![x.date, x.entered]);
    business!(invoices, "invoice", |x| biz::invoice(x, refs), |x: &red_ale_core::business::Invoice| {
        std::iter::once(x.opened).chain(x.posted).collect::<Vec<_>>()
    });
    business!(jobs, "job", biz::job, no_times);
    business!(orders, "order", biz::order, |x: &red_ale_core::business::Order| {
        std::iter::once(x.opened).chain(x.closed).collect::<Vec<_>>()
    });
    business!(tax_tables, "tax table", biz::tax_table, no_times);
    business!(vendors, "vendor", |x| biz::vendor(x, refs), no_times);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_lists_are_refused_only_when_set() {
        let nested = SlotValue::List(vec![SlotValue::List(Vec::new())]);
        let mut old = Frame::new();
        old.insert_key("kept", nested.clone());
        let mut new = old.clone();
        assert_eq!(new_nesting(Some(&old), &new, ""), None);
        new.insert_key("flat", SlotValue::List(vec![SlotValue::Integer(1)]));
        assert_eq!(new_nesting(Some(&old), &new, ""), None);
        let mut inner = Frame::new();
        inner.insert_key("deep", SlotValue::List(vec![SlotValue::Frame(Frame::new())]));
        new.insert_key("f", SlotValue::Frame(inner));
        assert_eq!(new_nesting(Some(&old), &new, "").as_deref(), Some("f/deep"));
    }
}
