//! Helpers for the tests of the financial statements (Account Summary, Balance Sheet, Income Statement and Profit &
//! Loss): run a built-in report through `report.run` and `report.export` on a copy of a fixture or on a book built
//! through the API, and read the figures back by row label.

#![allow(dead_code, clippy::missing_panics_doc, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError};
use red_ale_service::Settings;
use red_ale_service::config::ConfigStore;
use serde_json::{Map, Value, json};

/// 2024-03-15 12:00 UTC, the pinned now of `fixtures/reports/`.
pub const NOW: Time64 = Time64(1_710_504_000);

/// A dispatcher over a temporary directory holding the book, the Configuration and a report directory.
pub struct Setup {
    pub dir: tempfile::TempDir,
    pub d: Dispatcher,
}

/// The settings GnuCash's report tests run with (`design/reports.md` §13): no reversed accounts, an accounting
/// period of today to today, and `date_format`.
pub fn test_settings(date_format: &str) -> Value {
    json!({
        "accountingPeriod": { "start": "today", "end": "today" },
        "reports": { "directory": "reports", "reverseBalancedAccounts": "none", "dateFormat": date_format },
    })
}

impl Setup {
    /// An empty report directory and `config` as the Configuration; no book open.
    pub fn with_config(config: &Value) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("reports")).unwrap();
        std::fs::write(dir.path().join("config.json"), config.to_string()).unwrap();
        let d = dispatcher(&dir.path().join("config.json"));
        Self { dir, d }
    }

    /// A copy of `fixtures/reports/<name>`, opened read-only.
    pub fn fixture(name: &str, config: &Value) -> Self {
        let mut s = Self::with_config(config);
        std::fs::copy(format!("{}/../../fixtures/reports/{name}", env!("CARGO_MANIFEST_DIR")), s.book()).unwrap();
        let book = s.book().display().to_string();
        s.call("book.open", json!({ "path": book, "readOnly": true }));
        s
    }

    /// A new book, built by the caller through the API.
    pub fn new_book(config: &Value) -> Self {
        let mut s = Self::with_config(config);
        let book = s.book().display().to_string();
        s.call("book.create", json!({ "path": book }));
        s
    }

    pub fn book(&self) -> PathBuf {
        self.dir.path().join("book.gnucash")
    }

    /// A user script in the report directory.
    pub fn script(&self, path: &str, text: &str) {
        std::fs::write(self.dir.path().join("reports").join(path), text).unwrap();
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

    /// The HTML `report.export` gives for `report` with `options`.
    pub fn export(&mut self, report: &str, options: &Value) -> String {
        self.call("report.export", json!({ "report": report, "options": options }))["html"].as_str().unwrap().to_owned()
    }

    /// Files in the book's directory, to see that nothing was written beside the book.
    pub fn files(&self) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(self.dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    /// Account creation by `name`, `type`, optional `parent` and `commodity` (default USD).
    pub fn account(&mut self, name: &str, kind: &str, parent: Option<&str>, commodity: &str) {
        let mut a = json!({ "name": name, "type": kind, "commodity": commodity });
        if let Some(p) = parent {
            a["parent"] = json!(p);
        }
        self.call("account.create", a);
    }

    /// A transaction in USD on `date` moving `amount` from `from` to `to`.
    pub fn transfer(&mut self, date: &str, description: &str, from: &str, to: &str, amount: &str) {
        let splits = json!([{ "account": to, "value": amount }, { "account": from, "value": format!("-{amount}") }]);
        let t = json!({ "datePosted": date, "currency": "CURRENCY::USD", "description": description, "splits": splits });
        self.call("transaction.post", t);
    }
}

/// A dispatcher keeping its Configuration in `config`, with the fixed clock of `NOW`.
pub fn dispatcher(config: &Path) -> Dispatcher {
    let settings = Settings { config: ConfigStore::at(PathBuf::from(config)), ..Settings::default() };
    let mut d = Dispatcher::new(settings);
    let made = AtomicU32::new(0);
    d.set_env_factory(Box::new(move || {
        let seed = (0x00ed_u128 << 112) | (u128::from(made.fetch_add(1, Ordering::Relaxed)) << 80);
        Box::new(FixedEnv::new(NOW, seed))
    }));
    d
}

/// The options of a run of an `*.expected.json` file (`"Section/Name": value`) as the `options` param.
pub fn options_param(run: &Value) -> Value {
    let mut out = Map::new();
    run["options"].as_object().unwrap().iter().for_each(|(key, value)| {
        let (section, name) = key.split_once('/').unwrap();
        let entry = out.entry(section.to_owned()).or_insert_with(|| json!({}));
        entry[name] = value.clone();
    });
    Value::Object(out)
}

/// An expected-figure file of `fixtures/reports/`.
pub fn expected(name: &str) -> Value {
    let path = format!("{}/../../fixtures/reports/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The run of `expected` with this `id`.
pub fn expected_run<'a>(expected: &'a Value, id: &str) -> &'a Value {
    expected["runs"].as_array().unwrap().iter().find(|r| r["id"] == id).unwrap_or_else(|| panic!("no run {id}"))
}

/// Every block of the result, groups opened, in order.
pub fn blocks(result: &Value) -> Vec<&Value> {
    fn walk<'a>(b: &'a Value, out: &mut Vec<&'a Value>) {
        if b["kind"] == "group" {
            b["blocks"].as_array().unwrap().iter().for_each(|x| walk(x, out));
        } else {
            out.push(b);
        }
    }
    let mut out = vec![];
    result["sections"].as_array().unwrap().iter().flat_map(|s| s["blocks"].as_array().unwrap()).for_each(|b| walk(b, &mut out));
    out
}

