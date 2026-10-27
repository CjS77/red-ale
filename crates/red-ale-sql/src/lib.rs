//! The SQLite Backend: an anti-corruption layer between GnuCash 5.17's SQLite books and the core model
//! (`design/sql-fidelity.md`).
//!
//! Three layers: the connection ([`connection`]), the schema check ([`schema`]), and pure row decoders ([`rows`],
//! [`slots`], [`cell`], [`double`]) between a row's cells and the core model. Table and column names, `_num`/`_denom`
//! pairs, timestamp forms and slot rows stay in this crate, as element names stay in `red-ale-xml`.
//!
//! This version reads only: books open read-only, and nothing is ever written to the file or beside it.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod cell;
mod connection;
pub mod double;
pub mod error;
pub mod rows;
pub mod schema;
pub mod slots;
pub mod tables;

pub use error::SqlError;
