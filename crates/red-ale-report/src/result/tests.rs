use red_ale_core::{Book, FixedEnv, Guid, Time64, TimeZone};
use red_ale_locale::Locale;
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
    draft()
        .finish(
            run(),
            &DisplayContext {
                book: &book,
                zone: &zone,
                date_format: DateFormat::Locale,
                locale: Locale::c(),
            },
        )
        .unwrap()
}

fn cell(result: &ReportResult, row: usize, col: usize) -> &Cell {
    let Block::Table(t) = &result.sections[0].blocks[1] else {
        panic!("not a table")
    };
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
    assert_eq!(
        (cell(&r, 4, 1).local_date.as_deref(), cell(&r, 4, 1).display.as_deref()),
        (Some("2024-12-31"), Some("12/31/24"))
    );
    let line = &cell(&r, 5, 1).commodities.as_ref().unwrap()[0];
    assert_eq!(
        (line.display.as_deref(), line.converted_display.as_deref()),
        (Some("#608.00"), Some("$912.00"))
    );
    assert_eq!(cell(&r, 6, 1).display.as_deref(), Some("$116,006.33"));
    let Block::Chart(chart) = &r.sections[0].blocks[2] else {
        panic!("not a chart")
    };
    assert_eq!(chart.series[0].displays, Some(vec![Some("$2,500.00".into()), None]));
    assert_eq!(chart.hover_label(1, None), "02/01/24\nLiabilities : -$12.50");
    assert_eq!(chart.hover_label(0, Some(0)), "01/01/24\nAssets : $2,500.00\nLiabilities : $0.00");
}

