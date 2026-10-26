//! One CSV line, as GnuCash writes it.
//!
//! Translated from `gnc_csv_add_line` (`5.17:gnucash/import-export/csv-exp/csv-export-helpers.cpp:48-84`), GPL-2.0-or-later.
//! That file carries no copyright line; its author is Christopher Lam.

/// GnuCash ends every line, the header included, with CRLF, as the CSV specification asks (`EOLSTR`,
/// `csv-export-helpers.cpp:38-44`; the `\n` of Windows builds is not followed).
pub const EOL: &str = "\r\n";

const QUOTE: char = '"';

/// Append `fields` joined by `separator` to `out`, then [`EOL`]. A field is quoted when `quotes` is set, when it holds
/// the separator (a non-empty one), or when it holds `"`, LF or CR; a `"` inside is doubled, and newlines stay raw
/// inside the quotes.
pub fn add_line<S: AsRef<str>>(out: &mut String, fields: &[S], quotes: bool, separator: &str) {
    fields.iter().enumerate().for_each(|(i, field)| {
        if i > 0 {
            out.push_str(separator);
        }
        push_field(out, field.as_ref(), quotes, separator);
    });
    out.push_str(EOL);
}

fn push_field(out: &mut String, field: &str, quote_all: bool, separator: &str) {
    let quoted = quote_all || (!separator.is_empty() && field.contains(separator)) || field.contains(['"', '\n', '\r']);
    if quoted {
        out.push(QUOTE);
    }
    field.chars().for_each(|c| {
        out.push(c);
        if c == QUOTE {
            out.push(QUOTE);
        }
    });
    if quoted {
        out.push(QUOTE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(fields: &[&str], quotes: bool, separator: &str) -> String {
        let mut out = String::new();
        add_line(&mut out, fields, quotes, separator);
        out
    }

    /// `5.17:gnucash/import-export/csv-exp/test/test-csv-export-helpers.cpp:38-55` (`EmptyTests`). GnuCash's null
    /// separator is the empty string here.
    #[test]
    fn empty_lines() {
        assert_eq!(line(&[], false, ""), "\r\n");
        assert_eq!(line(&[], true, ","), "\r\n");
        assert_eq!(line(&[], false, ","), "\r\n");
        assert_eq!(line(&[], true, ""), "\r\n");
    }

    /// `test-csv-export-helpers.cpp:57-77` (`BasicTests`).
    #[test]
    fn basic_lines() {
        let fields = ["A", "B", "C", "", "D"];
        assert_eq!(line(&fields, false, ","), "A,B,C,,D\r\n");
        assert_eq!(line(&fields, false, ""), "ABCD\r\n");
        assert_eq!(line(&fields, false, ";"), "A;B;C;;D\r\n");
        assert_eq!(line(&fields, true, ","), "\"A\",\"B\",\"C\",\"\",\"D\"\r\n");
    }

    /// `test-csv-export-helpers.cpp:80-94` (`ForcedQuote`).
    #[test]
    fn forced_quotes() {
        assert_eq!(line(&["A", "B", "C", "\"", "D"], false, ","), "A,B,C,\"\"\"\",D\r\n");
        assert_eq!(line(&["A", "B", "C", "", ",D"], false, ","), "A,B,C,,\",D\"\r\n");
        assert_eq!(line(&["A", "B", "C", "\n", "D\r"], false, ";"), "A;B;C;\"\n\";\"D\r\"\r\n");
    }

    /// A separator of several characters, which GnuCash's custom separator allows, quotes a field only when the whole
    /// string occurs in it (`str.find (sep_view)`, `csv-export-helpers.cpp:55`).
    #[test]
    fn long_separators_quote_whole_matches_only() {
        assert_eq!(line(&["a:b", "a::b", "x"], false, "::"), "a:b::\"a::b\"::x\r\n");
        assert_eq!(line(&["Café \"Noir\""], false, ","), "\"Café \"\"Noir\"\"\"\r\n");
    }
}
