//! The XML Backend: an anti-corruption layer between GnuCash's XML book format and the core model.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod book;
pub mod budgets;
pub mod business;
pub mod dom;
pub mod entities;
pub mod envelope;
pub mod error;
pub mod export;
mod input;
pub mod read;
pub mod scheduled;
pub mod values;

pub use error::XmlError;

pub use book::{XmlResidue, read_book, write_accounts_to, write_book, write_book_text, write_book_to};
pub use export::{ExportCounts, check_accounts_export};
pub use read::{ReadBack, read_back, read_book_from};
