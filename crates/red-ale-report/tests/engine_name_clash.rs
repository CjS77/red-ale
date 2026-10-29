//! A report script and a module it imports that define a function of the same name and arity (`design/reports.md`
//! §2, "One name, one function"). Rhai may call either in the other's place, so the script does not load; renamed,
//! the module calls its own function.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{GUID, Host, empty_book, registry_with, template_with};
use red_ale_report::ReportError;
use red_ale_report::engine::Registry;
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;
use serde_json::json;

/// A library module whose public `getpair` calls its private `total_of`, as `lib/collectors` does.
const HELPERS: &str = "private fn total_of(list, commodity) { \"library\" }\nfn getpair(list, commodity) { total_of(list, commodity) }";

/// A module between the report and `lib/helpers`.
const MIDDLE: &str = "import \"lib/helpers\" as helpers;\nfn getpair(list, commodity) { helpers::getpair(list, commodity) }";

/// A report script importing `module` with its own private function `name`, of two parameters, which `render`
/// calls twice before the module's `getpair`: the second call puts it in Rhai's function resolution cache.
fn report(module: &str, name: &str) -> String {
    let body = format!("{name}(1, 2); {name}(1, 2);\nlet v = m::getpair([], 2);\n#{{ title: v, sections: [] }}");
    let helper = format!("private fn {name}(env, collector) {{ \"script\" }}");
    format!("import \"{module}\" as m;\n{}", template_with(&helper, &body))
}

fn load(user: &[(&str, &str)]) -> Registry {
    let library = Library::from_sources([("lib/helpers.rhai", HELPERS), ("lib/middle.rhai", MIDDLE)]);
    registry_with(library, user, Limits::default())
}

fn only_load_error(reg: &Registry) -> (String, Option<u32>, String) {
    match reg.problems() {
        [ReportError::Load(l)] => (l.source.clone(), l.line, l.message.clone()),
        other => panic!("not one report-load: {other:?}"),
    }
}

#[test]
fn a_report_defining_a_function_a_module_it_imports_defines_does_not_load() {
    let script = report("lib/helpers", "total_of");
    let reg = load(&[("report.rhai", &script)]);
    assert!(reg.templates().is_empty());
    let (source, line, message) = only_load_error(&reg);
    assert_eq!((source.as_str(), line), ("./report.rhai", Some(1)), "located at the import");
    assert_eq!(
        message,
        "lib/helpers.rhai and ./report.rhai both define fn total_of with 2 parameters; Rhai may call either in the \
         other's place, so one must be renamed"
    );
}

#[test]
fn a_module_imported_through_another_is_checked_too() {
    let script = report("lib/middle", "total_of");
    let (source, _, message) = only_load_error(&load(&[("report.rhai", &script)]));
    assert_eq!(source, "lib/middle.rhai", "located at the import in the module that imports it");
    assert!(
        message.starts_with("lib/helpers.rhai and ./report.rhai both define fn total_of with 2"),
        "{message}"
    );
}

#[test]
fn renamed_the_library_calls_its_own_helper() {
    let script = report("lib/helpers", "total_of_env");
    let reg = load(&[("report.rhai", &script)]);
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    let result = Host::new(empty_book()).run(&reg, GUID, &json!({})).unwrap();
    assert_eq!(result.title, "library");
}

/// GnuCash's General Ledger runs the Transaction Report's renderer; red-ale's imports its script. Every template
/// script defines `templates`, `options` and `render`, which the engine calls through the script's own AST.
#[test]
fn a_template_script_may_import_another() {
    let other = template_with("", "#{ title: \"other\", sections: [] }").replace(GUID, "0000000000000000000000000000abcd");
    let script = format!(
        "import \"./other\" as other;\n{}",
        template_with("", "let r = other::render(params, book, opts, ctx); r.title = \"mine\"; r")
    );
    let reg = load(&[("report.rhai", &script), ("other.rhai", &other)]);
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    let result = Host::new(empty_book()).run(&reg, GUID, &json!({})).unwrap();
    assert_eq!(result.title, "mine");
}

/// An import inside a function is resolved only when the run reaches it; the clash is then a `report-script` error.
#[test]
fn an_import_inside_a_function_is_checked_when_it_runs() {
    let body = "total_of(1, 2);\nimport \"lib/helpers\" as m;\n#{ title: m::getpair([], 2), sections: [] }";
    let script = template_with("private fn total_of(env, collector) { \"script\" }", body);
    let reg = load(&[("report.rhai", &script)]);
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    match Host::new(empty_book()).run(&reg, GUID, &json!({})) {
        Err(ReportError::Script(s)) => assert!(s.message.contains("both define fn total_of with 2"), "{}", s.message),
        other => panic!("{other:?}"),
    }
}

