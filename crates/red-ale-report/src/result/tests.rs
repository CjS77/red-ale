use red_ale_core::{Book, FixedEnv, Guid, Time64, TimeZone};
use serde_json::{Value, json};

use super::*;
use crate::number::Rational;
use crate::settings::DateFormat;

const BANK: &str = "4a1c0000000000000000000000000001";

/// A draft in the shape of `design/reports.md` §7, with one of each cell and a chart.
fn draft_json() -> Value {
    json!({
        "title": "Acme Ltd Balance Sheet 12/31/24",
        "sections": [{
            "title": null,
            "blocks": [
                { "kind": "text", "text": "Assets and liabilities", "role": "heading" },
                {
                    "kind": "table",
                    "columns": [ { "label": "Account title", "role": "label" }, { "label": "Balance", "role": "amount", "levels": 3 } ],
                    "rows": [
                        { "role": "heading", "cells": [ { "text": "Assets" }, null ] },
                        { "role": "account", "account": BANK, "depth": 1, "level": 1,
                          "cells": [ { "text": "Bank", "account": BANK, "link": { "account": BANK } },
                                     { "monetary": { "amount": { "num": "218600", "denom": "100" }, "commodity": "CURRENCY::USD" } } ] },
                        { "role": "data", "cells": [ { "text": "Share" }, { "number": "57.1428", "suffix": " %" } ] },
                        { "role": "data",
                          "cells": [ { "text": "Rate" }, { "price": { "num": "50005", "denom": "104" }, "currency": "USD" } ] },
                        { "role": "data", "cells": [ { "text": "Date" }, { "date": "2024-12-31T10:59:00Z" } ] },
                        { "role": "data", "cells": [ { "text": "GBP" }, { "commodities": [
                            { "monetary": { "amount": "608", "commodity": "GBP" },
                              "converted": { "amount": "912", "commodity": "USD" } } ] } ] },
                        { "role": "total", "level": 0,
                          "cells": [ { "text": "Total Assets" },
                                     { "monetary": { "amount": "116006.33", "commodity": "USD" }, "rule": true } ] },
                        { "role": "rule", "cells": [ { "text": "", "span": 2 } ] }
                    ]
                },
                {
                    "kind": "chart", "type": "bar", "title": ["Net Worth Barchart", "01/01/24 to 12/31/24"],
                    "yAxis": { "label": "USD", "format": "currency", "commodity": "CURRENCY::USD" },
                    "labels": ["01/01/24", "02/01/24"],
                    "series": [ { "label": "Assets", "color": "#0074D9", "values": [ "2500", null ],
                                  "fill": true, "link": { "report": { "template": BANK, "options": {} } } },
                                { "label": "Liabilities", "color": "#ff4136", "values": [ "0", "-12.5" ] } ]
                },
                { "kind": "group", "blocks": [ { "kind": "warning", "title": "No data", "text": "Nothing here" } ] }
            ]
        }]
    })
}

fn draft() -> ReportDraft {
    ReportDraft::from_json(&draft_json()).unwrap()
}

fn run() -> RunInfo {
    RunInfo {
        report: ReportIdentity {
            guid: crate::wire::Id(Guid::parse("c4173ac99b2b448289bf4d11c731af13").unwrap()),
            name: "Balance Sheet".into(),
            configuration: None,
        },
        now: Time64(1_736_933_400),
        options: serde_json::from_value(json!({ "General": { "Balance Sheet Date": { "relative": "end-accounting-period" } } })).unwrap(),
        log: vec![LogEntry {
            level: LogLevel::Print,
            text: "hello".into(),
            source: Some("standard/balance-sheet.rhai".into()),
            line: Some(3),
        }],
    }
}

/// A book whose GBP has the user symbol `#`, as GnuCash's account summary test sets it.
fn book() -> Book {
    let mut env = FixedEnv::new(Time64(0), 7);
    let mut book = Book::new_empty(&mut env);
    let mut gbp = red_ale_core::currencies::iso_currency("GBP").unwrap().to_commodity();
    gbp.slots.set_opt_str("user_symbol", Some("#"));
    book.commodities.insert(gbp.id.clone(), std::sync::Arc::new(gbp));
    book
}

fn finished() -> ReportResult {
    let book = book();
    let zone = TimeZone::utc();
    draft().finish(run(), &DisplayContext { book: &book, zone: &zone, date_format: DateFormat::Locale }).unwrap()
}

