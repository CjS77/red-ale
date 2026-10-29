//! The Dashboard and built-in saved configurations (`design/reports.md` §9, §11; `design/milestone6.md`, "Decided",
//! item 5): GnuCash's Reports › Multicolumn › Dashboard (`5.17:gnucash/gnome/report-menus.scm:113-121`) is a
//! read-only saved configuration of the Multicolumn View, laid out as `gnc:make-dashboard` lays it out
//! (`5.17:gnucash/report/reports/standard/dashboard.scm:40-65`). GnuCash has no test of it, so the tests check its
//! layout, that it gives the bytes of the same view saved by hand, and how it is named, refused and copied.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod report_runs;

use red_ale_rpc::codes;
use report_runs::Fixture;
use serde_json::{Value, json};

const DASHBOARD: &str = "ad80271c890b11dfa79f2dcedfd72085";
const VIEW: &str = "d8ba4a2e89e8479ca9f6eccdeb164588";
const EXPENSE_PIE: &str = "9bf1892805cb4336be6320fe48ce5446";
const INCOME_PIE: &str = "e1bd09b8a1dd49dd85760db9d82b045c";
const INCOME_EXPENSE_CHART: &str = "80769921e87943adade887b9835a7685";
const ACCOUNT_SUMMARY: &str = "3298541c236b494998b236dfad6ad752";

/// The Dashboard's options as a Multicolumn View saved by hand would take them.
fn by_hand() -> Value {
    let child = |report: &str, colspan: u32| json!({ "report": report, "colspan": colspan, "rowspan": 1 });
    json!({
        "General": { "Report name": "Dashboard", "Number of columns": 2 },
        "__general": { "report-list": [
            child(EXPENSE_PIE, 1), child(INCOME_PIE, 1), child(INCOME_EXPENSE_CHART, 2), child(ACCOUNT_SUMMARY, 2)
        ] }
    })
}

