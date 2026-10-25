//! Report options (`design/reports.md` §6): what a template lets its user choose, with GnuCash's option types and
//! semantics.
//!
//! - A template's script declares its options into an [`OptionDb`] (GnuCash's `gnc-register-*-option`).
//! - A client changes them with the wire form of each value ([`OptionDb::apply`]); an invalid value is a
//!   `report-option` error and nothing runs.
//! - `report.options` publishes them ([`OptionDb::describe`]) with the JSON Schema of the `options` param
//!   ([`OptionDb::params_schema`]).
//! - A saved report configuration keeps only what differs from the defaults ([`OptionDb::changed`]).
//!
//! An option is identified by section and name, GnuCash's English strings, and ordered by section name, then key.

mod db;
mod describe;
mod model;
mod summary;
mod value;

pub use db::{END_DATES, GENERAL, OPTIONS_SUMMARY, OptionDb, REPORT_NAME, START_DATES, STYLESHEET};
pub use describe::OptionInfo;
pub use model::{
    Choice, ChoiceKey, Classifier, DateUi, DateValue, MAX_TIME, MIN_TIME, OptionKind, OptionValue, PLOT_SIZE_MAX, PLOT_SIZE_MIN,
    PLOT_SIZE_PERCENT_MAX, ReportOption, parse_relative, relative_name,
};
pub use value::ValueContext;

#[cfg(test)]
mod tests;
