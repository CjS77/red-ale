//! Helpers for the tests of the shared modules under `scripts/lib/` (`design/reports.md` §12): run a render body
//! against the built-in library and read back what it printed.

#![allow(dead_code, clippy::missing_panics_doc, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_report::ReportError;
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;
use serde_json::json;

use crate::common::{EMPTY_RESULT, GUID, Host, registry_with, template_with};

/// The lines a render `body` prints on `host`'s book, with the built-in library to import from. `prelude` holds
/// script-level functions.
pub fn lines_with(host: &Host, prelude: &str, body: &str) -> Result<Vec<String>, ReportError> {
    let script = template_with(prelude, &format!("{body}\n{EMPTY_RESULT}"));
    let reg = registry_with(Library::builtin(), &[("test.rhai", &script)], Limits::default());
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    let result = host.run(&reg, GUID, &json!({}))?;
    Ok(result.log.into_iter().map(|l| l.text).collect())
}

/// [`lines_with`] with no prelude, unwrapped.
pub fn lines(host: &Host, body: &str) -> Vec<String> {
    lines_with(host, "", body).unwrap_or_else(|e| panic!("{e:?}"))
}

/// Compare printed lines with `want`, one by one, so a failure names the line.
pub fn check(got: &[String], want: &[&str]) {
    got.iter().zip(want).for_each(|(g, w)| assert_eq!(g, w, "{got:#?}"));
    assert_eq!(got.len(), want.len(), "{got:#?}");
}