/// The view's grid as rows of `(child template, colspan, rowspan)`.
fn grid(result: &Value) -> Vec<Vec<(String, i64, i64)>> {
    let grid = &result["sections"][0]["blocks"][0];
    assert_eq!(grid["kind"], "grid");
    let cell = |c: &Value| {
        let template = c["result"]["report"]["guid"].as_str().unwrap().to_owned();
        (template, c["colspan"].as_i64().unwrap(), c["rowspan"].as_i64().unwrap())
    };
    grid["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["cells"].as_array().unwrap().iter().map(cell).collect())
        .collect()
}

/// The `report.list` entry with `guid`.
fn listed(f: &mut Fixture, guid: &str) -> Vec<Value> {
    let list = f.call("report.list", &json!({}));
    list["reports"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["guid"] == guid)
        .cloned()
        .collect()
}

/// The error `method` gives: its code's name and its data, without the `help` every error carries.
fn error(f: &mut Fixture, method: &str, params: &Value) -> (String, Value) {
    let e = f.d.call(method, params.clone()).unwrap_err();
    let mut data = e.data.unwrap_or(Value::Null);
    if let Some(d) = data.as_object_mut() {
        d.remove("help");
    }
    (codes::name(e.code).unwrap().to_owned(), data)
}

fn config_path(f: &Fixture) -> std::path::PathBuf {
    f.book_path().with_file_name("config.json")
}

fn config(f: &Fixture) -> Value {
    serde_json::from_str(&std::fs::read_to_string(config_path(f)).unwrap()).unwrap()
}

#[test]
fn the_dashboard_is_listed_as_a_read_only_saved_configuration_with_its_menu_item() {
    let mut f = Fixture::open("test-data", "us", true);
    let expected = json!({
        "guid": DASHBOARD, "name": "Dashboard", "kind": "saved", "template": VIEW, "script": "standard/view-column.rhai",
        "menuPath": ["_Multicolumn"], "menuName": null, "menuTip": "A basic dashboard for your accounting data",
        "inMenu": true, "exportTypes": [], "readOnly": true
    });
    assert_eq!(listed(&mut f, DASHBOARD), std::slice::from_ref(&expected));
    let list = f.call("report.list", &json!({}));
    let read_only: Vec<&Value> = list["reports"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r.get("readOnly").is_some())
        .collect();
    assert_eq!(read_only, [&expected], "only the built-in configuration has readOnly");
    assert_eq!(list["problems"], json!([]));
    let options = f.call("report.options", &json!({ "report": "Dashboard" }));
    assert_eq!(options["report"], expected);
    let value = |section: &str, name: &str| {
        let all = options["options"].as_array().unwrap();
        all.iter().find(|o| o["section"] == section && o["name"] == name).unwrap()["value"].clone()
    };
    assert_eq!(value("General", "Number of columns"), json!(2));
    assert_eq!(value("General", "Report name"), json!("Dashboard"));
    assert_eq!(value("__general", "report-list"), by_hand()["__general"]["report-list"]);
}

/// Laid out by `render-view` at width 2 (`5.17:gnucash/report/reports/standard/view-column.scm:51-159`): the two pies
/// in row 0, the chart in row 1, the summary in row 2.
fn check_layout(f: &mut Fixture) -> Value {
    let by_name = f.run("Dashboard", &json!({}));
    let by_guid = f.run(DASHBOARD, &json!({}));
    assert_eq!(by_name, by_guid);
    let rows = [
        vec![(EXPENSE_PIE.to_owned(), 1, 1), (INCOME_PIE.to_owned(), 1, 1)],
        vec![(INCOME_EXPENSE_CHART.to_owned(), 2, 1)],
        vec![(ACCOUNT_SUMMARY.to_owned(), 2, 1)],
    ];
    assert_eq!(grid(&by_name), rows);
    let cells = by_name["sections"][0]["blocks"][0]["rows"].as_array().unwrap().iter();
    assert!(
        cells
            .flat_map(|r| r["cells"].as_array().unwrap())
            .all(|c| c.get("warning").is_none_or(Value::is_null)),
        "every child runs"
    );
    assert_eq!(by_name["report"]["configuration"], DASHBOARD);
    assert_eq!(by_name["title"], "");
    assert_eq!(f.export("Dashboard", &json!({})), f.export(DASHBOARD, &json!({})));
    by_name
}

#[test]
fn the_dashboard_runs_by_name_and_guid_as_gnucash_lays_it_out_on_xml_and_sqlite() {
    let mut xml = Fixture::open("test-data", "us", true);
    let mut sqlite = Fixture::open_sqlite("test-data", "us");
    let (a, b) = (check_layout(&mut xml), check_layout(&mut sqlite));
    assert_eq!(a["sections"], b["sections"]);
    assert_eq!(xml.export("Dashboard", &json!({})), sqlite.export("Dashboard", &json!({})));
    let two_columns = xml.run("Dashboard", &json!({ "General": { "Number of columns": 1 } }));
    assert_eq!(grid(&two_columns).len(), 4, "options change it for one run");
    assert_eq!(xml.call("book.info", &json!({}))["dirty"], false, "a run leaves the Book as it was");
    assert_eq!(xml.files(), ["book.gnucash", "config.json", "reports"]);
    assert!(config(&xml)["reports"].get("saved").is_none(), "the Configuration never stores it");
}

#[test]
fn the_dashboard_gives_the_bytes_of_a_multicolumn_view_saved_by_hand() {
    let mut f = Fixture::open("test-data", "us", true);
    let saved = f.call(
        "report.saveConfiguration",
        &json!({ "name": "Mine", "report": VIEW, "options": by_hand() }),
    );
    let mine = saved["guid"].as_str().unwrap().to_owned();
    assert_eq!(f.export(&mine, &json!({})), f.export("Dashboard", &json!({})));
    let (a, b) = (f.run(&mine, &json!({})), f.run("Dashboard", &json!({})));
    ["title", "options", "sections", "log"]
        .iter()
        .for_each(|k| assert_eq!(a[k], b[k], "{k}"));
}

#[test]
fn the_dashboard_cannot_be_changed_renamed_or_deleted_only_copied() {
    let mut f = Fixture::open("test-data", "us", true);
    let refused = [
        (
            "report.updateConfiguration",
            json!({ "configuration": DASHBOARD, "options": { "General": { "Number of columns": 3 } } }),
        ),
        ("report.updateConfiguration", json!({ "configuration": "Dashboard", "options": {} })),
        (
            "report.renameConfiguration",
            json!({ "configuration": "Dashboard", "name": "Board" }),
        ),
        ("report.deleteConfiguration", json!({ "configuration": DASHBOARD })),
    ];
    refused.iter().for_each(|(method, params)| {
        let (code, data) = error(&mut f, method, params);
        assert_eq!(code, "conflict", "{method}");
        assert_eq!(
            data,
            json!({ "kind": "configuration", "reason": "built-in", "guid": DASHBOARD, "name": "Dashboard" })
        );
    });
    assert!(config(&f)["reports"].get("saved").is_none(), "nothing written");

    let copy = f.call("report.saveConfiguration", &json!({ "name": "Board", "report": "Dashboard" }));
    assert_eq!((copy["kind"].clone(), copy["template"].clone()), (json!("saved"), json!(VIEW)));
    assert!(copy.get("readOnly").is_none() && copy["inMenu"] == false);
    assert_eq!(
        config(&f)["reports"]["saved"][0]["options"],
        by_hand(),
        "the copy stores the Dashboard's values"
    );
    f.call(
        "report.updateConfiguration",
        &json!({ "configuration": "Board", "options": { "General": { "Number of columns": 3 } } }),
    );
    assert_eq!(grid(&f.run("Board", &json!({})))[0].len(), 3, "the copy changes");
    assert_eq!(grid(&f.run("Dashboard", &json!({})))[0].len(), 2, "the Dashboard does not");

    let wrapper = json!({ "__general": { "report-list": [{ "report": DASHBOARD }] } });
    let run = f.run(VIEW, &wrapper);
    let cell = &run["sections"][0]["blocks"][0]["rows"][0]["cells"][0];
    assert_eq!(
        cell["result"]["report"]["configuration"], DASHBOARD,
        "a view can hold the Dashboard"
    );
}

/// GnuCash names a user's saved Dashboard from its "Report name", so built-in names are not reserved
/// (`design/notes/report-invoice.md` §6, question 5): the name then matches both, and the GUID always works.
#[test]
fn a_users_own_dashboard_shares_the_name_and_the_guid_tells_them_apart() {
    let mut f = Fixture::open("test-data", "us", true);
    let mine = f.call(
        "report.saveConfiguration",
        &json!({ "name": "Dashboard", "report": VIEW, "options": by_hand() }),
    );
    let mine = mine["guid"].as_str().unwrap().to_owned();
    let both = json!({ "matches": [DASHBOARD, mine] });
    assert_eq!(
        error(&mut f, "report.run", &json!({ "report": "Dashboard" })),
        ("conflict".to_owned(), both.clone())
    );
    let update = json!({ "configuration": "Dashboard", "options": {} });
    assert_eq!(error(&mut f, "report.updateConfiguration", &update), ("conflict".to_owned(), both));
    assert_eq!(f.export(&mine, &json!({})), f.export(DASHBOARD, &json!({})));
    let deleted = f.call("report.deleteConfiguration", &json!({ "configuration": mine }));
    assert_eq!(deleted["deleted"]["name"], "Dashboard");
    assert_eq!(listed(&mut f, DASHBOARD).len(), 1);
}

/// A GnuCash user's Dashboard as `saved-reports-2.8` holds it once saved: GnuCash gives it a new GUID and names it
/// from its "Report name" (`5.17:gnucash/report/report-core.scm:515-532`, `:620-643`), and the children, at their
/// defaults, set no options.
const GNUCASH_DASHBOARD: &str = r#"
;; Options for saved report "Dashboard", based on template "d8ba4a2e89e8479ca9f6eccdeb164588"
(let ()
  (define (options-gen)
    (let
         (
           (options (gnc:report-template-new-options/report-guid "d8ba4a2e89e8479ca9f6eccdeb164588" "Multicolumn View"))
           (new-embedded-report-ids '()) ;; only used with Multicolumn View Reports
         )
; Section: General

(let ((option (gnc:lookup-option options
                                 "General"
                                 "Report name")))
  ((lambda (o) (if o (gnc:option-set-value o "Dashboard"))) option))

(let ((option (gnc:lookup-option options
                                 "General"
                                 "Number of columns")))
  ((lambda (o) (if o (gnc:option-set-value o '2.0))) option))


; Section: __general

(let ((option (gnc:lookup-option options
                                 "__general"
                                 "report-list")))
  ((lambda (o) (if o (gnc:option-set-value o '((2 1 1 #f) (3 1 1 #f) (4 2 1 #f) (5 2 1 #f))))) option))

CHILDREN
      ;;;; Update Section: __general
      ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
      (let*
        (
          (option (gnc-lookup-option (gnc:optiondb options) "__general" "report-list"))
          (saved-report-list (GncOption-get-value option))
        )
        (
          (lambda (option)
            (if option ((gnc:option-setter option) (map (lambda (x y) (cons x (cdr y))) new-embedded-report-ids saved-report-list)))
          )
          option
        )
      )

      options
    )
  )
  (gnc:define-report
    'version 1
    'name "Dashboard"
    'report-guid "GUID"
    'parent-type "d8ba4a2e89e8479ca9f6eccdeb164588"
    'options-generator options-gen
    'menu-path (list gnc:menuname-custom)
    'renderer (gnc:report-template-renderer/report-guid "d8ba4a2e89e8479ca9f6eccdeb164588" "Multicolumn View")
  )
)
"#;

/// One embedded child at its template's defaults, as GnuCash writes it.
fn embedded(guid: &str, name: &str) -> String {
    format!(
        r#"
      ;;;; Options for embedded report
      ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
      ;; options for report "{name}"
      (let ((options (gnc:report-template-new-options/report-guid "{guid}" "{name}")))

        (set! new-embedded-report-ids
          (append
            new-embedded-report-ids
              (list (gnc:restore-report-by-guid-with-custom-template #f "{guid}" "{name}" "" options))
          )
        )
      )
"#
    )
}

fn gnucash_dashboard(guid: &str) -> String {
    let children = [
        (EXPENSE_PIE, "Expense Accounts"),
        (INCOME_PIE, "Income Accounts"),
        (INCOME_EXPENSE_CHART, "Income/Expense Chart"),
        (ACCOUNT_SUMMARY, "Account Summary"),
    ];
    let children: String = children.iter().map(|(g, n)| embedded(g, n)).collect();
    GNUCASH_DASHBOARD
        .replace("CHILDREN", &children)
        .replace("GUID\"", &format!("{guid}\""))
}

#[test]
fn importing_a_gnucash_users_dashboard_keeps_it_beside_the_built_in_one() {
    let mut f = Fixture::open("test-data", "us", true);
    let theirs = "0123456789abcdef0123456789abcdef";
    let imported = f.call("report.importConfigurations", &json!({ "text": gnucash_dashboard(theirs) }));
    assert_eq!(imported["skipped"], json!([]));
    assert_eq!(imported["imported"][0]["configuration"]["guid"], theirs);
    assert_eq!(imported["imported"][0]["configuration"]["name"], "Dashboard");
    assert_eq!(
        f.export(theirs, &json!({})),
        f.export(DASHBOARD, &json!({})),
        "GnuCash's Dashboard, saved, is red-ale's"
    );
    let (code, data) = error(&mut f, "report.run", &json!({ "report": "Dashboard" }));
    assert_eq!((code.as_str(), data), ("conflict", json!({ "matches": [DASHBOARD, theirs] })));

    let again = f.call("report.importConfigurations", &json!({ "text": gnucash_dashboard(DASHBOARD) }));
    assert_eq!(again["skipped"][0]["reason"], "exists", "the built-in GUID is taken");
}

#[test]
fn a_configuration_entry_with_the_built_in_guid_is_left_out_and_kept() {
    let mut f = Fixture::open("test-data", "us", true);
    let entry = json!({ "guid": DASHBOARD, "name": "Old board", "template": VIEW, "options": {} });
    let mut config = config(&f);
    config["reports"]["saved"] = json!([entry]);
    std::fs::write(config_path(&f), config.to_string()).unwrap();

    let list = f.call("report.list", &json!({}));
    let problems = list["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1);
    assert_eq!(
        (problems[0]["source"].clone(), problems[0]["code"].clone()),
        (json!("reports.saved"), json!("report-load"))
    );
    assert!(problems[0]["message"].as_str().unwrap().contains("Old board"));
    assert_eq!(listed(&mut f, DASHBOARD)[0]["name"], "Dashboard", "the built-in one wins");
    assert_eq!(error(&mut f, "report.run", &json!({ "report": "Old board" })).0, "not-found");

    f.call("report.saveConfiguration", &json!({ "name": "Other", "report": VIEW }));
    let saved = self::config(&f)["reports"]["saved"].clone();
    assert_eq!(saved[0], entry, "the entry left out is written back as it was");
    assert_eq!(saved[1]["name"], "Other");
}
