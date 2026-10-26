//! `Memo`: values by integer key, shared by every copy (`design/reports.md` §2, "The engine for a run").
//!
//! Rhai passes arrays and maps by value, and a function a script module creates cannot be called once it is returned,
//! so a script has no way to keep a table that outlives the call that filled it. GnuCash's exchange functions keep
//! one: `gnc:case-exchange-time-fn` memoises the price tables it builds by date
//! (`5.17:gnucash/report/commodity-utilities.scm:822-830`). A `Memo` is that table: a handle whose copies all see the
//! same entries, so `lib/exchange` can keep its tables in the function value it hands out. It holds at most the
//! map-size limit of entries, like an object map.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use rhai::{Dynamic, Engine, EvalAltResult, INT, Position};

use crate::query::Fallible;

/// A table of values by integer key, shared by every copy (Rhai type `Memo`).
#[derive(Debug, Clone, Default)]
pub struct Memo(Rc<RefCell<BTreeMap<INT, Dynamic>>>);

/// The text of the error past the map-size limit; `errors` reads "object map" in it as that limit.
const TOO_LARGE: &str = "Number of entries in a memo, counted as an object map";

pub(super) fn register(engine: &mut Engine, max_entries: usize) {
    engine.register_type_with_name::<Memo>("Memo");
    engine.register_fn("memo", Memo::default);
    engine.register_fn("to_string", |m: &mut Memo| format!("Memo({})", m.0.borrow().len()));
    engine.register_fn("get", |m: &mut Memo, key: INT| m.0.borrow().get(&key).cloned().unwrap_or(Dynamic::UNIT));
    engine.register_fn("contains", |m: &mut Memo, key: INT| m.0.borrow().contains_key(&key));
    engine.register_fn("len", |m: &mut Memo| INT::try_from(m.0.borrow().len()).unwrap_or(INT::MAX));
    engine.register_fn("set", move |m: &mut Memo, key: INT, value: Dynamic| -> Fallible<()> {
        let mut entries = m.0.borrow_mut();
        // A limit of 0 means none, as Rhai reads its own.
        if max_entries > 0 && !entries.contains_key(&key) && entries.len() >= max_entries {
            return Err(EvalAltResult::ErrorDataTooLarge(TOO_LARGE.to_owned(), Position::NONE).into());
        }
        entries.insert(key, value.flatten());
        Ok(())
    });
}
