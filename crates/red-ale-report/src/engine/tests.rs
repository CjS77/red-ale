use super::setup::{Setup, engine, engine_functions};

/// Read-only by construction (`design/reports.md` §1): no function a script can call returns core's Book or any
/// other core type, or the `Book` handle, so a script holds only handles and values. Methods take `&mut` of a
/// handle, as Rhai requires, never of the Book.
#[test]
fn no_script_function_hands_out_the_book() {
    let e = engine(&Setup::empty());
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

/// The functions a script may not replace are read from Rhai's metadata: the standard package's, the report API's
/// and red-ale's `sleep`. Empty, the load check would refuse nothing.
#[test]
fn the_engine_functions_are_listed_with_their_arity() {
    let listed = engine_functions();
    let has = |name: &str, arity: usize| listed.contains(&(name.to_owned(), arity));
    assert!(has("abs", 1), "the standard package");
    assert!(has("len", 1), "a method, with `this` counted");
    assert!(has("root", 1), "the query API");
    assert!(has("sleep", 1), "red-ale's override");
    assert!(
        !has("templates", 0) && !has("options", 3) && !has("render", 4),
        "every template script's entry points"
    );
}
