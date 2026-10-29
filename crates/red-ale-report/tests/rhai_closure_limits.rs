//! The two Rhai limits on closures that the report library works around (`README.md`, "Conventions a translator
//! needs"), and how Rhai resolves a function's name across the scripts of a run, which red-ale's engine checks
//! (`design/reports.md` §2, "One name, one function"), reproduced on a bare `Engine::new()` with Rhai's own
//! `StaticModuleResolver`, so none comes from how red-ale's engine sets up modules. The closure limits and the shared
//! resolution cache are Rhai bugs. When a Rhai upgrade makes one of these tests fail, the behaviour has changed: drop
//! the convention or check it explains, and this test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rhai::module_resolvers::StaticModuleResolver;
use rhai::{Dynamic, Engine, Module, Scope};

/// `main` run with `modules` (name, script) importable, each evaluated in order and seeing the ones before it.
fn run(modules: &[(&str, &str)], main: &str) -> Result<String, String> {
    let resolver = modules.iter().fold(StaticModuleResolver::new(), |mut resolver, (name, script)| {
        let mut engine = Engine::new();
        engine.set_module_resolver(resolver.clone());
        let ast = engine.compile(script).unwrap();
        resolver.insert(*name, Module::eval_ast_as_new(Scope::new(), &ast, &engine).unwrap());
        resolver
    });
    let mut engine = Engine::new();
    engine.set_module_resolver(resolver);
    engine.eval::<Dynamic>(main).map(|d| d.to_string()).map_err(|e| e.to_string())
}

/// A closure made in a module's function and returned can't be called by the importer: the function pointer is
/// looked up in the calling script, not in the module that defined it.
#[test]
fn a_closure_made_in_a_module_is_not_found_by_the_importer() {
    let m = [("m", "fn make() { |x| x + 1 } fn apply(f, x) { f.call(x) }")];
    let err = run(&m, r#"import "m" as m; let f = m::make(); f.call(2)"#).unwrap_err();
    assert!(err.starts_with("Function not found: anon$"), "{err}");
    assert_eq!(
        run(&m, r#"import "m" as m; m::apply(m::make(), 2)"#).unwrap(),
        "3",
        "called back inside its module, it works"
    );
}

/// A method of module `c` that passes a closure to a native function (`index_of`) works from module `d`, until `c`
/// has resolved the method twice itself. Rhai caches a resolution on its second lookup; `d` then gets the cached
/// method without `c`'s environment, and the closure is not found.
#[test]
fn a_method_called_from_another_module_after_its_own_loses_its_closures() {
    let c = "fn array.first_one() { this.index_of(|p| p == 1) } fn once(a) { a.first_one() } fn twice(a) { a.first_one(); a.first_one() }";
    let m = [("c", c), ("d", r#"import "c" as c; fn once(a) { a.first_one() }"#)];
    let main = |first: &str| run(&m, &format!(r#"import "c" as c; import "d" as d; {first}; d::once([1])"#));
    assert_eq!(main("c::once([1])").unwrap(), "0", "resolved once in c: still found");
    let err = main("c::twice([1])").unwrap_err();
    assert!(err.starts_with("Function not found: anon$"), "{err}");
}

/// Two scripts of a run that define a function of the same name and arity can each call the other's. Rhai looks a
/// call up in the scripts' function libraries, the module's own first (rhai 1.27.0 `src/func/func_call.rs:383-388`),
/// but keeps one function resolution cache for the whole run, keyed by the name and arity alone (`:367-375`,
/// `:821-834`). Calling a module's function pushes the module's library but no new cache (`src/func/script.rs:362-381`),
/// and a resolution is cached on its second lookup (`src/func/func_call.rs:416-423`), so whichever script looks the
/// name up second fixes it for every script after. The method test above is the same cache. red-ale refuses such
/// scripts (`design/reports.md` §2, "One name, one function").
#[test]
fn a_function_resolved_in_one_script_is_called_in_place_of_another_of_the_same_name_and_arity() {
    let m = [("m", r#"private fn total_of(a, b) { "module" } fn getpair(a, b) { total_of(a, b) }"#)];
    let main = |calls: &str| run(&m, &format!(r#"import "m" as m; fn total_of(a, b) {{ "script" }} {calls}"#)).unwrap();
    assert_eq!(main("m::getpair(1, 2)"), "module");
    assert_eq!(
        main("total_of(1, 2); m::getpair(1, 2)"),
        "module",
        "looked up once by the script: not cached"
    );
    assert_eq!(
        main("total_of(1, 2); total_of(1, 2); m::getpair(1, 2)"),
        "script",
        "the module calls the script's"
    );
    assert_eq!(
        main("total_of(1, 2); m::getpair(1, 2); total_of(1, 2)"),
        "module",
        "the script calls the module's"
    );
    assert_eq!(
        main("fn twice() { total_of(1, 2); total_of(1, 2) } twice(); m::getpair(1, 2)"),
        "script",
        "the cache outlives the function that filled it"
    );
}

/// A module's function calling a name its module does not define finds the importing script's function of that name
/// before the engine's: the module's library goes on top of the caller's, not in its place (rhai 1.27.0
/// `src/func/script.rs:377`, `src/func/func_call.rs:383-397`). So a report script's function with the name and arity
/// of one a library calls (`abs`, a method of the report API) replaces it in the library too. A method call `x.f()`
/// looks for a script `fn f()` reading `x` as `this` first (`src/parser.rs:2127-2136`), so a script function of one
/// parameter fewer replaces an engine method. red-ale refuses a script defining a function of an engine function's
/// name and arity, or one fewer (`design/reports.md` §2, "One name, one function").
#[test]
fn a_module_calls_the_importers_function_in_place_of_the_engines() {
    let m = [("m", "fn magnitude(x) { abs(x) } fn size(a) { a.len() }")];
    assert_eq!(run(&m, r#"import "m" as m; m::magnitude(-2)"#).unwrap(), "2");
    let shadowed = run(&m, r#"import "m" as m; fn abs(x) { "script" } m::magnitude(-2)"#).unwrap();
    assert_eq!(shadowed, "script");
    assert_eq!(run(&m, r#"import "m" as m; m::size([1, 2])"#).unwrap(), "2");
    let method = run(&m, r#"import "m" as m; fn len() { "script" } m::size([1, 2])"#).unwrap();
    assert_eq!(
        method, "script",
        "a method call reaches the script's function of one parameter fewer"
    );
}
