//! red-ale's MCP server: every JSON-RPC method as an MCP tool, over stdio.
//!
//! A conformist adapter to the MCP specification over `red-ale-rpc`'s [`Dispatcher`], with no logic of its own: tools
//! are generated from the dispatcher's method registry, and each call runs [`Dispatcher::call`]. See `design/mcp.md`.

#![forbid(unsafe_code)]

mod server;
pub mod tools;

use std::sync::{Arc, Mutex};

use red_ale_rpc::Dispatcher;

pub use server::{McpServer, ServeError};

/// Serve MCP on standard input and output until the client closes standard input. Nothing else may write to
/// standard output while this runs.
pub async fn serve_stdio(dispatcher: Arc<Mutex<Dispatcher>>) -> Result<(), ServeError> {
    let (input, output) = rmcp::transport::stdio();
    McpServer::new(dispatcher).serve(input, output).await
}
