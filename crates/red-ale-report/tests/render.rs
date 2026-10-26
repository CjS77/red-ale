//! The HTML renderer (`design/reports.md` §8): golden pages for each kind of table, each chart and each predefined
//! style sheet, the same bytes on every render, no script and nothing external, escaping of hostile text, and SVG
//! that stays inside its viewBox.
//!
//! Regenerate the golden pages with `UPDATE_SNAPSHOTS=1 cargo test -p red-ale-report --test render` and review the
//! diff.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use std::path::Path;

use proptest::prelude::*;
use red_ale_core::TimeZone;
use red_ale_report::render::{self, RenderContext, StyleSheet, StyleSheetTemplate, StyleSheets};
use red_ale_report::settings::DateFormat;
use red_ale_report::ReportResult;
use serde_json::{Value, json};

const BANK: &str = "4a1c0000000000000000000000000001";
const SAVINGS: &str = "4a1c0000000000000000000000000002";

fn result(title: &str, sections: Value) -> ReportResult {
    ReportResult::from_json(&json!({
        "version": "0.2.0",
        "report": { "guid": "c4173ac99b2b448289bf4d11c731af13", "name": "Balance Sheet", "configuration": null },
        "title": title,
        "headline": null,
        "now": "2025-01-15T09:30:00Z",
        "options": {},
        "sections": sections,
        "log": []
    }))
    .unwrap()
}

fn usd(amount: &str, display: &str) -> Value {
    json!({ "monetary": { "amount": amount, "commodity": "CURRENCY::USD" }, "rounded": amount, "display": display })
}

/// A two-column balance sheet: a staircase account table, a multi-commodity cell, totals with rules, an exchange
/// rate, a date, a percentage, a warning and a note.
fn balance_sheet() -> ReportResult {
    let assets = json!({
        "kind": "table",
        "title": "Assets",
        "columns": [ { "label": "Account", "role": "label" }, { "label": "Balance", "role": "amount", "levels": 3 } ],
        "rows": [
            { "role": "heading", "cells": [ { "text": "Assets" }, null ] },
            { "role": "account", "account": BANK, "depth": 1, "level": 1,
              "cells": [ { "text": "Bank", "account": BANK, "link": { "account": BANK } }, usd("2186", "$2,186.00") ] },
            { "role": "account", "account": SAVINGS, "depth": 2, "level": 2,
              "cells": [ { "text": "Savings", "account": SAVINGS }, usd("-250", "-$250.00") ] },
            { "role": "data", "depth": 1, "cells": [ { "text": "Foreign" }, { "commodities": [
                { "monetary": { "amount": "608", "commodity": "CURRENCY::GBP" }, "display": "#608.00",
                  "converted": { "amount": "912", "commodity": "CURRENCY::USD" }, "convertedDisplay": "$912.00" },
                { "monetary": { "amount": "-30", "commodity": "NASDAQ::FUNDS" }, "display": "-30. FUNDS" } ] } ] },
            { "role": "subtotal", "level": 1, "cells": [ { "text": "Total Bank" }, usd("1936", "$1,936.00") ] },
            { "role": "spacer", "cells": [ null, null ] },
            { "role": "total", "level": 0, "cells": [ { "text": "Total Assets" },
                { "monetary": { "amount": "116006.33", "commodity": "CURRENCY::USD" }, "rounded": "116006.33",
                  "display": "$116,006.33", "rule": true } ] },
            { "role": "rule", "cells": [ { "text": "", "span": 2 } ] },
            { "role": "grandTotal", "level": 0, "cells": [ { "text": "Net Worth" }, usd("115000", "$115,000.00") ] }
        ]
    });
    let rates = json!({
        "kind": "table",
        "columns": [ { "label": "", "role": "label" }, { "label": "Rate", "role": "amount" },
                     { "label": "Share", "role": "amount" }, { "label": "As of", "role": "date", "align": "center" } ],
        "rows": [
            { "cells": [ { "text": "1 GBP" }, { "price": "49925/104", "currency": "CURRENCY::USD", "display": "$480 + 5/104" },
                         { "number": "57.1428", "suffix": " %", "display": "57.14 %" },
                         { "date": "2024-12-31T10:59:00Z", "localDate": "2024-12-31", "display": "12/31/24" } ] },
            { "cells": [ { "text": "Spanning note", "span": 3 }, null ] }
        ]
    });
    result("Acme Ltd Balance Sheet 12/31/24", json!([
        { "title": null, "blocks": [
            { "kind": "text", "text": "Assets and liabilities", "role": "heading" },
            { "kind": "group", "blocks": [ assets, rates ] },
            { "kind": "warning", "title": "No accounts selected", "text": "This report requires accounts to be selected." },
            { "kind": "text", "text": "Figures in USD.\nRates are nearest in time.", "role": "note" }
        ] },
        { "title": "Options", "blocks": [ { "kind": "optionsSummary", "rows": [
            { "section": "General", "name": "Balance Sheet Date", "value": "relative . end-accounting-period" } ] } ] }
    ]))
}

