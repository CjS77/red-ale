//! `book.split_query(q)` (`design/reports.md` §5, "Book"): the reports' split query over
//! `ops::transactions::find::period_splits`.

use red_ale_core::ReconcileState;
use red_ale_core::ops::transactions::find::{DescriptionPattern, PeriodQuery, period_splits};
use red_ale_core::ops::transactions::sort::{SortKey, SortLevel};
use rhai::{Array, Dynamic, ImmutableString, Map};

use super::args::{account_arg, accounts_arg, optional_time_arg};
use super::{BookHandle, Fallible, script_error};

const FIELDS: [&str; 9] = ["accounts", "from", "to", "description", "closing", "reconcile", "isClosing", "sort", "unique"];

/// The sort keys by their script names, each a QOF sort parameter (`SortKey`'s documentation cites them).
const SORT_KEYS: [(&str, SortKey); 14] = [
    ("accountFullName", SortKey::AccountFullName),
    ("accountCode", SortKey::AccountCode),
    ("corrAccountFullName", SortKey::CorrAccountFullName),
    ("corrAccountCode", SortKey::CorrAccountCode),
    ("datePosted", SortKey::DatePosted),
    ("dateEntered", SortKey::DateEntered),
    ("dateReconciled", SortKey::DateReconciled),
    ("value", SortKey::Value),
    ("amount", SortKey::Amount),
    ("num", SortKey::Num),
    ("action", SortKey::Action),
    ("memo", SortKey::Memo),
    ("description", SortKey::Description),
    ("default", SortKey::Default),
];

/// The split query of `gnc:account-get-trans-type-splits-interval` and of the Transaction Report engine
/// (`5.17:gnucash/report/trep-engine.scm:2533-2553`): `#{ accounts, from, to, description, closing, reconcile,
/// isClosing, sort, unique }`.
pub(super) fn split_query(b: &mut BookHandle, q: Map) -> Fallible<Array> {
    let book = b.0.book();
    if let Some(key) = q.keys().find(|k| !FIELDS.contains(&k.as_str())) {
        return Err(script_error(format!("unknown split query field {key:?}")));
    }
    let field = |name: &str| q.get(name).cloned().unwrap_or(Dynamic::UNIT);
    let accounts = field("accounts");
    let accounts = match accounts.read_lock::<Array>() {
        Some(items) => accounts_arg(book, &items)?,
        None if accounts.is_unit() => Vec::new(),
        None => vec![account_arg(book, &accounts)?],
    };
    let query = PeriodQuery {
        accounts,
        from: optional_time_arg(&field("from"))?,
        to: optional_time_arg(&field("to"))?,
        description: description(&field("description"))?,
        closing: optional_bool(&field("closing"), "closing")?.unwrap_or(false),
        reconcile: reconcile_states(&field("reconcile"))?,
        is_closing: optional_bool(&field("isClosing"), "isClosing")?,
        sort: sort_levels(&field("sort"))?,
        unique: optional_bool(&field("unique"), "unique")?.unwrap_or(false),
    };
    let found = period_splits(book, &query).map_err(|e| script_error(e.to_string()))?;
    Ok(b.splits(found))
}

/// `true`, `false`, or `()` for none.
fn optional_bool(v: &Dynamic, name: &str) -> Fallible<Option<bool>> {
    if v.is_unit() {
        return Ok(None);
    }
    v.as_bool()
        .map(Some)
        .map_err(|_| script_error(format!("{name} must be true or false")))
}

/// `#{ text, cased, regexp }`, or `()` for none.
fn description(v: &Dynamic) -> Fallible<Option<DescriptionPattern>> {
    if v.is_unit() {
        return Ok(None);
    }
    let map = v
        .read_lock::<Map>()
        .ok_or_else(|| script_error("description must be #{ text, cased, regexp }"))?;
    let flag = |name: &str| optional_bool(&map.get(name).cloned().unwrap_or(Dynamic::UNIT), name).map(Option::unwrap_or_default);
    let text = map
        .get("text")
        .and_then(|t| t.read_lock::<ImmutableString>().map(|s| s.to_string()));
    Ok(Some(DescriptionPattern {
        text: text.ok_or_else(|| script_error("description needs its text"))?,
        cased: flag("cased")?,
        regexp: flag("regexp")?,
    }))
}

/// The states a split may be in, as `transaction.find`'s `reconcile.states` names them (`"n"`, `"c"`, `"y"`, `"f"`,
/// `"v"`), or `()` for every state but voided.
fn reconcile_states(v: &Dynamic) -> Fallible<Option<Vec<ReconcileState>>> {
    if v.is_unit() {
        return Ok(None);
    }
    let items = v
        .read_lock::<Array>()
        .ok_or_else(|| script_error("reconcile must be an array of states such as \"n\""))?;
    let state = |s: &Dynamic| {
        let text = s.read_lock::<ImmutableString>().map(|t| t.to_string()).unwrap_or_default();
        let mut chars = text.chars();
        match (chars.next().and_then(ReconcileState::from_char), chars.next()) {
            (Some(state), None) => Ok(state),
            _ => Err(script_error(format!("not a reconcile state: {s}"))),
        }
    };
    items.iter().map(state).collect::<Fallible<_>>().map(Some)
}

/// `[#{ key, ascending }, …]`: at most three levels, primary first; `ascending` defaults to true; the key `"none"`
/// compares equal. `()` is the default sort.
fn sort_levels(v: &Dynamic) -> Fallible<Vec<SortLevel>> {
    if v.is_unit() {
        return Ok(Vec::new());
    }
    let items = v
        .read_lock::<Array>()
        .ok_or_else(|| script_error("sort must be an array of #{ key, ascending }"))?;
    if items.len() > 3 {
        return Err(script_error("sort has at most three levels"));
    }
    items.iter().map(sort_level).collect()
}

fn sort_level(v: &Dynamic) -> Fallible<SortLevel> {
    let map = v
        .read_lock::<Map>()
        .ok_or_else(|| script_error("a sort level is #{ key, ascending }"))?;
    if let Some(k) = map.keys().find(|k| !["key", "ascending"].contains(&k.as_str())) {
        return Err(script_error(format!("unknown sort level field {k:?}")));
    }
    let name = map
        .get("key")
        .and_then(|k| k.read_lock::<ImmutableString>().map(|s| s.to_string()))
        .ok_or_else(|| script_error("a sort level needs its key"))?;
    let key = match name.as_str() {
        "none" => None,
        _ => Some(
            SORT_KEYS
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, k)| *k)
                .ok_or_else(|| script_error(format!("unknown sort key {name:?}")))?,
        ),
    };
    let ascending = optional_bool(&map.get("ascending").cloned().unwrap_or(Dynamic::UNIT), "ascending")?.unwrap_or(true);
    Ok(SortLevel { key, ascending })
}