#[test]
fn json_round_trips_with_host_fields() {
    let r = finished();
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["now"], json!("2025-01-15T09:30:00Z"));
    assert_eq!(
        v["report"],
        json!({ "guid": "c4173ac99b2b448289bf4d11c731af13", "name": "Balance Sheet", "configuration": null })
    );
    assert_eq!(v["headline"], Value::Null);
    let money = &v["sections"][0]["blocks"][1]["rows"][1]["cells"][1];
    assert_eq!(
        money["monetary"],
        json!({ "amount": { "num": "2186", "denom": "1" }, "commodity": "CURRENCY::USD" })
    );
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
    let cases: [(usize, &str, Value, &str); 17] = [
        (1, "/rows/0/cells", json!([null]), "rows[0].cells"),
        (1, "/rows/1/level", json!(3), "rows[1].level"),
        (1, "/rows/0/cells/1", json!({ "text": "a", "number": "1" }), "rows[0].cells[1]"),
        (1, "/rows/0/cells/1", json!({}), "rows[0].cells[1]"),
        (1, "/rows/1/cells/1/display", json!("$1"), "rows[1].cells[1].display"),
        (1, "/rows/3/cells/1/currency", Value::Null, "rows[3].cells[1].currency"),
        (1, "/rows/0/cells/0/places", json!(2), "rows[0].cells[0]"),
        (1, "/rows/2/cells/1/places", json!(19), "rows[2].cells[1].places"),
        (1, "/columns/0/levels", json!(2), "columns[0].levels"),
        (1, "/rows/2/cells/1/footnote", json!(1), "rows[2].cells[1].footnote"),
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

/// A footnote mark names one of the table's footnotes, numbered from 1.
#[test]
fn footnotes_are_numbered_from_one() {
    let mut v = draft_json();
    v["sections"][0]["blocks"][1]["footnotes"] = json!(["a note"]);
    v["sections"][0]["blocks"][1]["rows"][1]["cells"][1]["footnote"] = json!(1);
    assert!(ReportDraft::from_json(&v).unwrap().validate().is_ok());
    v["sections"][0]["blocks"][1]["rows"][1]["cells"][1]["footnote"] = json!(0);
    let e = ReportDraft::from_json(&v).unwrap().validate().unwrap_err();
    assert_eq!(e.path, "sections[0].blocks[1].rows[1].cells[1].footnote");
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
    let r = d
        .finish(
            run(),
            &DisplayContext {
                book: &book,
                zone: &zone,
                date_format: DateFormat::Iso,
                locale: Locale::c(),
            },
        )
        .unwrap();
    let Block::Chart(chart) = &r.sections[0].blocks[2] else {
        panic!("not a chart")
    };
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
    let ctx = DisplayContext {
        book: &book,
        zone: &zone,
        date_format: DateFormat::Iso,
        locale: Locale::c(),
    };
    let e = ReportDraft::from_json(&v).unwrap().finish(run(), &ctx).unwrap_err();
    assert_eq!(e.path, "sections[0].blocks[1].rows[1].cells[1].monetary.commodity");
    assert!(e.reason.contains("NASDAQ::NONE"));
}

#[test]
fn finished_results_need_their_host_fields() {
    let mut v = serde_json::to_value(finished()).unwrap();
    v["sections"][0]["blocks"][1]["rows"][1]["cells"][1]
        .as_object_mut()
        .unwrap()
        .remove("display");
    let e = ReportResult::from_json(&v).unwrap().validate().unwrap_err();
    assert_eq!(e.path, "sections[0].blocks[1].rows[1].cells[1].display");
    let mut wrong = finished();
    wrong.version = "9.0.0".into();
    assert_eq!(wrong.validate().unwrap_err().path, "version");
}

const SPLIT: &str = "5b1c0000000000000000000000000001";

/// A draft with each addition of `redAleApi` 0.5.0: the new link kinds, an `anchors` cell, a date period, a column
/// with `labelLines`, a text block's link and a list block.
fn additions_json() -> Value {
    json!({
        "title": "Additions",
        "sections": [{
            "blocks": [
                { "kind": "text", "text": "Barchart", "link": { "report": { "template": BANK, "options": {} } } },
                { "kind": "list", "items": [ { "text": "Bank", "account": BANK }, { "text": "Cash" } ] },
                {
                    "kind": "table",
                    "columns": [ { "label": "Date", "role": "date" },
                                 { "label": "Assets:Bank", "labelLines": ["Assets", "Bank"], "role": "amount" },
                                 { "label": "Invoice", "role": "text" } ],
                    "rows": [
                        { "cells": [ { "date": "2024-01-01T00:00:00Z", "until": "2024-12-31T23:59:59Z" },
                                     { "monetary": { "amount": "5", "commodity": "USD" }, "link": { "split": SPLIT } },
                                     { "anchors": [ { "text": "000002", "link": { "invoice": SPLIT } },
                                                    { "text": "000001", "link": { "invoice": BANK } } ] } ] },
                        { "cells": [ { "text": "L", "link": { "docLink": "file:///receipts/1.pdf" } },
                                     { "price": "2", "currency": "USD", "link": { "price": SPLIT } },
                                     { "text": "t", "link": { "transaction": SPLIT } } ] }
                    ]
                }
            ]
        }]
    })
}

fn finish_additions(v: &Value) -> Result<ReportResult, ResultError> {
    let book = book();
    let zone = TimeZone::utc();
    let ctx = DisplayContext {
        book: &book,
        zone: &zone,
        date_format: DateFormat::Locale,
        locale: Locale::c(),
    };
    ReportDraft::from_json(v)?.finish(run(), &ctx)
}

/// A date with `until` shows `<from> to <until>`, each as `qof-print-date` prints it
/// (`5.17:gnucash/report/reports/standard/balsheet-pnl.scm:1140-1153`); the other additions round-trip.
#[test]
fn additions_finish_and_round_trip() {
    let r = finish_additions(&additions_json()).unwrap();
    r.validate().unwrap();
    let Block::Table(t) = &r.sections[0].blocks[2] else {
        panic!("not a table")
    };
    let period = t.rows[0].cells[0].as_ref().unwrap();
    assert_eq!(
        (
            period.local_date.as_deref(),
            period.local_until.as_deref(),
            period.display.as_deref()
        ),
        (Some("2024-01-01"), Some("2024-12-31"), Some("01/01/24 to 12/31/24"))
    );
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(
        v["sections"][0]["blocks"][2]["rows"][1]["cells"][0]["link"],
        json!({ "docLink": "file:///receipts/1.pdf" })
    );
    assert_eq!(v["sections"][0]["blocks"][2]["columns"][1]["labelLines"], json!(["Assets", "Bank"]));
    assert_eq!(ReportResult::from_json(&v).unwrap(), r);
}

#[test]
fn additions_are_validated() {
    // Each case edits the additions draft at a path below block `n`, and names the path the error must give.
    let cases: [(usize, &str, Value, &str); 10] = [
        (0, "/link", json!({ "docLink": "" }), "link.docLink"),
        (2, "/columns/1/labelLines", json!([]), "columns[1].labelLines"),
        (2, "/rows/0/cells/0/until", json!("2023-12-31T00:00:00Z"), "rows[0].cells[0].until"),
        (2, "/rows/0/cells/1/until", json!("2025-01-01T00:00:00Z"), "rows[0].cells[1].until"),
        (2, "/rows/0/cells/0/localUntil", json!("2024-12-31"), "rows[0].cells[0].localUntil"),
        (2, "/rows/0/cells/2/anchors", json!([]), "rows[0].cells[2].anchors"),
        (2, "/rows/0/cells/2/link", json!({ "invoice": BANK }), "rows[0].cells[2]"),
        (2, "/rows/0/cells/2/display", json!("x"), "rows[0].cells[2].display"),
        (
            2,
            "/rows/0/cells/2/anchors/0/link",
            json!({ "docLink": "" }),
            "rows[0].cells[2].anchors[0].link.docLink",
        ),
        (2, "/rows/1/cells/0/link", json!({ "docLink": "" }), "rows[1].cells[0].link.docLink"),
    ];
    cases.into_iter().for_each(|(block, at, value, path)| {
        let (parent, last) = at.rsplit_once('/').unwrap();
        let mut v = additions_json();
        let target = v.pointer_mut(&format!("/sections/0/blocks/{block}{parent}")).unwrap();
        match last.parse::<usize>() {
            Ok(i) => target[i] = value,
            Err(_) => target[last] = value,
        }
        let e = ReportDraft::from_json(&v).unwrap().validate().unwrap_err();
        assert_eq!(e.path, format!("sections[0].blocks[{block}].{path}"), "{at}");
    });
    let e = finish_additions(&json!({ "title": "t", "sections": [{ "blocks": [{ "kind": "list", "items": [{ "txt": "a" }] }] }] }));
    assert_eq!(e.unwrap_err().path, "sections[0].blocks[0].items[0].txt");
}

/// A draft with the second set of `redAleApi` 0.5.0 additions: a row's `alternate`, a `warning` row, number cells
/// with the share display and half-up rounding, a table with no columns, and `exportError`.
fn additions_b_json() -> Value {
    json!({
        "title": "Additions B",
        "exportError": "No splits found",
        "sections": [{
            "blocks": [
                {
                    "kind": "table",
                    "columns": [ { "label": "Units", "role": "amount" }, { "label": "Rate", "role": "amount" } ],
                    "rows": [
                        { "alternate": true, "cells": [ { "number": "1234.5675", "places": 3, "numberFormat": "share" },
                                                        { "number": "3.625", "rounding": "halfUp", "suffix": "%" } ] },
                        { "role": "account", "alternate": false,
                          "cells": [ { "number": "-100", "places": 0, "numberFormat": "share" },
                                     { "number": "-3.625", "rounding": "halfUp", "suffix": "%" } ] },
                        { "cells": [ { "number": "0.125", "numberFormat": "share" }, { "number": "3.625", "rounding": "bankers" } ] },
                        { "role": "warning", "cells": [ { "text": "Lot has no purchase", "span": 2 } ] }
                    ]
                },
                { "kind": "table", "columns": [], "rows": [ { "cells": [] }, { "role": "rule", "cells": [] } ] }
            ]
        }]
    })
}

/// The share display is `gnc_share_print_info_places` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1013-1023`): exactly
/// `places` decimals, half away from zero, grouped, the point always printed; `halfUp` rounds a rate's tie away from
/// zero where banker's rounding goes to even.
#[test]
fn additions_b_finish_and_round_trip() {
    let r = finish_additions(&additions_b_json()).unwrap();
    r.validate().unwrap();
    let Block::Table(t) = &r.sections[0].blocks[0] else {
        panic!("not a table")
    };
    let shown: Vec<Option<&str>> = t.rows[..3]
        .iter()
        .flat_map(|row| row.cells.iter().map(|c| c.as_ref().unwrap().display.as_deref()))
        .collect();
    assert_eq!(
        shown,
        [
            Some("1,234.568"),
            Some("3.63%"),
            Some("-100."),
            Some("-3.63%"),
            Some("0.13"),
            Some("3.62")
        ]
    );
    assert_eq!(r.export_error.as_deref(), Some("No splits found"));
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["exportError"], "No splits found");
    assert_eq!(v["sections"][0]["blocks"][0]["rows"][1]["alternate"], false);
    assert!(v["sections"][0]["blocks"][0]["rows"][2].get("alternate").is_none());
    assert_eq!(v["sections"][0]["blocks"][0]["rows"][3]["role"], "warning");
    assert_eq!(ReportResult::from_json(&v).unwrap(), r);
}