fn chart(chart: &Value) -> ReportResult {
    result("Chart", json!([ { "title": null, "blocks": [ chart ] } ]))
}

/// Two bar series and a line: side by side, or with `stacked` the bars in one column, positives up and negatives
/// down.
fn net_worth_bar(stacked: bool) -> Value {
    json!({
        "kind": "chart", "type": "bar", "title": ["Net Worth Barchart", "01/01/24 to 03/31/24"], "stacked": stacked,
        "yAxis": { "label": "USD", "format": "currency", "commodity": "CURRENCY::USD" },
        "labels": ["01/01/24", "02/01/24", "03/01/24"],
        "series": [
            { "label": "Assets", "color": "#0074D9", "values": ["2500", "2750.5", null], "displays": ["$2,500.00", "$2,750.50", null] },
            { "label": "Liabilities", "color": "#FF4136", "values": ["-400", "-12.5", "-300"],
              "displays": ["-$400.00", "-$12.50", "-$300.00"] },
            { "label": "Net Worth", "color": "#2ECC40", "values": ["2100", "2738", "-300"], "kind": "line", "marker": "diamond",
              "displays": ["$2,100.00", "$2,738.00", "-$300.00"] }
        ]
    })
}

fn income_line() -> Value {
    json!({
        "kind": "chart", "type": "line", "title": ["Income & Expense Linechart"], "stacked": true,
        "width": { "pixels": 640 }, "height": { "pixels": 360 },
        "xAxis": { "label": "Month" }, "yAxis": { "label": "Percent", "format": "percent" },
        "labels": ["Jan", "Feb", "Mar", "Apr"],
        "series": [
            { "label": "Income", "color": "#0074d9", "values": ["10", "20.25", null, "5"], "fill": true, "lineWidth": "1.5",
              "displays": ["10.00%", "20.25%", null, "5.00%"] },
            { "label": "Expense", "color": "#ff4136", "values": ["5", "0", "7", "2"], "marker": "square",
              "displays": ["5.00%", "0.00%", "7.00%", "2.00%"] }
        ],
        "tooltip": { "grouped": true, "nonZeroOnly": true }
    })
}

fn expense_pie() -> Value {
    json!({
        "kind": "chart", "type": "pie", "title": ["Expense Accounts", "01/01/24 to 12/31/24"], "height": { "percent": 50 },
        "labels": ["Groceries", "Rent", "Utilities", "Other"],
        "series": [ { "label": "Accounts", "color": "#0074d9", "values": ["300", "1200", "0", "150.75"],
                      "colors": ["#ff4136", "#2ecc40", "#ffdc00", "#b10dc9"],
                      "displays": ["$300.00", "$1,200.00", "$0.00", "$150.75"] } ]
    })
}

fn price_scatter() -> Value {
    json!({
        "kind": "chart", "type": "scatter", "title": ["Price Scatterplot"], "legend": false,
        "xAxis": { "label": "Days", "scale": "linear" }, "yAxis": { "label": "USD", "format": "currency", "commodity": "USD" },
        "series": [ { "label": "Price", "color": "#0074d9", "line": "#ff851b", "marker": "cross", "points": [
            { "x": "0", "y": "100", "label": "01/01/24", "display": "$100.00" },
            { "x": "31", "y": "105/2", "label": "02/01/24", "display": "$52.50" },
            { "x": "60", "y": "180", "label": "03/01/24", "display": "$180.00" } ] } ],
        "tooltip": { "grouped": false }
    })
}

fn ctx(zone: &TimeZone) -> RenderContext<'_> {
    RenderContext { zone, date_format: DateFormat::Us, locale: red_ale_locale::Locale::c() }
}

