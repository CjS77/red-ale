//! Errors reading and writing XML books.

use thiserror::Error;

/// Why an XML book could not be read or written.
#[derive(Debug, Error)]
pub enum XmlError {
    /// The XML is not well-formed.
    #[error("XML syntax error at byte {offset}: {message}")]
    Syntax {
        /// Byte offset in the decompressed text.
        offset: u64,
        /// What the parser said.
        message: String,
    },
    /// The text is not valid UTF-8.
    #[error("the book is not valid UTF-8")]
    Encoding,
    /// The document has no root element.
    #[error("the document is empty")]
    Empty,
    /// The document is not a GnuCash v2 XML book.
    #[error("not a GnuCash XML book: {0}")]
    NotABook(String),
    /// An element GnuCash requires is missing or malformed.
    #[error("bad {element}: {message}")]
    Malformed {
        /// The element.
        element: String,
        /// What is wrong.
        message: String,
    },
    /// Reading or decompressing failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
