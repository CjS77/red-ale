//! Reading an account file: the records, their twelve fields, and the header.
//!
//! Translated from `create_regex` and `csv_import_sep_cb` (`5.17:gnucash/import-export/csv-imp/assistant-csv-account-import.c:112-133`,
//! `:276-323`) and `csv_import_read_file` and `fill_model_with_match` (`5.17:gnucash/import-export/csv-imp/csv-account-import.c:44-193`).
//! Copyright (C) 2012 Robert Fewell. GPL-2.0-or-later; this translation is GPL-3.0-or-later.
//!
//! GnuCash matches the whole file with one regular expression anchored at the end of the previous match (`\G`): a
//! field is `"…"` (with `""` for a quote) or anything up to the next separator, twelve of them, then any line breaks.
//! A record that doesn't fit ends the reading, silently, and an unquoted field can run over a line break into the
//! next record. red-ale reads record by record instead: a record ends at a line break outside quotes, and one that
//! doesn't have the twelve fields is an error row, the records after it still read (`design/api/import.md`,
//! "Deviations").
//!
//! Each field is then cleaned as `fill_model_with_match` does: an unquoted field is stripped of ASCII white space,
//! and one that then starts with a quote loses its first and last character with `""` read as `"`; a quoted field
//! is its text between the quotes.

use super::AccountRow;
use crate::pipeline::{IgnoredLine, RowError, Warning};

/// The separator between fields: GnuCash's radio buttons (`assistant-csv-account-import.c:276-297`), comma by
/// default. GnuCash's "custom" choice, an edited regular expression, is not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Separator {
    /// `,`.
    #[default]
    Comma,
    /// `;`.
    Semicolon,
    /// `:`.
    Colon,
}

impl Separator {
    /// The character.
    pub fn char(self) -> char {
        match self {
            Self::Comma => ',',
            Self::Semicolon => ';',
            Self::Colon => ':',
        }
    }
}

/// The fields of the account tree export, in order (`5.17:gnucash/import-export/csv-exp/csv-tree-export.cpp:59-64`).
pub const FIELDS: usize = 12;

/// An account file, read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountFile {
    /// The records with twelve fields, header rows left out.
    pub rows: Vec<AccountRow>,
    /// Records that can't be read.
    pub errors: Vec<RowError>,
    /// Empty records and header rows.
    pub ignored: Vec<IgnoredLine>,
    /// What the reader changed: characters XML can't hold, removed.
    pub warnings: Vec<Warning>,
}

/// Read `text` (file `file` of the import) with `separator`. `header_rows` is how many records to leave out at the
/// start; `None` takes one when the first record's Type field is `Type`, as GnuCash's assistant sets its header
/// spinner (`assistant-csv-account-import.c:316-319`, `csv-account-import.c:163-171`).
pub fn read(text: &str, separator: Separator, header_rows: Option<usize>, file: usize) -> AccountFile {
    let (text, stripped) = strip_invalid(text);
    let mut out = AccountFile::default();
    if stripped > 0 {
        let message = format!("{stripped} character(s) XML can't hold were removed, as GnuCash removes them");
        out.warnings.push(Warning {
            code: "invalid-characters",
            message,
            index: None,
        });
    }
    let records: Vec<Record> = records(&text, separator.char())
        .into_iter()
        .filter(|r| !keep_empty(r, file, &mut out.ignored))
        .collect();
    let detected = records
        .first()
        .and_then(|r| r.fields.as_ref().ok())
        .is_some_and(|f| f.first().map(String::as_str) == Some("Type"));
    let header = header_rows.unwrap_or(usize::from(detected));
    records.into_iter().enumerate().for_each(|(i, record)| {
        if i < header {
            out.ignored.push(IgnoredLine {
                file,
                line: record.line,
                reason: "headerRows",
            });
            return;
        }
        match record.fields {
            Ok(fields) if fields.len() == FIELDS => out.rows.push(AccountRow::from_fields(record.line, fields)),
            Ok(fields) => out.errors.push(error(
                file,
                record.line,
                format!("{} field(s), not the {FIELDS} of GnuCash's account export", fields.len()),
            )),
            Err(message) => out.errors.push(error(file, record.line, message)),
        }
    });
    out
}

