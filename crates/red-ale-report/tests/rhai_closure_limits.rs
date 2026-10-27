//! The two Rhai limits on closures that the report library works around (`README.md`, "Conventions a translator
//! needs"), reproduced on a bare `Engine::new()` with Rhai's own `StaticModuleResolver`, so neither comes from how
//! red-ale's engine sets up modules. Both are Rhai bugs. When a Rhai upgrade makes one of these tests fail, the bug is
//! fixed: drop the convention it explains and this test.

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
