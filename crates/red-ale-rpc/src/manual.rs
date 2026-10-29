//! The Manual this binary serves (`design/manual.md`): built once, on first use, from the API's own schema document
//! and the built-in report templates, so it documents exactly the methods, errors and reports that serve it. The
//! `help.*` methods read it, and so can the other doors (MCP resources, `red-ale help`, the static site).

use std::sync::OnceLock;

use red_ale_docs::{Manual, ReportTemplate};

use crate::dispatch::schema_document;

static MANUAL: OnceLock<Manual> = OnceLock::new();

/// The Manual, built on the first call.
pub fn manual() -> &'static Manual {
    MANUAL.get_or_init(|| Manual::build(&schema_document(&crate::methods::all()), &report_templates()))
}

/// The built-in report templates, with their options at their defaults, as the Manual's report Pages take them.
pub fn report_templates() -> Vec<ReportTemplate> {
    crate::methods::reports::builtin_templates()
}
