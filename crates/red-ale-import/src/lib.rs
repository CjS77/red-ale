//! red-ale's importers.
//!
//! GnuCash's importers are translated into Rust (`design/milestone3.md`, "Import"): the generic import backend and
//! matcher, the CSV importers and business import from GnuCash's C and C++, and the QIF importer from its Scheme. This
//! crate is that bounded context. It does no I/O: file content, Import settings and the Book come in as values, and
//! an import produces an Import proposal that the service applies as one Mutation.
//!
//! Every parser is a pure function from text to records: a malformed file is an error, never a panic.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod business;
pub mod csv;
pub mod import_map;
pub mod matcher;
pub mod pipeline;
pub mod qif;
