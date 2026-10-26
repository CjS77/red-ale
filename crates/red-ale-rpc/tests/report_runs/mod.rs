//! Running built-in reports through `report.run` and `report.export` on copies of the `fixtures/reports/` books,
//! with the settings GnuCash 5.17's report tests run under (`design/reports.md` §13): no reversed balances, USD, an
//! accounting period of today to today and the pinned now, 2024-03-15 12:00 UTC.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::missing_panics_doc)]

use std::path::{Path, PathBuf};

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::Dispatcher;
use red_ale_service::Settings;
use red_ale_service::config::ConfigStore;
use serde_json::{Value, json};

/// 2024-03-15 12:00 UTC, the pinned now of `fixtures/reports/`.
pub const NOW: Time64 = Time64(1_710_504_000);

/// A fixture book copied into a temporary directory, open read-only or for editing, and a dispatcher over it.
pub struct Fixture {
    dir: tempfile::TempDir,
    pub d: Dispatcher,
}

impl Fixture {
    /// `fixtures/reports/<name>.gnucash`, with `reports.dateFormat` set to `date_format`.
    pub fn open(name: &str, date_format: &str, read_only: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("reports")).unwrap();
        let config = json!({
            "accountingPeriod": { "start": "today", "end": "today" },
            "reports": { "directory": "reports", "reverseBalancedAccounts": "none", "dateFormat": date_format },
        });
        std::fs::write(dir.path().join("config.json"), config.to_string()).unwrap();
        let book = dir.path().join("book.gnucash");
        std::fs::copy(format!("{}/../../fixtures/reports/{name}.gnucash", env!("CARGO_MANIFEST_DIR")), &book).unwrap();
        let mut d = dispatcher(&dir.path().join("config.json"));
        let params = json!({ "path": book.display().to_string(), "readOnly": read_only });
        d.call("book.open", params).unwrap_or_else(|e| panic!("book.open: {e} {:?}", e.data));
        Self { dir, d }
    }

    /// `report.run` of `report` with `options`.
    pub fn run(&mut self, report: &str, options: &Value) -> Value {
        self.call("report.run", &json!({ "report": report, "options": options }))
    }

    /// The HTML of `report.export` of `report` with `options`.
    pub fn export(&mut self, report: &str, options: &Value) -> String {
        let export = self.call("report.export", &json!({ "report": report, "options": options }));
        export["html"].as_str().unwrap().to_owned()
    }

    pub fn call(&mut self, method: &str, params: &Value) -> Value {
        self.d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    /// The files beside the book, to see that a run wrote none.
    pub fn files(&self) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(self.dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }
}

/// `report.options` of `report` with no book open, as `section/name key type default` lines in GnuCash's order.
pub fn described_options(report: &str) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let mut d = dispatcher(&dir.path().join("config.json"));
    let described = d.call("report.options", json!({ "report": report })).unwrap_or_else(|e| panic!("report.options: {e} {:?}", e.data));
    let line = |o: &Value| format!("{}/{} {} {} {}", o["section"].as_str().unwrap(), o["name"].as_str().unwrap(), o["key"], o["type"], o["default"]);
    described["options"].as_array().unwrap().iter().map(line).collect()
}

/// A dispatcher keeping its Configuration in `config`, with a fixed clock at [`NOW`].
fn dispatcher(config: &Path) -> Dispatcher {
    let settings = Settings { config: ConfigStore::at(PathBuf::from(config)), ..Settings::default() };
    let mut d = Dispatcher::new(settings);
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(NOW, 0x00ed_u128 << 112))));
    d
}

/// Every block of every section of a result.
pub fn blocks(result: &Value) -> Vec<&Value> {
    result["sections"].as_array().unwrap().iter().flat_map(|s| s["blocks"].as_array().unwrap()).collect()
}

/// The blocks of `kind`.
pub fn blocks_of<'a>(result: &'a Value, kind: &str) -> Vec<&'a Value> {
    blocks(result).into_iter().filter(|b| b["kind"] == kind).collect()
}

/// The only block of `kind`.
pub fn only<'a>(result: &'a Value, kind: &str) -> &'a Value {
    let found = blocks_of(result, kind);
    assert_eq!(found.len(), 1, "one {kind} block: {result:#}");
    found[0]
}

/// The display strings of column `col` (from 0) of a table block, one per row.
pub fn column(table: &Value, col: usize) -> Vec<String> {
    let rows = table["rows"].as_array().unwrap();
    rows.iter().map(|r| r["cells"][col]["display"].as_str().map_or_else(|| r["cells"][col]["text"].to_string(), str::to_owned)).collect()
}

/// The display strings of every row of a table block, cell by cell.
pub fn table_rows(table: &Value) -> Vec<Vec<String>> {
    let width = table["columns"].as_array().unwrap().len();
    let cols: Vec<Vec<String>> = (0..width).map(|c| column(table, c)).collect();
    (0..cols[0].len()).map(|r| cols.iter().map(|c| c[r].clone()).collect()).collect()
}

/// A series' values as `num/denom` strings (`num` alone when whole).
pub fn values(series: &Value) -> Vec<String> {
    series["values"].as_array().unwrap().iter().map(rational).collect()
}

/// A Rational of the wire as `num/denom`, or `num` when whole.
pub fn rational(v: &Value) -> String {
    let (num, denom) = (v["num"].as_str().unwrap(), v["denom"].as_str().unwrap());
    if denom == "1" { num.to_owned() } else { format!("{num}/{denom}") }
}

/// The cell texts of each body row of the `index`th table (from 0) of an exported page, tags inside cells removed.
pub fn html_table(html: &str, index: usize) -> Vec<Vec<String>> {
    let table = html.split("<table").nth(index + 1).unwrap_or_else(|| panic!("no table {index} in the page"));
    let table = &table[..table.find("</table>").unwrap()];
    let body = table.split("<tbody>").nth(1).unwrap_or("");
    body.split("<tr").skip(1).map(|row| row.split("<td").skip(1).map(cell_text).collect()).collect()
}

/// The text of one `<td …>…</td>`, without the tags inside it and with the renderer's escapes undone.
fn cell_text(cell: &str) -> String {
    let inner = &cell[cell.find('>').unwrap() + 1..cell.find("</td>").unwrap()];
    let (text, _) = inner.chars().fold((String::new(), false), |(mut out, in_tag), c| match c {
        '<' => (out, true),
        '>' => (out, false),
        c if !in_tag => {
            out.push(c);
            (out, false)
        }
        _ => (out, true),
    });
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

/// Each of `wanted` appears in `html`, as the export of the same run shows it.
pub fn in_html(html: &str, wanted: &[&str]) {
    wanted.iter().for_each(|w| assert!(html.contains(&escape(w)), "{w:?} is not in the exported HTML"));
}

/// The HTML escaping of the renderer, for text compared against the page.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}
