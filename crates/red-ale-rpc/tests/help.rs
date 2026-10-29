//! The `help.*` methods and `data.help` on errors (`design/api/help.md`), with no book open.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_rpc::{Dispatcher, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

fn call(method: &str, params: Value) -> Value {
    Dispatcher::new(Settings::default())
        .call(method, params.clone())
        .unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
}

fn ids(list: &Value) -> Vec<&str> {
    list.as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect()
}

#[test]
fn topics_list_every_page_by_id() {
    let all = call("help.topics", Value::Null);
    let topics = ids(&all["topics"]);
    assert!(topics.contains(&"start") && topics.contains(&"method/transaction.post") && topics.contains(&"error/unbalanced"));
    let mut sorted = topics.clone();
    sorted.sort_unstable();
    assert_eq!(topics, sorted);
    let errors = call("help.topics", json!({ "kind": "error" }));
    let errors = errors["topics"].as_array().unwrap();
    assert_eq!(errors.len(), codes::ALL.len());
    assert!(errors.iter().all(|t| t["kind"] == "error"));
}

#[test]
fn get_renders_a_page_for_each_door() {
    let page = call("help.get", json!({ "id": "transaction.post" }));
    assert_eq!(page["id"], "method/transaction.post");
    assert_eq!(
        (page["format"].as_str(), page["door"].as_str()),
        (Some("markdown"), Some("jsonrpc"))
    );
    let content = page["content"].as_str().unwrap();
    assert!(content.starts_with("# transaction.post\n"), "{content}");
    assert!(content.contains("\"jsonrpc\":\"2.0\""), "{content}");
    assert!(content.contains("legal/gfdl"), "every served Page ends with the licence notice");
    let cli = call(
        "help.get",
        json!({ "id": "method/transaction.post", "door": "cli", "format": "text" }),
    );
    let text = cli["content"].as_str().unwrap();
    assert!(text.contains("red-ale exec --book book.gnucash --script - <<'EOF'"), "{text}");
    assert!(!text.contains("```"), "{text}");
    let mcp = call("help.get", json!({ "id": "method/transaction.post", "door": "mcp" }));
    assert!(mcp["content"].as_str().unwrap().contains("\"name\": \"unbalanced\""));
}

#[test]
fn get_refuses_an_unknown_page_and_points_at_help() {
    let e = Dispatcher::new(Settings::default())
        .call("help.get", json!({ "id": "concept/nothing" }))
        .unwrap_err();
    assert_eq!(e.code, codes::NOT_FOUND);
    assert_eq!(
        e.data,
        Some(json!({ "kind": "page", "id": "concept/nothing", "help": "error/not-found" }))
    );
}

#[test]
fn search_ranks_pages_for_a_query() {
    let hits = call("help.search", json!({ "query": "post a transaction", "limit": 3 }));
    let found = ids(&hits["hits"]);
    assert_eq!(found.len(), 3);
    assert_eq!(found[0], "method/transaction.post");
    let errors = call("help.search", json!({ "query": "balance", "kind": "error" }));
    assert!(errors["hits"].as_array().unwrap().iter().all(|h| h["kind"] == "error"));
    assert_eq!(
        call("help.search", json!({ "query": "post a transaction", "limit": 3 })),
        hits,
        "deterministic"
    );
    let e = Dispatcher::new(Settings::default())
        .call("help.search", json!({ "query": "x", "limit": 0 }))
        .unwrap_err();
    assert_eq!(e.code, codes::INVALID_PARAMS);
}

#[test]
fn gnucash_words_find_red_ale_pages() {
    // No Page says "TransLog"; the glossary records it as GnuCash's name for the journal.
    let hits = call("help.search", json!({ "query": "TransLog" }));
    let found = ids(&hits["hits"]);
    assert!(found.contains(&"method/book.replayJournal"), "{hits}");
}

#[test]
fn every_error_names_its_page() {
    let mut d = Dispatcher::new(Settings::default());
    let e = d.call("book.info", Value::Null).unwrap_err();
    assert_eq!(e.data, Some(json!({ "help": "error/no-book" })));
    let e = d.call("no.such", Value::Null).unwrap_err();
    assert_eq!(e.data, Some(json!({ "help": "error/method-not-found" })));
    let reply: Value = serde_json::from_str(&d.handle_text("{\"jsonrpc\": \"2.0\", \"id\": 1}").unwrap()).unwrap();
    assert_eq!(reply["error"]["data"], json!({ "help": "error/invalid-request" }));
}

#[test]
fn the_schema_names_each_methods_page() {
    let schema = Dispatcher::new(Settings::default()).schema();
    assert_eq!(schema["redAleApi"], "0.6.0");
    let methods = schema["methods"].as_array().unwrap();
    assert!(
        methods
            .iter()
            .all(|m| m["help"] == format!("method/{}", m["name"].as_str().unwrap()))
    );
    let help: Vec<&Value> = methods
        .iter()
        .filter(|m| m["name"].as_str().unwrap().starts_with("help."))
        .collect();
    assert_eq!(help.len(), 3);
    assert!(help.iter().all(|m| m["effect"] == "read" && m["target"].is_null()));
}