fn cell(result: &ReportResult, row: usize, col: usize) -> &Cell {
    let Block::Table(t) = &result.sections[0].blocks[1] else { panic!("not a table") };
    t.rows[row].cells[col].as_ref().unwrap()
}

#[test]
fn a_draft_reads_and_validates() {
    let d = draft();
    assert_eq!(d.sections[0].blocks.len(), 4);
    d.validate().unwrap();
}

#[test]
fn finishing_fills_every_display() {
    let r = finished();
    r.validate().unwrap();
    assert_eq!(r.version, RESULT_VERSION);
    assert_eq!(cell(&r, 1, 1).display.as_deref(), Some("$2,186.00"));
    assert_eq!(cell(&r, 1, 1).rounded, Some(Rational::from_int(2186)));
    assert_eq!(cell(&r, 2, 1).display.as_deref(), Some("57.14 %"));
    assert_eq!(cell(&r, 3, 1).display.as_deref(), Some("$480 + 85/104"));
    assert_eq!((cell(&r, 4, 1).local_date.as_deref(), cell(&r, 4, 1).display.as_deref()), (Some("2024-12-31"), Some("12/31/24")));
    let line = &cell(&r, 5, 1).commodities.as_ref().unwrap()[0];
    assert_eq!((line.display.as_deref(), line.converted_display.as_deref()), (Some("#608.00"), Some("$912.00")));
    assert_eq!(cell(&r, 6, 1).display.as_deref(), Some("$116,006.33"));
    let Block::Chart(chart) = &r.sections[0].blocks[2] else { panic!("not a chart") };
    assert_eq!(chart.series[0].displays, Some(vec![Some("$2,500.00".into()), None]));
    assert_eq!(chart.hover_label(1, None), "02/01/24\nLiabilities : -$12.50");
    assert_eq!(chart.hover_label(0, Some(0)), "01/01/24\nAssets : $2,500.00\nLiabilities : $0.00");
}

#[test]
fn json_round_trips_with_host_fields() {
    let r = finished();
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["now"], json!("2025-01-15T09:30:00Z"));
    assert_eq!(v["report"], json!({ "guid": "c4173ac99b2b448289bf4d11c731af13", "name": "Balance Sheet", "configuration": null }));
    assert_eq!(v["headline"], Value::Null);
    let money = &v["sections"][0]["blocks"][1]["rows"][1]["cells"][1];
    assert_eq!(money["monetary"], json!({ "amount": { "num": "2186", "denom": "1" }, "commodity": "CURRENCY::USD" }));
    assert_eq!(money["display"], json!("$2,186.00"));
    assert!(money.get("rule").is_none(), "false flags are left out");
    assert_eq!(v["sections"][0]["blocks"][2]["series"][0]["color"], json!("#0074d9"));
    let back = ReportResult::from_json(&v).unwrap();
    assert_eq!(back, r);
}

fn draft_error(edit: impl FnOnce(&mut Value)) -> ResultError {
    let mut v = draft_json();
    edit(&mut v);
    match ReportDraft::from_json(&v) {
        Ok(d) => d.validate().unwrap_err(),
        Err(e) => e,
    }
}

#[test]
fn mismatches_name_their_path() {
    let e = draft_error(|v| v["sections"][0]["blocks"][1]["rows"][1]["cells"][1]["monetary"]["amount"] = json!(1.5));
    assert_eq!(e.path, "sections[0].blocks[1].rows[1].cells[1].monetary.amount");
    let e = draft_error(|v| v["sections"][0]["blocks"][0]["colour"] = json!("red"));
    assert!(e.reason.contains("colour"), "{e:?}");
    let e = draft_error(|v| v["sections"][0]["blocks"][2]["series"][0]["color"] = json!("blue"));
    assert_eq!(e.path, "sections[0].blocks[2].series[0].color");
    let e = draft_error(|v| v["sections"][0]["blocks"][0]["kind"] = json!("picture"));
    assert_eq!(e.path, "sections[0].blocks[0].kind");
    let e = draft_error(|v| v["sections"][0]["blocks"][3]["blocks"][0]["text"] = json!(5));
    assert_eq!(e.path, "sections[0].blocks[3].blocks[0].text");
    let e = draft_error(|v| {
        v.as_object_mut().unwrap().remove("title");
    });
    assert!(e.reason.contains("title"), "{e:?}");
}