/// A predefined style sheet with every header and footer line on, so the golden pages show them.
fn sheet(name: &str) -> StyleSheet {
    let mut sheet = StyleSheets::predefined().get(name).unwrap().clone();
    let extra = match sheet.template() {
        StyleSheetTemplate::Easy | StyleSheetTemplate::Fancy | StyleSheetTemplate::Footer => {
            json!({ "General": {
                "Preparer": "A. Bookkeeper", "Prepared for": "Acme Ltd", "Show preparer info": true, "Footer": "Confidential" } })
        }
        StyleSheetTemplate::HeadOrTail => json!({ "General": {
            "Preparer": "A. Bookkeeper", "Prepared for": "Acme Ltd", "Additional Comments": "Draft", "Show comments at bottom": true } }),
        _ => json!({}),
    };
    sheet.apply(&extra).unwrap();
    sheet
}

/// The page, with red-ale's version written as `{version}` so the golden pages survive a release.
fn page(result: &ReportResult, sheet: Option<&StyleSheet>) -> String {
    let zone = TimeZone::fixed(3600).unwrap();
    let html = render::html(result, sheet, &ctx(&zone));
    let again = render::html(result, sheet, &ctx(&zone));
    assert_eq!(html, again, "two renders give the same bytes");
    check_page(&html);
    html.replace(concat!("red-ale ", env!("CARGO_PKG_VERSION")), "red-ale {version}")
}

fn golden(name: &str, html: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/render").join(format!("{name}.html"));
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, html).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {}; run with UPDATE_SNAPSHOTS=1", path.display()));
    assert!(html == expected, "{name}.html changed; regenerate with UPDATE_SNAPSHOTS=1 and review the diff");
}

#[test]
fn tables_render_through_each_predefined_style_sheet() {
    let report = balance_sheet();
    StyleSheets::predefined().iter().for_each(|s| {
        let slug: String = s.name().chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
        golden(&format!("sheet-{slug}"), &page(&report, Some(&sheet(s.name()))));
    });
    golden("sheet-none", &page(&report, None));
}

#[test]
fn charts_render_as_inline_svg() {
    golden("chart-bar", &page(&chart(&net_worth_bar(false)), None));
    golden("chart-stacked-bar", &page(&chart(&net_worth_bar(true)), None));
    golden("chart-line", &page(&chart(&income_line()), None));
    golden("chart-pie", &page(&chart(&expense_pie()), None));
    golden("chart-scatter", &page(&chart(&price_scatter()), None));
}

#[test]
fn staircase_amounts_take_the_column_of_their_level() {
    let html = page(&balance_sheet(), None);
    let row = |label: &str| html.lines().find(|l| l.contains(&format!(">{label}<"))).unwrap().to_owned();
    // Three amount positions: level 2 leftmost, level 0 rightmost.
    assert!(row("Savings").contains("</span></td><td class=\"number-cell neg\">-$250.00</td><td></td><td></td></tr>"));
    assert!(row("Bank").contains("<td></td><td class=\"number-cell\">$2,186.00</td><td></td></tr>"));
    assert!(row("Total Assets").contains("<td></td><td></td><td class=\"total-number-cell rule-above\">$116,006.33</td></tr>"));
    assert!(row("Savings").contains("style=\"--depth:2\""));
    assert!(html.contains("<th class=\"column-heading-right\" colspan=\"3\">Balance</th>"));
    assert!(html.contains("<td class=\"text-cell label-cell\" colspan=\"3\">Spanning note</td>"));
    let commodities = "<table class=\"commodities\"><tr><td>#608.00</td><td>$912.00</td></tr><tr><td class=\"neg\">-30. FUNDS</td></tr>";
    assert!(html.contains(commodities));
}

#[test]
fn links_are_data_attributes_never_hrefs() {
    let html = page(&balance_sheet(), None);
    let link = format!("data-account=\"{BANK}\" data-link=\"{{&quot;account&quot;:&quot;{BANK}&quot;}}\"><span class=\"link\">Bank</span>");
    assert!(html.contains(&link));
    assert!(!html.contains("href"));
    let mut no_links = StyleSheet::new("No links", StyleSheetTemplate::Plain);
    no_links.apply(&json!({ "General": { "Enable Links": false } })).unwrap();
    let html = page(&balance_sheet(), Some(&no_links));
    assert!(!html.contains("data-account") && !html.contains("data-link") && !html.contains("class=\"link\""));
}