#[test]
fn additions_b_are_validated() {
    // Each case edits the draft at a path below the first block, and names the path the error must give.
    let cases: [(&str, Value, &str); 4] = [
        ("/rows/3/alternate", json!(true), "rows[3].alternate"),
        ("/rows/3/cells/0/rounding", json!("halfUp"), "rows[3].cells[0]"),
        ("/rows/3/cells/0/numberFormat", json!("share"), "rows[3].cells[0]"),
        ("/rows/0/cells/0/rounding", json!("halfUp"), "rows[0].cells[0].rounding"),
    ];
    cases.into_iter().for_each(|(at, value, path)| {
        let (parent, last) = at.rsplit_once('/').unwrap();
        let mut v = additions_b_json();
        v.pointer_mut(&format!("/sections/0/blocks/0{parent}")).unwrap()[last] = value;
        let e = ReportDraft::from_json(&v).unwrap().validate().unwrap_err();
        assert_eq!(e.path, format!("sections[0].blocks[0].{path}"), "{at}");
    });
    let mut v = additions_b_json();
    v["export"] = json!([["a"]]);
    assert_eq!(ReportDraft::from_json(&v).unwrap().validate().unwrap_err().path, "exportError");
    let mut v = additions_b_json();
    v["sections"][0]["blocks"][0]["rows"][0]["cells"][0]["numberFormat"] = json!("plain");
    assert!(ReportDraft::from_json(&v).is_err(), "share and exact are the number formats");
}

