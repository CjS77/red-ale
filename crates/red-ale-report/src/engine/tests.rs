use std::sync::Arc;

use red_ale_core::{Book, FixedEnv, Time64, TimeZone};

use super::log::Log;
use super::setup::{Setup, engine};
use crate::limits::Limits;
use crate::query::Snapshot;

/// Read-only by construction (`design/reports.md` §1): no function a script can call returns core's Book or any
/// other core type, or the `Book` handle, so a script holds only handles and values. Methods take `&mut` of a
/// handle, as Rhai requires, never of the Book.
#[test]
fn no_script_function_hands_out_the_book() {
    let snap = Snapshot::new(&Book::new_empty(&mut FixedEnv::new(Time64(0), 0)), Time64(0), TimeZone::utc());
    let sources = Arc::new(std::collections::BTreeMap::default());
    let conventions = red_ale_locale::Locale::c().conventions();
    let e = engine(&Setup {
        snap,
        sources,
        limits: Limits::default(),
        log: Log::new(1024),
        conventions,
    });
    let signatures = e.gen_fn_signatures(false);
    assert!(
        signatures.iter().any(|s| s.starts_with("root(")),
        "the query API is registered: {signatures:?}"
    );
    signatures.iter().for_each(|s| {
        assert!(!s.contains("red_ale_core"), "a core type reaches scripts: {s}");
        assert!(!s.ends_with("-> Book"), "a function returns the Book: {s}");
    });
    let book_methods: Vec<&String> = signatures.iter().filter(|s| s.contains("Book")).collect();
    assert!(
        book_methods.iter().all(|s| s.contains("&mut Book") && !s.starts_with("set_")),
        "{book_methods:?}"
    );
}