fn error(file: usize, line: usize, message: String) -> RowError {
    RowError {
        file,
        line,
        column: None,
        message,
    }
}

/// Record an empty record as ignored, and say whether it was one. GnuCash's `\R*` passes over blank lines.
fn keep_empty(record: &Record, file: usize, ignored: &mut Vec<IgnoredLine>) -> bool {
    let empty = matches!(&record.fields, Ok(f) if f.len() == 1 && f[0].is_empty());
    if empty {
        ignored.push(IgnoredLine {
            file,
            line: record.line,
            reason: "empty",
        });
    }
    empty
}

/// `gnc_utf8_strip_invalid` (`5.17:libgnucash/core-utils/gnc-glib-utils.c:145-232`): remove the characters XML can't
/// hold. Returns the text and how many were removed.
fn strip_invalid(text: &str) -> (String, usize) {
    let kept: String = text.chars().filter(|c| xml_valid(*c)).collect();
    let removed = text.chars().count() - kept.chars().count();
    (kept, removed)
}

/// `UNICODE_VALID` as GnuCash narrows it to XML text.
fn xml_valid(c: char) -> bool {
    let code = u32::from(c);
    (code >= 0x20 || matches!(c, '\t' | '\n' | '\r')) && !(0xfdd0..=0xfdef).contains(&code) && code & 0xfffe != 0xfffe
}

/// One record: its line (from 1, counting every record), and its fields or why they can't be read.
#[derive(Debug)]
struct Record {
    line: usize,
    fields: Result<Vec<String>, String>,
}

/// Split the text into records and fields. A record ends at CR LF, CR or LF outside quotes.
fn records(text: &str, separator: char) -> Vec<Record> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < chars.len() {
        let (fields, next) = record(&chars, pos, separator);
        out.push(Record {
            line: out.len() + 1,
            fields,
        });
        pos = next;
    }
    out
}

/// The record starting at `start`, and where the next one starts.
fn record(chars: &[char], start: usize, separator: char) -> (Result<Vec<String>, String>, usize) {
    let mut fields = Vec::new();
    let mut pos = start;
    let mut problem = None;
    loop {
        let (field, end) = if chars.get(pos) == Some(&'"') {
            quoted(chars, pos)
        } else {
            unquoted(chars, pos, separator)
        };
        match field {
            Ok(f) => fields.push(f),
            Err(e) => {
                problem.get_or_insert(e);
            }
        }
        pos = end;
        match chars.get(pos) {
            Some(c) if *c == separator => pos += 1,
            Some('\r') if chars.get(pos + 1) == Some(&'\n') => return (finish(fields, problem), pos + 2),
            Some('\r' | '\n') => return (finish(fields, problem), pos + 1),
            None => return (finish(fields, problem), pos),
            Some(_) => {
                problem.get_or_insert_with(|| "text after a closing quote".to_owned());
                let (_, end) = unquoted(chars, pos, separator);
                pos = end;
                if chars.get(pos) == Some(&separator) {
                    pos += 1;
                }
            }
        }
    }
}

fn finish(fields: Vec<String>, problem: Option<String>) -> Result<Vec<String>, String> {
    problem.map_or(Ok(fields), Err)
}

/// An unquoted field: up to the separator or a line break, stripped of ASCII white space, then unquoted as
/// `fill_model_with_match` does when it starts with a quote.
fn unquoted(chars: &[char], start: usize, separator: char) -> (Result<String, String>, usize) {
    let len = chars[start..]
        .iter()
        .take_while(|c| **c != separator && !matches!(c, '\r' | '\n'))
        .count();
    let text: String = chars[start..start + len].iter().collect();
    let stripped = text.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\u{0b}');
    let field = match stripped.strip_prefix('"') {
        Some(rest) if !rest.is_empty() => rest
            .chars()
            .take(rest.chars().count() - 1)
            .collect::<String>()
            .replace("\"\"", "\""),
        _ => stripped.to_owned(),
    };
    (Ok(field), start + len)
}

