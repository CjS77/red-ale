//! The template registry (`design/reports.md` §2, "The shape of a report script"; §11): loading scripts, their
//! `templates()`, GUIDs, names and lookup.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Host, empty_book, registry, registry_with};
use red_ale_core::Guid;
use red_ale_report::ReportError;
use red_ale_report::engine::{Lookup, Registry, ScriptSource, TemplateKind};
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;
use serde_json::json;

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "cccccccccccccccccccccccccccccccc";

/// A script declaring templates from `records` (Rhai map literals), all rendering their `params.label` as the title.
fn script(records: &[String]) -> String {
    format!(
        "fn templates() {{ [{}] }}\n\
         fn options(params, o, book) {{ o.string(\"General\", \"Label\", \"a\", \"\", params.label); }}\n\
         fn render(params, book, opts, ctx) {{ #{{ title: opts.get(\"General\", \"Label\"), sections: [] }} }}\n",
        records.join(", ")
    )
}

fn record(guid: &str, name: &str, label: &str) -> String {
    format!("#{{ guid: \"{guid}\", name: \"{name}\", params: #{{ label: \"{label}\" }} }}")
}

fn load_message(e: &ReportError) -> String {
    match e {
        ReportError::Load(l) => format!("{}: {}", l.source, l.message),
        other => panic!("not report-load: {other:?}"),
    }
}

#[test]
fn one_script_declares_several_templates_told_apart_by_params() {
    let text = script(&[record(A, "Assets Chart", "assets"), record(B, "Income Chart", "income")]);
    let reg = registry(&[("charts.rhai", &text)]);
    let names: Vec<&str> = reg.templates().iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Assets Chart", "Income Chart"]);
    let t = reg.find("Income Chart").unwrap();
    assert_eq!((t.kind, t.script.as_str(), t.in_menu, t.menu_path.is_empty()), (TemplateKind::User, "./charts.rhai", true, true));
    let host = Host::new(empty_book());
    assert_eq!(host.run(&reg, A, &json!({})).unwrap().title, "assets");
    assert_eq!(host.run(&reg, "Income Chart", &json!({})).unwrap().title, "income");
}

#[test]
fn menu_fields_are_kept() {
    let rec = format!("#{{ guid: \"{A}\", name: \"N\", menuPath: [\"_Assets & Liabilities\"], menuName: \"Menu\", menuTip: \"Tip\", inMenu: false, params: #{{ label: \"x\" }} }}");
    let reg = registry(&[("m.rhai", &script(&[rec]))]);
    let t = reg.find(A).unwrap();
    assert_eq!(t.menu_path, ["_Assets & Liabilities"]);
    assert_eq!((t.menu_name.as_deref(), t.menu_tip.as_deref(), t.in_menu), (Some("Menu"), Some("Tip"), false));
}

#[test]
fn names_that_several_templates_share_are_ambiguous() {
    let one = script(&[record(A, "Balance Sheet", "first")]);
    let two = script(&[record(B, "Balance Sheet", "second")]);
    let reg = registry(&[("one.rhai", &one), ("two.rhai", &two)]);
    let guids = [A, B].map(|g| Guid::parse(g).unwrap()).to_vec();
    assert_eq!(reg.find("Balance Sheet").unwrap_err(), Lookup::Ambiguous(guids));
    assert_eq!(reg.find(B).unwrap().name, "Balance Sheet", "a GUID is never ambiguous");
    assert_eq!(reg.find("Nothing").unwrap_err(), Lookup::NotFound);
    assert_eq!(reg.find(C).unwrap_err(), Lookup::NotFound);
}

