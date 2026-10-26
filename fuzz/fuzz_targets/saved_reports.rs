//! The s-expression reader behind `report.importConfigurations` and `report.importStyleSheets`, and the readers of
//! the saved-report and style-sheet forms built on it. It reads Scheme text and never evaluates it.

#![no_main]

use libfuzzer_sys::fuzz_target;
use red_ale_report::import::sexp::{read_all, read_one};
use red_ale_report::import::{saved_reports, style_sheets};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = read_all(&text);
    let _ = read_one(&text);
    let _ = saved_reports(&text);
    let _ = style_sheets(&text);
});
