//! red-ale's MCP server: every JSON-RPC method as an MCP tool and every Page of the Manual as a resource, over stdio.
//!
//! A conformist adapter to the MCP specification over `red-ale-rpc`'s [`Dispatcher`], with no logic of its own: tools
//! are generated from the dispatcher's method registry, and each call runs [`Dispatcher::call`]; resources, the
//! instructions and the tools' descriptions come from the Manual. See `design/mcp.md`.

#![forbid(unsafe_code)]

pub mod manual;
mod server;
pub mod tools;

use std::sync::{Arc, Mutex};

use red_ale_rpc::Dispatcher;

pub use manual::ToolDescriptions;
pub use server::{McpServer, ServeError};

/// Serve MCP on standard input and output until the client closes standard input, accepting messages of up to
/// `max_request_bytes` and describing tools as `descriptions` asks. Nothing else may write to standard output while
/// this runs.
pub async fn serve_stdio(
    dispatcher: Arc<Mutex<Dispatcher>>,
    max_request_bytes: usize,
    descriptions: ToolDescriptions,
) -> Result<(), ServeError> {
    let (input, output) = rmcp::transport::stdio();
    McpServer::new(dispatcher)
        .with_max_request_bytes(max_request_bytes)
        .with_tool_descriptions(descriptions)
        .serve(input, output)
        .await
}
