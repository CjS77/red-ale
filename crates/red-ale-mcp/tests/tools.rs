//! The MCP server over an in-memory pipe, spoken to in raw JSON-RPC as an agent's client would.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

use common::Client;
use red_ale_testkit::TempBook;
use serde_json::{Map, Value, json};

fn defs(schema: &Value) -> &Map<String, Value> {
    schema["$defs"].as_object().unwrap()
}

/// Every `$ref` in `schema`.
fn refs(schema: &Value) -> Vec<String> {
    match schema {
        Value::Object(map) => map
            .iter()
            .flat_map(|(k, v)| {
                if k == "$ref" {
                    vec![v.as_str().unwrap().to_owned()]
                } else {
                    refs(v)
                }
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(refs).collect(),
        _ => Vec::new(),
    }
}

/// `tool`'s input schema is `params` made self-contained: the definition `params` names, with the same definitions
/// beside it, every one of them resolvable inside the tool's own schema.
fn assert_same_params(name: &str, tool_schema: &Value, params: &Value, document: &Value) {
    assert_eq!(tool_schema["type"], "object", "{name}: MCP needs an object schema");
    let root = params["$ref"].as_str().unwrap().strip_prefix("#/$defs/").unwrap();
    let mut own = tool_schema.as_object().unwrap().clone();
    let own_defs = own.remove("$defs").unwrap_or_else(|| json!({}));
    assert_eq!(Value::Object(own), defs(document)[root], "{name}: the params definition");
    own_defs
        .as_object()
        .unwrap()
        .iter()
        .for_each(|(def, schema)| assert_eq!(schema, &defs(document)[def], "{name}: $defs.{def}"));
    refs(tool_schema).iter().for_each(|r| {
        let def = r
            .strip_prefix("#/$defs/")
            .unwrap_or_else(|| panic!("{name}: {r} points outside the schema"));
        assert!(own_defs.get(def).is_some(), "{name}: {r} does not resolve inside the tool's schema");
    });
}

fn expected_hints(effect: &str) -> Value {
    match effect {
        "read" => json!({ "readOnlyHint": true }),
        "write" => json!({ "readOnlyHint": false, "destructiveHint": false }),
        "destructive" => json!({ "readOnlyHint": false, "destructiveHint": true }),
        other => panic!("unknown effect {other}"),
    }
}

/// Done-when check for milestones 2 and 3: every JSON-RPC method is an MCP tool with the same name and params schema, and
/// hints that match its Effect; there are no other tools.
#[tokio::test]
async fn every_method_is_a_tool_with_its_params_schema_and_hints() {
    let document = red_ale_testkit::dispatcher().schema();
    let mut client = Client::start().await;
    let listed = client.request("tools/list", json!({})).await;
    assert!(listed["result"].get("nextCursor").is_none(), "one page");
    let tools = listed["result"]["tools"].as_array().unwrap();

    let methods = document["methods"].as_array().unwrap();
    let method_names: BTreeSet<&str> = methods.iter().map(|m| m["name"].as_str().unwrap()).chain(["api.schema"]).collect();
    let tool_names: BTreeSet<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(tool_names, method_names, "one tool per method, and nothing else");
    assert_eq!(tools.len(), tool_names.len(), "no tool is listed twice");

    let tool = |name: &str| tools.iter().find(|t| t["name"] == name).unwrap();
    methods.iter().for_each(|m| {
        let name = m["name"].as_str().unwrap();
        let t = tool(name);
        let description = t["description"].as_str().unwrap();
        assert!(description.starts_with(m["summary"].as_str().unwrap()), "{name}: {description}");
        assert!(
            description.ends_with(&format!("Manual: {}", m["help"].as_str().unwrap())),
            "{name}: {description}"
        );
        assert_eq!(t["annotations"], expected_hints(m["effect"].as_str().unwrap()), "{name}");
        assert_same_params(name, &t["inputSchema"], &m["params"], &document);
    });
    // Writing the Configuration changes the server's environment, so those tools are not read-only either.
    assert_eq!(tool("report.saveConfiguration")["annotations"], expected_hints("write"));
    assert_eq!(tool("report.deleteStyleSheet")["annotations"], expected_hints("destructive"));
    assert_eq!(tool("report.export")["annotations"], expected_hints("read"));
    // The hints milestone 3 names for the methods v0.3 adds.
    ["import.apply", "importMap.delete"]
        .iter()
        .for_each(|name| assert_eq!(tool(name)["annotations"], expected_hints("destructive"), "{name}"));
    [
        "import.preview",
        "export.csvTree",
        "export.csvTransactions",
        "export.accounts",
        "config.get",
        "config.schema",
        "price.fetchStatus",
    ]
    .iter()
    .for_each(|name| assert_eq!(tool(name)["annotations"], expected_hints("read"), "{name}"));
    ["config.set", "price.fetchQuotes"]
        .iter()
        .for_each(|name| assert_eq!(tool(name)["annotations"], expected_hints("write"), "{name}"));
    // The hints milestone 4 names for the budget methods.
    ["budget.delete", "budget.estimate", "budget.scrubSigns"]
        .iter()
        .for_each(|name| assert_eq!(tool(name)["annotations"], expected_hints("destructive"), "{name}"));
    ["budget.get", "budget.list"]
        .iter()
        .for_each(|name| assert_eq!(tool(name)["annotations"], expected_hints("read"), "{name}"));
    [
        "budget.create",
        "budget.update",
        "budget.copy",
        "budget.setAmounts",
        "budget.setNotes",
    ]
    .iter()
    .for_each(|name| assert_eq!(tool(name)["annotations"], expected_hints("write"), "{name}"));
    // And for Since Last Run (`design/api/scheduled-transactions.md`, "Methods and MCP").
    ["scheduledTransaction.sinceLastRun", "scheduledTransaction.discard"]
        .iter()
        .for_each(|name| assert_eq!(tool(name)["annotations"], expected_hints("read"), "{name}"));
    assert_eq!(tool("scheduledTransaction.apply")["annotations"], expected_hints("write"));
    // And for the rest of the scheduled transaction methods.
    assert_eq!(tool("scheduledTransaction.delete")["annotations"], expected_hints("destructive"));
    [
        "scheduledTransaction.get",
        "scheduledTransaction.list",
        "scheduledTransaction.occurrences",
    ]
    .iter()
    .for_each(|name| assert_eq!(tool(name)["annotations"], expected_hints("read"), "{name}"));
    [
        "scheduledTransaction.create",
        "scheduledTransaction.update",
        "scheduledTransaction.fromTransaction",
        "scheduledTransaction.loan",
    ]
    .iter()
    .for_each(|name| assert_eq!(tool(name)["annotations"], expected_hints("write"), "{name}"));
    let schema_tool = tool("api.schema");
    assert_eq!(schema_tool["annotations"], expected_hints("read"));
    assert_eq!(schema_tool["inputSchema"]["type"], "object");

    let schema = client.call("api.schema", json!({})).await;
    assert_eq!(schema["structuredContent"]["redAleApi"], document["redAleApi"]);
}

#[tokio::test]
async fn an_agent_creates_a_book_adds_an_account_and_saves() {
    let book = TempBook::new_path();
    let mut client = Client::start().await;

    let created = client.call("book.create", json!({ "path": book.path_str() })).await;
    assert_eq!(created["isError"], false, "{created}");
    assert_eq!(created["structuredContent"]["counts"]["accounts"], 1);
    let text: Value = serde_json::from_str(created["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, created["structuredContent"], "the text block holds the same JSON");

    let account = client
        .call("account.create", json!({ "name": "Assets", "type": "ASSET", "commodity": "USD" }))
        .await;
    assert_eq!(account["isError"], false, "{account}");
    assert_eq!(account["structuredContent"]["fullName"], "Assets");

    let saved = client.call("book.save", json!({})).await;
    assert_eq!(saved["isError"], false, "{saved}");
    let mut reader = red_ale_testkit::dispatcher();
    red_ale_testkit::call(&mut reader, "book.open", &json!({ "path": book.path_str(), "readOnly": true }));
    let accounts = red_ale_testkit::call(&mut reader, "account.list", &json!({}));
    assert!(
        accounts.to_string().contains("\"fullName\":\"Assets\""),
        "the saved file holds the account: {accounts}"
    );

    let info = client.request("tools/call", json!({ "name": "book.info" })).await;
    assert_eq!(
        info["result"]["structuredContent"]["dirty"], false,
        "absent arguments read as no params: {info}"
    );
}

#[tokio::test]
async fn errors_are_tool_results_with_red_ales_error_object() {
    let mut client = Client::start().await;

    let no_book = client.call("account.list", json!({})).await;
    assert_eq!(no_book["isError"], true);
    assert_eq!(no_book["structuredContent"]["error"]["code"], 1000);
    assert_eq!(no_book["structuredContent"]["error"]["name"], "no-book");
    let text: Value = serde_json::from_str(no_book["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, no_book["structuredContent"]);

    let typo = client.call("book.create", json!({ "path": "x.gnucash", "compresed": false })).await;
    assert_eq!(
        typo["isError"], true,
        "invalid params are shown to the model, which can correct them"
    );
    assert_eq!(typo["structuredContent"]["error"]["name"], "invalid-params");

    let unknown = client
        .request("tools/call", json!({ "name": "no.such.tool", "arguments": {} }))
        .await;
    assert_eq!(unknown["error"]["code"], -32602, "an unknown tool is a protocol error: {unknown}");
}

/// Keys and the network switch are a person's to change: `config.set` through MCP refuses them and writes nothing,
/// while a service's URL may change, which clears the key bound to the old host.
#[tokio::test]
async fn config_set_through_mcp_refuses_keys_and_the_network_switch() {
    let mut dispatcher = red_ale_testkit::dispatcher();
    dispatcher
        .call(
            "config.set",
            json!({ "changes": { "quotes.services.alphavantage.key": "SECRET-AV" } }),
        )
        .unwrap();
    let mut client = Client::start_with(dispatcher).await;
    let refused = |result: &Value| {
        assert_eq!(result["isError"], true, "{result}");
        assert_eq!(result["structuredContent"]["error"]["name"], "conflict", "{result}");
        result["structuredContent"]["error"]["data"].clone()
    };

    let network = client.call("config.set", json!({ "changes": { "quotes.network": true } })).await;
    assert_eq!(
        refused(&network),
        json!({ "reason": "refused-over-mcp", "paths": ["quotes.network"], "help": "error/conflict" })
    );
    let key = client
        .call(
            "config.set",
            json!({ "changes": { "quotes.services.finnhub.key": "MINE", "locale": "environment" } }),
        )
        .await;
    assert_eq!(refused(&key)["paths"], json!(["quotes.services.finnhub.key"]));
    let clear = client
        .call("config.set", json!({ "changes": { "quotes.services.alphavantage.key": null } }))
        .await;
    assert_eq!(
        refused(&clear)["paths"],
        json!(["quotes.services.alphavantage.key"]),
        "clearing a key is a change too"
    );
    let whole = client.call("config.set", json!({ "changes": { "quotes.services.alphavantage": { "securityUrl": "https://www.alphavantage.co/x", "format": "alphavantage" } } })).await;
    assert_eq!(
        refused(&whole)["paths"],
        json!(["quotes.services.alphavantage.key"]),
        "replacing a service drops its key"
    );

    let got = client.call("config.get", json!({})).await;
    let value = &got["structuredContent"]["value"];
    assert_eq!(value["quotes"]["network"], false);
    assert_eq!(value["locale"], "C", "a refused call writes nothing");
    assert_eq!(value["quotes"]["services"]["alphavantage"]["key"], json!({ "set": true }));
    assert!(!got.to_string().contains("SECRET-AV"), "no key in any result");

    let moved = client
        .call(
            "config.set",
            json!({ "changes": { "quotes.services.alphavantage.securityUrl": "https://evil.example/q?k={key}" } }),
        )
        .await;
    assert_eq!(moved["isError"], false, "{moved}");
    assert_eq!(
        moved["structuredContent"]["clearedKeys"],
        json!(["quotes.services.alphavantage.key"])
    );
    let key = client
        .call("config.get", json!({ "path": "quotes.services.alphavantage.key" }))
        .await;
    assert_eq!(key["structuredContent"]["value"], json!({ "set": false }));
    let ok = client
        .call(
            "config.set",
            json!({ "changes": { "quotes.network": false, "import.settings.useBayes": false } }),
        )
        .await;
    assert_eq!(ok["isError"], false, "setting the switch to what it is changes nothing: {ok}");
}

#[tokio::test]
async fn an_agent_imports_accounts_and_prices_from_csv() {
    let book = TempBook::new_path();
    let mut client = Client::start().await;
    let created = client.call("book.create", json!({ "path": book.path_str() })).await;
    assert_eq!(created["isError"], false, "{created}");

    let accounts = "\"Type\",\"Full Account Name\",\"Account Name\",\"Account Code\",\"Description\",\"Account Color\",\"Notes\",\
                    \"Symbol\",\"Namespace\",\"Hidden\",\"Tax Info\",\"Placeholder\"\n\
                    ASSET,Assets,Assets,,,,,USD,CURRENCY,F,F,T\nBANK,Assets:Checking,Checking,100,,,,USD,CURRENCY,F,F,F\n";
    let preview = client
        .call(
            "import.preview",
            json!({ "format": "csvAccounts", "files": [{ "text": accounts }] }),
        )
        .await;
    assert_eq!(preview["isError"], false, "{preview}");
    let proposal = &preview["structuredContent"];
    assert_eq!(proposal["accounts"].as_array().map(Vec::len), Some(2), "{proposal}");
    let applied = client.call("import.apply", json!({ "proposal": proposal["id"] })).await;
    assert_eq!(applied["isError"], false, "{applied}");

    let commodity = json!({ "namespace": "NASDAQ", "mnemonic": "ACME", "fullname": "Acme Corp", "fraction": 1000 });
    assert_eq!(client.call("commodity.create", commodity).await["isError"], false);
    let layout = json!({ "fromCommodity": "NASDAQ::ACME", "toCurrency": "USD", "columns": ["Date", "Amount"] });
    let prices = "2024-03-01,10.5\n2024-03-04,10.75\n";
    let preview = client
        .call(
            "import.preview",
            json!({ "format": "csvPrices", "files": [{ "text": prices }], "layout": layout }),
        )
        .await;
    assert_eq!(preview["isError"], false, "{preview}");
    let proposal = &preview["structuredContent"];
    assert_eq!(proposal["prices"].as_array().map(Vec::len), Some(2), "{proposal}");
    let applied = client.call("import.apply", json!({ "proposal": proposal["id"] })).await;
    assert_eq!(applied["isError"], false, "{applied}");
    assert_eq!(
        applied["structuredContent"]["prices"]["added"].as_array().map(Vec::len),
        Some(2),
        "{applied}"
    );
}
