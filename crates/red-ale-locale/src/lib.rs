//! red-ale's Locales.
//!
//! GnuCash takes number formats, date formats, day names and the first day of the week from the C library's locale
//! (`design/milestone3.md`, "Locales"). red-ale ships the same data as a table generated from glibc's locale sources,
//! so its output does not depend on the locales a machine has installed. This crate is that table and the lookups
//! over it. It does no I/O: the process environment comes in as a function.
//!
//! - [`Locale`]: a supported locale by name ([`Locale::by_name`]), or from an environment
//!   ([`Locale::from_environment`]); [`Locale::supported`] lists the names.
//! - [`Conventions`]: GnuCash's view of a locale's number conventions (`gnc_localeconv`).
//! - [`CivilDate`] and [`Weekday`]: dates in the locale's `D_FMT`, day names ([`day_name`]), the first day of the
//!   week and date intervals.
//!
//! # Where the data comes from
//!
//! LC_NUMERIC, LC_MONETARY and LC_TIME come from glibc's locale sources, read by `scripts/gen-locales.py` into
//! `src/table.rs`, which is committed. Each source file carries glibc's notice that "the Free Software Foundation does
//! not claim any copyright interest in the locale data contained in this file"; the script refuses a file without
//! it. The C locale is built into glibc rather than read from a file, and its values are written in the script.
//!
//! The first day of the week and the interval form of date ranges come from ICU in GnuCash, so here from ICU4X and
//! its compiled CLDR data (`Unicode-3.0`).
//!
//! # Supported locales
//!
//! C, and for each language GnuCash 5.17 ships a translation for, its main territories in glibc, with the common
//! English, French, German, Spanish, Italian and Dutch variants: 74 locales.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

mod conventions;
mod data;
mod date;
mod icu;
mod locale;
mod table;

pub use conventions::Conventions;
pub use data::{LocaleData, Monetary, Numeric, Time};
pub use date::{CivilDate, Weekday};
pub use locale::{Category, Locale, LocaleError, day_name};
