//! The Multicolumn View (`standard/view-column.rhai`; `design/notes/report-statements-charts.md` §8.3-§8.6, §8.8):
//! the grid GnuCash lays out, the host's runs of the children, and the warnings for children that cannot run.
//! GnuCash has no test of the view's output, so the cases are worked by hand from `view-column.scm:51-159`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod common;

use common::{Host, empty_book, fixture, registry_with};
use red_ale_core::Guid;
use red_ale_report::engine::{Configurations, Registry};
use red_ale_report::library::Library;
use red_ale_report::limits::{Limit, Limits};
use red_ale_report::result::{Block, GridBlock, GridCell, Link};
use red_ale_report::{ReportError, ReportResult};
use serde_json::{Map, Value, json};

const VIEW: &str = "d8ba4a2e89e8479ca9f6eccdeb164588";
const ACCOUNT_SUMMARY: &str = "3298541c236b494998b236dfad6ad752";
const BALANCE_SHEET: &str = "c4173ac99b2b448289bf4d11c731af13";
const LEAF: &str = "000000000000000000000000000000e1";
const THROWS: &str = "000000000000000000000000000000e2";
const SPINS: &str = "000000000000000000000000000000e3";

/// Cheap children: "Leaf" titles itself with its "Label" option, "Throws" fails, "Spins" never ends.
const CHILDREN: &str = r#"
fn templates() {
    [#{ guid: "000000000000000000000000000000e1", name: "Leaf", params: #{ kind: "leaf" } },
     #{ guid: "000000000000000000000000000000e2", name: "Throws", params: #{ kind: "throws" } },
     #{ guid: "000000000000000000000000000000e3", name: "Spins", params: #{ kind: "spins" } }]
}
fn options(params, o, book) { o.string("General", "Label", "a", "", "leaf"); }
fn render(params, book, opts, ctx) {
    if params.kind == "throws" { throw "no figures today"; }
    if params.kind == "spins" { loop { } }
    #{ title: opts.get("General", "Label"), sections: [#{ blocks: [#{ kind: "text", text: "body" }] }] }
}
"#;

/// Saved configurations by GUID: (template, stored options).
#[derive(Default)]
struct Saved(Vec<(Guid, Guid, Map<String, Value>)>);

impl Saved {
    fn with(mut self, guid: &str, template: &str, options: Value) -> Self {
        let Value::Object(options) = options else {
            panic!("options are an object")
        };
        self.0.push((g(guid), g(template), options));
        self
    }
}

impl Configurations for Saved {
    fn configuration(&self, guid: Guid) -> Option<(Guid, &Map<String, Value>)> {
        self.0.iter().find(|(s, _, _)| *s == guid).map(|(_, t, o)| (*t, o))
    }
}

fn g(s: &str) -> Guid {
    Guid::parse(s).unwrap()
}

fn reg() -> Registry {
    let limits = Limits::default().with(Limit::Operations, 2_000_000);
    let reg = registry_with(Library::builtin(), &[("children.rhai", CHILDREN)], limits);
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    reg
}

fn child(report: &str, colspan: u32, rowspan: u32) -> Value {
    json!({ "report": report, "colspan": colspan, "rowspan": rowspan })
}

fn list(children: Vec<Value>) -> Value {
    json!({ "__general": { "report-list": children } })
}

/// Run `report` on `host`, with the saved configurations `saved`, as a saved configuration (its stored values) or
/// with `changes`.
fn run_on(host: &Host, saved: &Saved, report: &str, changes: Value) -> Result<ReportResult, ReportError> {
    let reg = reg();
    let mut ctx = host.ctx();
    ctx.configurations = saved;
    match saved.configuration(g(report)) {
        Some((template, stored)) => {
            let stored = Value::Object(stored.clone());
            reg.run(reg.template(template).unwrap(), Some((g(report), &stored)), Some(&changes), &ctx)
        }
        None => reg.run(reg.template(g(report)).unwrap(), None, Some(&changes), &ctx),
    }
}

fn run(changes: Value) -> ReportResult {
    run_on(&Host::new(empty_book()), &Saved::default(), VIEW, changes).unwrap_or_else(|e| panic!("{e:?}"))
}

fn grid(r: &ReportResult) -> &GridBlock {
    match r.sections[0].blocks.as_slice() {
        [Block::Grid(g)] => g,
        other => panic!("one grid: {other:?}"),
    }
}

/// Each row as (child, colspan, rowspan) per cell.
fn layout(g: &GridBlock) -> Vec<Vec<(u32, u32, u32)>> {
    g.rows
        .iter()
        .map(|row| row.cells.iter().map(|c| (c.child, c.colspan, c.rowspan)).collect())
        .collect()
}

fn title(cell: &GridCell) -> &str {
    &cell.result.as_ref().unwrap_or_else(|| panic!("a result: {cell:?}")).title
}

fn warning(cell: &GridCell) -> &str {
    let w = cell.warning.as_ref().unwrap_or_else(|| panic!("a warning: {cell:?}"));
    assert_eq!(w.title, "Report error");
    &w.text
}

#[test]
fn no_children_give_an_empty_untitled_grid() {
    let r = run(json!({}));
    assert_eq!((r.title.as_str(), r.report.name.as_str()), ("", "Multicolumn View"));
    assert!(grid(&r).rows.is_empty());
    assert_eq!(r.options["General"]["Number of columns"], json!(1));
}

#[test]
fn rows_close_at_the_width_and_row_spans_take_room_below() {
    // Two columns: A (1×2) and B fill row 0; row 1 starts 1 wide with A's room, so C alone closes it; D (3 wide)
    // opens row 2 and closes it at once.
    let mut changes = list(vec![child(LEAF, 1, 2), child(LEAF, 1, 1), child(LEAF, 1, 1), child(LEAF, 3, 1)]);
    changes["General"] = json!({ "Number of columns": 2 });
    let r = run(changes);
    let rows = vec![vec![(0, 1, 2), (1, 1, 1)], vec![(2, 1, 1)], vec![(3, 3, 1)]];
    assert_eq!(layout(grid(&r)), rows);
    assert_eq!(title(&grid(&r).rows[0].cells[0]), "leaf");
    let link = grid(&r).rows[0].cells[0].link.clone().unwrap();
    let Link::Report(link) = link else { panic!("a report link") };
    assert_eq!(
        (link.template.0, link.configuration),
        (g(LEAF), None),
        "Single Report runs the child alone"
    );
}

#[test]
fn zero_columns_and_zero_spans_follow_gnucash() {
    // Width 0: every child closes its row. A column span of 0 adds nothing, a row span of 0 still takes its own row.
    let mut changes = list(vec![child(LEAF, 0, 0), child(LEAF, 1, 1)]);
    changes["General"] = json!({ "Number of columns": 0 });
    assert_eq!(layout(grid(&run(changes))), vec![vec![(0, 0, 0)], vec![(1, 1, 1)]]);
    // Width 1 with a span-0 child: the row stays open for the next one.
    let one = list(vec![child(LEAF, 0, 1), child(LEAF, 1, 1), child(LEAF, 1, 1)]);
    assert_eq!(layout(grid(&run(one))), vec![vec![(0, 0, 1), (1, 1, 1)], vec![(2, 1, 1)]]);
}

#[test]
fn template_and_saved_configuration_children_run_on_the_views_book() {
    let saved = Saved::default().with(
        "000000000000000000000000000000c1",
        BALANCE_SHEET,
        json!({
            "General": { "Report name": "My Balance Sheet", "Stylesheet": "Gone" }
        }),
    );
    let mut changes = list(vec![
        json!({ "report": ACCOUNT_SUMMARY, "options": { "General": { "Report name": "Summary" } } }),
        child("000000000000000000000000000000c1", 1, 1),
    ]);
    changes["General"] = json!({ "Number of columns": 2, "Add options summary": "always" });
    let host = Host::new(fixture("balsheet-pnl.gnucash"));
    let r = run_on(&host, &saved, VIEW, changes).unwrap_or_else(|e| panic!("{e:?}"));
    let cells = &grid(&r).rows[0].cells;
    let summary = cells[0].result.as_ref().unwrap();
    assert_eq!(summary.report.guid.0, g(ACCOUNT_SUMMARY));
    assert_eq!(
        summary.options["General"]["Report name"], "Summary",
        "a template child with options"
    );
    assert!(summary.title.contains("Account Summary"), "{}", summary.title);
    let sheet = cells[1].result.as_ref().unwrap();
    assert_eq!(sheet.report.configuration.map(|c| c.0), Some(g("000000000000000000000000000000c1")));
    assert_eq!(sheet.options["General"]["Report name"], "My Balance Sheet");
    assert!(!sheet.sections.is_empty());
    assert_eq!(
        sheet.options["General"]["Stylesheet"], "Default",
        "the child's style sheet is ignored"
    );
    let Some(Link::Report(link)) = &cells[1].link else {
        panic!("a report link")
    };
    assert_eq!(link.configuration.map(|c| c.0), Some(g("000000000000000000000000000000c1")));

    let Some(Block::OptionsSummary(o)) = r.sections.last().and_then(|s| s.blocks.first()) else {
        panic!("a summary")
    };
    let rows: Vec<(&str, &str)> = o.rows.iter().map(|r| (r.name.as_str(), r.value.as_str())).take(2).collect();
    assert_eq!(rows, [("Embedded Report", "Summary"), ("Embedded Report", "My Balance Sheet")]);
    assert_eq!(o.rows[0].section, "");
}

#[test]
fn children_that_cannot_run_leave_a_warning_in_their_cell() {
    let gone = "000000000000000000000000000000c9";
    let saved = Saved::default().with(
        "000000000000000000000000000000c2",
        VIEW,
        list(vec![
            child(gone, 1, 1),
            child(THROWS, 1, 1),
            json!({ "report": LEAF, "options": { "General": { "Missing": 1 } } }),
            child(LEAF, 1, 1),
        ]),
    );
    let r = run_on(&Host::new(empty_book()), &saved, "000000000000000000000000000000c2", json!({})).unwrap();
    let cells: Vec<&GridCell> = grid(&r).rows.iter().flat_map(|row| &row.cells).collect();
    assert!(warning(cells[0]).contains(gone), "a saved configuration deleted since");
    assert!(cells[0].link.is_none());
    assert!(warning(cells[1]).contains("no figures today"), "{}", warning(cells[1]));
    assert!(warning(cells[2]).contains("Missing"), "options that no longer apply");
    assert_eq!(title(cells[3]), "leaf", "the others still run");
}

#[test]
fn nested_views_cycles_and_the_depth_limit() {
    let [a, b] = ["000000000000000000000000000000a1", "000000000000000000000000000000a2"];
    // A holds Leaf and B; B holds A again: the cycle stops at A's second appearance.
    let saved = Saved::default()
        .with(a, VIEW, list(vec![child(LEAF, 1, 1), child(b, 1, 1)]))
        .with(b, VIEW, list(vec![child(a, 1, 1)]));
    let r = run_on(&Host::new(empty_book()), &saved, a, json!({})).unwrap();
    let inner = r.sections[0].blocks.clone();
    let Block::Grid(top) = &inner[0] else { panic!() };
    let b_result = top.rows[1].cells[0].result.as_ref().unwrap();
    let Block::Grid(b_grid) = &b_result.sections[0].blocks[0] else {
        panic!()
    };
    assert!(warning(&b_grid.rows[0].cells[0]).contains("embeds itself"));

    // Template views nested five deep: the children of the view at depth 4 are not run.
    let nest = (0..5).fold(
        json!([child(LEAF, 1, 1)]),
        |inner, _| json!([{ "report": VIEW, "options": { "__general": { "report-list": inner } } }]),
    );
    let r = run(json!({ "__general": { "report-list": nest } }));
    let deepest = (0..4).fold(grid(&r).clone(), |g, _| {
        let Block::Grid(below) = &g.rows[0].cells[0].result.as_ref().unwrap().sections[0].blocks[0] else {
            panic!()
        };
        below.clone()
    });
    assert!(warning(&deepest.rows[0].cells[0]).contains("at most 4 deep"), "{deepest:?}");
}

#[test]
fn at_most_100_children_run_and_engine_limits_stop_the_whole_run() {
    let r = run(list((0..101).map(|_| child(LEAF, 1, 1)).collect()));
    let cells: Vec<&GridCell> = grid(&r).rows.iter().flat_map(|row| &row.cells).collect();
    assert_eq!(title(cells[99]), "leaf");
    assert!(warning(cells[100]).contains("at most 100"));

    let spins = run_on(&Host::new(empty_book()), &Saved::default(), VIEW, list(vec![child(SPINS, 1, 1)]));
    assert!(
        matches!(spins, Err(ReportError::Limit(ref l)) if l.limit == Limit::Operations),
        "{spins:?}"
    );
}

#[test]
fn a_run_leaves_the_book_as_it_was() {
    let host = Host::new(fixture("balsheet-pnl.gnucash"));
    let before = host.book.clone();
    run_on(
        &host,
        &Saved::default(),
        VIEW,
        list(vec![child(ACCOUNT_SUMMARY, 1, 1), child(BALANCE_SHEET, 1, 1)]),
    )
    .unwrap();
    assert_eq!(host.book, before);
}
