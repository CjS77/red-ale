//! The Multicolumn Balance Sheet and Income Statement (`standard/balsheet-pnl.rhai`, translated from
//! `5.17:gnucash/report/reports/standard/balsheet-pnl.scm`, cited as BP) through `report.run` and `report.export`:
//! every multicolumn check of GnuCash's own test on `fixtures/reports/balsheet-pnl.gnucash` and its SQLite copy, and
//! hand-worked cases on `balsheet-pnl-extras.gnucash` and `-trading`.
//!
//! GnuCash's test reads the text of HTML table cells (`5.17:gnucash/report/test/test-report-extras.scm:96-109`). The
//! result has no HTML table, so [`gnucash_rows`] rebuilds what GnuCash's table holds from the result's rows, as the
//! Scheme lays them out: a label cell and one cell per column (BP:346-369, full names and no amount indenting in every
//! run the test makes); the exchange-rate rows back into the one title row that holds them (BP:770-797), each rate
//! `"<one unit> <rate>"`; and each cell's texts in the order Guile's `sxpath` reads them, breadth first, so the amounts
//! shown (linked or not) come before the originals GnuCash prints in `<small>` with a trailing space (BP:404-410), and
//! a P&L header's `" to <end>"` is one text after its start (BP:1143-1147). The fixture's full names use `.`, the
//! engine's separator in GnuCash's test environment; red-ale's is `:`, so the expected names are compared with `:`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

mod statements;

use serde_json::Value;
use statements::{Setup, expected, options_param, tables, test_settings};

/// One row of GnuCash's table: the texts of each of its cells, from column 1; and beside each text, the display
/// strings of the result it is made from, which `report.export`'s HTML must hold.
#[derive(Debug, Default)]
struct GnuCashRow {
    cells: Vec<Vec<String>>,
    pieces: Vec<String>,
}

/// The texts of a data cell in the order `sxpath` reads them.
fn cell_texts(cell: &Value, pieces: &mut Vec<String>) -> Vec<String> {
    let s = |v: &Value| v.as_str().unwrap().to_owned();
    if cell.is_null() {
        return vec![];
    }
    if let Some(text) = cell["text"].as_str() {
        pieces.push(text.to_owned());
        return vec![text.to_owned()];
    }
    if let Some(lines) = cell["commodities"].as_array() {
        let shown = lines.iter().map(|l| if l["convertedDisplay"].is_string() { s(&l["convertedDisplay"]) } else { s(&l["display"]) });
        let originals = lines.iter().filter(|l| l["convertedDisplay"].is_string()).map(|l| format!("{} ", s(&l["display"])));
        let texts: Vec<String> = shown.chain(originals).collect();
        pieces.extend(texts.iter().map(|t| t.trim().to_owned()));
        return texts;
    }
    if cell["until"].is_string() {
        let display = s(&cell["display"]);
        let (from, until) = display.split_once(" to ").unwrap();
        pieces.extend([from.to_owned(), until.to_owned()]);
        return vec![from.to_owned(), format!(" to {until}")];
    }
    let display = s(&cell["display"]);
    pieces.push(display.clone());
    vec![display]
}

/// The rows of GnuCash's first table, rebuilt from the result's first table.
fn gnucash_rows(result: &Value) -> Vec<GnuCashRow> {
    let table = tables(result)[0];
    let mut out: Vec<GnuCashRow> = vec![];
    table["rows"].as_array().unwrap().iter().for_each(|row| {
        let cells = row["cells"].as_array().unwrap();
        let label = &cells[0];
        if label["monetary"].is_object() {
            // An exchange rate: its texts join the title row's cells, one "<unit> <rate>" per commodity.
            let title = out.last_mut().unwrap();
            let unit = label["display"].as_str().unwrap().to_owned();
            cells[1..].iter().enumerate().for_each(|(i, c)| {
                let rate = cell_texts(c, &mut title.pieces).join("");
                title.cells[i + 1].push(format!("{unit} {rate}"));
            });
            title.pieces.push(unit);
            return;
        }
        let mut r = GnuCashRow::default();
        let label_text: Vec<String> = label["text"].as_str().map(|t| vec![t.to_owned()]).unwrap_or_default();
        r.pieces.extend(label_text.clone());
        r.cells.push(label_text);
        let data: Vec<Vec<String>> = cells[1..].iter().map(|c| cell_texts(c, &mut r.pieces)).collect();
        r.cells.extend(data);
        out.push(r);
    });
    out
}

/// GnuCash's 1-based index, negative from the end, into `len` items.
fn pick(n: i64, len: usize) -> usize {
    let len = i64::try_from(len).unwrap();
    usize::try_from(if n < 0 { len + n } else { n - 1 }).unwrap()
}

/// The texts `(sxml->table-row-col sxml 1 row col)` reads, `null` row or column meaning all; and the display strings
/// they come from.
fn select(rows: &[GnuCashRow], sel: &Value) -> (Vec<String>, Vec<String>) {
    let chosen: Vec<&GnuCashRow> = match sel["row"].as_i64() {
        Some(n) => vec![&rows[pick(n, rows.len())]],
        None => rows.iter().collect(),
    };
    let mut texts = vec![];
    let mut pieces = vec![];
    chosen.iter().for_each(|r| {
        let cols: Vec<usize> = match sel["column"].as_i64() {
            Some(n) => vec![pick(n, r.cells.len())],
            None => (0..r.cells.len()).collect(),
        };
        cols.iter().filter(|c| **c < r.cells.len()).for_each(|c| texts.extend(r.cells[*c].iter().cloned()));
        pieces.extend(r.pieces.iter().cloned());
    });
    (texts, pieces)
}

/// The expected strings of a check, account names with red-ale's separator.
fn want(check: &Value) -> Vec<String> {
    let names = check["select"]["column"] == 1;
    check["expected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .map(|v| if names && v.starts_with("Root") { v.replace('.', ":") } else { v.to_owned() })
        .collect()
}

/// test-balsheet-pnl.scm:493-650: every check of the multicolumn runs of `balsheet-pnl.expected.json`, through
/// `report.run` and `report.export`, on the XML book and its SQLite copy. The runs at :617-650 give the P&L's options
/// to the balance sheet's GUID, which GnuCash renders as a balance sheet (`5.17:gnucash/report/report-core.scm:389-409`).
#[test]
fn expected_figures_of_gnucash_test() {
    let e = expected("balsheet-pnl.expected.json");
    let settings = test_settings("iso");
    let books = [
        ("xml", Setup::fixture("balsheet-pnl.gnucash", &settings)),
        ("sqlite", Setup::sqlite_fixture("balsheet-pnl.gnucash", &settings)),
    ];
    let mut checked = 0;
    for (format, mut s) in books {
        e["runs"].as_array().unwrap().iter().filter(|r| r["id"].as_str().unwrap().starts_with("multicol")).for_each(|run| {
            let options = options_param(run);
            let template = run["templateGuid"].as_str().unwrap();
            let result = s.run(template, &options);
            let html = s.export(template, &options);
            let rows = gnucash_rows(&result);
            run["checks"].as_array().unwrap().iter().for_each(|c| {
                let what = format!("{format} {}: {} (test line {})", run["id"], c["name"], c["line"]);
                let (got, pieces) = select(&rows, &c["select"]);
                assert_eq!(got, want(c), "{what}");
                pieces.iter().filter(|p| !p.is_empty()).for_each(|p| assert!(html.contains(p.as_str()), "{what}: {p} is in the HTML"));
                checked += 1;
            });
        });
    }
    assert_eq!(checked, 2 * 16, "the 16 multicolumn checks, on both books");
}

