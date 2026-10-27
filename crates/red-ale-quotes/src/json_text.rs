//! JSON read with every number kept as the text it was written as, so a price never passes through a float.
//!
//! Before `serde_json` parses an answer, each number outside a string is put in quotes: `{"c": 261.74}` is read as
//! `{"c": "261.74"}`. `1.0945` thus reaches the price builder as the text `1.0945`, as GnuCash's `property_tree`
//! passes it on. `serde_json`'s `arbitrary_precision` would do the same, but Cargo turns a feature on for every
//! crate in the build, and that one changes how every other crate's untagged and flattened types read numbers.

use serde_json::Value;

/// Parse JSON text, every number becoming a string of its digits.
pub fn parse(body: &[u8]) -> Result<Value, serde_json::Error> {
    let text = String::from_utf8_lossy(body);
    serde_json::from_str(&quote_numbers(&text))
}

/// Where the scanner is: outside strings, inside one, or just after a backslash inside one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Outside,
    InString,
    Escaped,
}

/// The text with each number token outside a string wrapped in quotes. A number token is a run of the characters
/// JSON numbers are made of, starting with `-` or a digit; anything that is not valid JSON stays invalid.
fn quote_numbers(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut state = State::Outside;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        state = match (state, c) {
            (State::Outside, '-' | '0'..='9') => {
                let token: String = std::iter::once(c)
                    .chain(std::iter::from_fn(|| {
                        chars.next_if(|n| matches!(n, '0'..='9' | '.' | 'e' | 'E' | '+' | '-'))
                    }))
                    .collect();
                if is_number(&token) {
                    out.push('"');
                    out.push_str(&token);
                    out.push('"');
                } else {
                    out.push_str(&token);
                }
                continue;
            }
            (State::Outside, '"') | (State::Escaped, _) => State::InString,
            (State::InString, '\\') => State::Escaped,
            (State::InString, '"') => State::Outside,
            (s, _) => s,
        };
        out.push(c);
    }
    out
}

/// JSON's number grammar: `-`, then `0` or digits not starting with `0`, an optional fraction, an optional exponent.
fn is_number(token: &str) -> bool {
    let digits = |s: &str| s.bytes().take_while(u8::is_ascii_digit).count();
    let rest = token.strip_prefix('-').unwrap_or(token);
    let int = digits(rest);
    if int == 0 || (int > 1 && rest.starts_with('0')) {
        return false;
    }
    let rest = &rest[int..];
    let rest = match rest.strip_prefix('.') {
        Some(frac) if digits(frac) > 0 => &frac[digits(frac)..],
        Some(_) => return false,
        None => rest,
    };
    match rest.strip_prefix(['e', 'E']) {
        None => rest.is_empty(),
        Some(exp) => {
            let exp = exp.strip_prefix(['+', '-']).unwrap_or(exp);
            digits(exp) > 0 && digits(exp) == exp.len()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_keep_their_digits() {
        let v = parse(br#"{"c": 261.74, "t": 1582641000, "e": -1.5E-3, "z": 0, "big": 123456789012345678901234567890.5}"#).unwrap();
        assert_eq!(
            v,
            json!({ "c": "261.74", "t": "1582641000", "e": "-1.5E-3", "z": "0", "big": "123456789012345678901234567890.5" })
        );
    }

    #[test]
    fn strings_are_left_alone() {
        let v = parse(br#"{"a": "12.5", "b\"9": "x\\", "c": [1, "2", true, null, {"d": 3.10}]}"#).unwrap();
        assert_eq!(
            v,
            json!({ "a": "12.5", "b\"9": "x\\", "c": ["1", "2", true, null, { "d": "3.10" }] })
        );
    }

    #[test]
    fn invalid_json_stays_invalid() {
        assert!(parse(b"<html>429 Too Many Requests</html>").is_err());
        assert!(parse(b"{\"a\": 1.2.3}").is_err());
        assert!(parse(b"{\"a\": 01}").is_err());
        assert!(parse(b"{\"a\": 1.}").is_err());
        assert!(parse(b"{\"a\": 1e}").is_err());
        assert!(parse(b"{\"a\": -}").is_err());
        assert!(parse(b"").is_err());
    }
}
