//! red-ale's Locales.
//!
//! GnuCash takes number formats, date formats, day names and the first day of the week from the C library's locale
//! (`design/milestone3.md`, "Locales"). red-ale ships the same data as a table generated from glibc's locale sources,
//! so its output does not depend on the locales a machine has installed. This crate is that table and the lookups
//! over it. It does no I/O.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]
