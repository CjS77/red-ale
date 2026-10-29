//! Examples: method calls with their expected results, written in Pages as fenced `example` blocks, that the test
//! suite runs (`design/manual.md`, "Examples are tests").
//!
//! ````markdown
//! ```example fixture=reports/test-data now=2024-03-15T12:00:00Z
//! {"method": "account.balance", "params": {"account": "Root:Bank"}}
//! {"result": {"balances": "…"}}
//! ```
//! ````
//!
//! After `example` come attributes, `key=value` each:
//!
//! - `fixture`: the book is a copy of `fixtures/<fixture>.gnucash`, put at `book.gnucash` in an empty directory. Without
//!   it, `book.gnucash` is a new, empty book.
//! - `book`: `open` (the default) opens that book before the first call; `closed` leaves the file there unopened, for
//!   an Example that opens it itself; `none` makes no file at all.
//! - `now`: the clock, as `YYYY-MM-DDTHH:MM:SSZ`; by default 2024-07-01T12:00:00Z.
//! - `zone`: the local time zone, as a POSIX `TZ` string read as GnuCash reads one, with its reversed sign (`XYZ11` is
//!   UTC+11); by default UTC.
//!
//! Then lines in pairs: a request, `{"method": …, "params": …}`, then what it gives, `{"result": …}` or
//! `{"error": {"code": …, "message": …, "data": …}}`. Calls run in the directory holding the book, so paths are
//! relative to it. Blank lines and lines starting with `#` are skipped.
//!
//! An expected value compares exactly, except:
//! - the string `"…"` matches any value;
//! - an object with a `"…"` key may have fields the Example does not list;
//! - an array whose last item is `"…"` may have more items after those listed.

use serde_json::{Map, Value};

/// The marker of a value compared by shape only.
pub const ANY: &str = "…";

/// What state the Example's book starts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookState {
    /// `book.gnucash` is open.
    Open,
    /// `book.gnucash` exists and is not open.
    Closed,
    /// There is no book file and none is open.
    None,
}

/// What a call should give.
#[derive(Debug, Clone, PartialEq)]
pub enum Expected {
    /// The result.
    Result(Value),
    /// The error object: `code`, `message`, `data`.
    Error(Value),
}

/// One method call of an Example.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// The method.
    pub method: String,
    /// The params, `{}` when the request gives none.
    pub params: Value,
    /// What it should give.
    pub expected: Expected,
}

/// One Example.
#[derive(Debug, Clone, PartialEq)]
pub struct Example {
    /// The Page it is in.
    pub page: String,
    /// Its number in the Page, from 1.
    pub number: usize,
    /// The fixture its book copies, relative to `fixtures/` and without `.gnucash`.
    pub fixture: Option<String>,
    /// The book's state before the first call.
    pub book: BookState,
    /// The clock, `YYYY-MM-DDTHH:MM:SSZ`, if not the default.
    pub now: Option<String>,
    /// The local time zone as a POSIX `TZ` string with GnuCash's sign, if not UTC.
    pub zone: Option<String>,
    /// The calls, in order.
    pub calls: Vec<Call>,
}

impl Example {
    /// A name for test output: `method/account.balance #2`.
    pub fn name(&self) -> String {
        format!("{} #{}", self.page, self.number)
    }
}

/// One fenced `example` block of a body: its attributes text and its lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Block<'a> {
    /// What follows `example` on the opening fence.
    pub attributes: &'a str,
    /// The lines inside the fence.
    pub lines: Vec<&'a str>,
}

/// A piece of a Markdown body: text, or an Example block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece<'a> {
    /// Lines of Markdown, each with its newline.
    Text(&'a str),
    /// An `example` block.
    Example(Block<'a>),
}

/// Split a body into Markdown and Example blocks. An unclosed block runs to the end.
pub(crate) fn pieces(body: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = find_fence(rest) {
        if start > 0 {
            out.push(Piece::Text(&rest[..start]));
        }
        let after = &rest[start..];
        let (open, inside) = after.split_once('\n').unwrap_or((after, ""));
        let attributes = open.trim().trim_start_matches("```example").trim();
        let (block, next) = match inside
            .find("\n```")
            .map(|i| i + 1)
            .or_else(|| inside.starts_with("```").then_some(0))
        {
            Some(end) => (&inside[..end], inside[end..].split_once('\n').map_or("", |(_, r)| r)),
            None => (inside, ""),
        };
        out.push(Piece::Example(Block {
            attributes,
            lines: block.lines().collect(),
        }));
        rest = next;
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest));
    }
    out
}