#[allow(clippy::needless_pass_by_value)]
fn grid_json(cell: Value) -> Value {
    json!({ "title": "", "sections": [{ "blocks": [{ "kind": "grid", "width": "2", "rows": [{ "cells": [cell] }] }] }] })
}

/// A script lays out a grid's cells; their link, result and warning are the host's, and a finished cell holds a
/// result or a warning, not both, with spans of at most 100.
#[test]
fn grid_cells_leave_their_children_to_the_host() {
    let cell = json!({ "rowspan": 1, "colspan": 0, "child": 0 });
    ReportDraft::from_json(&grid_json(cell.clone())).unwrap().validate().unwrap();
    let at = |edit: &dyn Fn(&mut Value)| {
        let mut c = cell.clone();
        edit(&mut c);
        ReportDraft::from_json(&grid_json(c)).unwrap().validate().unwrap_err().path
    };
    let prefix = "sections[0].blocks[0].rows[0].cells[0]";
    assert_eq!(at(&|c| c["rowspan"] = json!(101)), format!("{prefix}.rowspan"));
    assert_eq!(
        at(&|c| c["warning"] = json!({ "title": "t", "text": "x" })),
        format!("{prefix}.warning")
    );
    assert_eq!(at(&|c| c["link"] = json!({ "account": BANK })), format!("{prefix}.link"));

    let finished = |c: Value| {
        let mut v = grid_json(c);
        let fields = [
            ("version", json!(RESULT_VERSION)),
            ("now", json!("2024-12-31T10:59:00Z")),
            ("options", json!({})),
            ("log", json!([])),
        ];
        fields.into_iter().for_each(|(k, f)| v[k] = f);
        v["report"] = json!({ "guid": BANK, "name": "View", "configuration": null });
        v["headline"] = Value::Null;
        ReportResult::from_json(&v).unwrap().validate()
    };
    assert_eq!(finished(cell.clone()).unwrap_err().path, prefix, "the host fills every cell");
    let child = json!({ "report": { "guid": BANK, "name": "Leaf", "configuration": null }, "title": "Leaf", "headline": null,
                        "options": {}, "log": [], "sections": [{ "blocks": [{ "kind": "optionsSummary", "rows": [] }] }] });
    let mut both = cell.clone();
    both["result"] = child.clone();
    both["warning"] = json!({ "title": "Report error", "text": "x" });
    assert_eq!(finished(both).unwrap_err().path, prefix);
    let mut one = cell;
    one["result"] = child;
    one["link"] = json!({ "report": { "template": BANK, "configuration": BANK, "options": {} } });
    finished(one.clone()).unwrap();
    one["result"]["sections"][0]["blocks"][0] = json!({ "kind": "text", "text": "x", "link": { "docLink": "" } });
    assert_eq!(
        finished(one).unwrap_err().path,
        format!("{prefix}.result.sections[0].blocks[0].link.docLink")
    );
}

