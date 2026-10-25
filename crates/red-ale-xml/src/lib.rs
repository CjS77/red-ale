//! The XML Backend: an anti-corruption layer between GnuCash's XML book format and the core model.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod business;
pub mod dom;
pub mod envelope;
pub mod error;
pub mod values;

pub use error::XmlError;
