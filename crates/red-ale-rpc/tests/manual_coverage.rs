//! The Manual covers the registries (`design/manual.md`, "Coverage is checked"): every method, error code and report
//! template has its Reference page, written up, with Examples; every link resolves; every `from` names a checklist
//! entry; and the compiled-in sources have no problems.
//!
//! While the Manual is being written, `docs/coverage-pending.txt` lists the gaps known to be open. The list may only
//! shrink: a gap not on it fails, and so does an entry that is no longer a gap, so it is crossed off when the Page is
//! written.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use red_ale_docs::{coverage, error_page_id};
use red_ale_rpc::{Dispatcher, codes};
use red_ale_service::Settings;

const PENDING: &str = include_str!("../../../docs/coverage-pending.txt");

/// The sections of GnuCash's documentation the Manual adapts. The checklist that maps them to Pages comes with the
/// GnuCash documentation work; until then no Page may name one.
const CHECKLIST: [&str; 0] = [];

#[test]
fn the_manual_sources_have_no_problems() {
    let problems: Vec<String> = red_ale_rpc::manual::manual().problems().iter().map(ToString::to_string).collect();
    assert!(problems.is_empty(), "the Manual's sources have problems:\n{}", problems.join("\n"));
}

#[test]
fn the_only_gaps_are_the_pending_ones() {
    let manual = red_ale_rpc::manual::manual();
    let gaps: BTreeSet<String> = coverage::all(manual, &CHECKLIST).iter().map(ToString::to_string).collect();
    let pending: BTreeSet<String> = PENDING
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect();
    let new: Vec<&String> = gaps.difference(&pending).collect();
    let closed: Vec<&String> = pending.difference(&gaps).collect();
    assert!(
        new.is_empty(),
        "the Manual has gaps that are not in docs/coverage-pending.txt; write the Pages:\n{}",
        new.iter().map(|g| g.as_str()).collect::<Vec<_>>().join("\n")
    );
    assert!(
        closed.is_empty(),
        "these are no longer gaps; cross them off docs/coverage-pending.txt:\n{}",
        closed.iter().map(|g| g.as_str()).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn every_method_and_error_names_an_existing_page() {
    let manual = red_ale_rpc::manual::manual();
    let schema = Dispatcher::new(Settings::default()).schema();
    let methods = schema["methods"].as_array().unwrap();
    let missing: Vec<&str> = methods
        .iter()
        .map(|m| m["help"].as_str().unwrap())
        .filter(|id| manual.get(id).is_none())
        .collect();
    assert!(missing.is_empty(), "api.schema names Pages that don't exist: {missing:?}");
    let errors: Vec<&str> = codes::ALL
        .iter()
        .map(|(_, name)| *name)
        .filter(|name| manual.get(&error_page_id(name)).is_none())
        .collect();
    assert!(errors.is_empty(), "error codes with no Page: {errors:?}");
}

#[test]
fn every_report_template_has_a_page() {
    let manual = red_ale_rpc::manual::manual();
    let templates = red_ale_rpc::manual::report_templates();
    assert!(templates.len() > 40, "{} templates", templates.len());
    let missing: Vec<&str> = templates
        .iter()
        .filter(|t| manual.get(&red_ale_docs::report_page_id(&t.name)).is_none())
        .map(|t| t.name.as_str())
        .collect();
    assert!(missing.is_empty(), "report templates with no Page: {missing:?}");
    let described = templates.iter().filter(|t| !t.options.is_empty()).count();
    assert_eq!(described, templates.len(), "every built-in template's options are described");
}