/// A draft with the `redAleApi` 0.6.0 additions the Printable Invoice needs: the exact number display, class names on
/// blocks and cells, the report's CSS and an image block.
fn additions_c_json() -> Value {
    json!({
        "title": "",
        "css": ".entries-table > table { width: 100% }\n",
        "sections": [{
            "blocks": [
                { "kind": "text", "text": "Invoice #0001", "role": "heading", "class": "invoice-title" },
                { "kind": "image", "src": "file:///logo.png", "class": "picture" },
                { "kind": "group", "class": "main-table", "blocks": [ { "kind": "warning", "title": "t", "text": "x", "class": "w" } ] },
                {
                    "kind": "table", "class": "entries-table",
                    "columns": [ { "label": "Quantity", "role": "amount" }, { "label": "Discount", "role": "amount" } ],
                    "rows": [
                        { "cells": [ { "number": "1/8", "numberFormat": "exact", "class": "qty" },
                                     { "number": "1/3", "numberFormat": "exact", "suffix": " %" } ] },
                        { "cells": [ { "number": "2", "numberFormat": "exact" },
                                     { "number": "-1234.5", "numberFormat": "exact", "class": "a b-c d_1" } ] }
                    ]
                }
            ]
        }]
    })
}

/// The exact display is `xaccPrintAmount` with `gnc_default_print_info(FALSE)`
/// (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:833-860`, `:1083-1180`): at least `frac_digits` decimals, no maximum,
/// no rounding, a remainder that is not decimal as `w + n/d`.
#[test]
fn additions_c_finish_and_round_trip() {
    let r = finish_additions(&additions_c_json()).unwrap();
    r.validate().unwrap();
    let Block::Table(t) = &r.sections[0].blocks[3] else {
        panic!("not a table")
    };
    let shown: Vec<Option<&str>> = t
        .rows
        .iter()
        .flat_map(|row| row.cells.iter().map(|c| c.as_ref().unwrap().display.as_deref()))
        .collect();
    assert_eq!(shown, [Some("0.125"), Some("1/3 %"), Some("2.00"), Some("-1,234.50")]);
    assert_eq!(r.css.as_deref(), Some(".entries-table > table { width: 100% }\n"));
    assert_eq!(r.sections[0].blocks[0].class(), Some("invoice-title"));
    assert_eq!(r.log.len(), 1, "no entry for CSS that passes");
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(
        v["sections"][0]["blocks"][1],
        json!({ "kind": "image", "src": "file:///logo.png", "class": "picture" })
    );
    assert_eq!(ReportResult::from_json(&v).unwrap(), r);
    let plain = serde_json::to_value(finished()).unwrap();
    assert!(plain.get("css").is_none(), "absent unless a report sets it");
}

#[test]
fn additions_c_are_validated() {
    let cases: [(&str, Value, &str); 7] = [
        ("/0/class", json!("invoice  title"), "sections[0].blocks[0].class"),
        ("/0/class", json!(""), "sections[0].blocks[0].class"),
        ("/1/class", json!("1st"), "sections[0].blocks[1].class"),
        ("/2/blocks/0/class", json!("a\"b"), "sections[0].blocks[2].blocks[0].class"),
        ("/1/src", json!("https://example.com/logo.png"), "sections[0].blocks[1].src"),
        (
            "/3/rows/0/cells/0/places",
            json!(2),
            "sections[0].blocks[3].rows[0].cells[0].places",
        ),
        (
            "/3/rows/0/cells/1/class",
            json!("x y "),
            "sections[0].blocks[3].rows[0].cells[1].class",
        ),
    ];
    cases.into_iter().for_each(|(at, value, path)| {
        let (parent, last) = at.rsplit_once('/').unwrap();
        let mut v = additions_c_json();
        v.pointer_mut(&format!("/sections/0/blocks{parent}")).unwrap()[last] = value;
        let e = ReportDraft::from_json(&v).unwrap().validate().unwrap_err();
        assert_eq!(e.path, path, "{at}");
    });
    let mut v = additions_c_json();
    v["sections"][0]["blocks"][3]["rows"][0]["cells"][0]["rounding"] = json!("halfUp");
    let e = ReportDraft::from_json(&v).unwrap().validate().unwrap_err();
    assert_eq!(e.path, "sections[0].blocks[3].rows[0].cells[0].rounding");
}

/// CSS the CSS template refuses (`design/reports.md` §8) is left out of the result with a `debug` log entry, as the
/// note's open question 3 settles; a finished result carrying it is invalid.
#[test]
fn refused_css_is_left_out_with_a_log_entry() {
    let mut v = additions_c_json();
    v["css"] = json!(".picture { background: URL(https://example.com/x.png) }");
    let r = finish_additions(&v).unwrap();
    assert_eq!(r.css, None);
    let entry = r.log.last().unwrap();
    assert_eq!(entry.level, LogLevel::Debug);
    assert_eq!(entry.text, "the report's CSS was left out: CSS may not contain \"url(\"");
    let mut bad = r;
    bad.css = Some("</style><script>".into());
    assert_eq!(bad.validate().unwrap_err().path, "css");
}
