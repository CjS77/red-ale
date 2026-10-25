//! red-ale's published language: JSON-RPC 2.0 methods, their schemas and error codes, a transport-free dispatcher,
//! and the HTTP server.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]
// Method handlers take their params by value: that is the signature `method` registers.
#![allow(clippy::needless_pass_by_value)]

pub mod dispatch;
pub mod error;
pub mod methods;
pub mod server;
pub mod wire;

pub use dispatch::{Context, Dispatcher, Effect, Method, method};
pub use error::{RpcError, codes};

/// The version of the API's schema. Within a major version, changes are additive only.
pub const API_VERSION: &str = "0.1.0";
