//! The CSV and fixed-width tokenizers (`red_ale_import::csv::tokenize`). The first byte picks the separators, the
//! escape flag and the column widths; the rest is the file.

#![no_main]

use libfuzzer_sys::fuzz_target;
use red_ale_import::csv::{CsvLayout, FileFormat, tokenize};

const SEPARATORS: [&str; 8] = [",", ";", "\t", " ", ",;", "\"", "\\", ""];

fuzz_target!(|data: &[u8]| {
    let Some((first, rest)) = data.split_first() else { return };
    let text = String::from_utf8_lossy(rest);
    let separators = SEPARATORS[usize::from(first & 7)].to_owned();
    let escape = first & 8 != 0;
    let _ = tokenize(&text, &CsvLayout { separators, escape, ..CsvLayout::default() });
    let widths = rest.iter().take(usize::from(first >> 4)).map(|b| u32::from(b % 8)).collect();
    let _ = tokenize(&text, &CsvLayout { format: FileFormat::FixedWidth, column_widths: widths, ..CsvLayout::default() });
});