#[test]
fn style_sheet_options_reach_the_page() {
    let mut s = StyleSheet::new("Mine", StyleSheetTemplate::Easy);
    s.apply(&json!({
        "Colors": { "Grand Total Cell Color": "123456", "Background Color": "fafafaff" },
        "Tables": { "Table cell padding": "2.5", "Table border width": 0 },
        "Fonts": { "Negative Values in Red": false, "Title": "Liberation Serif Italic 20" }
    }))
    .unwrap();
    let html = page(&balance_sheet(), Some(&s));
    assert!(html.contains("tr.grand-total{background-color:#123456}"));
    assert!(html.contains("background-color:#fafafa;"));
    assert!(html.contains("padding:2.5px;"));
    assert!(html.contains(".neg{}"));
    assert!(html.contains("h1.report-title{margin:0 0 4px;font-family:\"Liberation Serif\",sans-serif;font-size:20pt;font-style:italic;}"));
    let tail = page(&balance_sheet(), Some(&sheet("Head or Tail")));
    assert!(!tail.contains("tr.primary-subheading{"), "Head or Tail's invalid subheading colour sets none");
    assert!(tail.contains("<p>Report Creation Date: 01/15/2025 10:30:00 +0100</p>"));
    let css = StyleSheets::predefined().get("CSS-based stylesheet (experimental)").unwrap().clone();
    let html = page(&balance_sheet(), Some(&css));
    assert!(html.contains("/* default style */") && html.contains("<body class=\"sheet-css\">"));
}

#[test]
fn hostile_text_is_escaped_everywhere() {
    let evil = "</title><script>alert(1)</script><img src=x onerror=\"alert(2)\">";
    let report = result(evil, json!([ { "title": evil, "blocks": [
        { "kind": "text", "text": evil },
        { "kind": "warning", "title": evil, "text": evil },
        { "kind": "table", "title": evil, "columns": [ { "label": evil, "role": "label" }, { "label": evil, "role": "amount" } ],
          "rows": [ { "cells": [ { "text": evil }, { "number": "-1", "display": evil, "suffix": evil } ] } ] },
        { "kind": "chart", "type": "bar", "title": [evil], "labels": [evil], "xAxis": { "label": evil }, "yAxis": { "label": evil },
          "series": [ { "label": evil, "color": "#000000", "values": ["1"], "displays": [evil] } ] },
        { "kind": "chart", "type": "pie", "title": [evil], "labels": [evil],
          "series": [ { "label": evil, "color": "#000000", "values": ["1"], "displays": [evil] } ] },
        { "kind": "optionsSummary", "rows": [ { "section": evil, "name": evil, "value": evil } ] }
    ] } ]));
    let mut s = StyleSheet::new("Evil", StyleSheetTemplate::HeadOrTail);
    s.apply(&json!({
        "General": { "Preparer": evil, "Prepared for": evil, "Additional Comments": evil },
        "Fonts": { "Text cell": "x\"; } body { background: url(http://evil.example/) } p { 12" }
    }))
    .unwrap();
    let html = page(&report, Some(&s));
    assert!(!html.contains("<script") && !html.contains("<img"));
    assert!(html.contains("<title>&lt;/title&gt;&lt;script&gt;"));
    assert!(html.contains("<p>Prepared by: <b>&lt;/title&gt;&lt;script&gt;"));
    assert!(html.contains("font-family:\"x  body  background urlhttpevil.example  p\",sans-serif;font-size:12pt;"));
    assert!(html.contains("&lt;/title&gt;&lt;script&gt;alert(1)&lt;/script&gt;&lt;img src=x onerror=&quot;alert(2)&quot;&gt;"));
    let mut css = StyleSheet::new("CSS", StyleSheetTemplate::Css);
    assert!(css.apply(&json!({ "General": { "CSS": "p{}</style><script>alert(1)</script>" } })).is_err());
}

#[test]
fn a_missing_style_sheet_renders_with_the_base_look() {
    let sheets = StyleSheets::predefined();
    assert!(sheets.get("Deleted").is_none());
    let html = page(&balance_sheet(), sheets.get("Deleted"));
    assert!(html.contains("<body class=\"sheet-none\">"));
    assert_eq!(render::missing_style_sheet("Deleted").text, "style sheet \"Deleted\" does not exist; rendered with no style sheet");
}

// ---- Checks every page passes ----

