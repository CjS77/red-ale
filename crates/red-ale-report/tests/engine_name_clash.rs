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
    assert!(message.starts_with("lib/helpers.rhai and ./report.rhai both define fn total_of with 2"), "{message}");
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