/// The tables of the result, in order.
pub fn tables(result: &Value) -> Vec<&Value> {
    blocks(result).into_iter().filter(|b| b["kind"] == "table").collect()
}

/// The text of a label cell, `""` for an empty one.
pub fn cell_text(cell: &Value) -> String {
    cell["text"].as_str().unwrap_or("").to_owned()
}

/// The display strings of a cell: a monetary or price cell's `display`; for a `commodities` cell each line's
/// amount and converted amount; none for an empty cell.
pub fn displays(cell: &Value) -> Vec<String> {
    if cell.is_null() {
        return vec![];
    }
    if let Some(lines) = cell["commodities"].as_array() {
        return lines
            .iter()
            .flat_map(|l| [l["display"].as_str(), l["convertedDisplay"].as_str()])
            .flatten()
            .map(str::to_owned)
            .collect();
    }
    cell["display"].as_str().map(|s| vec![s.to_owned()]).unwrap_or_default()
}

/// The label column of a table: the column with role `label`.
fn label_column(table: &Value) -> usize {
    table["columns"].as_array().unwrap().iter().position(|c| c["role"] == "label").unwrap_or(0)
}

/// The amount column of a table: the last column with role `amount`.
fn amount_column(table: &Value) -> usize {
    table["columns"].as_array().unwrap().iter().rposition(|c| c["role"] == "amount").unwrap()
}

/// The labels of every row of `table` that has one, in order.
pub fn labels(table: &Value) -> Vec<String> {
    let col = label_column(table);
    table["rows"].as_array().unwrap().iter().map(|r| &r["cells"][col]).filter(|c| !c.is_null()).map(cell_text).collect()
}

/// The rows of every table of `result` whose label is `label`.
pub fn rows_labelled<'a>(result: &'a Value, label: &str) -> Vec<(&'a Value, &'a Value)> {
    tables(result)
        .into_iter()
        .flat_map(|t| {
            let col = label_column(t);
            t["rows"].as_array().unwrap().iter().filter(move |r| cell_text(&r["cells"][col]) == label).map(move |r| (t, r))
        })
        .collect()
}

