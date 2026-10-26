//! Helpers for the chart report tests: a dispatcher on a copy of a `fixtures/reports/` book, the expected figures of
//! `<book>.expected.json` (`fixtures/README.md`), and the first table of a report result read as GnuCash's tests
//! read the HTML table (`5.17:gnucash/report/test/test-report-extras.scm:96-109`).

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::missing_panics_doc, clippy::needless_pass_by_value)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use red_ale_core::{Book, FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError};
use red_ale_service::Settings;
use red_ale_service::config::ConfigStore;
use serde_json::{Map, Value, json};

/// 2024-03-15 12:00 UTC, the pinned now of `fixtures/reports/`.
pub const NOW: Time64 = Time64(1_710_504_000);

/// The fixture's directory.
pub fn fixtures() -> PathBuf {
    PathBuf::from(format!("{}/../../fixtures/reports", env!("CARGO_MANIFEST_DIR")))
}

/// A dispatcher with a fixed clock and UTC, a Configuration holding the report settings GnuCash's report tests run
/// with (`design/reports.md` §13), and a copy of a report fixture open for writing.
pub struct Charts {
    dir: tempfile::TempDir,
    pub d: Dispatcher,
}

impl Charts {
    /// `fixture` (`charts-net.gnucash`) open under `date_format` (`iso`, `locale`, …).
    pub fn open(fixture: &str, date_format: &str) -> Self {
        let mut charts = Self::new(date_format);
        let book = charts.dir.path().join("book.gnucash");
        std::fs::copy(fixtures().join(fixture), &book).unwrap();
        charts.call("book.open", json!({ "path": book.display().to_string() }));
        charts
    }

    /// No book yet.
    pub fn new(date_format: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = json!({ "reports": { "reverseBalancedAccounts": "none", "dateFormat": date_format } });
        std::fs::write(dir.path().join("config.json"), config.to_string()).unwrap();
        let d = dispatcher(&dir.path().join("config.json"));
        Self { dir, d }
    }

    /// The temporary directory books live in.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn call(&mut self, method: &str, params: Value) -> Value {
        self.d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    pub fn err(&mut self, method: &str, params: Value) -> RpcError {
        self.d.call(method, params.clone()).err().unwrap_or_else(|| panic!("{method} {params} should fail"))
    }

    /// `report.run` of `report` with `options`.
    pub fn run(&mut self, report: &str, options: &Value) -> Value {
        self.call("report.run", json!({ "report": report, "options": options }))
    }

    /// `report.export` of `report` with `options`: the HTML.
    pub fn export(&mut self, report: &str, options: &Value) -> String {
        self.call("report.export", json!({ "report": report, "options": options }))["html"].as_str().unwrap().to_owned()
    }

    /// The open Book.
    pub fn book(&mut self) -> Book {
        self.d.context().book().unwrap().clone()
    }

    /// The files beside the book, to see that a report writes none.
    pub fn files(&self) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(self.dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }
}

/// A dispatcher keeping its Configuration in `config`, with a fixed clock and sequential GUIDs.
fn dispatcher(config: &Path) -> Dispatcher {
    let settings = Settings { config: ConfigStore::at(PathBuf::from(config)), ..Settings::default() };
    let mut d = Dispatcher::new(settings);
    let made = AtomicU32::new(0);
    d.set_env_factory(Box::new(move || {
        let seed = (0x00ed_u128 << 112) | (u128::from(made.fetch_add(1, Ordering::Relaxed)) << 80);
        Box::new(FixedEnv::new(NOW, seed))
    }));
    d
}

/// The first table of a report result, as text: the column headers and each row's cells, a cell's `display`, else
/// its `text`, empty for `null`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableView {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Every block of a result, groups opened, in order.
pub fn blocks(result: &Value) -> Vec<Value> {
    fn walk(blocks: &[Value], out: &mut Vec<Value>) {
        blocks.iter().for_each(|b| {
            if b["kind"] == "group" {
                walk(b["blocks"].as_array().unwrap(), out);
            } else {
                out.push(b.clone());
            }
        });
    }
    let mut out = Vec::new();
    result["sections"].as_array().unwrap().iter().for_each(|s| walk(s["blocks"].as_array().unwrap(), &mut out));
    out
}

