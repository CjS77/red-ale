//! `report.importConfigurations` and `report.importStyleSheets` (`design/api/reports.md`): GnuCash's saved report
//! configurations and style sheets, from `fixtures/gnucash-userdata/`, written by hand exactly as GnuCash 5.17 writes
//! them (`5.17:gnucash/report/report-core.scm:620-643`, `5.17:gnucash/report/html-style-sheet.scm:145-166`,
//! `5.17:bindings/guile/options.scm:130-172`), since the oracle has no GUI to save them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod chart_reports;

use chart_reports::Charts;
use serde_json::{Value, json};

const NET_WORTH_LINECHART: &str = "d8b63264186b11e19038001558291366";
const BALANCE_SHEET: &str = "c4173ac99b2b448289bf4d11c731af13";
const NET_WORTH_SAVED: &str = "5e1f0a2b3c4d4e5f8a9b0c1d2e3f4a5b";
const BALANCE_SHEET_SAVED: &str = "8c4d5e6f708142939405c6d7e8f9a0b1";
const BANK: &str = "a0000000000000000000000000000006";
const LIABILITY: &str = "a0000000000000000000000000000008";

fn userdata(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../../fixtures/gnucash-userdata/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn config(c: &Charts) -> Value {
    serde_json::from_str(&std::fs::read_to_string(c.path().join("config.json")).unwrap()).unwrap()
}

fn import_reports(c: &mut Charts, dry_run: bool) -> Value {
    c.call(
        "report.importConfigurations",
        json!({ "text": userdata("saved-reports-2.8"), "dryRun": dry_run }),
    )
}

fn skipped(result: &Value) -> Vec<(String, String)> {
    let one = |s: &Value| {
        (
            s["name"].as_str().unwrap_or("").to_owned(),
            s["reason"].as_str().unwrap().to_owned(),
        )
    };
    result["skipped"].as_array().unwrap().iter().map(one).collect()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect()
}

/// The net worth chart keeps every option the entry sets except the currency and the end date (`end-prev-fin-year`,
/// the end of the accounting period), which are the template's defaults; the
/// Balance Sheet keeps GnuCash's GUID and name, reads the 2.x setter, the 4.x commodity form and the
/// `end-accounting-period` spelling, and drops the unknown "Use Tabs". The budget entry's `(gnc-budget-lookup …)` is
/// read as data and its GUID kept, though this book has no such budget. The Transaction Report entry keeps its start
/// date and ignores the `__reg/query` option the Transaction Report doesn't have. The untranslated Customer Summary
/// (v0.6) is skipped without its options being read.
#[test]
fn imports_saved_configurations() {
    let mut c = Charts::new("iso");
    let result = import_reports(&mut c, false);
    assert_eq!(result["dryRun"], false);
    assert_eq!(skipped(&result), pairs(&[("Customers this year", "untranslated")]));
    let imported = result["imported"].as_array().unwrap();
    assert_eq!(imported.len(), 5);

    let net = &imported[0];
    assert_eq!(net["line"], 3);
    assert_eq!(net["configuration"]["guid"], NET_WORTH_SAVED);
    assert_eq!(net["configuration"]["template"], NET_WORTH_LINECHART);
    assert_eq!(net["configuration"]["kind"], "saved");
    let want = json!({
        "Accounts": { "Accounts": [LIABILITY, BANK] },
        "Display": { "Line Width": "2.5", "Plot Width": { "pixels": 800 }, "Show table": true },
        "General": { "Price Source": "pricedb-latest",
                     "Report name": "Net worth, monthly", "Start Date": { "absolute": "2024-01-01T10:59:00Z" },
                     "Step Size": "QuarterDelta", "Stylesheet": "Easy" },
    });
    assert_eq!(net["options"], want);
    assert_eq!(net["ignored"], json!([]));

    let transactions = &imported[1];
    assert_eq!(transactions["configuration"]["name"], "Transactions this month");
    assert_eq!(
        transactions["options"],
        json!({ "General": { "Start Date": { "relative": "start-this-month" } } })
    );
    assert_eq!(
        transactions["ignored"],
        json!([{ "section": "__reg", "name": "query", "reason": "unknown-option" }])
    );

    let budget = &imported[2];
    assert_eq!(budget["configuration"]["name"], "Household budget");
    assert_eq!(
        budget["options"],
        json!({ "General": { "Budget": "b0000000000000000000000000000001" } })
    );

    let sheet = &imported[3];
    assert_eq!(sheet["configuration"]["guid"], BALANCE_SHEET_SAVED);
    assert_eq!(sheet["configuration"]["name"], "Year-end balance sheet");
    let want = json!({
        "Accounts": { "Levels of Subaccounts": 2 },
        "Display": { "Parent account balances": "recursive-bal" },
        "General": { "Report name": "Year-end balance sheet", "Single column Balance Sheet": false, "Stylesheet": "Head or Tail" },
    });
    assert_eq!(
        sheet["options"], want,
        "the default currency and end-accounting-period date are not kept"
    );
    assert_eq!(
        sheet["ignored"],
        json!([{ "section": "General", "name": "Use Tabs", "reason": "unknown-option" }])
    );

    let saved = config(&c)["reports"]["saved"].clone();
    assert_eq!(saved.as_array().unwrap().len(), 5);
    assert_eq!(saved[0]["guid"], NET_WORTH_SAVED);
    assert_eq!(saved[3]["options"], sheet["options"]);
    let list = c.call("report.list", json!({}));
    assert!(
        list["reports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["name"] == "Net worth, monthly" && r["kind"] == "saved")
    );
}

/// Both spellings of the accounting period's dates read alike: GnuCash 5.17 writes `end-prev-fin-year`
/// (`5.17:libgnucash/engine/gnc-option-date.cpp:319`) and reads back `end-accounting-period`
/// (`5.17:bindings/guile/gnc-optiondb.i:894-897`).
#[test]
fn both_accounting_period_spellings() {
    let mut c = Charts::open("charts-daily.gnucash", "iso");
    let text = userdata("saved-reports-2.8");
    let entry = |end: &str| text.replace("'(relative . end-prev-fin-year)", &format!("'(relative . {end})"));
    let imported = |c: &mut Charts, end: &str| c.call("report.importConfigurations", json!({ "text": entry(end), "dryRun": true }));
    let (fin_year, period) = (imported(&mut c, "end-prev-fin-year"), imported(&mut c, "end-accounting-period"));
    assert_eq!(fin_year["imported"][0]["options"], period["imported"][0]["options"]);
    assert!(
        fin_year["imported"][0]["options"]["General"].get("End Date").is_none(),
        "both are the template's default"
    );
    let month = imported(&mut c, "end-this-month");
    assert_eq!(
        month["imported"][0]["options"]["General"]["End Date"],
        json!({ "relative": "end-this-month" })
    );
    let start_text = text.replace("'(absolute . 1704106740)", "'(relative . start-prev-fin-year)");
    let result = c.call("report.importConfigurations", json!({ "text": start_text, "dryRun": true }));
    assert!(
        result["imported"][0]["options"]["General"].get("Start Date").is_none(),
        "start-prev-fin-year is the default start"
    );
}

#[test]
fn dry_run_writes_nothing_and_a_second_import_skips() {
    let mut c = Charts::new("iso");
    let before = config(&c);
    let result = import_reports(&mut c, true);
    assert_eq!(
        (result["dryRun"].as_bool(), result["imported"].as_array().unwrap().len()),
        (Some(true), 5)
    );
    let children = result["imported"][4]["options"]["__general"]["report-list"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(children, 3, "a dry run reads the view's children too");
    assert_eq!(config(&c), before);
    import_reports(&mut c, false);
    let again = import_reports(&mut c, false);
    assert_eq!(again["imported"], json!([]));
    let reasons: Vec<String> = skipped(&again).into_iter().map(|s| s.1).collect();
    assert_eq!(reasons, ["exists", "exists", "exists", "exists", "untranslated", "exists"]);
}

#[test]
fn name_conflicts_are_skipped() {
    let mut c = Charts::new("iso");
    c.call(
        "report.saveConfiguration",
        json!({ "name": "Year-end balance sheet", "report": "Balance Sheet" }),
    );
    let result = import_reports(&mut c, false);
    assert_eq!(skipped(&result)[0], ("Year-end balance sheet".to_owned(), "conflict".to_owned()));
}

/// The imported configuration runs as the template with the same options set by hand.
#[test]
fn imported_configurations_run_as_set_by_hand() {
    let mut c = Charts::open("charts-daily.gnucash", "iso");
    let imported = import_reports(&mut c, false);
    [(NET_WORTH_SAVED, NET_WORTH_LINECHART, 0), (BALANCE_SHEET_SAVED, BALANCE_SHEET, 3)]
        .iter()
        .for_each(|(saved, template, i)| {
            let by_import = c.call("report.run", json!({ "report": saved }));
            let options = imported["imported"][i]["options"].clone();
            let by_hand = c.call("report.run", json!({ "report": template, "options": options }));
            assert_eq!(by_import["sections"], by_hand["sections"], "{template}");
            assert_eq!(by_import["options"], by_hand["options"], "{template}");
            assert_eq!(by_import["report"]["configuration"], json!(saved));
        });
    let run = c.call("report.run", json!({ "report": NET_WORTH_SAVED }));
    let chart = chart_reports::chart(&run);
    assert_eq!(chart["width"], json!({ "pixels": 800 }));
    assert_eq!(chart["series"][0]["lineWidth"], json!({ "num": "5", "denom": "2" }));
}

/// A form red-ale does not read is an error for its entry alone, and nothing in it runs.
#[test]
fn malformed_entries_are_skipped_never_run() {
    let mut c = Charts::new("iso");
    let text = userdata("saved-reports-2.8");
    let evil = text.replacen("'2.5", "(begin (system \"touch pwned\") 2.5)", 1);
    let result = c.call("report.importConfigurations", json!({ "text": evil }));
    let first = &result["skipped"][0];
    assert_eq!((first["reason"].as_str(), first["line"].as_u64()), (Some("malformed"), Some(3)));
    assert!(first["message"].as_str().unwrap().contains("not a literal value"), "{first}");
    assert_eq!(result["imported"].as_array().unwrap().len(), 4, "the other entries still import");
    assert!(!c.path().join("pwned").exists());

    // Cut inside the Balance Sheet entry, so it is left unfinished.
    let truncated = &text[..text.find("'name \"Year-end balance sheet\"").unwrap()];
    let result = c.call("report.importConfigurations", json!({ "text": truncated, "dryRun": true }));
    assert_eq!(result["skipped"].as_array().unwrap().last().unwrap()["reason"], "malformed");

    let bad_value = text.replace("'QuarterDelta", "'FortnightDelta");
    let result = c.call("report.importConfigurations", json!({ "text": bad_value, "dryRun": true }));
    let first = &result["skipped"][0];
    assert_eq!(first["reason"], "invalid-option");
    assert!(
        first["message"].as_str().unwrap().starts_with("option General/Step Size"),
        "{first}"
    );

    let result = c.call("report.importConfigurations", json!({ "text": "" }));
    assert_eq!(
        (
            result["imported"].as_array().unwrap().len(),
            result["skipped"].as_array().unwrap().len()
        ),
        (0, 0)
    );
    assert!(
        c.err("report.importConfigurations", json!({ "path": "/etc/passwd" }))
            .message
            .contains("unknown field")
    );
}

#[test]
fn imports_style_sheets() {
    let mut c = Charts::new("iso");
    let result = c.call("report.importStyleSheets", json!({ "text": userdata("stylesheets-2.0") }));
    assert_eq!(skipped(&result), pairs(&[("Easy", "unchanged"), ("Acme branded", "untranslated")]));
    let imported = result["imported"].as_array().unwrap();
    let names: Vec<&str> = imported.iter().map(|i| i["styleSheet"]["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Default", "Acme", "Plain CSS"]);
    assert_eq!(
        imported[1]["ignored"],
        json!([
            { "section": "Images", "name": "Heading Alignment", "reason": "not-rendered" },
            { "section": "Images", "name": "Logo", "reason": "not-rendered" },
        ])
    );
    assert_eq!(
        imported[2]["ignored"],
        json!([{ "section": "General", "name": "Obsolete Option", "reason": "unknown-option" }])
    );

    let records = config(&c)["reports"]["styleSheets"].clone();
    assert_eq!(
        records[0],
        json!({ "name": "Default", "template": "Plain",
        "options": { "General": { "Background Color": "fffaf0" }, "Tables": { "Table cell padding": 6 } } })
    );
    assert_eq!(
        records[1],
        json!({ "name": "Acme", "template": "Footer", "options": {
        "Colors": { "Text Color": "000080" },
        "Fonts": { "Title": "DejaVu Serif Bold 18" },
        "General": { "Footer": "Acme Ltd\n\"Confidential\"", "Prepared for": "Acme Ltd", "Preparer": "A. Bookkeeper",
                     "Show preparer info": true },
        "Tables": { "Table border width": 2 },
        "Images": { "Heading Alignment": "center", "Logo": "/home/me/acme-logo.png" } } })
    );
    assert_eq!(
        records[2]["options"]["General"]["CSS"],
        "body { color: #333; }\ntd.number-cell { font-weight: bold; }\n"
    );

    let sheets = c.call("report.styleSheets", json!({}));
    let acme = sheets["styleSheets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Acme")
        .unwrap()
        .clone();
    assert_eq!(acme["predefined"], false);
    let again = c.call("report.importStyleSheets", json!({ "text": userdata("stylesheets-2.0") }));
    assert_eq!(again["imported"], json!([]), "existing entries are kept, not overwritten");
}

/// A saved configuration imported before its style sheet still applies; once the style sheet is imported, an export
/// renders through it.
#[test]
fn configurations_name_style_sheets_imported_later() {
    let mut c = Charts::open("charts-daily.gnucash", "iso");
    let text = userdata("saved-reports-2.8").replace("'Easy", "'Acme");
    let result = c.call("report.importConfigurations", json!({ "text": text }));
    assert_eq!(result["imported"][0]["options"]["General"]["Stylesheet"], "Acme");
    c.call("report.importStyleSheets", json!({ "text": userdata("stylesheets-2.0") }));
    let html = c.call("report.export", json!({ "report": NET_WORTH_SAVED }))["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(html.contains("A. Bookkeeper"), "rendered through the imported Acme style sheet");
}

#[test]
fn style_sheet_values_red_ale_refuses_are_skipped() {
    let mut c = Charts::new("iso");
    let text = userdata("stylesheets-2.0").replace("td.number-cell", "@import url(x.css); td.number-cell");
    let result = c.call("report.importStyleSheets", json!({ "text": text, "dryRun": true }));
    let last = result["skipped"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        (last["name"].as_str(), last["reason"].as_str()),
        (Some("Plain CSS"), Some("invalid-option"))
    );
}

const VIEW: &str = "d8ba4a2e89e8479ca9f6eccdeb164588";
const VIEW_SAVED: &str = "ae5f6a7b8c9d40e1b2c3d4e5f6a7b8c9";
const ACCOUNT_SUMMARY: &str = "3298541c236b494998b236dfad6ad752";

/// The fixture's "Board pack" set by hand: each embedded report is its template with the options GnuCash copied into
/// it, and the spans of the report-list at its position. The Balance Sheet was copied from "Year-end balance sheet"
/// and keeps only the copy. The nested view's own children were not saved, so it has none.
fn board_pack() -> Value {
    json!({
        "General": { "Number of columns": 2, "Report name": "Board pack" },
        "__general": { "report-list": [
            { "report": ACCOUNT_SUMMARY, "colspan": 1, "rowspan": 1, "options": {
                "Accounts": { "Levels of Subaccounts": 1 }, "Display": { "Account Code": false },
                "General": { "Report name": "Summary" } } },
            { "report": BALANCE_SHEET, "colspan": 1, "rowspan": 2, "options": {
                "Accounts": { "Levels of Subaccounts": 2 }, "General": { "Report name": "Year-end balance sheet" } } },
            { "report": VIEW, "colspan": 2, "rowspan": 1, "options": { "General": { "Number of columns": 3 } } },
        ] },
    })
}

fn cells(result: &Value) -> Vec<Value> {
    let grid = &result["sections"][0]["blocks"][0];
    assert_eq!(grid["kind"], "grid");
    grid["rows"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["cells"].as_array().unwrap().clone())
        .collect()
}

/// A saved Multicolumn View imports with its embedded reports as template children, and runs as the same options set
/// by hand: each cell holds what its child gives run alone.
#[test]
fn imports_a_saved_multicolumn_view() {
    let mut c = Charts::open("charts-daily.gnucash", "iso");
    let result = import_reports(&mut c, false);
    let view = &result["imported"][4];
    assert_eq!(view["line"], 267);
    assert_eq!(
        (view["configuration"]["guid"].as_str(), view["configuration"]["template"].as_str()),
        (Some(VIEW_SAVED), Some(VIEW))
    );
    assert_eq!(view["options"], board_pack());
    assert_eq!(
        view["ignored"],
        json!([{ "child": 2, "section": "__general", "name": "report-list", "reason": "unsaved-children" }])
    );

    let by_import = c.call("report.run", json!({ "report": VIEW_SAVED }));
    let by_hand = c.call("report.run", json!({ "report": VIEW, "options": board_pack() }));
    assert_eq!(by_import["sections"], by_hand["sections"]);
    assert_eq!(by_import["options"], by_hand["options"]);
    let cells = cells(&by_import);
    assert_eq!(cells.len(), 3);
    let children = board_pack()["__general"]["report-list"].clone();
    cells.iter().zip(children.as_array().unwrap()).for_each(|(cell, child)| {
        let alone = c.call("report.run", json!({ "report": child["report"], "options": child["options"] }));
        ["report", "title", "options", "sections", "log"]
            .iter()
            .for_each(|k| assert_eq!(cell["result"][k], alone[k], "{} {k}", child["report"]));
        assert_eq!((&cell["colspan"], &cell["rowspan"]), (&child["colspan"], &child["rowspan"]));
    });
    assert_eq!(cells[0]["result"]["options"]["General"]["Report name"], "Summary");
    assert_eq!(
        cells[2]["result"]["sections"][0]["blocks"][0]["rows"],
        json!([]),
        "the nested view has no children"
    );
}

/// An entry with an embedded report whose template red-ale lacks is skipped whole, naming it, so importing again once
/// it is translated adds the view with all its children (`design/notes/report-statements-charts.md`, Q5).
#[test]
fn a_view_with_an_untranslated_child_is_skipped_whole() {
    let mut c = Charts::new("iso");
    let summary = format!("\"{ACCOUNT_SUMMARY}\" \"Account Summary\"");
    let text = userdata("saved-reports-2.8").replace(&summary, "\"4166a20981985fd2b07ff8cb3b7d384e\" \"Customer Summary\"");
    let result = c.call("report.importConfigurations", json!({ "text": text }));
    let last = result["skipped"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        (last["name"].as_str(), last["reason"].as_str()),
        (Some("Board pack"), Some("untranslated"))
    );
    let message = last["message"].as_str().unwrap();
    assert!(
        message.contains("4166a20981985fd2b07ff8cb3b7d384e")
            && message.contains("Customer Summary")
            && message.contains("embedded report 0"),
        "{message}"
    );
    let again = import_reports(&mut c, false);
    assert_eq!(again["imported"].as_array().unwrap().len(), 1);
    assert_eq!(again["imported"][0]["configuration"]["name"], "Board pack");
}

/// The last entry of an import of the fixture with `from` replaced by `to`: `Ok` with the skip when "Board pack" is
/// skipped, else `Err` with the last imported.
fn board_pack_with(c: &mut Charts, from: &str, to: &str) -> Result<Value, Value> {
    let text = userdata("saved-reports-2.8");
    assert!(text.contains(from), "{from}");
    let result = c.call(
        "report.importConfigurations",
        json!({ "text": text.replace(from, to), "dryRun": true }),
    );
    let last = |key: &str| result[key].as_array().unwrap().last().unwrap().clone();
    let (skipped, imported) = (last("skipped"), last("imported"));
    if skipped["name"] == "Board pack" {
        Ok(skipped)
    } else {
        Err(imported)
    }
}

/// An embedded report that is not as GnuCash writes one makes its entry `malformed`; one whose values red-ale refuses
/// makes it `invalid-option`, naming the child; an option its template lacks is ignored, naming the child.
#[test]
fn embedded_reports_red_ale_refuses_or_ignores() {
    let mut c = Charts::new("iso");
    let mut skip = |from: &str, to: &str| {
        let s = board_pack_with(&mut c, from, to).unwrap();
        (s["reason"].as_str().unwrap().to_owned(), s["message"].as_str().unwrap().to_owned())
    };
    let (reason, message) = skip("(13 1 2 #f) ", "");
    assert_eq!(reason, "malformed");
    assert!(message.contains("places 2 reports, and 3 embedded reports"), "{message}");
    let (reason, message) = skip("\"Summary\"", "(string-append \"Sum\" \"mary\")");
    assert_eq!(reason, "malformed");
    assert!(message.contains("not a literal value"), "{message}");

    let (reason, message) = skip("'1)", "'seven)");
    assert_eq!(reason, "invalid-option");
    assert!(
        message.starts_with("embedded report 0: option Accounts/Levels of Subaccounts"),
        "{message}"
    );
    let (reason, message) = skip("'3.0)", "'30.0)");
    assert_eq!(reason, "invalid-option");
    assert!(
        message.contains("child 2: Multicolumn View option General/Number of columns"),
        "{message}"
    );

    let imported = board_pack_with(&mut c, "\"Account Code\"", "\"Account Codes\"").unwrap_err();
    assert_eq!(
        imported["ignored"][0],
        json!({ "child": 0, "section": "Display", "name": "Account Codes", "reason": "unknown-option" })
    );
}
