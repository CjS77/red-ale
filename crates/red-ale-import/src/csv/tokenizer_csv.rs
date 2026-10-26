//! The delimited (CSV) tokenizer.
//!
//! Translated from `5.17:gnucash/import-export/csv-imp/gnc-tokenizer-csv.cpp`.
//! Copyright (C) 2015 Geert Janssens <geert@kobaltwit.be>. GPL-2.0-or-later; this translation is GPL-3.0-or-later.
//!
//! GnuCash splits fields with boost's `escaped_list_separator` (escape `\`, quote `"`, the separators the user chose)
//! after rewriting each line so that boost accepts it. Every quirk of that combination is kept:
//!
//! - each physical line is trimmed of leading and trailing C-locale whitespace before anything else, and a quoted
//!   field that spans lines is joined with a single space in place of the line break (`:71-91`);
//! - a file that ends inside an open quote loses its last logical line (`:86-91`: it is never tokenized);
//! - with escapes off every `\` is literal; with escapes on only `\\`, `\"` and `\n` are escapes, and any other `\`
//!   is literal (`:94-116`);
//! - `""` inside a field is a literal `"`, but `""` standing alone between separators (or the line's ends) is an
//!   empty field (`:118-138`);
//! - outside quotes a `"` toggles quoting anywhere in a field, so `a"b,c"d` is one field `ab,cd`;
//! - a separator at the end of the line gives one more, empty, field;
//! - separators are bytes: a multi-byte separator splits on each of its bytes, as in GnuCash.
//!
//! Rows are byte strings in GnuCash; here invalid UTF-8 that a multi-byte separator could produce is replaced with
//! U+FFFD.

use super::tokenizer::{Row, TokenizeError, getline_split, trim_c_space};

/// State carried from one physical line to the next while a quoted field is open.
#[derive(Default)]
struct Pending {
    line: Vec<u8>,
    inside_quotes: bool,
    rows: Vec<Row>,
}

/// `GncCsvTokenizer::tokenize` (`gnc-tokenizer-csv.cpp:53-152`) over text whose line endings are already `\n`.
pub(crate) fn tokenize(text: &str, separators: &str, enable_escape: bool) -> Result<Vec<Row>, TokenizeError> {
    let seps = separators.as_bytes();
    let done = getline_split(text).try_fold(Pending::default(), |mut state, buffer| {
        let buffer = trim_c_space(buffer).as_bytes();
        state.inside_quotes ^= odd_quote_count(buffer, enable_escape);
        state.line.extend_from_slice(buffer);
        if state.inside_quotes {
            state.line.push(b' ');
            return Ok(state);
        }
        let prepared = protect_double_quotes(escape_backslashes(&state.line, enable_escape), seps);
        state.rows.push(split_fields(&prepared, seps)?);
        state.line.clear();
        Ok(state)
    })?;
    Ok(done.rows)
}

/// Whether the quotes on this physical line toggle the quoting state an odd number of times (`:75-84`). With
/// escapes on, a quote right after a backslash does not count.
fn odd_quote_count(buffer: &[u8], enable_escape: bool) -> bool {
    let toggles = buffer
        .iter()
        .enumerate()
        .filter(|&(pos, &b)| b == b'"' && (pos == 0 || !enable_escape || buffer[pos - 1] != b'\\'))
        .count();
    toggles % 2 == 1
}

/// Make boost read stray backslashes literally (`:94-116`): with escapes off every `\` is doubled; with escapes on
/// a `\` not followed by `"`, `\` or `n` is doubled, and a known escape is left whole.
fn escape_backslashes(line: &[u8], enable_escape: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(line.len() + 8);
    let mut pos = 0;
    while let Some(&b) = line.get(pos) {
        let next = line.get(pos + 1).copied();
        match (b, next) {
            (b'\\', Some(n @ (b'"' | b'\\' | b'n'))) if enable_escape => {
                out.extend_from_slice(&[b'\\', n]);
                pos += 2;
            }
            (b'\\', _) => {
                out.extend_from_slice(b"\\\\");
                pos += 1;
            }
            _ => {
                out.push(b);
                pos += 1;
            }
        }
    }
    out
}