/// The blocks of `kind`.
pub fn blocks_of(result: &Value, kind: &str) -> Vec<Value> {
    blocks(result).into_iter().filter(|b| b["kind"] == kind).collect()
}

/// The first chart of a result.
pub fn chart(result: &Value) -> Value {
    blocks_of(result, "chart").into_iter().next().unwrap_or_else(|| panic!("no chart: {result}"))
}

/// The first table of a result.
pub fn table(result: &Value) -> TableView {
    let t = blocks_of(result, "table").into_iter().next().unwrap_or_else(|| panic!("no table: {result}"));
    let header = t["columns"].as_array().unwrap().iter().map(|c| c["label"].as_str().unwrap().to_owned()).collect();
    let cell = |c: &Value| c["display"].as_str().or_else(|| c["text"].as_str()).unwrap_or("").to_owned();
    let rows = t["rows"].as_array().unwrap().iter().map(|r| r["cells"].as_array().unwrap().iter().map(cell).collect()).collect();
    TableView { header, rows }
}

/// A series' values as decimal text (`"25"`, `"-5/2"`), `null` as `-`.
pub fn values(series: &Value) -> Vec<String> {
    let value = |v: &Value| if v.is_null() { "-".to_owned() } else { rational(v) };
    series["values"].as_array().unwrap().iter().map(value).collect()
}

/// A wire Rational as `num/denom`, or `num` when the denominator is 1.
pub fn rational(v: &Value) -> String {
    let (num, denom) = (v["num"].as_str().unwrap(), v["denom"].as_str().unwrap());
    if denom == "1" { num.to_owned() } else { format!("{num}/{denom}") }
}

