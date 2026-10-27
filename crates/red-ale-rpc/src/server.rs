//! The JSON-RPC 2.0 server over HTTP, on localhost unless configured otherwise.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use jsonrpsee::RpcModule;
use jsonrpsee::server::{Server, ServerConfig, ServerHandle};
use jsonrpsee::types::{ErrorObjectOwned, Params};
use serde_json::Value;

use crate::dispatch::Dispatcher;
use crate::error::{RpcError, codes};

/// Errors starting the server.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// The address could not be bound.
    #[error("cannot listen on {addr}: {source}")]
    Bind {
        /// The address.
        addr: SocketAddr,
        /// The cause.
        source: std::io::Error,
    },
    /// A method could not be registered (a duplicate name: a bug).
    #[error("cannot register {0}")]
    Register(String),
}

fn to_error_object(e: RpcError) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(e.code, e.message, e.data)
}

fn params_value(params: &Params<'_>) -> Result<Value, RpcError> {
    params.as_str().map_or(Ok(Value::Null), |text| {
        serde_json::from_str(text).map_err(|e| RpcError::new(codes::PARSE_ERROR, e.to_string()))
    })
}

fn call(dispatcher: &Mutex<Dispatcher>, name: &str, params: &Params<'_>) -> Result<Value, ErrorObjectOwned> {
    let params = params_value(params).map_err(to_error_object)?;
    // A poisoned lock means a method panicked; the dispatcher's state is still whole because mutations are atomic.
    let mut guard = dispatcher.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.call(name, params).map_err(to_error_object)
}

/// Start serving `dispatcher` on `addr`, accepting requests of up to `limits.0` bytes and sending responses of up to
/// `limits.1` (the Configuration's `jsonRpc.maxRequestBytes` and `jsonRpc.maxResponseBytes`). Methods run one at a
/// time, on a blocking thread; a Quote fetch's prices are stored as soon as its requests are done.
/// The caller keeps its handle on `dispatcher` so it can close the book on shutdown.
pub async fn serve(
    dispatcher: Arc<Mutex<Dispatcher>>,
    addr: SocketAddr,
    limits: (u32, u32),
) -> Result<(SocketAddr, ServerHandle), ServerError> {
    crate::quote_fetches::settle_when_fetched(&dispatcher);
    let mut names: Vec<&'static str> = dispatcher
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .methods()
        .map(|m| m.name)
        .collect();
    names.push("api.schema");
    let mut module = RpcModule::new(dispatcher);
    names.into_iter().try_for_each(|name| {
        module
            .register_blocking_method(name, move |params, ctx: Arc<Arc<Mutex<Dispatcher>>>, _| call(&ctx, name, &params))
            .map(|_| ())
            .map_err(|e| ServerError::Register(format!("{name}: {e}")))
    })?;
    let config = ServerConfig::builder()
        .max_request_body_size(limits.0)
        .max_response_body_size(limits.1)
        .build();
    let builder = Server::builder().set_config(config);
    let server = builder.build(addr).await.map_err(|source| ServerError::Bind { addr, source })?;
    let local = server.local_addr().map_err(|source| ServerError::Bind { addr, source })?;
    Ok((local, server.start(module)))
}
