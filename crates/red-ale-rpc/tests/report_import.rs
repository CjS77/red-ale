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
/// read as data and its GUID kept, though this book has no such budget. The untranslated Transaction Report is
/// skipped without its options being read.
#[test]
fn imports_saved_configurations() {
    let mut c = Charts::new("iso");
    let result = import_reports(&mut c, false);
    assert_eq!(result["dryRun"], false);
    assert_eq!(
        skipped(&result),
        pairs(&[("Transactions this month", "untranslated")])
    );
    let imported = result["imported"].as_array().unwrap();
    assert_eq!(imported.len(), 3);

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

    let budget = &imported[1];
    assert_eq!(budget["configuration"]["name"], "Household budget");
    assert_eq!(budget["options"], json!({ "General": { "Budget": "b0000000000000000000000000000001" } }));

    let sheet = &imported[2];
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
    assert_eq!(saved.as_array().unwrap().len(), 3);
    assert_eq!(saved[0]["guid"], NET_WORTH_SAVED);
    assert_eq!(saved[2]["options"], sheet["options"]);
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
        (Some(true), 3)
    );
    assert_eq!(config(&c), before);
    import_reports(&mut c, false);
    let again = import_reports(&mut c, false);
    assert_eq!(again["imported"], json!([]));
    let reasons: Vec<String> = skipped(&again).into_iter().map(|s| s.1).collect();
    assert_eq!(reasons, ["exists", "untranslated", "exists", "exists"]);
}

#[test]
fn name_conflicts_are_skipped() {
    let mut c = Charts::new("iso");
    c.call(
        "report.saveConfiguration",
        json!({ "name": "Year-end balance sheet", "report": "Balance Sheet" }),
    );
    let result = import_reports(&mut c, false);
    assert_eq!(skipped(&result)[1], ("Year-end balance sheet".to_owned(), "conflict".to_owned()));
}

/// The imported configuration runs as the template with the same options set by hand.
#[test]
fn imported_configurations_run_as_set_by_hand() {
    let mut c = Charts::open("charts-daily.gnucash", "iso");
    let imported = import_reports(&mut c, false);
    [(NET_WORTH_SAVED, NET_WORTH_LINECHART, 0), (BALANCE_SHEET_SAVED, BALANCE_SHEET, 2)]
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
    assert_eq!(result["imported"].as_array().unwrap().len(), 2, "the other entries still import");
    assert!(!c.path().join("pwned").exists());

    let truncated = &text[..text.len() / 2];
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