/// Turn `""` inside a field into the escape `\"` (`:118-138`). A `""` with a separator or the line's start before
/// it and a separator or the line's end after it is an empty quoted field and is left alone.
fn protect_double_quotes(mut line: Vec<u8>, seps: &[u8]) -> Vec<u8> {
    let mut found = find_pair(&line, 0);
    while let Some(pos) = found {
        let opens_field = pos == 0 || seps.contains(&line[pos - 1]);
        let closes_field = pos + 2 >= line.len() || seps.contains(&line[pos + 2]);
        if !(opens_field && closes_field) {
            line[pos] = b'\\';
        }
        found = find_pair(&line, pos + 2);
    }
    line
}

/// `line.find("\"\"", from)`.
fn find_pair(line: &[u8], from: usize) -> Option<usize> {
    line.get(from..)?.windows(2).position(|w| w == b"\"\"").map(|p| p + from)
}

/// boost's `escaped_list_separator<char>("\\", separators, "\"")` driven by `boost::tokenizer`: an empty line gives
/// no fields; otherwise there is always one more field than there are unquoted separators.
fn split_fields(line: &[u8], seps: &[u8]) -> Result<Row, TokenizeError> {
    if line.is_empty() {
        return Ok(Vec::new());
    }
    let mut fields = Vec::new();
    let mut token = Vec::new();
    let mut in_quote = false;
    let mut pos = 0;
    while let Some(&c) = line.get(pos) {
        if c == b'\\' {
            token.push(unescape(line.get(pos + 1).copied(), seps)?);
            pos += 2;
            continue;
        }
        if seps.contains(&c) && !in_quote {
            fields.push(String::from_utf8_lossy(&std::mem::take(&mut token)).into_owned());
        } else if c == b'"' && !seps.contains(&c) {
            in_quote = !in_quote;
        } else {
            token.push(c);
        }
        pos += 1;
    }
    fields.push(String::from_utf8_lossy(&token).into_owned());
    Ok(fields)
}

/// boost's `do_escape`: `\n` is a newline; a quote, separator or backslash after `\` is itself; anything else, or
/// nothing, is an error.
fn unescape(next: Option<u8>, seps: &[u8]) -> Result<u8, TokenizeError> {
    match next {
        Some(b'n') => Ok(b'\n'),
        Some(b @ (b'"' | b'\\')) => Ok(b),
        Some(b) if seps.contains(&b) => Ok(b),
        _ => Err(TokenizeError::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(line: &str, seps: &str, escape: bool) -> Row {
        tokenize(line, seps, escape).unwrap().remove(0)
    }

    #[test]
    fn quoted_field_spans_lines_joined_by_a_space() {
        let rows = tokenize("a,\"b  \n  c\",d\ne,f", ",", true).unwrap();
        assert_eq!(rows, vec![vec!["a", "b c", "d"], vec!["e", "f"]]);
    }

    #[test]
    fn unterminated_quote_drops_the_last_line() {
        let rows = tokenize("a,b\n\"c,d\ne", ",", true).unwrap();
        assert_eq!(rows, vec![vec!["a", "b"]]);
    }

    #[test]
    fn lines_are_trimmed_and_empty_lines_give_empty_rows() {
        let rows = tokenize("  a , b  \n\n", ",", true).unwrap();
        assert_eq!(rows, [vec!["a ".to_owned(), " b".to_owned()], Vec::new()]);
    }

    #[test]
    fn quotes_toggle_mid_field() {
        assert_eq!(one("a\"b,c\"d,e", ",", true), ["ab,cd", "e"]);
    }

    #[test]
    fn escapes() {
        assert_eq!(one("a\\nb,c", ",", true), ["a\nb", "c"]);
        assert_eq!(one("a\\nb,c", ",", false), ["a\\nb", "c"]);
        assert_eq!(one("a\\,b", ",", true), ["a\\", "b"]);
        assert_eq!(one("a\\\\b", ",", true), ["a\\b"]);
        assert_eq!(one("trailing\\", ",", true), ["trailing\\"]);
    }

    #[test]
    fn several_separators_and_no_separator() {
        assert_eq!(one("a;b,c", ",;", true), ["a", "b", "c"]);
        assert_eq!(one("a,b", "", true), ["a,b"]);
    }
}
