//! red-ale's reports.
//!
//! GnuCash's reports are Guile Scheme; red-ale translates each one into a Rhai script that declares its options and
//! returns a structured report result, rendered to HTML on request (`design/reports.md`). This crate is that bounded
//! context. It does no I/O: script sources, report settings and the time zone come in as values.
//!
//! - [`number`]: [`Rational`](number::Rational) and [`Monetary`](number::Monetary), display rounding and GnuCash's
//!   amount formatting.
//! - [`options`]: report option declarations, values, validation, the published option schema and the changes a
//!   saved report configuration keeps.
//! - [`result`]: the report result, the published language for reports, with its validation and JSON form.
//! - [`settings`]: the GnuCash preferences reports read, kept in red-ale's Configuration.
//! - [`error`]: [`ReportError`](error::ReportError), whose kinds map to the API's error codes 3000 to 3004.
//! - [`limits`]: the bounds a report run stays within.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod error;
pub mod limits;
pub mod number;
pub mod options;
pub mod result;
pub mod settings;
pub mod wire;

pub use error::{ReportError, ReportErrorKind};
pub use number::{Monetary, Rational};
pub use result::ReportResult;