/// The display strings of the amount of the one row labelled `label`, leaving out a section heading of the same
/// name (the Balance Sheet's "Equity" heading above the Equity account).
pub fn amount_of(result: &Value, label: &str) -> Vec<String> {
    let mut rows = rows_labelled(result, label);
    if rows.len() > 1 {
        rows.retain(|(_, r)| r["role"] != "heading");
    }
    assert_eq!(rows.len(), 1, "one row labelled {label:?}: {}", all_rows(result));
    let (table, row) = rows[0];
    displays(&row["cells"][amount_column(table)])
}

/// The level of the amount of the one row labelled `label`.
pub fn level_of(result: &Value, label: &str) -> u64 {
    let rows = rows_labelled(result, label);
    assert_eq!(rows.len(), 1, "one row labelled {label:?}");
    rows[0].1["level"].as_u64().unwrap()
}

/// Every row of every table as `label: amounts`, for failure messages.
pub fn all_rows(result: &Value) -> String {
    tables(result)
        .iter()
        .flat_map(|t| {
            let (lc, ac) = (label_column(t), t["columns"].as_array().unwrap().iter().rposition(|c| c["role"] == "amount"));
            t["rows"].as_array().unwrap().iter().map(move |r| {
                let amount = ac.map(|a| displays(&r["cells"][a])).unwrap_or_default();
                format!("{} {}: {amount:?}", r["role"].as_str().unwrap_or("data"), cell_text(&r["cells"][lc]))
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A named check of one figure: the row labelled `label` shows `want`, in `report.run`'s result and in
/// `report.export`'s HTML. `line` is the 5.17 test line the figure comes from.
pub struct Check<'a> {
    pub name: &'a str,
    pub line: u32,
    pub label: &'a str,
    pub want: &'a [&'a str],
}

/// Run the `checks` against `result` and `html`.
pub fn assert_checks(result: &Value, html: &str, checks: &[Check<'_>]) {
    checks.iter().for_each(|c| {
        let got = amount_of(result, c.label);
        assert_eq!(got, c.want, "{} (test line {}): {}", c.name, c.line, all_rows(result));
        c.want.iter().for_each(|w| assert!(html.contains(w), "{} (test line {}): {w} is in the HTML", c.name, c.line));
    });
}

/// The position, from 0, of the one row labelled `label` in its table.
pub fn position_of(result: &Value, label: &str) -> usize {
    let rows = rows_labelled(result, label);
    assert_eq!(rows.len(), 1, "one row labelled {label:?}");
    let (table, row) = rows[0];
    table["rows"].as_array().unwrap().iter().position(|r| std::ptr::eq(r, row)).unwrap()
}

/// The checks' rows stand where GnuCash's test reads them: its row `row` (counted from 1 in the HTML table) is the
/// result row at `row - offset`, `offset` being 1 plus the rows GnuCash's table has before the result's (the Balance
/// Sheet's row of spacer cells).
pub fn assert_rows_at(result: &Value, rows: &[(&str, u32, &str)], offset: u32) {
    rows.iter().for_each(|(name, row, label)| {
        let want = usize::try_from(row - offset).unwrap();
        assert_eq!(position_of(result, label), want, "{name}: GnuCash's row {row} is {label}: {}", all_rows(result));
    });
}

/// The rows of the exchange-rates table: `[amount, price]` display strings.
pub fn rates(result: &Value) -> Vec<String> {
    let rates: Vec<&Value> = tables(result).into_iter().filter(|t| is_rates_table(t)).collect();
    rates
        .iter()
        .flat_map(|t| t["rows"].as_array().unwrap())
        .flat_map(|r| r["cells"].as_array().unwrap().iter().flat_map(displays))
        .collect()
}

/// The exchange-rates table of `lib/html-utilities`: two amount columns, the first headed "Exchange rate(s)" or
/// unlabelled when empty, price cells in the second.
pub fn is_rates_table(t: &Value) -> bool {
    let cols = t["columns"].as_array().unwrap();
    cols.len() == 2 && cols.iter().all(|c| c["role"] == "amount" && c["levels"].is_null()) && t["columns"][1]["label"] == ""
}
