//! A Multicolumn View's report-list (`design/notes/report-statements-charts.md` §8.4): its children name templates or
//! saved configurations, a template child's options are checked against its template when a caller sets them, and a
//! saved configuration's stored children are not checked again.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod common;

use common::{Host, empty_book, registry};
use red_ale_core::Guid;
use red_ale_report::ReportError;
use red_ale_report::engine::{Configurations, Registry};
use serde_json::{Map, Value, json};

const VIEW: &str = "000000000000000000000000000000f1";
const CHILD: &str = "000000000000000000000000000000f2";
const SAVED: &str = "000000000000000000000000000000f3";
const NOBODY: &str = "000000000000000000000000000000f4";

const SCRIPT: &str = r#"
fn templates() {
    [#{ guid: "000000000000000000000000000000f1", name: "View", params: #{ view: true } },
     #{ guid: "000000000000000000000000000000f2", name: "Child", params: #{ view: false } }]
}
fn options(params, o, book) {
    if params.view {
        o.report_placement("__general", "report-list");
        o.number_range("General", "Number of columns", "a", "Number of columns before wrapping to a new row.", 1, 0, 20, 1);
    } else {
        o.number_range("Display", "Levels", "a", "How deep.", 2, 1, 10, 1);
    }
}
fn render(params, book, opts, ctx) {
    if params.view {
        for c in opts.get("__general", "report-list") { print(`${type_of(c)} ${c.report} ${c.colspan} ${c.rowspan}`); }
    }
    #{ title: "t", sections: [] }
}
"#;

/// One saved configuration, `SAVED`, of the child template.
struct Saved(Map<String, Value>);

impl Configurations for Saved {
    fn configuration(&self, guid: Guid) -> Option<(Guid, &Map<String, Value>)> {
        (guid == Guid::parse(SAVED).unwrap()).then(|| (Guid::parse(CHILD).unwrap(), &self.0))
    }
}

fn setup() -> (Registry, Host) {
    let reg = registry(&[("view.rhai", SCRIPT)]);
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    (reg, Host::new(empty_book()))
}

fn run(children: Value) -> Result<red_ale_report::ReportResult, ReportError> {
    let (reg, host) = setup();
    let saved = Saved(Map::new());
    let mut ctx = host.ctx();
    ctx.configurations = &saved;
    let view = reg.find(VIEW).unwrap();
    reg.run(view, None, Some(&json!({ "__general": { "report-list": children } })), &ctx)
}

fn reason(children: Value) -> String {
    match run(children.clone()) {
        Err(ReportError::Option(e)) => {
            assert_eq!((e.section.as_str(), e.name.as_str()), ("__general", "report-list"));
            e.reason
        }
        other => panic!("{children} should be refused: {other:?}"),
    }
}

#[test]
fn children_name_templates_and_saved_configurations() {
    let children = json!([
        { "report": CHILD, "options": { "Display": { "Levels": 4 }, "General": { "Report name": "Child" } }, "colspan": 2 },
        { "report": SAVED, "rowspan": 0 },
        { "report": VIEW },
    ]);
    let result = run(children).unwrap();
    let kept = json!([
        { "report": CHILD, "options": { "Display": { "Levels": 4 } }, "colspan": 2, "rowspan": 1 },
        { "report": SAVED, "colspan": 1, "rowspan": 0 },
        { "report": VIEW, "colspan": 1, "rowspan": 1 },
    ]);
    assert_eq!(
        result.options["__general"]["report-list"],
        kept,
        "a template child keeps only the options that differ from its defaults"
    );
    let log: Vec<String> = result.log.into_iter().map(|l| l.text).collect();
    assert_eq!(
        log,
        [format!("map {CHILD} 2 1"), format!("map {SAVED} 1 0"), format!("map {VIEW} 1 1")],
        "the script reads the wire form"
    );
}

#[test]
fn unknown_reports_and_invalid_child_options_are_refused() {
    assert_eq!(
        reason(json!([{ "report": CHILD }, { "report": NOBODY }])),
        format!("child 1: no report template or saved report configuration has GUID {NOBODY}")
    );
    assert!(
        reason(json!([{ "report": SAVED, "options": { "Display": { "Levels": 3 } } }]))
            .ends_with("options go only with a template's GUID"),
        "a saved configuration runs with its own values"
    );
    assert_eq!(
        reason(json!([{ "report": CHILD, "options": { "Display": { "Levels": 11 } } }])),
        "child 0: Child option Display/Levels: 11 is outside 1 to 10"
    );
    assert_eq!(
        reason(json!([{ "report": CHILD, "options": { "Display": { "Depth": 1 } } }])),
        "child 0: Child option Display/Depth: unknown option"
    );
    let nested = json!([{ "report": VIEW, "options": { "__general": { "report-list": [{ "report": NOBODY }] } } }]);
    assert_eq!(
        reason(nested),
        format!("child 0: View option __general/report-list: child 0: no report template or saved report configuration has GUID {NOBODY}"),
        "a nested view's children are checked too"
    );
}

#[test]
fn a_saved_configurations_children_are_not_checked_again() {
    let (reg, host) = setup();
    let view = reg.find(VIEW).unwrap();
    let stored = json!({ "__general": { "report-list": [{ "report": NOBODY, "colspan": 1, "rowspan": 1 }] } });
    let saved = Guid::parse(SAVED).unwrap();
    let changes = json!({ "General": { "Number of columns": 2 } });
    let result = reg.run(view, Some((saved, &stored)), Some(&changes), &host.ctx()).unwrap();
    assert_eq!(
        result.options["__general"]["report-list"][0]["report"],
        NOBODY,
        "a child deleted since it was saved is the run's warning, not an error"
    );
    let err = reg.run(view, None, Some(&stored), &host.ctx()).unwrap_err();
    assert!(matches!(err, ReportError::Option(_)), "set by the caller, it is checked");
}
