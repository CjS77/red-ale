//! The XML Backend: an anti-corruption layer between GnuCash's XML book format and the core model.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod book;
pub mod business;
pub mod dom;
pub mod entities;
pub mod envelope;
pub mod error;
mod input;
pub mod read;
pub mod values;

pub use error::XmlError;

pub use book::{XmlResidue, read_book, write_book, write_book_text, write_book_to};
pub use read::{ReadBack, read_back, read_book_from};