/// A quoted field starting at `start`: its text, with `""` read as `"`, and where it ends. It may hold separators and
/// line breaks.
fn quoted(chars: &[char], start: usize) -> (Result<String, String>, usize) {
    let mut text = String::new();
    let mut pos = start + 1;
    while pos < chars.len() {
        match (chars[pos], chars.get(pos + 1)) {
            ('"', Some('"')) => {
                text.push('"');
                pos += 2;
            }
            ('"', _) => return (Ok(text), pos + 1),
            (c, _) => {
                text.push(c);
                pos += 1;
            }
        }
    }
    (Err("a quote that is never closed".to_owned()), pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(text: &str) -> Vec<Result<Vec<String>, String>> {
        records(text, ',').into_iter().map(|r| r.fields).collect()
    }

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn fields_are_cleaned_as_gnucash_cleans_them() {
        assert_eq!(
            fields("a, b ,\"c, d\",\"e \"\"f\"\"\",  \"g\"\"h\"  ,\"\""),
            vec![Ok(strings(&["a", "b", "c, d", "e \"f\"", "g\"h", ""]))]
        );
        assert_eq!(
            fields("\" kept \",x"),
            vec![Ok(strings(&[" kept ", "x"]))],
            "a quoted field is not stripped"
        );
        assert_eq!(
            fields("\"two\nlines\",x\r\ny\rz"),
            vec![Ok(strings(&["two\nlines", "x"])), Ok(strings(&["y"])), Ok(strings(&["z"]))]
        );
        assert_eq!(
            fields("\"a\"b,c\nd"),
            vec![Err("text after a closing quote".to_owned()), Ok(strings(&["d"]))]
        );
        assert_eq!(fields("\"open,\nd"), vec![Err("a quote that is never closed".to_owned())]);
        assert_eq!(fields("a,\n"), vec![Ok(strings(&["a", ""]))]);
    }

    #[test]
    fn header_detection_empty_lines_and_field_counts() {
        let row = "BANK,Assets:Bank,Bank,,,,,USD,CURRENCY,F,F,F";
        let file = read(
            &format!(
                "Type,Full Account Name,Account Name,Account Code,Description,Account Color,Notes,Symbol,Namespace,Hidden,Tax Info,Placeholder\n\n{row}\nBANK,short\n{row}\n"
            ),
            Separator::Comma,
            None,
            0,
        );
        assert_eq!(file.rows.iter().map(|r| r.line).collect::<Vec<_>>(), [3, 5]);
        assert_eq!(
            file.ignored.iter().map(|l| (l.line, l.reason)).collect::<Vec<_>>(),
            [(2, "empty"), (1, "headerRows")]
        );
        assert_eq!(
            file.errors,
            vec![error(0, 4, "2 field(s), not the 12 of GnuCash's account export".to_owned())]
        );
        assert_eq!(read(row, Separator::Comma, None, 0).rows.len(), 1, "no header");
        assert!(read(row, Separator::Comma, Some(1), 0).rows.is_empty(), "the caller's header rows");
        let semi = read(&row.replace(',', ";"), Separator::Semicolon, None, 0);
        assert_eq!(semi.rows[0].full_name, "Assets:Bank");
    }

    #[test]
    fn characters_xml_cant_hold_are_removed() {
        let file = read("BANK,A\u{1}B,A\u{1}B,,,,,USD,CURRENCY,F,F,F", Separator::Comma, None, 2);
        assert_eq!(file.rows[0].full_name, "AB");
        assert_eq!(file.warnings[0].code, "invalid-characters");
    }
}