/// Where the next line opening an `example` fence starts.
fn find_fence(text: &str) -> Option<usize> {
    let mut offset = 0;
    text.split_inclusive('\n').find_map(|line| {
        let here = offset;
        offset += line.len();
        let opens = line
            .trim()
            .strip_prefix("```example")
            .is_some_and(|r| r.is_empty() || r.starts_with(' '));
        opens.then_some(here)
    })
}

/// Parse a block of the Page `page` as its Example number `number`.
pub(crate) fn parse(page: &str, number: usize, block: &Block<'_>) -> Result<Example, String> {
    let mut example = Example {
        page: page.to_owned(),
        number,
        fixture: None,
        book: BookState::Open,
        now: None,
        zone: None,
        calls: Vec::new(),
    };
    block.attributes.split_whitespace().try_for_each(|a| attribute(&mut example, a))?;
    let lines: Vec<&str> = block
        .lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    if lines.is_empty() || !lines.len().is_multiple_of(2) {
        return Err("an Example needs its lines in pairs: a request, then its result or error".to_owned());
    }
    example.calls = lines.chunks(2).map(|pair| call(pair[0], pair[1])).collect::<Result<_, _>>()?;
    Ok(example)
}

fn attribute(example: &mut Example, text: &str) -> Result<(), String> {
    let (key, value) = text.split_once('=').ok_or_else(|| format!("attribute {text} is not key=value"))?;
    match key {
        "fixture" => example.fixture = Some(value.to_owned()),
        "now" => example.now = Some(value.to_owned()),
        "zone" => example.zone = Some(value.to_owned()),
        "book" => {
            example.book = match value {
                "open" => BookState::Open,
                "closed" => BookState::Closed,
                "none" => BookState::None,
                other => return Err(format!("book={other}: expected open, closed or none")),
            }
        }
        other => return Err(format!("unknown attribute {other}")),
    }
    Ok(())
}

fn call(request: &str, response: &str) -> Result<Call, String> {
    let mut request = object(request, "request")?;
    let Some(Value::String(method)) = request.remove("method") else {
        return Err("a request needs a \"method\" string".to_owned());
    };
    let params = request.remove("params").unwrap_or_else(|| Value::Object(Map::new()));
    if let Some(key) = request.keys().next() {
        return Err(format!("a request has only method and params, not {key}"));
    }
    let mut response = object(response, "response")?;
    let expected = match (response.remove("result"), response.remove("error"), response.is_empty()) {
        (Some(result), None, true) => Expected::Result(result),
        (None, Some(error), true) => Expected::Error(error),
        _ => return Err(format!("the response to {method} must be {{\"result\": …}} or {{\"error\": …}}")),
    };
    Ok(Call { method, params, expected })
}

fn object(line: &str, what: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str(line) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(format!("a {what} must be a JSON object: {line}")),
        Err(e) => Err(format!("a {what} is not JSON ({e}): {line}")),
    }
}

/// Every way `actual` differs from `expected`, one line each with its path from `at`: empty when they match. `"…"`
/// marks what is compared by shape (see the module documentation).
pub fn mismatches(expected: &Value, actual: &Value, at: &str) -> Vec<String> {
    match (expected, actual) {
        (Value::String(s), _) if s == ANY => Vec::new(),
        (Value::Object(e), Value::Object(a)) => object_mismatches(e, a, at),
        (Value::Array(e), Value::Array(a)) => array_mismatches(e, a, at),
        _ if expected == actual => Vec::new(),
        _ => vec![format!("{at}: expected {expected}, got {actual}")],
    }
}

fn object_mismatches(expected: &Map<String, Value>, actual: &Map<String, Value>, at: &str) -> Vec<String> {
    let open = expected.contains_key(ANY);
    let listed = expected.iter().filter(|(k, _)| *k != ANY).flat_map(|(k, e)| match actual.get(k) {
        Some(a) => mismatches(e, a, &format!("{at}.{k}")),
        None => vec![format!("{at}.{k}: missing")],
    });
    let extra = actual
        .keys()
        .filter(|k| !open && !expected.contains_key(*k))
        .map(|k| format!("{at}.{k}: not expected"));
    listed.chain(extra).collect()
}