/// HTML-escaped text, as the renderer escapes it.
pub fn escaped(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The expected figures of a fixture, `<book>.expected.json`.
pub struct Expected(pub Value);

impl Expected {
    /// `name` without its extension (`charts-net`).
    pub fn load(name: &str) -> Self {
        let text = std::fs::read_to_string(fixtures().join(format!("{name}.expected.json"))).unwrap();
        Self(serde_json::from_str(&text).unwrap())
    }

    /// The run with `id`.
    pub fn run(&self, id: &str) -> &Value {
        self.0["runs"].as_array().unwrap().iter().find(|r| r["id"] == id).unwrap_or_else(|| panic!("no run {id}"))
    }

    /// The GUID of an account the file names: a key of `accounts`, or `group.role` where accounts are grouped.
    pub fn account(&self, name: &str) -> String {
        let accounts = &self.0["accounts"];
        if let Some(guid) = accounts[name].as_str() {
            return guid.to_owned();
        }
        let (group, role) = name.split_once('.').unwrap_or_else(|| panic!("no account {name}"));
        accounts[group][role].as_str().unwrap_or_else(|| panic!("no account {name}")).to_owned()
    }

    /// The instant a date name of `dates` stands for (`"2024-03-01 00:00:00 UTC, …"`), or a day as local (UTC)
    /// midnight, `gnc-dmy2time64` (`"1970-01-01"`), as a Timestamp.
    fn absolute(&self, day: &str) -> String {
        let named = self.0["dates"][day].as_str().map(|d| d[..19].replace(' ', "T"));
        named.map_or_else(|| format!("{day}T00:00:00Z"), |d| format!("{d}Z"))
    }

    /// A run's options in the form `report.run` takes: sections and names nested, accounts as GUIDs, absolute
    /// dates as Timestamps and currencies as commodity ids.
    pub fn options(&self, run: &Value) -> Value {
        let mut out = Map::new();
        run["options"].as_object().unwrap().iter().for_each(|(key, value)| {
            let (section, name) = key.split_once('/').unwrap();
            let section = out.entry(section).or_insert_with(|| json!({}));
            section[name] = self.wire(name, value);
        });
        Value::Object(out)
    }

    fn wire(&self, name: &str, value: &Value) -> Value {
        match value {
            Value::Object(m) if m.contains_key("absolute") => json!({ "absolute": self.absolute(m["absolute"].as_str().unwrap()) }),
            Value::Array(a) if name == "Accounts" => a.iter().map(|n| json!(self.account(n.as_str().unwrap()))).collect(),
            Value::String(s) if name == "Report's currency" => json!(format!("CURRENCY::{s}")),
            other => other.clone(),
        }
    }
}

/// Run a run of `expected` on `charts` through `report.run` and `report.export`, and check every assertion: each
/// expected display string at its table position (and in the HTML), or each count.
pub fn check_run(charts: &mut Charts, expected: &Expected, id: &str) -> usize {
    let run = expected.run(id).clone();
    let template = run["templateGuid"].as_str().unwrap();
    let options = expected.options(&run);
    let result = charts.run(template, &options);
    assert_eq!(result["report"]["name"], run["template"], "{id}");
    let html = charts.export(template, &options);
    let view = table(&result);
    let checks = run["checks"].as_array().unwrap();
    checks.iter().for_each(|c| check(&view, &html, id, c));
    checks.len()
}

/// One assertion of an expected-figures file.
fn check(view: &TableView, html: &str, id: &str, c: &Value) {
    let what = format!("{id}: \"{}\" (test line {})", c["name"].as_str().unwrap(), c["line"]);
    if c["select"]["columns"].is_array() {
        return check_implied(view, html, &what, c);
    }
    let got = select(view, &c["select"]);
    if let Some(count) = c["count"].as_u64() {
        assert_eq!(got.len() as u64, count, "{what}: {got:?}");
        return;
    }
    let want: Vec<String> = c["expected"].as_array().unwrap().iter().map(|e| e.as_str().unwrap().to_owned()).collect();
    assert_eq!(got, want, "{what}");
    want.iter().for_each(|w| assert!(html.contains(&escaped(w)), "{what}: {w} is not in the HTML"));
}

/// A relation check: the implied text of each column, row by row.
fn check_implied(view: &TableView, html: &str, what: &str, c: &Value) {
    let columns = c["select"]["columns"].as_array().unwrap();
    columns.iter().for_each(|col| {
        let col = usize::try_from(col.as_u64().unwrap()).unwrap();
        let want: Vec<&str> = c["implied"][format!("column{col}")].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        let got: Vec<&str> = view.rows.iter().map(|r| r[col - 1].as_str()).collect();
        assert_eq!(got, want, "{what}: column {col}");
        want.iter().for_each(|w| assert!(html.contains(&escaped(w)), "{what}: {w} is not in the HTML"));
    });
}

/// The cells a `select` names: row 0 the headers, row N the Nth data row (-1 the last), every data row for a `null`
/// row; column N the Nth cell, every cell for a `null` column, less the first with `dropFirst`.
fn select(view: &TableView, select: &Value) -> Vec<String> {
    let column = select["column"].as_u64().map(|c| usize::try_from(c).unwrap() - 1);
    let rows: Vec<&Vec<String>> = match select["row"].as_i64() {
        None => view.rows.iter().collect(),
        Some(0) => vec![&view.header],
        Some(-1) => vec![view.rows.last().unwrap()],
        Some(n) => vec![&view.rows[usize::try_from(n - 1).unwrap()]],
    };
    let drop = usize::from(select["dropFirst"].as_bool().unwrap_or(false));
    match column {
        Some(c) => rows.iter().map(|r| r[c].clone()).collect(),
        None => rows.iter().flat_map(|r| r.iter().skip(drop).cloned()).collect(),
    }
}

/// Run every template on the open book, twice exporting each: the Book is unchanged and not dirty, nothing is
/// written beside it, the exports are the same bytes and hold no script.
pub fn check_read_only(charts: &mut Charts, templates: &[&str], options: &Value) {
    let before = charts.book();
    let files = charts.files();
    templates.iter().for_each(|t| {
        charts.run(t, options);
        let (a, b) = (charts.export(t, options), charts.export(t, options));
        assert_eq!(a, b, "{t}: two exports differ");
        assert!(!a.to_ascii_lowercase().contains("<script"), "{t}: the HTML holds a script");
    });
    assert_eq!(charts.book(), before, "the Book is unchanged");
    assert_eq!(charts.call("book.info", json!({}))["dirty"], false);
    assert_eq!(charts.files(), files, "no journal or other file was written");
}