/// No script, nothing that loads, and well-formed markup with SVG inside its viewBox.
fn check_page(html: &str) {
    let lower = html.to_ascii_lowercase();
    ["<script", "<iframe", "<object", "<embed", "<link", "<img", "<image", "<use", "<foreignobject"]
        .iter()
        .for_each(|bad| assert!(!lower.contains(bad), "the page contains {bad:?}"));
    // Escaped text may say anything; markup and the stylesheet may not load or run anything.
    let style = &lower[lower.find("<style>").unwrap()..lower.find("</style>").unwrap()];
    ["url(", "@import", "http:", "https:", "javascript:", "\\"].iter().for_each(|bad| assert!(!style.contains(bad), "the CSS has {bad:?}"));
    lower.split('<').skip(1).map(|t| &t[..t.find('>').unwrap_or(t.len())]).for_each(|tag| {
        let outside_values: String = tag.split('"').step_by(2).collect();
        [" href=", " src=", " xlink:href=", " on"].iter().for_each(|bad| assert!(!outside_values.contains(bad), "<{tag}> has {bad:?}"));
        assert!(!tag.contains("http:") && !tag.contains("https:") && !tag.contains("url("), "<{tag}> names a URL");
    });
    assert!(html.starts_with("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">"));
    check_well_formed(html);
    svgs(html).iter().for_each(|svg| check_svg(svg));
}

const VOID: [&str; 4] = ["meta", "br", "hr", "!DOCTYPE"];

/// A simple XML-ish parse: every tag closes in order (HTML's void elements excepted), attribute values are quoted,
/// and every `&` starts an entity.
fn check_well_formed(html: &str) {
    let mut stack: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        check_text(&rest[..open]);
        let after = &rest[open..];
        let end = after.find('>').expect("an unclosed tag");
        let tag = &after[1..end];
        rest = &after[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            assert_eq!(stack.pop().as_deref(), Some(name), "</{name}> closes the wrong element");
            continue;
        }
        let name: String = tag.chars().take_while(|c| !c.is_whitespace() && *c != '/').collect();
        check_attributes(tag);
        if tag.ends_with('/') || VOID.contains(&name.as_str()) {
            continue;
        }
        if name == "style" {
            let close = rest.find("</style>").expect("an unclosed style");
            rest = &rest[close + "</style>".len()..];
            continue;
        }
        stack.push(name);
    }
    check_text(rest);
    assert!(stack.is_empty(), "unclosed: {stack:?}");
}

fn check_text(text: &str) {
    assert!(!text.contains('>'), "a raw > in text: {text:?}");
    text.split('&').skip(1).for_each(|after| {
        let entity: String = after.chars().take_while(|c| *c != ';').collect();
        let ok = (entity.chars().all(|c| c.is_ascii_alphanumeric() || c == '#')) && !entity.is_empty() && after.contains(';');
        assert!(ok, "a bare & before {after:?}");
    });
}

fn check_attributes(tag: &str) {
    let quotes = tag.matches('"').count();
    assert!(quotes.is_multiple_of(2), "unbalanced quotes in <{tag}>");
    tag.split('"').step_by(2).for_each(|outside| assert!(!outside.contains('<') && !outside.contains('\''), "<{tag}>"));
    tag.split('"').skip(1).step_by(2).for_each(check_text);
}

fn svgs(html: &str) -> Vec<&str> {
    html.match_indices("<svg ").map(|(at, _)| &html[at..at + html[at..].find("</svg>").unwrap()]).collect()
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    tag.find(&key).map(|at| {
        let value = &tag[at + key.len()..];
        &value[..value.find('"').unwrap()]
    })
}

fn numbers(text: &str) -> Vec<f64> {
    // Floating point only to read the test's own output; the renderer has none.
    text.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).filter(|s| !s.is_empty()).map(|s| s.parse().unwrap()).collect()
}

