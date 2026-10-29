//! The MCP server handler: lists the tools and the Manual's resources, runs each call through the dispatcher and reads
//! each resource from the Manual.

use std::borrow::Cow;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use red_ale_rpc::manual::manual;
use red_ale_rpc::{Dispatcher, Origin, RpcError, codes};
use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData, Implementation, ListResourceTemplatesResult,
    ListResourcesResult, ListToolsResult, PaginatedRequestParams, ProtocolVersion, ReadResourceRequestParams, ReadResourceResponse,
    ReadResourceResult, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::{QuitReason, RequestContext, RoleServer, ServerInitializeError, ServiceExt};
use rmcp::transport::async_rw::{AsyncRwTransport, DEFAULT_MAX_LINE_LENGTH};
use serde_json::{Map, Value, json};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::manual::{self as pages, ToolDescriptions};
use crate::tools;

/// The MCP revision red-ale is designed and tested against; it negotiates this one or an earlier one.
const PROTOCOL: ProtocolVersion = ProtocolVersion::V_2025_11_25;

/// Errors serving MCP.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    /// The client did not complete the MCP handshake.
    #[error("starting the MCP session: {0}")]
    Start(#[from] Box<ServerInitializeError>),
    /// The session's task failed.
    #[error("the MCP session failed: {0}")]
    Session(#[from] tokio::task::JoinError),
}

/// red-ale's MCP server: one tool per method of a shared [`Dispatcher`].
#[derive(Debug, Clone)]
pub struct McpServer {
    dispatcher: Arc<Mutex<Dispatcher>>,
    tools: Arc<[Tool]>,
    max_request_bytes: usize,
}

impl McpServer {
    /// A server over `dispatcher`, with its tools generated from the dispatcher's methods. A Quote fetch's prices are
    /// stored as soon as its requests are done.
    pub fn new(dispatcher: Arc<Mutex<Dispatcher>>) -> Self {
        red_ale_rpc::quote_fetches::settle_when_fetched(&dispatcher);
        let tools = tools::tools(&lock(&dispatcher), ToolDescriptions::Full).into();
        Self {
            dispatcher,
            tools,
            max_request_bytes: DEFAULT_MAX_LINE_LENGTH,
        }
    }

    /// The same server, its tools described as `descriptions` asks (`red-ale mcp --tool-descriptions`).
    #[must_use]
    pub fn with_tool_descriptions(mut self, descriptions: ToolDescriptions) -> Self {
        self.tools = tools::tools(&lock(&self.dispatcher), descriptions).into();
        self
    }

    /// The same server, accepting messages of up to `bytes` (the Configuration's `mcp.maxRequestBytes`) instead of
    /// rmcp's default of 16 MiB.
    #[must_use]
    pub fn with_max_request_bytes(mut self, bytes: usize) -> Self {
        self.max_request_bytes = bytes;
        self
    }

    /// Serve MCP over `input` and `output` until the client closes `input`.
    pub async fn serve<R, W>(self, input: R, output: W) -> Result<(), ServeError>
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let transport = AsyncRwTransport::new_server(input, output).with_max_line_length(self.max_request_bytes);
        let running = ServiceExt::serve(self, transport).await.map_err(Box::new)?;
        match running.waiting().await? {
            QuitReason::JoinError(e) => Err(ServeError::Session(e)),
            _ => Ok(()),
        }
    }
}

/// A poisoned lock means a method panicked; the dispatcher's state is still whole because mutations are atomic.
fn lock(dispatcher: &Mutex<Dispatcher>) -> MutexGuard<'_, Dispatcher> {
    dispatcher.lock().unwrap_or_else(PoisonError::into_inner)
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder().enable_resources().enable_tools().build();
        let mut info = ServerConfig::new(capabilities);
        info.instructions = pages::instructions(manual());
        info.protocol_version = PROTOCOL;
        info.server_info = Implementation::new("red-ale", env!("CARGO_PKG_VERSION"));
        info
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(ProtocolVersion::known_up_to(&PROTOCOL))
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(self.tools.to_vec())))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|t| t.name == name).cloned()
    }

    async fn call_tool(&self, request: CallToolRequestParams, _context: RequestContext<RoleServer>) -> Result<CallToolResponse, ErrorData> {
        if self.get_tool(&request.name).is_none() {
            return Err(ErrorData::invalid_params(format!("unknown tool: {}", request.name), None));
        }
        let dispatcher = Arc::clone(&self.dispatcher);
        let name = request.name.into_owned();
        let params = request.arguments.map_or(Value::Null, Value::Object);
        let outcome = tokio::task::spawn_blocking(move || lock(&dispatcher).call_from(Origin::Mcp, &name, params))
            .await
            .map_err(|e| ErrorData::internal_error(format!("the call did not finish: {e}"), None))?;
        Ok(tool_result(outcome).into())
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(ListResourcesResult::with_all_items(pages::resources(manual()))))
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourceTemplatesResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(ListResourceTemplatesResult::with_all_items(pages::templates())))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResponse, ErrorData>> + Send + '_ {
        let read = pages::read(manual(), &request.uri)
            .map(|contents| ReadResourceResult::new(vec![contents]).into())
            .ok_or_else(|| ErrorData::resource_not_found(format!("no Page of the Manual at {}", request.uri), None));
        std::future::ready(read)
    }
}

/// The result as JSON text, and as structured content when it is an object (MCP's structured content is an object).
/// An error carries the error object, with its code's stable name, as the CLI prints it.
fn tool_result(outcome: Result<Value, RpcError>) -> CallToolResult {
    match outcome {
        Ok(value @ Value::Object(_)) => CallToolResult::structured(value),
        Ok(value) => CallToolResult::success(vec![ContentBlock::text(value.to_string())]),
        Err(e) => CallToolResult::structured_error(json!({ "error": error_object(&e) })),
    }
}

fn error_object(e: &RpcError) -> Value {
    let name = codes::ALL.iter().find(|(code, _)| *code == e.code).map(|(_, name)| *name);
    let fields = [
        ("code", Some(json!(e.code))),
        ("name", name.map(Value::from)),
        ("message", Some(json!(e.message))),
        ("data", e.data.clone()),
    ];
    Value::Object(
        fields
            .into_iter()
            .filter_map(|(k, v)| Some((k.to_owned(), v?)))
            .collect::<Map<_, _>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_carry_the_code_name() {
        let e = RpcError::new(codes::NO_BOOK, "no book is open").with_data(json!({ "hint": "open one" }));
        assert_eq!(
            error_object(&e),
            json!({ "code": 1000, "name": "no-book", "message": "no book is open", "data": { "hint": "open one" } })
        );
        assert_eq!(error_object(&RpcError::new(9999, "?")), json!({ "code": 9999, "message": "?" }));
    }

    #[test]
    fn only_object_results_are_structured() {
        assert_eq!(tool_result(Ok(json!({ "a": 1 }))).structured_content, Some(json!({ "a": 1 })));
        let list = tool_result(Ok(json!([1, 2])));
        assert_eq!(list.structured_content, None);
        assert_eq!(list.is_error, Some(false));
        assert_eq!(tool_result(Err(RpcError::new(codes::NO_BOOK, "x"))).is_error, Some(true));
    }
}
