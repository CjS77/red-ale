//! `red-ale mcp`: standard output carries MCP messages and nothing else, and the book is closed when the client goes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{ChildStdin, Command, Stdio};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_red-ale");

fn send(stdin: &mut ChildStdin, message: &Value) {
    writeln!(stdin, "{message}").unwrap();
    stdin.flush().unwrap();
}

/// Read standard output up to the response to `id`, checking that every line is a JSON-RPC 2.0 message.
fn response(lines: &mut impl Iterator<Item = std::io::Result<String>>, id: u64) -> Value {
    lines
        .map(|line| line.unwrap())
        .map(|line| serde_json::from_str::<Value>(&line).unwrap_or_else(|e| panic!("not a protocol message ({e}): {line}")))
        .inspect(|message| assert_eq!(message["jsonrpc"], "2.0", "{message}"))
        .find(|message| message["id"] == id)
        .expect("the server answers")
}

#[test]
fn mcp_writes_only_protocol_messages_and_releases_the_lock_on_exit() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("book.gnucash");
    std::fs::copy(
        format!("{}/../../fixtures/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR")),
        &book,
    )
    .unwrap();
    let book = book.display().to_string();
    let mut server = Command::new(BIN)
        .args(["mcp", "--book", &book, "--retention-days", "0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = server.stdin.take().unwrap();
    let mut lines = BufReader::new(server.stdout.take().unwrap()).lines();

    let client = json!({ "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } });
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": client }),
    );
    assert_eq!(response(&mut lines, 1)["result"]["serverInfo"]["name"], "red-ale");
    send(&mut stdin, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    send(&mut stdin, &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    assert!(
        response(&mut lines, 2)["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "transaction.post")
    );
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "book.info" } }),
    );
    let info = response(&mut lines, 3);
    assert_eq!(
        info["result"]["structuredContent"]["readOnly"], false,
        "the book named by --book is open: {info}"
    );
    assert!(Path::new(&format!("{book}.LCK")).exists());

    drop(stdin);
    let rest: Vec<String> = lines.map(Result::unwrap).filter(|l| !l.trim().is_empty()).collect();
    assert!(rest.is_empty(), "nothing else on standard output: {rest:?}");
    assert!(server.wait().unwrap().success());
    let mut stderr = String::new();
    server.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
    assert!(stderr.contains("red-ale: opened"), "reports go to standard error: {stderr}");
    assert!(
        !Path::new(&format!("{book}.LCK")).exists(),
        "closing standard input closes the book and releases the lock"
    );
}

#[test]
fn sigterm_stops_mcp_and_releases_the_lock_while_standard_input_is_open() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("new.gnucash").display().to_string();
    let created = Command::new(BIN)
        .args(["exec", "--create", "--book", &book, "book.info"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let mut server = Command::new(BIN)
        .args(["mcp", "--book", &book])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = server.stdin.take().unwrap();
    let mut lines = BufReader::new(server.stdout.take().unwrap()).lines();
    let client = json!({ "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } });
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": client }),
    );
    response(&mut lines, 1);
    assert!(Path::new(&format!("{book}.LCK")).exists());
    Command::new("kill").args(["-TERM", &server.id().to_string()]).status().unwrap();
    assert!(server.wait().unwrap().success());
    assert!(
        !Path::new(&format!("{book}.LCK")).exists(),
        "SIGTERM closes the book and releases the lock"
    );
}