#[test]
fn the_first_definition_of_a_guid_wins() {
    let builtin = Library::from_sources([("standard/report.rhai", "fn templates() { [#{ guid: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\", name: \"Built in\" }] }\nfn options(p, o, b) {}\nfn render(p, b, o, c) { #{ title: \"builtin\", sections: [] } }")]);
    let mine = script(&[record(A, "Mine", "mine"), record(C, "Other", "other")]);
    let later = script(&[record(C, "Later", "later")]);
    let reg = registry_with(builtin, &[("b.rhai", &later), ("a.rhai", &mine)], Limits::default());
    let names: Vec<(&str, TemplateKind)> = reg.templates().iter().map(|t| (t.name.as_str(), t.kind)).collect();
    assert_eq!(names, [("Built in", TemplateKind::Builtin), ("Other", TemplateKind::User)]);
    let problems: Vec<String> = reg.problems().iter().map(load_message).collect();
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems[0].starts_with("./a.rhai: template \"Mine\" repeats the GUID"), "{problems:?}");
    assert!(problems[1].starts_with("./b.rhai: template \"Later\" repeats the GUID"), "{problems:?}");
}

#[test]
fn a_built_in_script_that_does_not_load_is_a_bug() {
    let broken = Library::from_sources([("standard/broken.rhai", "fn templates() { [ }")]);
    let e = Registry::load(&broken, &[], Limits::default()).unwrap_err();
    assert_eq!(e.kind().name(), "internal-error");
    let dup = "fn templates() { [#{ guid: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\", name: \"X\" }] }\nfn options(p, o, b) {}\nfn render(p, b, o, c) { () }";
    let twice = Library::from_sources([("standard/a.rhai", dup), ("standard/b.rhai", dup)]);
    assert_eq!(Registry::load(&twice, &[], Limits::default()).unwrap_err().kind().name(), "internal-error");
}

#[test]
fn user_scripts_that_do_not_load_are_listed_and_the_rest_load() {
    let good = script(&[record(A, "Good", "good")]);
    let cases = [
        ("syntax.rhai", "fn templates( {".to_owned(), "./syntax.rhai: "),
        ("not-array.rhai", "fn templates() { 5 }\nfn options(p, o, b) {}\nfn render(p, b, o, c) {}".to_owned(), "templates() returns an array"),
        ("no-render.rhai", "fn templates() { [] }\nfn options(p, o, b) {}".to_owned(), "needs fn render() with 4 parameters"),
        ("bad-guid.rhai", script(&[record("ABC", "Bad", "x")]), "guid must be 32 lowercase hex digits"),
        ("upper.rhai", script(&[record(&B.to_uppercase(), "Upper", "x")]), "guid must be 32 lowercase hex digits"),
        ("no-name.rhai", script(&[format!("#{{ guid: \"{B}\" }}")]), "name is required"),
        ("extra.rhai", script(&[format!("#{{ guid: \"{B}\", name: \"E\", colour: 1 }}")]), "unknown field \"colour\""),
        ("throws.rhai", "fn templates() { throw \"no\" }\nfn options(p, o, b) {}\nfn render(p, b, o, c) {}".to_owned(), "./throws.rhai: no"),
    ];
    let mut scripts: Vec<(&str, &str)> = cases.iter().map(|(p, t, _)| (*p, t.as_str())).collect();
    scripts.push(("good.rhai", &good));
    scripts.push(("../escape.rhai", &good));
    scripts.push(("helpers.rhai", "fn helper() { 1 }"));
    let reg = registry(&scripts);
    assert_eq!(reg.templates().iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Good"]);
    let problems: Vec<String> = reg.problems().iter().map(load_message).collect();
    assert_eq!(problems.len(), cases.len() + 1, "a script without templates() is a module, not a problem: {problems:?}");
    assert!(problems[0].contains("relative, ends in .rhai"), "{problems:?}");
    cases.iter().for_each(|(_, _, want)| assert!(problems.iter().any(|p| p.contains(want)), "{want}: {problems:?}"));
    let syntax = reg.problems().iter().find_map(|p| match p {
        ReportError::Load(l) if l.source == "./syntax.rhai" => Some(l.clone()),
        _ => None,
    });
    assert_eq!(syntax.and_then(|l| l.line), Some(1));
}

#[test]
fn the_registry_can_be_shared_between_threads() {
    fn shareable<T: Send + Sync>(_: &T) {}
    let reg = Registry::load(&Library::builtin(), &[ScriptSource { path: "a.rhai".into(), text: script(&[record(A, "A", "a")]) }], Limits::default());
    shareable(&reg.unwrap());
}
