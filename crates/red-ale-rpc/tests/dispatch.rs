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
    assert!(
        d.handle_text(r#"{"jsonrpc":"2.0","method":"book.info"}"#).is_none(),
        "notifications get no reply"
    );
    let reply: Value = serde_json::from_str(&d.handle_text("{not json").unwrap()).unwrap();
    assert_eq!(reply["error"]["code"], codes::PARSE_ERROR);
    let reply: Value = serde_json::from_str(
        &d.handle_text(r#"[{"jsonrpc":"2.0","id":"a","method":"nope"},{"jsonrpc":"1.0","id":2,"method":"x"}]"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reply[0]["error"]["code"], codes::METHOD_NOT_FOUND);
    assert_eq!(reply[1]["error"]["code"], codes::INVALID_REQUEST);
    let reply: Value = serde_json::from_str(
        &d.handle_text(r#"{"jsonrpc":"2.0","id":3,"method":"book.open","params":[1]}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reply["error"]["code"], codes::INVALID_PARAMS);
}

#[test]
fn open_info_save_close() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy(&dir, "synthetic/rich.gnucash");
    let mut d = Dispatcher::new(Settings::default());
    let info = d.call("book.open", json!({ "path": path })).unwrap();
    assert_eq!(info["readOnly"], false);
    assert_eq!(
        d.call("book.open", json!({ "path": path })).unwrap_err().code,
        codes::BOOK_ALREADY_OPEN
    );
    assert_eq!(
        d.call("book.open", json!({ "path": path, "bogus": 1 })).unwrap_err().code,
        codes::INVALID_PARAMS
    );
    let saved = d.call("book.save", json!({})).unwrap();
    assert!(saved["backup"].is_string());
    assert_eq!(
        d.call("book.close", json!({})).unwrap(),
        json!({ "saved": false, "discarded": false })
    );
    assert_eq!(d.call("book.info", json!({})).unwrap_err().code, codes::NO_BOOK);
}

#[test]
fn refusals_have_stable_codes() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = Dispatcher::new(Settings::default());
    assert_eq!(
        d.call("book.open", json!({ "path": "postgres://user@host/db" })).unwrap_err().code,
        codes::SQL_BOOK
    );
    let xml = copy(&dir, "synthetic/rich.gnucash");
    assert_eq!(
        d.call("book.open", json!({ "path": format!("sqlite3://{xml}") })).unwrap_err().code,
        codes::FILE_FORMAT
    );
    assert_eq!(
        d.call("book.open", json!({ "path": "https://example.com/book" })).unwrap_err().code,
        codes::UNSUPPORTED_URI
    );
    assert_eq!(
        d.call("book.open", json!({ "path": dir.path().join("missing").display().to_string() }))
            .unwrap_err()
            .code,
        codes::IO_ERROR
    );
    assert_eq!(
        d.call(
            "book.open",
            json!({ "path": fixture("gnucash/currency_tree_xml.gnucash"), "readOnly": true })
        )
        .unwrap_err()
        .code,
        codes::FILE_FORMAT
    );
    assert_eq!(d.call("book.open", json!({})).unwrap_err().code, codes::INVALID_PARAMS);

    let locked = dir.path().join("locked.gnucash");
    std::fs::copy(fixture("synthetic/rich.gnucash"), &locked).unwrap();
    let mut other = Dispatcher::new(Settings::default());
    other.call("book.open", json!({ "path": locked.display().to_string() })).unwrap();
    assert_eq!(
        d.call("book.open", json!({ "path": locked.display().to_string() }))
            .unwrap_err()
            .code,
        codes::BOOK_LOCKED
    );
    let ro = d
        .call("book.open", json!({ "path": locked.display().to_string(), "readOnly": true }))
        .unwrap();
    assert_eq!(ro["readOnlyReasons"][0]["code"], "requested");
    assert_eq!(d.call("book.save", json!({})).unwrap_err().code, codes::READ_ONLY);
}

#[test]
fn sqlite_books_open_for_writing_locked_and_say_their_format() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy(&dir, "sqlite/synthetic/rich.gnucash");
    let mut d = Dispatcher::new(Settings::default());
    let info = d.call("book.open", json!({ "path": format!("sqlite3://{path}") })).unwrap();
    assert_eq!(
        json!([info["readOnly"], info["format"], info["compressed"]]),
        json!([false, "sqlite", false])
    );
    assert_eq!(info["versions"][0], json!({ "table": "Gnucash", "version": 5_000_017 }));
    assert_eq!(info["versions"].as_array().unwrap().len(), 24);
    let err = d.call("book.save", json!({ "compressed": true })).unwrap_err();
    assert_eq!(err.code, codes::INVALID_PARAMS);
    let saved = d.call("book.save", json!({})).unwrap();
    assert_eq!(
        json!([saved["backup"], saved["compressed"]]),
        json!([null, false]),
        "nothing to write makes no backup"
    );

    let mut other = Dispatcher::new(Settings::default());
    let err = other.call("book.open", json!({ "path": path })).unwrap_err();
    assert_eq!(err.code, codes::BOOK_LOCKED);
    let data = err.data.unwrap();
    assert_eq!(data["path"], json!(path));
    assert_eq!(data["holder"]["pid"], json!(std::process::id()));
    assert!(data["holder"]["hostname"].is_string());
    d.call("book.close", json!({})).unwrap();
    other.call("book.open", json!({ "path": path })).unwrap();
    other.call("book.close", json!({})).unwrap();

    let piecash = tempfile::tempdir().unwrap();
    let path = copy(&piecash, "piecash/simple_sample.sqlite.gnucash");
    let info = d.call("book.open", json!({ "path": path, "readOnly": true })).unwrap();
    let codes: Vec<&str> = info["readOnlyReasons"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["code"].as_str())
        .collect();
    assert!(codes.starts_with(&["requested"]) && codes.contains(&"schema-version"), "{codes:?}");
}

#[test]
fn schema_lists_every_method_with_its_effect() {
    let d = Dispatcher::new(Settings::default());
    let schema = d.schema();
    assert_eq!(schema["redAleApi"], red_ale_rpc::API_VERSION);
    let open = schema["methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "book.open")
        .unwrap();
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

#[test]
fn book_create_makes_sqlite_books_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.gnucash").display().to_string();
    let mut d = Dispatcher::new(Settings::default());
    let refused = [
        json!({ "path": path, "format": "sqlite", "compressed": true }),
        json!({ "path": format!("sqlite3://{path}"), "format": "xml" }),
        json!({ "path": format!("xml://{path}"), "format": "sqlite" }),
    ];
    refused.iter().for_each(|params| {
        assert_eq!(
            d.call("book.create", params.clone()).unwrap_err().code,
            codes::INVALID_PARAMS,
            "{params}"
        );
    });
    assert!(!std::path::Path::new(&path).exists());
    let info = d.call("book.create", json!({ "path": path, "format": "sqlite" })).unwrap();
    assert_eq!(
        json!([info["format"], info["readOnly"], info["dirty"]]),
        json!(["sqlite", false, false])
    );
    assert_eq!(info["features"][0]["name"], "ISO-8601 formatted date strings in SQLite3 databases.");
    d.call("book.close", json!({})).unwrap();
    assert_eq!(d.call("book.create", json!({ "path": path })).unwrap_err().code, codes::FILE_EXISTS);
    let info = d.call("book.open", json!({ "path": path })).unwrap();
    assert_eq!(json!([info["format"], info["readOnly"]]), json!(["sqlite", false]));
    d.call("book.close", json!({})).unwrap();
    let uri = dir.path().join("uri.gnucash").display().to_string();
    let info = d.call("book.create", json!({ "path": format!("sqlite3://{uri}") })).unwrap();
    assert_eq!(info["format"], "sqlite");
}
