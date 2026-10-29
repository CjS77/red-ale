//! A raw MCP client over an in-memory pipe, speaking JSON-RPC as an agent's client would.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::sync::{Arc, Mutex};

use red_ale_mcp::McpServer;
use red_ale_rpc::Dispatcher;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf};

/// A raw MCP client: one JSON-RPC message per line.
pub struct Client {
    lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
    writer: WriteHalf<DuplexStream>,
    next_id: u64,
    /// The result of `initialize`.
    pub init: Value,
}

impl Client {
    /// Start a server over a fresh deterministic dispatcher and complete the handshake.
    pub async fn start() -> Self {
        Self::start_with(red_ale_testkit::dispatcher()).await
    }

    /// Start a server over `dispatcher` and complete the handshake.
    pub async fn start_with(dispatcher: Dispatcher) -> Self {
        Self::start_server(McpServer::new(Arc::new(Mutex::new(dispatcher)))).await
    }

    /// Start `server` and complete the handshake.
    pub async fn start_server(server: McpServer) -> Self {
        let (ours, theirs) = tokio::io::duplex(1 << 16);
        let (server_in, server_out) = tokio::io::split(theirs);
        tokio::spawn(server.serve(server_in, server_out));
        let (reader, writer) = tokio::io::split(ours);
        let mut client = Self {
            lines: BufReader::new(reader).lines(),
            writer,
            next_id: 0,
            init: Value::Null,
        };
        let init = client
            .request(
                "initialize",
                json!({ "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }),
            )
            .await;
        assert_eq!(init["result"]["protocolVersion"], "2025-11-25", "{init}");
        assert_eq!(init["result"]["serverInfo"]["name"], "red-ale");
        let capabilities = init["result"]["capabilities"].as_object().unwrap();
        assert_eq!(
            capabilities.keys().collect::<Vec<_>>(),
            ["resources", "tools"],
            "tools and resources"
        );
        client.init = init["result"].clone();
        client
            .send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await;
        client
    }

    pub async fn send(&mut self, message: &Value) {
        self.writer.write_all(format!("{message}\n").as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap();
    }

    /// Send a request and return its response, skipping any notifications.
    pub async fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        loop {
            let line = self.lines.next_line().await.unwrap().expect("the server closed the connection");
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
        }
    }

    /// Call a tool and return its result.
    pub async fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let response = self.request("tools/call", json!({ "name": tool, "arguments": arguments })).await;
        response.get("result").cloned().unwrap_or_else(|| panic!("{tool}: {response}"))
    }
}