/// A user script whose text is `prelude` and then one template.
fn with_template(prelude: &str) -> String {
    format!("{prelude}\n{}", template_with("", "#{ title: \"t\", sections: [] }"))
}

fn only_load_error_at(reg: &Registry) -> (String, Option<u32>, Option<u32>, String) {
    match reg.problems() {
        [ReportError::Load(l)] => (l.source.clone(), l.line, l.column, l.message.clone()),
        other => panic!("not one report-load: {other:?}"),
    }
}

/// Rhai calls a script's `fn abs(x)` in place of the engine's `abs`, in every script of the run, the libraries
/// included. The script does not load, located at the function.
#[test]
fn a_report_defining_an_engine_function_does_not_load() {
    let script = with_template("// The magnitude.\nprivate fn abs(x) { x }");
    let reg = load(&[("report.rhai", &script)]);
    assert!(reg.templates().is_empty());
    assert_eq!(
        only_load_error_at(&reg),
        (
            "./report.rhai".to_owned(),
            Some(2),
            Some(12),
            "./report.rhai defines fn abs with 1 parameter, which replaces the report engine's abs in every script of \
             the run, so it must be renamed"
                .to_owned()
        )
    );
}

/// `x.len()` looks for a script `fn len()` reading `x` as `this` before the engine's `len(x)`.
#[test]
fn a_function_a_method_call_would_reach_does_not_load() {
    let script = with_template("fn len() { 0 }");
    let (source, line, column, message) = only_load_error_at(&load(&[("report.rhai", &script)]));
    assert_eq!((source.as_str(), line, column), ("./report.rhai", Some(1), Some(4)));
    assert_eq!(
        message,
        "./report.rhai defines fn len with 0 parameters, which a method call x.len(…) reaches in place of the report \
         engine's len of 1 in every script of the run, so it must be renamed"
    );
}

/// The report API's functions are the engine's too.
#[test]
fn a_function_of_the_report_api_is_not_replaced() {
    let script = with_template("fn root(book) { () }");
    let (_, _, _, message) = only_load_error_at(&load(&[("report.rhai", &script)]));
    assert!(
        message.starts_with("./report.rhai defines fn root with 1 parameter, which replaces"),
        "{message}"
    );
}

/// A module imported at the top of a report is checked when the report loads, located in the module.
#[test]
fn a_user_module_defining_an_engine_function_does_not_load_its_importer() {
    let module = "fn magnitude(x) { abs(x) }\n\nfn abs(x) { x }";
    let script = with_template("import \"./helpers\" as h;");
    let reg = load(&[("helpers.rhai", module), ("report.rhai", &script)]);
    let problems: Vec<(String, Option<u32>, Option<u32>)> = reg
        .problems()
        .iter()
        .map(|p| match p {
            ReportError::Load(l) => (l.source.clone(), l.line, l.column),
            other => panic!("not report-load: {other:?}"),
        })
        .collect();
    let at_abs = ("./helpers.rhai".to_owned(), Some(3), Some(4));
    assert_eq!(
        problems,
        [at_abs.clone(), at_abs],
        "the module itself, then the report importing it"
    );
}

/// A built-in script that would replace an engine function is red-ale's bug.
#[test]
fn a_built_in_script_defining_an_engine_function_is_an_internal_error() {
    let library = Library::from_sources([("lib/bad.rhai", "fn abs(x) { x }")]);
    match Registry::load(&library, &[], Limits::default()) {
        Err(ReportError::Internal(m)) => assert!(m.contains("lib/bad.rhai defines fn abs with 1 parameter"), "{m}"),
        other => panic!("{other:?}"),
    }
}

/// A name the engine does not have loads, and the engine's function is still the engine's.
#[test]
fn a_report_reusing_no_engine_name_loads() {
    let script = template_with("fn magnitude(x) { abs(x) }", "#{ title: `${magnitude(-2)}`, sections: [] }");
    let reg = load(&[("report.rhai", &script)]);
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    assert_eq!(Host::new(empty_book()).run(&reg, GUID, &json!({})).unwrap().title, "2");
}
