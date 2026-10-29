//! The Multicolumn View (`standard/view-column.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/view-column.scm`) through `report.run` and `report.export`, on copies of
//! `fixtures/reports/test-data.gnucash` and its SQLite copy. GnuCash has no test of the view's output; a child's
//! cell must hold what running the child alone gives.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod report_runs;

use report_runs::Fixture;
use serde_json::{Value, json};

const VIEW: &str = "d8ba4a2e89e8479ca9f6eccdeb164588";
const ACCOUNT_SUMMARY: &str = "3298541c236b494998b236dfad6ad752";
const BALANCE_SHEET: &str = "c4173ac99b2b448289bf4d11c731af13";

fn side_by_side() -> Value {
    json!({
        "General": { "Number of columns": 2 },
        "__general": { "report-list": [
            { "report": ACCOUNT_SUMMARY, "options": { "General": { "Report name": "Summary" } } },
            { "report": BALANCE_SHEET, "rowspan": 2 }
        ] }
    })
}

fn cells(result: &Value) -> Vec<Value> {
    let grid = &result["sections"][0]["blocks"][0];
    assert_eq!(grid["kind"], "grid");
    grid["rows"].as_array().unwrap().iter().flat_map(|r| r["cells"].as_array().unwrap().clone()).collect()
}

/// A child's cell holds the result of running it alone, without the run's version and now.
fn check_children(f: &mut Fixture) -> Value {
    let view = f.run(VIEW, &side_by_side());
    let cells = cells(&view);
    let alone = [
        f.run(ACCOUNT_SUMMARY, &json!({ "General": { "Report name": "Summary" } })),
        f.run(BALANCE_SHEET, &json!({})),
    ];
    cells.iter().zip(&alone).for_each(|(cell, alone)| {
        let child = &cell["result"];
        ["report", "title", "options", "sections", "log"]
            .iter()
            .for_each(|k| assert_eq!(child[k], alone[k], "{k}"));
        assert!(child.get("version").is_none() && child.get("now").is_none());
        assert_eq!(cell["link"]["report"]["template"], alone["report"]["guid"], "Single Report");
    });
    assert_eq!((cells[1]["colspan"].clone(), cells[1]["rowspan"].clone()), (json!(1), json!(2)));
    assert_eq!(view["title"], "");
    view
}

#[test]
fn children_run_on_the_views_book_in_xml_and_sqlite() {
    let mut xml = Fixture::open("test-data", "us", true);
    let mut sqlite = Fixture::open_sqlite("test-data", "us");
    let (a, b) = (check_children(&mut xml), check_children(&mut sqlite));
    assert_eq!(a["sections"], b["sections"], "both backends give the same children");
    let (html, again) = (xml.export(VIEW, &side_by_side()), xml.export(VIEW, &side_by_side()));
    assert_eq!(html, again, "the same bytes each time");
    assert!(html.contains("<table class=\"grid\" style=\"width: 100%\">"));
    assert!(html.contains("<td rowspan=\"2\" colspan=\"1\" style=\"vertical-align: top\">"));
    assert_eq!(sqlite.export(VIEW, &side_by_side()), html);
    assert_eq!(xml.call("book.info", &json!({}))["dirty"], false, "a run leaves the Book as it was");
    assert_eq!(xml.files(), ["book.gnucash", "config.json", "reports"]);
}

#[test]
fn a_saved_configuration_child_follows_its_changes_and_warns_once_deleted() {
    let mut f = Fixture::open("test-data", "us", true);
    let saved = f.call(
        "report.saveConfiguration",
        &json!({ "name": "Short", "report": BALANCE_SHEET, "options": { "General": { "Report name": "Short" } } }),
    );
    let guid = saved["guid"].as_str().unwrap().to_owned();
    let options = json!({ "__general": { "report-list": [{ "report": guid }] }, "General": { "Add options summary": "always" } });
    let view = f.call(
        "report.saveConfiguration",
        &json!({ "name": "Board pack", "report": VIEW, "options": options }),
    );
    let run = f.run(view["guid"].as_str().unwrap(), &json!({}));
    let cell = &cells(&run)[0];
    assert_eq!(cell["result"]["report"]["configuration"], json!(guid));
    assert_eq!(cell["link"]["report"]["configuration"], json!(guid));
    let summary = run["sections"].as_array().unwrap().last().unwrap()["blocks"][0].clone();
    assert_eq!(summary["rows"][0], json!({ "section": "", "name": "Embedded Report", "value": "Short" }));

    f.call(
        "report.updateConfiguration",
        &json!({ "configuration": "Short", "options": { "General": { "Report name": "Shorter" } } }),
    );
    let run = f.run("Board pack", &json!({}));
    assert_eq!(cells(&run)[0]["result"]["options"]["General"]["Report name"], "Shorter", "a reference, not a copy");

    f.call("report.deleteConfiguration", &json!({ "configuration": "Short" }));
    let run = f.run("Board pack", &json!({}));
    let cell = &cells(&run)[0];
    assert_eq!(cell["warning"]["title"], "Report error");
    assert!(cell["warning"]["text"].as_str().unwrap().contains(&guid));
    assert!(f.export("Board pack", &json!({})).contains("<h3>Report error</h3>"));
}
