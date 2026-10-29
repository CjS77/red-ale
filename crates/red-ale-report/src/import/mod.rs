//! Importing GnuCash's saved report configurations and style sheets (`design/reports.md` §15): an anti-corruption
//! layer in front of the Scheme files GnuCash keeps in its user data directory, `saved-reports-2.8` (or
//! `saved-reports-2.4`, `5.17:gnucash/report/gnc-report.h:35-36`) and `stylesheets-2.0`
//! (`5.17:gnucash/report/html-style-sheet.scm:142-143`).
//!
//! The files are Scheme programs; GnuCash evaluates them at start-up (`5.17:gnucash/report/gnc-report.cpp:55-112`).
//! red-ale never evaluates them. [`sexp`] reads the text as data, [`forms`] matches each top-level form against the
//! exact shapes GnuCash writes, and [`values`] turns each literal option value into red-ale's wire form for its option.
//! A form of any other shape is an error for its entry alone.

pub mod forms;
pub mod sexp;
pub mod values;

pub use forms::{EmbeddedForm, Malformed, OptionForm, SavedReportForm, StyleSheetForm};
pub use values::{Ignored, WireOptions, to_wire};

/// Every entry of a `saved-reports-2.8` or `saved-reports-2.4` file, in file order. Text that cannot be read as data
/// ends the entries with one [`Malformed`] for the rest of the file.
pub fn saved_reports(text: &str) -> Vec<Result<SavedReportForm, Malformed>> {
    entries(text, forms::saved_report)
}

/// Every entry of a `stylesheets-2.0` file, in file order, as [`saved_reports`] reads them.
pub fn style_sheets(text: &str) -> Vec<Result<StyleSheetForm, Malformed>> {
    entries(text, forms::style_sheet)
}

fn entries<T>(text: &str, entry: impl Fn(&sexp::Form) -> Result<T, Malformed>) -> Vec<Result<T, Malformed>> {
    let read = sexp::read_all(text);
    let unread = read.error.map(|e| {
        Err(Malformed {
            line: e.line,
            message: format!("cannot read the rest of the file: {}", e.message),
        })
    });
    read.forms.iter().map(entry).chain(unread).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_error_ends_the_entries() {
        let found = style_sheets("(define x 1)\n(let ((template");
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(Result::is_err));
        assert_eq!(found[1].as_ref().err().map(|m| m.line), Some(2));
    }

    proptest::proptest! {
        #[test]
        fn malformed_text_never_panics(text in "[()'\"#;. a-z0-9:/\\-\n]{0,300}") {
            let _ = saved_reports(&text);
            let _ = style_sheets(&text);
        }
    }
}
