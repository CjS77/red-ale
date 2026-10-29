//! The Manual through MCP: every Page a resource, the method template, the instructions from the Start Page, and
//! tool descriptions from the Reference pages within their budget.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use common::Client;
use red_ale_docs::{Door, Format};
use red_ale_mcp::manual::DESCRIPTION_BUDGET;
use red_ale_mcp::{McpServer, ToolDescriptions};
use red_ale_rpc::manual::manual;
use serde_json::{Value, json};

#[tokio::test]
async fn every_page_is_a_markdown_resource() {
    let mut client = Client::start().await;
    let listed = client.request("resources/list", json!({})).await;
    assert!(listed["result"].get("nextCursor").is_none(), "one page: {listed}");
    let resources = listed["result"]["resources"].as_array().unwrap();

    let uris: BTreeSet<&str> = resources.iter().map(|r| r["uri"].as_str().unwrap()).collect();
    let expected: Vec<String> = manual().pages().iter().map(|p| format!("red-ale://manual/{}", p.id)).collect();
    assert_eq!(uris, expected.iter().map(String::as_str).collect(), "one resource per Page");
    assert_eq!(resources.len(), manual().pages().len());
    resources.iter().zip(manual().pages()).for_each(|(r, page)| {
        assert_eq!(r["name"], page.id.as_str());
        assert_eq!(r["title"], page.title.as_str());
        assert_eq!(r["description"], page.summary.as_str());
        assert_eq!(r["mimeType"], "text/markdown");
    });
    assert!(uris.contains("red-ale://manual/start"));
    assert!(uris.contains("red-ale://manual/method/transaction.post"));
}

#[tokio::test]
async fn reading_a_resource_gives_its_page_with_examples_as_tool_calls() {
    let mut client = Client::start().await;
    let uri = "red-ale://manual/method/transaction.post";
    let read = client.request("resources/read", json!({ "uri": uri })).await;
    let contents = read["result"]["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 1, "{read}");
    assert_eq!(contents[0]["uri"], uri);
    assert_eq!(contents[0]["mimeType"], "text/markdown");
    let page = manual().get("method/transaction.post").unwrap();
    let text = contents[0]["text"].as_str().unwrap();
    assert_eq!(text, manual().render(page, Format::Markdown, Door::Mcp));
    assert!(text.contains("Tool call:"), "{text}");

    let missing = client
        .request("resources/read", json!({ "uri": "red-ale://manual/method/no.such" }))
        .await;
    assert_eq!(missing["error"]["code"], -32002, "MCP's resource-not-found: {missing}");
    let elsewhere = client.request("resources/read", json!({ "uri": "file:///etc/passwd" })).await;
    assert_eq!(elsewhere["error"]["code"], -32002, "{elsewhere}");
}

#[tokio::test]
async fn the_method_template_is_listed() {
    let mut client = Client::start().await;
    let listed = client.request("resources/templates/list", json!({})).await;
    let templates = listed["result"]["resourceTemplates"].as_array().unwrap();
    assert_eq!(templates.len(), 1, "{listed}");
    assert_eq!(templates[0]["uriTemplate"], "red-ale://manual/method/{name}");
    assert_eq!(templates[0]["mimeType"], "text/markdown");
    let read = client
        .request("resources/read", json!({ "uri": "red-ale://manual/method/book.open" }))
        .await;
    assert!(read["result"]["contents"][0]["text"].as_str().unwrap().contains("book.open"));
}

#[tokio::test]
async fn the_instructions_come_from_the_start_page() {
    let client = Client::start().await;
    let instructions = client.init["instructions"].as_str().unwrap();
    let start = manual().start().unwrap();
    assert!(instructions.starts_with(&start.summary), "{instructions}");
    assert!(instructions.contains("red-ale://manual/start"), "{instructions}");
    assert!(instructions.split_whitespace().count() <= 100, "short: {instructions}");
}

async fn descriptions(client: &mut Client) -> Vec<(String, String)> {
    let listed = client.request("tools/list", json!({})).await;
    listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["name"].as_str().unwrap().to_owned(),
                t["description"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[tokio::test]
async fn tool_descriptions_lead_with_the_reference_page_within_the_budget() {
    let mut client = Client::start().await;
    let described = descriptions(&mut client).await;
    described.iter().for_each(|(name, description)| {
        let words = description.split_whitespace().count();
        assert!(words <= DESCRIPTION_BUDGET, "{name}: {words} words: {description}");
    });
    let post = &described.iter().find(|(n, _)| n == "transaction.post").unwrap().1;
    let lead = manual().get("method/transaction.post").unwrap().lead().unwrap();
    let first_sentence = lead
        .split_inclusive(". ")
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(post.contains(first_sentence.trim()), "the lead is in: {post}");
    assert!(post.ends_with("Manual: method/transaction.post"), "{post}");
    let schema = &described.iter().find(|(n, _)| n == "api.schema").unwrap().1;
    assert!(!schema.contains("Manual:"), "api.schema has no Page: {schema}");
}

#[tokio::test]
async fn short_tool_descriptions_are_the_summary() {
    let dispatcher = Arc::new(Mutex::new(red_ale_testkit::dispatcher()));
    let server = McpServer::new(dispatcher).with_tool_descriptions(ToolDescriptions::Short);
    let mut client = Client::start_server(server).await;
    let document = red_ale_testkit::dispatcher().schema();
    let summaries: Vec<(&str, &Value)> = document["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["name"].as_str().unwrap(), &m["summary"]))
        .collect();
    let described = descriptions(&mut client).await;
    summaries.iter().for_each(|(name, summary)| {
        let description = &described.iter().find(|(n, _)| n == name).unwrap().1;
        assert_eq!(summary.as_str(), Some(description.as_str()), "{name}");
    });
}