fn array_mismatches(expected: &[Value], actual: &[Value], at: &str) -> Vec<String> {
    let (listed, open) = match expected.split_last() {
        Some((Value::String(s), head)) if s == ANY => (head, true),
        _ => (expected, false),
    };
    let length_ok = if open {
        actual.len() >= listed.len()
    } else {
        actual.len() == listed.len()
    };
    if !length_ok {
        return vec![format!(
            "{at}: expected {} items{}, got {}",
            listed.len(),
            if open { " or more" } else { "" },
            actual.len()
        )];
    }
    listed
        .iter()
        .zip(actual)
        .enumerate()
        .flat_map(|(i, (e, a))| mismatches(e, a, &format!("{at}[{i}]")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BODY: &str = "Intro.\n\n```example fixture=reports/test-data now=2024-03-15T12:00:00Z\n\
        {\"method\": \"account.balance\", \"params\": {\"account\": \"Root:Bank\"}}\n\
        {\"result\": {\"balances\": \"…\"}}\n```\n\nAfter.\n\n```example book=none\n\
        # a comment\n{\"method\": \"book.info\"}\n{\"error\": {\"code\": 1000, \"message\": \"…\"}}\n```\n";

    #[test]
    fn blocks_are_found_between_text() {
        let pieces = pieces(BODY);
        assert_eq!(pieces.len(), 4, "{pieces:?}");
        assert_eq!(pieces[0], Piece::Text("Intro.\n\n"));
        assert_eq!(pieces[2], Piece::Text("\nAfter.\n\n"));
        let Piece::Example(block) = &pieces[3] else { panic!("{pieces:?}") };
        assert_eq!(block.attributes, "book=none");
        assert_eq!(block.lines.len(), 3);
    }

    #[test]
    fn examples_parse() {
        let blocks: Vec<Block<'_>> = pieces(BODY)
            .into_iter()
            .filter_map(|p| match p {
                Piece::Example(b) => Some(b),
                Piece::Text(_) => None,
            })
            .collect();
        let first = parse("method/account.balance", 1, &blocks[0]).unwrap();
        assert_eq!(first.fixture.as_deref(), Some("reports/test-data"));
        assert_eq!(first.now.as_deref(), Some("2024-03-15T12:00:00Z"));
        assert_eq!(first.book, BookState::Open);
        assert_eq!(first.calls[0].params, json!({ "account": "Root:Bank" }));
        let second = parse("method/book.info", 2, &blocks[1]).unwrap();
        assert_eq!(second.book, BookState::None);
        assert_eq!(second.calls[0].params, json!({}));
        assert_eq!(second.calls[0].expected, Expected::Error(json!({ "code": 1000, "message": "…" })));
    }

    #[test]
    fn malformed_examples_say_why() {
        let block = |lines: Vec<&'static str>, attributes: &'static str| Block { attributes, lines };
        assert!(
            parse("p", 1, &block(vec!["{\"method\": \"a\"}"], ""))
                .unwrap_err()
                .contains("pairs")
        );
        assert!(
            parse("p", 1, &block(vec!["{}", "{\"result\": 1}"], ""))
                .unwrap_err()
                .contains("method")
        );
        assert!(
            parse("p", 1, &block(vec!["{\"method\": \"a\"}", "{\"ok\": 1}"], ""))
                .unwrap_err()
                .contains("result")
        );
        assert!(parse("p", 1, &block(vec!["{\"method\": \"a\"}", "{\"result\": 1}"], "colour=red")).is_err());
    }

    #[test]
    fn shapes_compare() {
        let actual = json!({ "a": 1, "b": [1, 2, 3], "c": { "d": "x", "e": "y" } });
        assert!(mismatches(&json!({ "a": 1, "b": [1, "…"], "c": { "d": "x", "…": "…" } }), &actual, "result").is_empty());
        assert!(mismatches(&json!({ "a": "…", "b": "…", "c": "…" }), &actual, "result").is_empty());
        assert_eq!(
            mismatches(&json!({ "a": 2, "b": "…", "c": "…" }), &actual, "result"),
            ["result.a: expected 2, got 1"]
        );
        assert_eq!(
            mismatches(&json!({ "a": 1, "b": "…" }), &actual, "result"),
            ["result.c: not expected"]
        );
        assert_eq!(
            mismatches(&json!({ "a": 1, "b": [1, 2], "c": "…" }), &actual, "r"),
            ["r.b: expected 2 items, got 3"]
        );
        assert_eq!(mismatches(&json!({ "z": 1, "…": "…" }), &actual, "r"), ["r.z: missing"]);
    }
}
