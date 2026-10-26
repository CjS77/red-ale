//! Turning file text into a table of strings: the part every CSV importer shares.
//!
//! Translated from `5.17:gnucash/import-export/csv-imp/gnc-tokenizer.cpp` and `gnc-tokenizer.hpp`.
//! Copyright (C) 2015 Geert Janssens <geert@kobaltwit.be>. GPL-2.0-or-later; this translation is GPL-3.0-or-later.
//!
//! GnuCash reads the file, converts it to UTF-8 with the encoding the user chose
//! (`boost::locale::conv::to_utf` with its default `skip` method, so byte sequences invalid in that encoding are
//! dropped, not replaced), then turns `\r\n` and lone `\r` into `\n` (`gnc-tokenizer.cpp:104-114`). red-ale's service
//! does the conversion before calling in; to read a file as GnuCash does it must drop invalid sequences too. The line
//! ending step is done here, by [`tokenize`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::settings::CsvLayout;
use super::{tokenizer_csv, tokenizer_fw};

/// How the file's columns are delimited: GnuCash's `GncImpFileFormat` (`gnc-tokenizer.hpp:49-53`).
///
/// GnuCash's third value, `UNKNOWN`, selects a dummy tokenizer that is never used for an import, so it has no
/// counterpart here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileFormat {
    /// Fields separated by separator characters, with quoting.
    #[default]
    Csv,
    /// Fields at fixed character offsets.
    FixedWidth,
}

/// A failure to split the text into fields.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TokenizeError {
    /// Boost's `escaped_list_separator` rejected a line (`gnc-tokenizer-csv.cpp:146-149`).
    #[error("There was an error parsing the file.")]
    Malformed,
}

/// One line of the file, split into fields.
pub type Row = Vec<String>;

/// Splits `text` into rows of fields with the layout's file format, separators, escape setting and column widths,
/// as `GncTxImport::tokenize` has its tokenizer do (`gnc-import-tx.cpp:421-427`).
pub fn tokenize(text: &str, layout: &CsvLayout) -> Result<Vec<Row>, TokenizeError> {
    let text = normalize_line_endings(text);
    match layout.file_format {
        FileFormat::Csv => tokenizer_csv::tokenize(&text, &layout.separators, layout.enable_escape),
        FileFormat::FixedWidth => Ok(tokenizer_fw::tokenize(&text, &layout.column_widths)),
    }
}

/// `\r\n` and lone `\r` become `\n` (`gnc-tokenizer.cpp:110-113`).
pub fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The lines `std::getline` yields: split on `\n`, with no empty last line after a final `\n`.
pub(crate) fn getline_split(text: &str) -> impl Iterator<Item = &str> {
    let body = text.strip_suffix('\n').unwrap_or(text);
    body.split('\n').take(if text.is_empty() { 0 } else { usize::MAX })
}

/// `std::isspace` in the classic C locale, which is GnuCash's C++ global locale: boost's `trim_copy` uses it.
pub(crate) fn is_c_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0B' | '\x0C' | '\r')
}

/// `boost::trim_copy` with the classic locale.
pub(crate) fn trim_c_space(s: &str) -> &str {
    s.trim_matches(is_c_space)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getline_matches_std() {
        let lines = |s: &str| getline_split(s).map(str::to_owned).collect::<Vec<_>>();
        assert!(lines("").is_empty());
        assert_eq!(lines("a"), ["a"]);
        assert_eq!(lines("a\n"), ["a"]);
        assert_eq!(lines("a\n\n"), ["a", ""]);
        assert_eq!(lines("\n"), [""]);
        assert_eq!(lines("a\nb"), ["a", "b"]);
    }

    #[test]
    fn line_endings() {
        assert_eq!(normalize_line_endings("a\r\nb\rc\n"), "a\nb\nc\n");
        assert_eq!(normalize_line_endings("a\r\r\nb"), "a\n\nb");
    }
}