#[test]
fn validation_checks_what_the_schema_cannot() {
    // Each case edits block `n` of the draft at a path below it, and names the path the error must give.
    let cases: [(usize, &str, Value, &str); 16] = [
        (1, "/rows/0/cells", json!([null]), "rows[0].cells"),
        (1, "/rows/1/level", json!(3), "rows[1].level"),
        (1, "/rows/0/cells/1", json!({ "text": "a", "number": "1" }), "rows[0].cells[1]"),
        (1, "/rows/0/cells/1", json!({}), "rows[0].cells[1]"),
        (1, "/rows/1/cells/1/display", json!("$1"), "rows[1].cells[1].display"),
        (1, "/rows/3/cells/1/currency", Value::Null, "rows[3].cells[1].currency"),
        (1, "/rows/0/cells/0/places", json!(2), "rows[0].cells[0]"),
        (1, "/rows/2/cells/1/places", json!(19), "rows[2].cells[1].places"),
        (1, "/columns/0/levels", json!(2), "columns[0].levels"),
        (2, "/series/0/values", json!(["1"]), "series[0].values"),
        (2, "/series/0/colors", json!(["#000000"]), "series[0].colors"),
        (2, "/yAxis/commodity", Value::Null, "yAxis.commodity"),
        (2, "/type", json!("scatter"), "series[0].values"),
        (2, "/type", json!("pie"), "series"),
        (2, "/width", json!({ "percent": 150 }), "width"),
        (3, "/blocks", json!([{ "kind": "optionsSummary", "rows": [] }]), "blocks[0].kind"),
    ];
    cases.into_iter().for_each(|(block, at, value, path)| {
        let (parent, last) = at.rsplit_once('/').unwrap();
        let pointer = format!("/sections/0/blocks/{block}{parent}");
        let e = draft_error(|v| {
            let parent = v.pointer_mut(&pointer).unwrap();
            match last.parse::<usize>() {
                Ok(i) => parent[i] = value,
                Err(_) => parent[last] = value,
            }
        });
        assert_eq!(e.path, format!("sections[0].blocks[{block}].{path}"));
    });
}

#[test]
fn scatter_charts_have_points() {
    let mut v = draft_json();
    v["sections"][0]["blocks"][2] = json!({
        "kind": "chart", "type": "scatter", "xAxis": { "scale": "linear" },
        "series": [ { "label": "FUNDS", "color": "#123456", "line": "#abcdef", "marker": "diamond",
                      "points": [ { "x": "0", "y": "480.5", "label": "01/01/24" }, { "x": "1", "y": "1/3" } ] } ]
    });
    let d = ReportDraft::from_json(&v).unwrap();
    d.validate().unwrap();
    let book = book();
    let zone = TimeZone::utc();
    let r = d.finish(run(), &DisplayContext { book: &book, zone: &zone, date_format: DateFormat::Iso }).unwrap();
    let Block::Chart(chart) = &r.sections[0].blocks[2] else { panic!("not a chart") };
    assert_eq!(chart.series[0].points.as_ref().unwrap()[1].display.as_deref(), Some("1/3"));
    assert_eq!(chart.hover_label(0, Some(0)), "01/01/24\nFUNDS : 480.5");
    assert_eq!(chart.hover_label(1, Some(0)), "1\nFUNDS : 1/3");
}

#[test]
fn unknown_commodities_are_result_errors() {
    let mut v = draft_json();
    v["sections"][0]["blocks"][1]["rows"][1]["cells"][1]["monetary"]["commodity"] = json!("NASDAQ::NONE");
    let book = book();
    let zone = TimeZone::utc();
    let ctx = DisplayContext { book: &book, zone: &zone, date_format: DateFormat::Iso };
    let e = ReportDraft::from_json(&v).unwrap().finish(run(), &ctx).unwrap_err();
    assert_eq!(e.path, "sections[0].blocks[1].rows[1].cells[1].monetary.commodity");
    assert!(e.reason.contains("NASDAQ::NONE"));
}

#[test]
fn finished_results_need_their_host_fields() {
    let mut v = serde_json::to_value(finished()).unwrap();
    v["sections"][0]["blocks"][1]["rows"][1]["cells"][1].as_object_mut().unwrap().remove("display");
    let e = ReportResult::from_json(&v).unwrap().validate().unwrap_err();
    assert_eq!(e.path, "sections[0].blocks[1].rows[1].cells[1].display");
    let mut wrong = finished();
    wrong.version = "9.0.0".into();
    assert_eq!(wrong.validate().unwrap_err().path, "version");
}
