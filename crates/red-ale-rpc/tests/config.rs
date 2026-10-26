//! The Configuration methods through the dispatcher: `config.schema`, `config.get` and `config.set`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use std::path::{Path, PathBuf};

use red_ale_report::limits::Limit;
use red_ale_report::settings::ReportSettings;
use red_ale_rpc::{Dispatcher, Origin, RpcError, codes};
use red_ale_service::Settings;
use red_ale_service::config::ConfigStore;
use serde_json::{Value, json};

struct Server {
    dir: tempfile::TempDir,
    d: Dispatcher,
}

impl Server {
    /// A dispatcher over a Configuration file holding `text`.
    fn with(text: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("red-ale.json"), text).unwrap();
        let d = Dispatcher::new(Settings { config: ConfigStore::at(dir.path().join("red-ale.json")), ..Settings::default() });
        Self { dir, d }
    }

    fn file(&self) -> PathBuf {
        self.dir.path().join("red-ale.json")
    }

    fn text(&self) -> String {
        std::fs::read_to_string(self.file()).unwrap()
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        self.d.call(method, params.clone()).err().unwrap_or_else(|| panic!("{method} {params} should fail"))
    }

    fn set(&mut self, changes: Value) -> Value {
        self.ok("config.set", json!({ "changes": changes }))
    }

    fn get(&mut self, path: &str) -> Value {
        self.ok("config.get", json!({ "path": path }))["value"].clone()
    }
}

#[test]
fn the_schema_declares_every_setting() {
    let mut s = Server::with("{}");
    let schema = s.ok("config.schema", json!({}));
    assert_eq!(schema["$schema"], "https://json-schema.org/draft/2020-12/schema");
    let top: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
    assert_eq!(top, ["accountingPeriod", "docLinks", "reports", "locale", "defaultCurrency", "jsonRpc", "mcp", "import", "quotes"]);
    assert_eq!(schema["properties"]["locale"]["enum"], json!(["C", "environment"]));
    let key = &schema["properties"]["quotes"]["properties"]["services"]["additionalProperties"]["properties"]["key"];
    assert_eq!((key["writeOnly"].clone(), key["x-refusedOverMcp"].clone()), (json!(true), json!(true)));
    assert_eq!(schema["properties"]["quotes"]["properties"]["network"]["x-refusedOverMcp"], true);
    assert_eq!(schema["properties"]["reports"]["properties"]["saved"]["readOnly"], true);
}

/// The report settings declared in the Configuration are the ones `red-ale-report` reads, with its defaults.
#[test]
fn report_declarations_match_the_report_context() {
    let schema = red_ale_service::config::schema::SCHEMA.clone();
    let reports = &schema["properties"]["reports"]["properties"];
    let declared: Vec<(String, u64)> =
        reports["limits"]["properties"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v["default"].as_u64().unwrap())).collect();
    let wanted: Vec<(String, u64)> = Limit::all()
        .map(|l| (if l == Limit::Time { "seconds".to_owned() } else { l.as_str().to_owned() }, l.default_max()))
        .collect();
    assert_eq!(declared, wanted);
    ["reverseBalancedAccounts", "dateFormat", "weekStart"].iter().for_each(|setting| {
        reports[setting]["enum"].as_array().unwrap().iter().for_each(|choice| {
            let read: Result<ReportSettings, _> = serde_json::from_value(json!({ *setting: choice }));
            assert!(read.is_ok(), "{setting} {choice}");
        });
    });
    let defaults = json!({ "currency": reports["currency"]["default"], "reverseBalancedAccounts": reports["reverseBalancedAccounts"]["default"],
        "dateFormat": reports["dateFormat"]["default"], "weekStart": reports["weekStart"]["default"] });
    assert_eq!(serde_json::from_value::<ReportSettings>(defaults).unwrap(), ReportSettings::default());
}

