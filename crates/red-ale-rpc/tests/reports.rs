//! The report methods (`design/api/reports.md`) over user scripts in a temporary report directory, on copies of the
//! `reports/balsheet-pnl.gnucash` fixture. Every template here is a test script; listings leave the built-in reports
//! out.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use red_ale_service::config::ConfigStore;
use serde_json::{Value, json};

/// 2024-03-15 12:00 UTC, the pinned now of `fixtures/reports/`.
const NOW: Time64 = Time64(1_710_504_000);

const LISTING: &str = "0000000000000000000000000000a001";
const SHARED: &str = "0000000000000000000000000000a002";
const THROWS: &str = "0000000000000000000000000000a003";
const INVALID: &str = "0000000000000000000000000000a004";
const SPINS: &str = "0000000000000000000000000000a005";

/// Two templates: "Account Listing", which reads the Book and has options of several types, and "Shared", whose
/// name a saved configuration takes in the tests.
const LISTING_SCRIPT: &str = r##"
fn templates() {
    [#{ guid: "0000000000000000000000000000a001", name: "Account Listing", menuPath: ["_Test"], menuTip: "Lists accounts" },
     #{ guid: "0000000000000000000000000000a002", name: "Shared", inMenu: false, params: #{ copy: true } }]
}

fn options(params, o, book) {
    o.string("General", "Report Title", "a", "Title for this report.", "Accounts");
    o.number_range("Display", "Levels", "a", "How deep.", 2, 1, 10, 1);
    o.boolean("Display", "Show Chart", "b", "Draw a chart.", true);
    o.account_list("Accounts", "Accounts", "a", "Accounts to list.", [], #{ types: ["BANK", "ASSET"] });
}

fn render(params, book, opts, ctx) {
    print(`levels=${opts.get("Display", "Levels")}`);
    let rows = book.accounts().map(|a| #{ cells: [#{ text: a.name }] });
    let blocks = [#{ kind: "table", columns: [#{ label: "Account", role: "label" }], rows: rows }];
    if opts.get("Display", "Show Chart") {
        blocks.push(#{ kind: "chart", type: "bar", title: ["Accounts <b>"], labels: ["a", "b"],
                       series: [#{ label: "s", color: "#0074d9", values: [rational(1), rational(5, 2)] }] });
    }
    #{ title: opts.get("General", "Report Title"), sections: [#{ title: "Accounts", blocks: blocks }] }
}
"##;

const FAILING_SCRIPT: &str = r#"
fn templates() {
    [#{ guid: "0000000000000000000000000000a003", name: "Throws" },
     #{ guid: "0000000000000000000000000000a004", name: "Invalid" },
     #{ guid: "0000000000000000000000000000a005", name: "Spins" }]
}
fn options(params, o, book) { }
fn render(params, book, opts, ctx) {
    let name = opts.get("General", "Report name");
    if name == "Throws" { throw "no figures today"; }
    if name == "Invalid" { return #{ title: 3, sections: [] }; }
    loop { }
}
"#;

/// A report directory with the test scripts, one that does not compile and one that is not a script.
struct Setup {
    dir: tempfile::TempDir,
    d: Dispatcher,
}

impl Setup {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let reports = dir.path().join("reports");
        std::fs::create_dir_all(reports.join("mine")).unwrap();
        std::fs::write(reports.join("mine/listing.rhai"), LISTING_SCRIPT).unwrap();
        std::fs::write(reports.join("failing.rhai"), FAILING_SCRIPT).unwrap();
        std::fs::write(reports.join("broken.rhai"), "fn templates( {").unwrap();
        std::fs::write(reports.join("notes.txt"), "not a script").unwrap();
        let config = json!({ "reports": { "directory": "reports", "limits": { "operations": 200_000 }, "dateFormat": "iso" } });
        std::fs::write(dir.path().join("config.json"), config.to_string()).unwrap();
        let book = dir.path().join("book.gnucash");
        std::fs::copy(
            format!("{}/../../fixtures/reports/balsheet-pnl.gnucash", env!("CARGO_MANIFEST_DIR")),
            &book,
        )
        .unwrap();
        let d = dispatcher(&dir.path().join("config.json"));
        Self { dir, d }
    }

    fn book(&self) -> String {
        self.dir.path().join("book.gnucash").display().to_string()
    }

    fn config(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.dir.path().join("config.json")).unwrap()).unwrap()
    }

    fn open(&mut self, read_only: bool) {
        let book = self.book();
        self.call("book.open", json!({ "path": book, "readOnly": read_only }));
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.d
            .call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        self.d
            .call(method, params.clone())
            .err()
            .unwrap_or_else(|| panic!("{method} {params} should fail"))
    }

    /// Files in the book's directory, to see that nothing was written beside the book.
    fn files(&self) -> Vec<String> {
        let entries = std::fs::read_dir(self.dir.path()).unwrap();
        let mut names: Vec<String> = entries.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }
}

/// A dispatcher keeping its Configuration in `config`, with a fixed clock. Each environment it makes counts GUIDs
/// from its own seed, so a configuration saved with no book open never gets a GUID a later session hands out.
fn dispatcher(config: &Path) -> Dispatcher {
    let settings = Settings {
        config: ConfigStore::at(PathBuf::from(config)),
        ..Settings::default()
    };
    let mut d = Dispatcher::new(settings);
    let made = AtomicU32::new(0);
    d.set_env_factory(Box::new(move || {
        let seed = (0x00ed_u128 << 112) | (u128::from(made.fetch_add(1, Ordering::Relaxed)) << 80);
        Box::new(FixedEnv::new(NOW, seed))
    }));
    d
}

/// The user templates and saved configurations of a `report.list` result, leaving out the built-in reports.
fn listed(list: &Value) -> Vec<&Value> {
    list["reports"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] != "builtin")
        .collect()
}

fn names(list: &Value) -> Vec<&str> {
    listed(list).iter().map(|r| r["name"].as_str().unwrap()).collect()
}

#[test]
fn list_gives_every_template_and_the_scripts_that_did_not_load() {
    let mut s = Setup::new();
    let list = s.call("report.list", json!({}));
    assert_eq!(names(&list), ["Account Listing", "Invalid", "Shared", "Spins", "Throws"]);
    let user = listed(&list);
    let listing = user[0];
    assert_eq!(listing["guid"], LISTING);
    assert_eq!(listing["kind"], "user");
    assert_eq!(listing["script"], "./mine/listing.rhai");
    assert_eq!(listing["menuPath"], json!(["_Test"]));
    assert_eq!(listing["menuTip"], "Lists accounts");
    assert_eq!(listing["inMenu"], true);
    assert_eq!(user[2]["guid"], SHARED);
    assert_eq!(user[2]["inMenu"], false, "templates out of the menus are listed too");
    let problems = list["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(
        (problems[0]["source"].as_str(), problems[0]["code"].as_str()),
        (Some("./broken.rhai"), Some("report-load"))
    );
    assert_eq!(problems[0]["line"], 1);
}

#[test]
fn options_describe_defaults_values_and_the_params_schema_without_a_book() {
    let mut s = Setup::new();
    let options = s.call("report.options", json!({ "report": "Account Listing" }));
    assert_eq!(options["report"]["guid"], LISTING);
    let described: Vec<String> = options["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| format!("{}/{}", o["section"].as_str().unwrap(), o["name"].as_str().unwrap()))
        .collect();
    let expected = [
        "Accounts/Accounts",
        "Display/Levels",
        "Display/Show Chart",
        "General/Report name",
        "General/Stylesheet",
    ];
    assert_eq!(described[..5], expected[..], "sections by name, options by key: {described:?}");
    let stylesheet = options["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "Stylesheet")
        .unwrap();
    assert_eq!(stylesheet["choices"].as_array().unwrap().len(), 6, "the predefined style sheets");
    assert_eq!(
        options["paramsSchema"]["properties"]["Display"]["properties"]["Levels"]["default"],
        json!(2)
    );
    assert_eq!(
        s.err("report.options", json!({ "report": "No Such Report" })).code,
        codes::NOT_FOUND
    );
}

#[test]
fn running_and_exporting_leave_the_book_as_it_was() {
    let mut s = Setup::new();
    s.open(false);
    let before = s.d.context().book().unwrap().clone();
    let files = s.files();
    let result = s.call(
        "report.run",
        json!({ "report": "Account Listing", "options": { "Display": { "Levels": 3 } } }),
    );
    assert_eq!(result["report"]["guid"], LISTING);
    assert_eq!(result["now"], "2024-03-15T12:00:00Z", "now comes from the session's clock");
    assert_eq!(result["options"]["Display"]["Levels"], json!(3));
    assert_eq!(result["log"][0]["text"], "levels=3");
    assert!(result["sections"][0]["blocks"][0]["rows"].as_array().unwrap().len() > 3);
    let export = s.call("report.export", json!({ "report": LISTING }));
    assert_eq!(export["title"], "Accounts");
    assert_eq!(s.d.context().book().unwrap(), &before, "the Book is unchanged");
    assert_eq!(s.call("book.info", json!({}))["dirty"], false);
    assert_eq!(s.files(), files, "no journal or other file was written");
    assert!(!s.files().iter().any(|f| Path::new(f).extension().is_some_and(|x| x == "log")));
}

#[test]
fn reports_run_on_a_read_only_book_and_need_one() {
    let mut s = Setup::new();
    assert_eq!(s.err("report.run", json!({ "report": "Account Listing" })).code, codes::NO_BOOK);
    s.open(true);
    let result = s.call(
        "report.run",
        json!({ "report": "Account Listing", "now": "2024-01-31T23:00:00-02:00" }),
    );
    assert_eq!(result["now"], "2024-02-01T01:00:00Z", "the caller's now");
    assert!(
        s.call("report.export", json!({ "report": "Account Listing" }))["html"]
            .as_str()
            .unwrap()
            .starts_with("<!DOCTYPE html>")
    );
}

#[test]
fn two_exports_are_byte_identical_and_hold_no_script() {
    let mut s = Setup::new();
    s.open(true);
    let params = json!({ "report": "Account Listing", "styleSheet": "Head or Tail", "options": { "Accounts": { "Levels": 1 } } });
    assert_eq!(s.err("report.export", params).code, codes::REPORT_OPTION, "an unknown option");
    let params = json!({ "report": "Account Listing", "styleSheet": "Technicolor" });
    let (a, b) = (s.call("report.export", params.clone()), s.call("report.export", params));
    let html = a["html"].as_str().unwrap();
    assert_eq!(html, b["html"].as_str().unwrap());
    assert!(!html.to_ascii_lowercase().contains("<script"));
    assert!(html.contains("<svg") && html.contains("Accounts &lt;b&gt;"), "the chart, escaped");
    assert!(html.contains("sheet-fancy"), "Technicolor is a Fancy style sheet");
    let err = s.err("report.export", json!({ "report": "Account Listing", "styleSheet": "Nope" }));
    assert_eq!(err.code, codes::REPORT_OPTION);
    assert_eq!(err.data.unwrap()["name"], "Stylesheet");
}

#[test]
fn errors_carry_the_report_codes() {
    let mut s = Setup::new();
    s.open(true);
    let err = s.err("report.run", json!({ "report": THROWS }));
    assert_eq!(err.code, codes::REPORT_SCRIPT);
    assert_eq!(err.data.as_ref().unwrap()["source"], "./failing.rhai");
    assert!(err.data.unwrap()["message"].as_str().unwrap().contains("no figures today"));
    let err = s.err("report.run", json!({ "report": INVALID }));
    assert_eq!(err.code, codes::REPORT_RESULT);
    assert_eq!(err.data.unwrap()["path"], "title");
    let err = s.err("report.run", json!({ "report": SPINS }));
    assert_eq!(err.code, codes::REPORT_LIMIT);
    assert_eq!(err.data.unwrap()["limit"], "operations", "reports.limits.operations applies");
    let err = s.err(
        "report.run",
        json!({ "report": "Account Listing", "options": { "Display": { "Levels": 11 } } }),
    );
    assert_eq!(err.code, codes::REPORT_OPTION);
    assert_eq!(err.data.unwrap()["name"], "Levels");
    let err = s.err(
        "report.run",
        json!({ "report": "Account Listing", "options": { "Display": { "Levels": 2 } }, "nope": 1 }),
    );
    assert_eq!(err.code, codes::INVALID_PARAMS, "unknown params are refused");
    std::fs::write(s.dir.path().join("reports/mine/listing.rhai"), "fn templates() { [] ").unwrap();
    let err = s.err("report.run", json!({ "report": LISTING }));
    assert_eq!(err.code, codes::NOT_FOUND, "a script that no longer loads defines nothing");
    let problems = s.call("report.list", json!({}))["problems"].clone();
    assert!(
        problems
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["source"] == "./mine/listing.rhai" && p["code"] == "report-load")
    );
}

#[test]
fn saved_configurations_round_trip_through_the_configuration_file() {
    let mut s = Setup::new();
    let saved = s.call(
        "report.saveConfiguration",
        json!({ "name": "Deep", "report": "Account Listing", "options": { "Display": { "Levels": 4, "Show Chart": true } } }),
    );
    assert_eq!((saved["kind"].as_str(), saved["template"].as_str()), (Some("saved"), Some(LISTING)));
    let guid = saved["guid"].as_str().unwrap().to_owned();
    let stored = s.config()["reports"]["saved"].clone();
    assert_eq!(
        stored[0]["options"],
        json!({ "Display": { "Levels": 4 } }),
        "only what differs from the defaults"
    );
    assert_eq!(s.config()["reports"]["dateFormat"], "iso", "other keys are kept");

    let err = s.err("report.saveConfiguration", json!({ "name": "Deep", "report": "Account Listing" }));
    assert_eq!(err.code, codes::CONFLICT);
    let bad = json!({ "name": "Bad", "report": "Account Listing", "options": { "Display": { "Levels": 0 } } });
    let err = s.err("report.saveConfiguration", bad);
    assert_eq!(err.code, codes::REPORT_OPTION);

    let options = s.call("report.options", json!({ "report": "Deep" }));
    let levels = options["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "Levels")
        .unwrap()
        .clone();
    assert_eq!((levels["default"].clone(), levels["value"].clone()), (json!(2), json!(4)));

    s.open(true);
    let run = s.call(
        "report.run",
        json!({ "report": guid, "options": { "General": { "Report Title": "Mine" } } }),
    );
    assert_eq!(run["report"]["configuration"], json!(guid));
    let seen = (run["title"].as_str(), run["log"][0]["text"].as_str());
    assert_eq!(seen, (Some("Mine"), Some("levels=4")), "saved values, then the caller's");
    let over = s.call("report.run", json!({ "report": "Deep", "options": { "Display": { "Levels": 5 } } }));
    assert_eq!(over["log"][0]["text"], "levels=5");

    let changes = json!({ "configuration": "Deep", "options": { "Display": { "Levels": null, "Show Chart": false } } });
    let updated = s.call("report.updateConfiguration", changes);
    assert_eq!(updated["guid"], json!(guid));
    assert_eq!(
        s.config()["reports"]["saved"][0]["options"],
        json!({ "Display": { "Show Chart": false } }),
        "null returns to the default"
    );
    assert_eq!(
        s.err(
            "report.updateConfiguration",
            json!({ "configuration": "Account Listing", "options": {} })
        )
        .code,
        codes::NOT_FOUND
    );

    s.call("report.saveConfiguration", json!({ "name": "Shared", "report": "Deep" }));
    let err = s.err("report.run", json!({ "report": "Shared" }));
    assert_eq!(err.code, codes::CONFLICT, "a template and a saved configuration share the name");
    assert_eq!(err.data.unwrap()["matches"].as_array().unwrap().len(), 2);
    assert_eq!(
        s.config()["reports"]["saved"][1]["template"],
        LISTING,
        "a configuration of a configuration configures its template"
    );
    assert_eq!(
        s.err("report.renameConfiguration", json!({ "configuration": "Shared", "name": "Deep" }))
            .code,
        codes::CONFLICT
    );
    let renamed = s.call("report.renameConfiguration", json!({ "configuration": "Shared", "name": "Copy" }));
    assert_eq!(renamed["name"], "Copy");
    let list = s.call("report.list", json!({}));
    assert_eq!(
        names(&list),
        ["Account Listing", "Copy", "Deep", "Invalid", "Shared", "Spins", "Throws"]
    );

    let deleted = s.call("report.deleteConfiguration", json!({ "configuration": guid }));
    assert_eq!(deleted["deleted"]["name"], "Deep");
    assert_eq!(s.config()["reports"]["saved"].as_array().unwrap().len(), 1);
    assert_eq!(
        s.err("report.deleteConfiguration", json!({ "configuration": "Deep" })).code,
        codes::NOT_FOUND
    );
    assert_eq!(
        s.call("book.info", json!({}))["dirty"],
        false,
        "Configuration methods never touch the Book"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn style_sheets_are_created_changed_and_deleted_in_the_configuration() {
    let mut s = Setup::new();
    let list = s.call("report.styleSheets", json!({}));
    let templates: Vec<&str> = list["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(templates, ["CSS", "Easy", "Fancy", "Footer", "Head or Tail", "Plain"]);
    let sheets: Vec<&str> = list["styleSheets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        sheets,
        [
            "CSS-based stylesheet (experimental)",
            "Default",
            "Easy",
            "Footer",
            "Head or Tail",
            "Technicolor"
        ]
    );

    let options = json!({ "General": { "Preparer": "A. Bookkeeper" }, "Images": { "Logo": "logo.png" } });
    let created = s.call(
        "report.createStyleSheet",
        json!({ "name": "Mine", "template": "Footer", "options": options }),
    );
    assert_eq!(
        (created["template"].as_str(), created["predefined"].as_bool()),
        (Some("Footer"), Some(false))
    );
    assert_eq!(
        s.config()["reports"]["styleSheets"][0]["options"],
        options,
        "image options are kept though red-ale ignores them"
    );
    assert_eq!(
        s.err("report.createStyleSheet", json!({ "name": "Mine", "template": "Plain" }))
            .code,
        codes::CONFLICT
    );
    assert_eq!(
        s.err("report.createStyleSheet", json!({ "name": "Other", "template": "Gothic" }))
            .code,
        codes::NOT_FOUND
    );
    let css = json!({ "General": { "CSS": "body { background: url(http://x) }" } });
    assert_eq!(
        s.err(
            "report.createStyleSheet",
            json!({ "name": "Other", "template": "CSS", "options": css })
        )
        .code,
        codes::REPORT_OPTION
    );

    let general = json!({ "Preparer": null, "Prepared for": "B", "Show preparer info": true });
    s.call(
        "report.updateStyleSheet",
        json!({ "name": "Mine", "options": { "General": general } }),
    );
    let stored = &s.config()["reports"]["styleSheets"][0]["options"];
    let general = json!({ "Prepared for": "B", "Show preparer info": true });
    assert_eq!(
        stored,
        &json!({ "General": general, "Images": { "Logo": "logo.png" } }),
        "merged, not replaced"
    );
    s.call(
        "report.updateStyleSheet",
        json!({ "name": "Default", "options": { "Tables": { "Table border width": 3 } } }),
    );
    assert_eq!(
        s.err("report.updateStyleSheet", json!({ "name": "Nope", "options": {} })).code,
        codes::NOT_FOUND
    );

    let styled = json!({ "name": "Styled", "report": "Account Listing", "options": { "General": { "Stylesheet": "Mine" } } });
    s.call("report.saveConfiguration", styled);
    s.open(true);
    let styled = s.call("report.export", json!({ "report": "Styled" }))["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        styled.contains("sheet-footer") && styled.contains("Prepared for: <b>B</b>"),
        "{styled}"
    );

    let deleted = s.call("report.deleteStyleSheet", json!({ "name": "Mine" }));
    assert_eq!(deleted["reset"], false);
    let base = s.call("report.export", json!({ "report": "Styled" }))["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(base.contains("sheet-none"), "a deleted style sheet renders with the base look");
    let run = s.call("report.run", json!({ "report": "Styled" }));
    assert_eq!(
        run["options"]["General"]["Stylesheet"], "Mine",
        "the saved configuration still names it"
    );
    let reset = s.call("report.deleteStyleSheet", json!({ "name": "Default" }));
    assert_eq!(reset["reset"], true);
    assert_eq!(s.config()["reports"]["styleSheets"], json!([]));
    assert_eq!(s.err("report.deleteStyleSheet", json!({ "name": "Mine" })).code, codes::NOT_FOUND);
}

#[test]
fn configuration_methods_work_with_no_book_and_declare_their_target() {
    let mut s = Setup::new();
    s.call("report.saveConfiguration", json!({ "name": "No book", "report": LISTING }));
    s.call("report.createStyleSheet", json!({ "name": "Plainer", "template": "Plain" }));
    let schema = s.d.schema();
    let declared = |name: &str| {
        let m = schema["methods"].as_array().unwrap().iter().find(|m| m["name"] == name).unwrap();
        (m["effect"].as_str().unwrap().to_owned(), m["target"].clone())
    };
    let expect = |effect: &str, target: Value| (effect.to_owned(), target);
    ["report.list", "report.options", "report.run", "report.export", "report.styleSheets"]
        .iter()
        .for_each(|m| assert_eq!(declared(m), expect("read", Value::Null), "{m}"));
    let writes = [
        "saveConfiguration",
        "updateConfiguration",
        "renameConfiguration",
        "createStyleSheet",
        "updateStyleSheet",
    ];
    writes
        .iter()
        .for_each(|m| assert_eq!(declared(&format!("report.{m}")), expect("write", json!("configuration")), "{m}"));
    ["report.deleteConfiguration", "report.deleteStyleSheet"]
        .iter()
        .for_each(|m| assert_eq!(declared(m), expect("destructive", json!("configuration")), "{m}"));
    let errors: Vec<(i64, &str)> = schema["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["code"].as_i64().unwrap(), e["name"].as_str().unwrap()))
        .collect();
    let names = ["report-script", "report-limit", "report-option", "report-result", "report-load"];
    names
        .iter()
        .zip(3000..)
        .for_each(|(name, code)| assert!(errors.contains(&(code, *name)), "{code} {name}"));
}

/// A template that offers CSV and, asked for it, says why it has nothing to export, as the Transaction Report does
/// with `gnc:html-document-set-export-error` (`5.17:gnucash/report/trep-engine.scm:2524`).
const EXPORT_ERROR_SCRIPT: &str = r#"
fn templates() { [#{ guid: "0000000000000000000000000000a006", name: "No Splits", exportTypes: ["csv"] }] }
fn options(params, o, book) { }
fn render(params, book, opts, ctx) {
    let draft = #{ title: "No Splits", sections: [] };
    if ctx.export_type() == "csv" { draft.exportError = "No splits found"; }
    draft
}
"#;

#[test]
fn an_export_error_answers_export_csv() {
    let mut s = Setup::new();
    std::fs::write(s.dir.path().join("reports/export-error.rhai"), EXPORT_ERROR_SCRIPT).unwrap();
    s.open(true);
    assert!(s.call("report.run", json!({ "report": "No Splits" })).get("exportError").is_none());
    let err = s.err("report.exportCsv", json!({ "report": "No Splits" }));
    assert_eq!(err.code, codes::INVALID);
    assert_eq!(err.message, "No splits found");
    assert_eq!(err.data.unwrap(), json!({ "reason": "export-error", "message": "No splits found" }));
}