/// Every coordinate of every shape and text lies within the viewBox.
fn check_svg(svg: &str) {
    let view = numbers(attr(svg, "viewBox").unwrap());
    let (w, h) = (view[2], view[3]);
    let inside = |x: f64, y: f64, what: &str| {
        assert!((0.0..=w).contains(&x) && (0.0..=h).contains(&y), "{what}: ({x}, {y}) outside {w}×{h}");
    };
    svg.split('<').skip(1).map(|t| &t[..t.find('>').unwrap_or(t.len())]).for_each(|tag| {
        let num = |name: &str| attr(tag, name).map(|v| v.parse::<f64>().unwrap());
        if let (Some(x), Some(y)) = (num("x"), num("y")) {
            inside(x, y, tag);
            let (dw, dh) = (num("width").unwrap_or(0.0), num("height").unwrap_or(0.0));
            assert!(dw >= 0.0 && dh >= 0.0, "{tag}");
            inside(x + dw, y + dh, tag);
        }
        if let (Some(cx), Some(cy)) = (num("cx"), num("cy")) {
            let r = num("r").unwrap_or(0.0);
            inside(cx - r, cy - r, tag);
            inside(cx + r, cy + r, tag);
        }
        if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (num("x1"), num("y1"), num("x2"), num("y2")) {
            inside(x1, y1, tag);
            inside(x2, y2, tag);
        }
        if let Some(points) = attr(tag, "points") {
            numbers(points).chunks(2).for_each(|p| inside(p[0], p[1], tag));
        }
        if let Some(d) = attr(tag, "d") {
            path_points(d).into_iter().for_each(|(x, y)| inside(x, y, tag));
        }
        if let Some(t) = attr(tag, "transform") {
            let n = numbers(t);
            inside(n[1], n[2], tag);
        }
    });
}

/// The end points of a path's `M`, `L` and `A` commands.
fn path_points(d: &str) -> Vec<(f64, f64)> {
    d.split(|c: char| c.is_ascii_alphabetic())
        .filter_map(|args| {
            let n = numbers(args);
            (n.len() >= 2).then(|| (n[n.len() - 2], n[n.len() - 1]))
        })
        .collect()
}

// ---- Properties ----

fn rational() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        (any::<i64>(), 0_u32..6).prop_map(|(n, places)| json!(format!("{n}/{}", 10_i64.pow(places)))),
        (-1000_i64..1000).prop_map(|n| json!(n.to_string())),
        Just(json!("123456789012345678901234567890/7")),
    ]
}

fn plot_size() -> impl Strategy<Value = Value> {
    prop_oneof![(1_u32..=100).prop_map(|p| json!({ "percent": p })), (1_u32..3000).prop_map(|p| json!({ "pixels": p }))]
}

fn any_chart() -> impl Strategy<Value = Value> {
    let kind = prop_oneof![Just("bar"), Just("line"), Just("pie"), Just("scatter")];
    let labels = proptest::collection::vec("[a-zA-Z0-9 <&>]{0,40}", 0..14);
    let titles = proptest::collection::vec("[a-z ]{0,80}", 0..5);
    (kind, labels, titles, plot_size(), plot_size(), any::<bool>(), any::<bool>(), 1_usize..5, proptest::collection::vec(rational(), 60))
        .prop_map(|(kind, labels, titles, width, height, stacked, legend, series, values)| {
            let n = labels.len();
            let series_count = if kind == "pie" { 1 } else { series };
            let series: Vec<Value> = (0..series_count)
                .map(|s| {
                    let vals: Vec<Value> = (0..n).map(|i| values[(s * 13 + i) % values.len()].clone()).collect();
                    let points: Vec<Value> = (0..n)
                        .map(|i| {
                            let pick = |k: usize| match &values[k % values.len()] { Value::Null => json!("0"), v => v.clone() };
                            json!({ "x": pick(s + i), "y": pick(s * 7 + i + 3) })
                        })
                        .collect();
                    let marker = ["circle", "square", "diamond", "cross", "plus"][s % 5];
                    let stack = if s % 2 == 0 { "a" } else { "b" };
                    let mut series =
                        json!({ "label": format!("S{s} <x>"), "color": "#336699", "stack": stack, "fill": s == 0, "marker": marker });
                    if kind == "scatter" {
                        series["points"] = Value::Array(points);
                        series["line"] = json!("#ff0000");
                    } else {
                        series["values"] = Value::Array(vals);
                        if s == 1 {
                            series["kind"] = json!("line");
                        }
                    }
                    series
                })
                .collect();
            json!({ "kind": "chart", "type": kind, "title": titles, "width": width, "height": height, "stacked": stacked,
                    "legend": legend, "xAxis": { "label": "x axis" }, "yAxis": { "label": "y axis", "format": "percent" },
                    "labels": labels, "series": series })
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Any chart renders deterministically, well-formed, with every coordinate inside its viewBox.
    #[test]
    fn charts_stay_inside_their_view_box(c in any_chart()) {
        let report = chart(&c);
        let zone = TimeZone::utc();
        let html = render::html(&report, None, &ctx(&zone));
        prop_assert_eq!(&html, &render::html(&report, None, &ctx(&zone)));
        check_page(&html);
    }
}