#[test]
fn get_fills_in_defaults_and_names_unknown_keys_and_problems() {
    let mut s = Server::with(r#"{"other": 1, "import": {"settings": {"matchThreshold": 2, "autoAddThreshold": -1}}}"#);
    let all = s.ok("config.get", json!({}));
    assert_eq!(all["path"], Value::Null);
    assert_eq!(all["value"]["import"]["settings"]["matchThreshold"], 2);
    assert_eq!(all["value"]["import"]["settings"]["enableSkip"], true);
    assert_eq!(all["value"]["mcp"]["maxRequestBytes"], 64 << 20);
    assert_eq!(all["unknown"], json!(["other"]));
    assert_eq!(all["problems"], json!([{ "path": "import.settings.autoAddThreshold", "message": "must be at least 0" }]));
    assert_eq!(all["configFile"], s.file().display().to_string());
    let one = s.ok("config.get", json!({ "path": "import" }));
    assert_eq!((one["unknown"].clone(), one["problems"].as_array().unwrap().len()), (json!([]), 1));
    assert_eq!(s.get("quotes.sources.nyse"), json!(["alphavantage", "twelvedata"]));
    assert_eq!(s.err("config.get", json!({ "path": "nosuch.setting" })).code, codes::INVALID_PARAMS);
    assert_eq!(s.err("config.get", json!({ "path": "import." })).code, codes::INVALID_PARAMS);
}

#[test]
fn set_writes_the_file_needs_no_book_and_null_restores_defaults() {
    let mut s = Server::with(r#"{"other": {"kept": true}}"#);
    let set = s.set(json!({ "import.settings.matchThreshold": 4, "locale": "environment", "reports.dateFormat": "iso" }));
    assert_eq!(set["values"], json!({ "import.settings.matchThreshold": 4, "locale": "environment", "reports.dateFormat": "iso" }));
    assert_eq!(set["clearedKeys"], json!([]));
    let file: Value = serde_json::from_str(&s.text()).unwrap();
    assert_eq!(file, json!({ "other": { "kept": true }, "import": { "settings": { "matchThreshold": 4 } }, "locale": "environment", "reports": { "dateFormat": "iso" } }));
    s.set(json!({ "import.settings.matchThreshold": null, "quotes.sources": { "tsp": ["frankfurter"] } }));
    assert_eq!(s.get("import.settings.matchThreshold"), 1);
    assert_eq!(s.get("quotes.sources.tsp"), json!(["frankfurter"]));
    assert_eq!(s.get("quotes.sources.currency"), json!(["frankfurter", "alphavantage"]), "a whole group merges over its defaults");
    let file: Value = serde_json::from_str(&s.text()).unwrap();
    assert!(file.get("import").is_none(), "an emptied group is removed: {file}");
    assert_eq!(file["other"]["kept"], true, "unknown keys are kept");
}

#[test]
fn a_failed_set_changes_nothing() {
    let text = "{\n  \"import\": { \"settings\": { \"matchThreshold\": 2 } }\n}\n";
    let mut s = Server::with(text);
    let e = s.err("config.set", json!({ "changes": { "locale": "environment", "import.settings.matchThreshold": "two", "quotes.network": "yes" } }));
    assert_eq!(e.code, codes::INVALID_PARAMS);
    let paths: Vec<&str> = e.data.as_ref().unwrap()["problems"].as_array().unwrap().iter().map(|p| p["path"].as_str().unwrap()).collect();
    assert_eq!(paths, ["import.settings.matchThreshold", "quotes.network"]);
    assert_eq!(s.text(), text, "the file is untouched, byte for byte");
    assert_eq!(s.err("config.set", json!({ "changes": {} })).code, codes::INVALID_PARAMS);
    assert_eq!(s.err("config.set", json!({ "changes": { "reports.saved": [] } })).code, codes::INVALID_PARAMS);
    assert_eq!(s.err("config.set", json!({ "changes": { "nosuch": 1 } })).code, codes::INVALID_PARAMS);
    assert_eq!(s.err("config.set", json!({ "changes": { "quotes.sources.usa": ["nosuch"] } })).code, codes::INVALID_PARAMS);
    assert_eq!(s.text(), text);
    let entries: Vec<_> = std::fs::read_dir(s.dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1, "no temporary file is left");
}

#[test]
fn keys_never_appear_in_results_or_errors() {
    let mut s = Server::with("{}");
    let set = s.set(json!({ "quotes.services.twelvedata.key": "SECRET-TD" }));
    assert_eq!(set["values"]["quotes.services.twelvedata.key"], json!({ "set": true }));
    let all = s.ok("config.get", json!({}));
    assert!(!all.to_string().contains("SECRET-TD"));
    assert_eq!(s.get("quotes.services.twelvedata")["key"], json!({ "set": true }));
    let e = s.err("config.set", json!({ "changes": { "quotes.services.finnhub.key": 987_654 } }));
    assert!(!format!("{e:?}").contains("987654"), "{e:?}");
    assert!(s.text().contains("SECRET-TD"), "the key is kept in the file");
}

#[test]
fn a_key_is_cleared_when_its_service_moves_host() {
    let mut s = Server::with(r#"{"quotes": {"services": {"finnhub": {"key": "F"}}}}"#);
    let moved = s.set(json!({ "quotes.services.finnhub.securityUrl": "https://elsewhere.example/q?s={symbol}&t={key}" }));
    assert_eq!(moved["clearedKeys"], json!(["quotes.services.finnhub.key"]));
    assert_eq!(s.get("quotes.services.finnhub.key"), json!({ "set": false }));
    assert!(!s.text().contains("\"F\""));
    s.set(json!({ "quotes.services.finnhub.key": "G" }));
    let back = s.set(json!({ "quotes.services.finnhub.securityUrl": null }));
    assert_eq!(back["clearedKeys"], json!(["quotes.services.finnhub.key"]), "restoring the default URL is a move too");
}

#[test]
fn through_mcp_keys_and_the_network_switch_are_refused() {
    let mut s = Server::with("{}");
    let before = s.text();
    let e = s.d.call_from(Origin::Mcp, "config.set", json!({ "changes": { "quotes.network": true } })).unwrap_err();
    assert_eq!(e.code, codes::CONFLICT);
    assert_eq!(e.data.unwrap(), json!({ "reason": "refused-over-mcp", "paths": ["quotes.network"] }));
    assert_eq!(s.text(), before);
    s.d.call_from(Origin::Mcp, "config.set", json!({ "changes": { "locale": "environment" } })).unwrap();
    s.set(json!({ "quotes.network": true }));
    assert_eq!(s.get("quotes.network"), true, "a person may");
}

#[test]
fn the_existing_methods_are_shortcuts_to_the_same_settings() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("book.gnucash");
    let mut s = Server::with("{}");
    s.ok("book.create", json!({ "path": book.display().to_string() }));
    s.ok("period.set", json!({ "start": "previous-month", "fiscalYearEnd": "06-30" }));
    assert_eq!(s.get("accountingPeriod"), json!({ "start": "previous-month", "end": "year", "fiscalYearEnd": "06-30" }));
    s.set(json!({ "accountingPeriod.end": "2024-12-31", "docLinks.pathHead": "file:///srv/docs/" }));
    assert_eq!(s.ok("period.get", json!({}))["end"], "2024-12-31");
    assert_eq!(s.ok("docLink.pathHead", json!({}))["pathHead"], "file:///srv/docs/");
    assert_eq!(s.ok("book.info", json!({}))["dirty"], false, "the Configuration is not book data");
    assert!(s.err("config.set", json!({ "changes": { "accountingPeriod.start": "soon" } })).message.contains("accountingPeriod.start"));
    assert!(Path::new(&s.file()).exists());
}

#[test]
fn the_configuration_methods_declare_their_effects() {
    let s = Server::with("{}");
    let schema = s.d.schema();
    let method = |name: &str| schema["methods"].as_array().unwrap().iter().find(|m| m["name"] == name).unwrap().clone();
    assert_eq!((method("config.schema")["effect"].clone(), method("config.schema")["target"].clone()), (json!("read"), Value::Null));
    assert_eq!((method("config.get")["effect"].clone(), method("config.get")["target"].clone()), (json!("read"), Value::Null));
    assert_eq!((method("config.set")["effect"].clone(), method("config.set")["target"].clone()), (json!("write"), json!("configuration")));
}
