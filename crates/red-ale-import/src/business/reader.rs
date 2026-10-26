//! Reading a business import file into rows of named fields, as GnuCash's business importers read theirs.
//!
//! Translated from `gnc_bi_import_read_file` (`5.17:gnucash/import-export/bi-import/dialog-bi-import.c:97-242`,
//! Copyright (C) 2009 Sebastian Held, Rob Laan) and `gnc_customer_import_read_file`
//! (`5.17:gnucash/import-export/customer-import/dialog-customer-import.c:67-202`, Copyright (C) 2009 Sebastian Held),
//! with the four regular expressions their dialogs offer (`5.17:gnucash/import-export/bi-import/dialog-bi-import-gui.c:306-344`,
//! `5.17:gnucash/import-export/customer-import/dialog-customer-import-gui.c:279-317`). All GPL-2.0-or-later; this
//! translation is GPL-3.0-or-later.
//!
//! Each line is matched whole against the separator's expression: every field is either free of the separator (and,
//! in the quoted forms, of quotes), or quoted, where the fields `quoted_escapes` names may hold `""`. A line that
//! doesn't match is ignored, as GnuCash ignores it. Values are stripped of ASCII white space (`g_strstrip`). The
//! expressions are built from the field names, each as GnuCash writes it, and compiled with `regex`, whose
//! leftmost-first alternation chooses as GLib's PCRE does.
//!
//! Deviations (`design/api/import.md`, "Deviations from GnuCash"): lines are read whole, where GnuCash reads 999
//! bytes at a time and so splits a longer line in two; a carriage return ending a line is removed before matching,
//! where GnuCash keeps it and so ignores a CRLF line whose last field is quoted; characters XML can't hold, which
//! GnuCash removes silently (`gnc_utf8_strip_invalid`), are removed with a warning; and the customer importer's
//! quoted semicolon expression, whose last field GnuCash reads with `[^;]*` and so keeps its quotes, reads that field
//! as it reads the others.

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::pipeline::IgnoredLine;

/// How fields are separated: the four choices of GnuCash's business import dialogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Separator {
    /// Semicolons; fields can't be quoted (the dialogs' default).
    #[default]
    Semicolon,
    /// Commas; fields can't be quoted.
    Comma,
    /// Semicolons, each field optionally quoted.
    SemicolonQuoted,
    /// Commas, each field optionally quoted.
    CommaQuoted,
}

impl Separator {
    /// The name `design/api/import.md` gives it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Semicolon => "semicolon",
            Self::Comma => "comma",
            Self::SemicolonQuoted => "semicolonQuoted",
            Self::CommaQuoted => "commaQuoted",
        }
    }

    /// The separator by its name.
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Semicolon, Self::Comma, Self::SemicolonQuoted, Self::CommaQuoted].into_iter().find(|s| s.as_str() == name)
    }

    fn char(self) -> char {
        match self {
            Self::Semicolon | Self::SemicolonQuoted => ';',
            Self::Comma | Self::CommaQuoted => ',',
        }
    }

    /// Whether fields may be quoted.
    pub fn quoted(self) -> bool {
        matches!(self, Self::SemicolonQuoted | Self::CommaQuoted)
    }
}

/// One line that matched: its number in the file (from 1) and its fields, in the layout's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The line, from 1.
    pub line: usize,
    /// The fields, stripped.
    pub fields: Vec<String>,
}

/// What reading a file gave.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Read {
    /// The lines that matched.
    pub rows: Vec<Row>,
    /// The lines that didn't.
    pub ignored: Vec<IgnoredLine>,
    /// Lines from which characters XML can't hold were removed.
    pub stripped: Vec<usize>,
}

/// The expression for one field: GnuCash's `(?<f>[^;]*)`, or in the quoted forms `((?<f>[^";]*)|"(?<f>[^"]*)")`,
/// with `([^"]|"")*` inside the quotes for the fields that may hold doubled quotes. Rust's `regex` has no duplicate
/// names, so the quoted form names its two groups `<f>_u` and `<f>_q`.
fn field_pattern(name: &str, sep: Separator, escapes: bool) -> String {
    let c = sep.char();
    if !sep.quoted() {
        return format!("(?P<{name}>[^{c}]*)");
    }
    let inner = if escapes { r#"(?:[^"]|"")*"# } else { r#"[^"]*"# };
    format!(r#"(?:(?P<{name}_u>[^"{c}]*)|"(?P<{name}_q>{inner})")"#)
}

/// The whole line's expression: an optional byte-order mark, then the fields joined by the separator.
pub fn line_pattern(fields: &[&str], quoted_escapes: &[&str], sep: Separator) -> Result<Regex, regex::Error> {
    let body: Vec<String> = fields.iter().map(|f| field_pattern(f, sep, quoted_escapes.contains(f))).collect();
    Regex::new(&format!("^\u{feff}?{}$", body.join(&sep.char().to_string())))
}

/// `g_ascii_isspace`: space, tab, line feed, vertical tab, form feed and carriage return.
fn is_glib_ascii_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

/// `g_strstrip`.
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_glib_ascii_space)
}

