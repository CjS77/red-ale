//! Online quotes for red-ale (`design/api/quotes.md`): reading Quote services' answers into quote records, planning
//! the requests a Quote fetch makes from the Configuration, and running them over HTTP.
//!
//! This crate is where red-ale meets the network, and the only place it does. The domain side, which commodities
//! are fetched and how a quote record becomes a Price, is pure and lives in `red_ale_core::ops::quotes`; the
//! JSON-RPC methods that start a fetch and store its prices live in `red-ale-rpc`.
//!
//! - [`format`]: one parser per response format, pure functions from an answer's bytes to a quote record;
//! - [`json_text`]: JSON with numbers kept as their text, so no price passes through a float;
//! - [`plan`]: from a commodity and the Configuration to the requests that may answer it, keys included;
//! - [`fetch`]: running a plan on its own thread and tokio runtime, cancelled by dropping its handle.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod fetch;
pub mod format;
mod http;
pub mod json_text;
pub mod plan;
pub mod secret;
