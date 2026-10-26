//! The dispatcher's framing, error codes and book methods.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_rpc::{Dispatcher, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

fn fixture(name: &str) -> String {
    format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn copy(dir: &tempfile::TempDir, name: &str) -> String {
    let dest = dir.path().join("book.gnucash");
    std::fs::copy(fixture(name), &dest).unwrap();
    dest.display().to_string()
}

#[test]
fn json_rpc_framing() {
    let mut d = Dispatcher::new(Settings::default());
    let reply: Value = serde_json::from_str(&d.handle_text(r#"{"jsonrpc":"2.0","id":1,"method":"book.info"}"#).unwrap()).unwrap();
    assert_eq!(reply["error"]["code"], codes::NO_BOOK);
    assert_eq!(reply["id"], 1);
    assert!(d.handle_text(r#"{"jsonrpc":"2.0","method":"book.info"}"#).is_none(), "notifications get no reply");
    let reply: Value = serde_json::from_str(&d.handle_text("{not json").unwrap()).unwrap();
    assert_eq!(reply["error"]["code"], codes::PARSE_ERROR);
    let reply: Value = serde_json::from_str(&d.handle_text(r#"[{"jsonrpc":"2.0","id":"a","method":"nope"},{"jsonrpc":"1.0","id":2,"method":"x"}]"#).unwrap()).unwrap();
    assert_eq!(reply[0]["error"]["code"], codes::METHOD_NOT_FOUND);
    assert_eq!(reply[1]["error"]["code"], codes::INVALID_REQUEST);
    let reply: Value = serde_json::from_str(&d.handle_text(r#"{"jsonrpc":"2.0","id":3,"method":"book.open","params":[1]}"#).unwrap()).unwrap();
    assert_eq!(reply["error"]["code"], codes::INVALID_PARAMS);
}

#[test]
fn open_info_save_close() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy(&dir, "synthetic/rich.gnucash");
    let mut d = Dispatcher::new(Settings::default());
    let info = d.call("book.open", json!({ "path": path })).unwrap();
    assert_eq!(info["readOnly"], false);
    assert_eq!(d.call("book.open", json!({ "path": path })).unwrap_err().code, codes::BOOK_ALREADY_OPEN);
    assert_eq!(d.call("book.open", json!({ "path": path, "bogus": 1 })).unwrap_err().code, codes::INVALID_PARAMS);
    let saved = d.call("book.save", json!({})).unwrap();
    assert!(saved["backup"].is_string());
    assert_eq!(d.call("book.close", json!({})).unwrap(), json!({ "saved": false, "discarded": false }));
    assert_eq!(d.call("book.info", json!({})).unwrap_err().code, codes::NO_BOOK);
}

#[test]
fn refusals_have_stable_codes() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = Dispatcher::new(Settings::default());
    let sqlite = copy(&dir, "piecash/simple_sample.sqlite.gnucash");
    assert_eq!(d.call("book.open", json!({ "path": sqlite })).unwrap_err().code, codes::SQL_BOOK);
    assert_eq!(d.call("book.open", json!({ "path": format!("sqlite3://{sqlite}") })).unwrap_err().code, codes::SQL_BOOK);
    assert_eq!(d.call("book.open", json!({ "path": "https://example.com/book" })).unwrap_err().code, codes::UNSUPPORTED_URI);
    assert_eq!(d.call("book.open", json!({ "path": dir.path().join("missing").display().to_string() })).unwrap_err().code, codes::IO_ERROR);
    assert_eq!(d.call("book.open", json!({ "path": fixture("gnucash/currency_tree_xml.gnucash"), "readOnly": true })).unwrap_err().code, codes::FILE_FORMAT);
    assert_eq!(d.call("book.open", json!({})).unwrap_err().code, codes::INVALID_PARAMS);

    let locked = dir.path().join("locked.gnucash");
    std::fs::copy(fixture("synthetic/rich.gnucash"), &locked).unwrap();
    let mut other = Dispatcher::new(Settings::default());
    other.call("book.open", json!({ "path": locked.display().to_string() })).unwrap();
    assert_eq!(d.call("book.open", json!({ "path": locked.display().to_string() })).unwrap_err().code, codes::BOOK_LOCKED);
    let ro = d.call("book.open", json!({ "path": locked.display().to_string(), "readOnly": true })).unwrap();
    assert_eq!(ro["readOnlyReasons"][0]["code"], "requested");
    assert_eq!(d.call("book.save", json!({})).unwrap_err().code, codes::READ_ONLY);
}

#[test]
fn schema_lists_every_method_with_its_effect() {
    let d = Dispatcher::new(Settings::default());
    let schema = d.schema();
    assert_eq!(schema["redAleApi"], red_ale_rpc::API_VERSION);
    let open = schema["methods"].as_array().unwrap().iter().find(|m| m["name"] == "book.open").unwrap();
    assert_eq!(open["effect"], "read");
    assert!(schema["errors"].as_array().unwrap().iter().any(|e| e["name"] == "sql-book"));
}

/// Every method declares its Effect target: none for a read, the Configuration for the settings red-ale keeps
/// outside the Book, the Book for everything else (`design/api.md`, "Effects").
#[test]
fn schema_lists_every_method_with_its_effect_target() {
    let schema = Dispatcher::new(Settings::default()).schema();
    let methods = schema["methods"].as_array().unwrap();
    let configuration = ["period.set", "docLink.setPathHead", "config.set"];
    methods.iter().for_each(|m| {
        let name = m["name"].as_str().unwrap();
        let expected = match m["effect"].as_str().unwrap() {
            "read" => Value::Null,
            _ if configuration.contains(&name) || name.starts_with("report.") => json!("configuration"),
            _ => json!("book"),
        };
        assert_eq!(m["target"], expected, "{name}");
    });
}