/// Characters `gnc_utf8_strip_invalid` removes: those XML 1.0 can't hold
/// (`5.17:libgnucash/core-utils/gnc-glib-utils.c:155-208`).
fn xml_invalid(c: char) -> bool {
    let u = u32::from(c);
    (u < 0x20 && !matches!(c, '\t' | '\n' | '\r')) || matches!(u, 0xFFFE | 0xFFFF)
}

/// The value of one field of a match.
fn field_value(caps: &regex::Captures<'_>, name: &str, sep: Separator) -> String {
    let value = if sep.quoted() {
        caps.name(&format!("{name}_u")).or_else(|| caps.name(&format!("{name}_q")))
    } else {
        caps.name(name)
    };
    strip(value.map_or("", |m| m.as_str())).to_owned()
}

/// Read `text` (file `file`) line by line against `pattern`.
pub fn read(text: &str, file: usize, fields: &[&str], pattern: &Regex, sep: Separator) -> Read {
    let mut out = Read::default();
    text.split_terminator('\n').enumerate().for_each(|(i, raw)| {
        let line = i + 1;
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let clean: String = raw.chars().filter(|c| !xml_invalid(*c)).collect();
        if clean.len() != raw.len() {
            out.stripped.push(line);
        }
        match pattern.captures(&clean) {
            Some(caps) => out.rows.push(Row { line, fields: fields.iter().map(|f| field_value(&caps, f, sep)).collect() }),
            None => out.ignored.push(IgnoredLine { file, line, reason: "noMatch" }),
        }
    });
    out
}

/// GnuCash's `un_escape` (`5.17:gnucash/import-export/bi-import/dialog-bi-import.c:957-974`): each `""` becomes `"`.
/// GnuCash also drops a lone quote, and the character after it stands; red-ale keeps a lone quote, so text from an
/// unquoted field is not changed.
pub fn un_escape(s: &str) -> String {
    s.replace("\"\"", "\"")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const FIELDS: [&str; 3] = ["id", "notes", "price"];

    fn rows(text: &str, sep: Separator) -> Read {
        let re = line_pattern(&FIELDS, &["notes"], sep).unwrap();
        read(text, 0, &FIELDS, &re, sep)
    }

    #[test]
    fn plain_separators_split_and_strip() {
        let r = rows("A1; some notes ;12\nA2;x\n\nA3;\"q\";4\r\n", Separator::Semicolon);
        let got: Vec<(usize, Vec<String>)> = r.rows.iter().map(|r| (r.line, r.fields.clone())).collect();
        assert_eq!(got, [(1, vec!["A1".into(), "some notes".into(), "12".into()]), (4, vec!["A3".into(), "\"q\"".into(), "4".into()])]);
        assert_eq!(r.ignored.iter().map(|l| l.line).collect::<Vec<_>>(), [2, 3]);
    }

    #[test]
    fn quoted_fields_and_doubled_quotes() {
        let r = rows("\"A,1\",\"say \"\"hi\"\", ok\",\"7\"\n\u{feff}B,,\n\"C\"x,a,b\n\"D\",\"no \"\"\",1\n", Separator::CommaQuoted);
        assert_eq!(r.rows[0].fields, ["A,1", "say \"\"hi\"\", ok", "7"]);
        assert_eq!(r.rows[1].fields, ["B", "", ""]);
        assert_eq!(r.rows[2].fields, ["D", "no \"\"", "1"]);
        assert_eq!(r.ignored.iter().map(|l| l.line).collect::<Vec<_>>(), [3]);
        assert_eq!(un_escape("say \"\"hi\"\", 5\" pipe"), "say \"hi\", 5\" pipe");
    }

    #[test]
    fn a_quote_is_not_allowed_in_an_unquoted_field_of_the_quoted_forms() {
        let r = rows("A;b\"c;1\n", Separator::SemicolonQuoted);
        assert!(r.rows.is_empty());
        let r = rows("A;b\"c;1\n", Separator::Semicolon);
        assert_eq!(r.rows[0].fields[1], "b\"c");
    }

    #[test]
    fn control_characters_are_stripped_and_reported() {
        let r = rows("A;n\u{1}o;1\n", Separator::Semicolon);
        assert_eq!(r.rows[0].fields[1], "no");
        assert_eq!(r.stripped, [1]);
    }
}
