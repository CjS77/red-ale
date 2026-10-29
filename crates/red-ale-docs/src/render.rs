//! Rendering a Page: as Markdown or plain text, with its Examples shown for one door (JSON-RPC, the CLI or MCP).

use std::fmt::Write as _;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::example::{self, BookState, Call, Example, Expected, Piece};
use crate::page::Page;

/// How a Page is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// Markdown (CommonMark with tables).
    #[default]
    Markdown,
    /// Plain text for a terminal: no markup, links followed by their target in parentheses.
    Text,
}

/// The door Examples are shown for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Door {
    /// JSON-RPC 2.0 requests and responses.
    #[default]
    JsonRpc,
    /// `red-ale exec` command lines and what they print.
    Cli,
    /// MCP tool calls and their results.
    Mcp,
}

/// What rendering needs from the Manual besides the Page.
pub(crate) trait Lookup {
    /// The title of the Page with this ID.
    fn title_of(&self, id: &str) -> Option<&str>;
    /// The stable name of an error code.
    fn error_name(&self, code: i64) -> Option<&str>;
}

/// `page` as Markdown, with its Examples shown for `door`, its related Pages and its licence footer.
pub(crate) fn markdown(page: &Page, door: Door, lookup: &dyn Lookup) -> String {
    let body = examples_for(page, door, lookup);
    let related = page.related.iter().fold(String::new(), |mut out, id| {
        let _ = writeln!(out, "- [{}]({id})", lookup.title_of(id).unwrap_or(id));
        out
    });
    let related = if related.is_empty() {
        String::new()
    } else {
        format!("\n## Related\n\n{related}")
    };
    let summary = if page.summary.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", page.summary)
    };
    format!(
        "# {}\n\n{summary}{}\n{related}\n---\n\n{}\n",
        page.title,
        body.trim_end(),
        footer(page)
    )
}

/// The one-line licence notice every served Page ends with (`design/manual.md`, "Licence").
fn footer(page: &Page) -> String {
    let from = page
        .from
        .as_ref()
        .map(|f| format!("Adapted from GnuCash's documentation, `{f}`. "))
        .unwrap_or_default();
    format!(
        "*red-ale Manual, `{}`. {from}Licensed under the GNU Free Documentation License, version 1.1 or later, with no \
         Invariant Sections: see [the notices](legal/notices) and [the licence](legal/gfdl).*",
        page.id
    )
}

/// The body with each `example` block replaced by its calls as `door` shows them. A block that does not parse is
/// left as it is.
fn examples_for(page: &Page, door: Door, lookup: &dyn Lookup) -> String {
    let mut number = 0;
    example::pieces(&page.body)
        .into_iter()
        .map(|piece| match piece {
            Piece::Text(text) => text.to_owned(),
            Piece::Example(block) => {
                number += 1;
                match example::parse(&page.id, number, &block) {
                    Ok(e) => example_markdown(&e, door, lookup),
                    Err(_) => format!("```example {}\n{}\n```\n", block.attributes, block.lines.join("\n")),
                }
            }
        })
        .collect()
}

/// One Example as Markdown for `door`.
pub(crate) fn example_markdown(e: &Example, door: Door, lookup: &dyn Lookup) -> String {
    let calls = match door {
        Door::JsonRpc => e.calls.iter().zip(1..).map(|(c, id)| jsonrpc(c, id)).collect(),
        Door::Cli => cli(e),
        Door::Mcp => e.calls.iter().map(|c| mcp(c, lookup)).collect(),
    };
    format!("*Example: {}.*\n\n{calls}", setting(e))
}

/// The book and clock an Example runs with, in words.
fn setting(e: &Example) -> String {
    let book = match &e.fixture {
        Some(f) => format!("a copy of the `{f}` example book"),
        None => "a new, empty book".to_owned(),
    };
    let state = match e.book {
        BookState::Open => format!("with {book} open as `book.gnucash`"),
        BookState::Closed => format!("with {book} saved as `book.gnucash`, not open"),
        BookState::None => "with no book".to_owned(),
    };
    let now = e.now.as_ref().map(|n| format!(", at {n}")).unwrap_or_default();
    let zone = e.zone.as_ref().map(|z| format!(", in the time zone `{z}`")).unwrap_or_default();
    format!("{state}{now}{zone}")
}

fn json_block(value: &Value, pretty: bool) -> String {
    let text = if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    };
    format!("```json\n{}\n```\n\n", text.unwrap_or_default())
}

fn jsonrpc(call: &Call, id: u32) -> String {
    let request = json!({ "jsonrpc": "2.0", "id": id, "method": call.method, "params": call.params });
    let response = match &call.expected {
        Expected::Result(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Expected::Error(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
    };
    format!(
        "Request:\n\n{}Response:\n\n{}",
        json_block(&request, false),
        json_block(&response, true)
    )
}

/// The Example as one `red-ale exec` command, a script for several calls, and what it prints.
fn cli(e: &Example) -> String {
    let book = if e.book == BookState::Open { "--book book.gnucash " } else { "" };
    let command = match e.calls.as_slice() {
        [one] => format!("red-ale exec {book}{}{}", one.method, cli_params(&one.params)),
        calls => {
            let lines = calls.iter().fold(String::new(), |mut out, c| {
                let _ = writeln!(out, "{} {}", c.method, compact(&c.params));
                out
            });
            format!("red-ale exec {book}--script - <<'EOF'\n{lines}EOF")
        }
    };
    let failure = e.calls.iter().find_map(|c| match &c.expected {
        Expected::Error(err) => Some(err),
        Expected::Result(_) => None,
    });
    let printed = match failure {
        Some(err) => format!(
            "It fails with exit status 2, printing on standard error:\n\n{}",
            json_block(&json!({ "error": err }), true)
        ),
        None => format!("It prints:\n\n{}", json_block(&cli_output(e), true)),
    };
    format!("```sh\n{command}\n```\n\n{printed}")
}

/// What `exec` prints: the one result, or every result of a script.
fn cli_output(e: &Example) -> Value {
    let results: Vec<Value> = e
        .calls
        .iter()
        .filter_map(|c| match &c.expected {
            Expected::Result(r) => Some(r.clone()),
            Expected::Error(_) => None,
        })
        .collect();
    match <[Value; 1]>::try_from(results) {
        Ok([one]) => one,
        Err(all) => Value::Array(all),
    }
}

fn cli_params(params: &Value) -> String {
    if params.as_object().is_some_and(serde_json::Map::is_empty) {
        String::new()
    } else {
        format!(" '{}'", compact(params).replace('\'', "'\\''"))
    }
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// The MCP tool call and its result, as `red-ale mcp` answers it: an object result as structured content, any other
/// as text, an error as structured content with `isError` and the code's name.
fn mcp(call: &Call, lookup: &dyn Lookup) -> String {
    let request = json!({ "name": call.method, "arguments": call.params });
    let result = match &call.expected {
        Expected::Result(r @ Value::Object(_)) => json!({ "structuredContent": r }),
        Expected::Result(r) => json!({ "content": [{ "type": "text", "text": compact(r) }] }),
        Expected::Error(err) => json!({ "isError": true, "structuredContent": { "error": named(err, lookup) } }),
    };
    format!(
        "Tool call:\n\n{}Result:\n\n{}",
        json_block(&request, false),
        json_block(&result, true)
    )
}

/// The error object with the code's name after the code, as the MCP server gives it.
fn named(error: &Value, lookup: &dyn Lookup) -> Value {
    let Value::Object(fields) = error else { return error.clone() };
    let name = fields.get("code").and_then(Value::as_i64).and_then(|c| lookup.error_name(c));
    let mut out = serde_json::Map::new();
    fields.iter().for_each(|(k, v)| {
        out.insert(k.clone(), v.clone());
        if let (true, Some(name)) = (k == "code", name) {
            out.insert("name".to_owned(), json!(name));
        }
    });
    Value::Object(out)
}

/// Markdown as plain text for a terminal.
pub(crate) fn text(markdown: &str) -> String {
    let mut writer = TextWriter::default();
    Parser::new_ext(markdown, Options::ENABLE_TABLES).for_each(|event| writer.event(event));
    let lines: Vec<&str> = writer.out.lines().map(str::trim_end).collect();
    let mut out = lines.join("\n").trim().to_owned();
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.push('\n');
    out
}

/// The state of turning Markdown events into text.
#[derive(Debug, Default)]
struct TextWriter {
    out: String,
    /// Each open list's next number, `None` for a bulleted one.
    lists: Vec<Option<u64>>,
    /// Inside a fenced or indented code block.
    in_code: bool,
    /// Where each open heading's or link's text starts in `out`, with a link's target.
    marks: Vec<(usize, String)>,
    /// Cells written in the current table row.
    cells: usize,
}

impl TextWriter {
    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) if self.in_code => t.lines().for_each(|l| {
                let _ = writeln!(self.out, "    {l}");
            }),
            Event::Text(t) => self.out.push_str(&t),
            Event::Code(c) => {
                let _ = write!(self.out, "`{c}`");
            }
            Event::SoftBreak => self.out.push(' '),
            Event::HardBreak => self.newline(),
            Event::Rule => self.out.push_str("\n----\n\n"),
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { .. } => {
                self.blank_line();
                self.marks.push((self.out.len(), String::new()));
            }
            Tag::CodeBlock(_) => {
                self.blank_line();
                self.in_code = true;
            }
            Tag::List(first) => {
                self.newline();
                self.lists.push(first);
            }
            Tag::Item => self.item(),
            Tag::Link { dest_url, .. } => self.marks.push((self.out.len(), dest_url.to_string())),
            Tag::TableRow | Tag::TableHead => self.cells = 0,
            Tag::TableCell => {
                if self.cells > 0 {
                    self.out.push_str(" | ");
                }
                self.cells += 1;
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(level) => self.underline(level),
            TagEnd::Paragraph if self.lists.is_empty() => self.out.push_str("\n\n"),
            TagEnd::Paragraph | TagEnd::Item | TagEnd::TableRow | TagEnd::TableHead => self.newline(),
            TagEnd::CodeBlock => {
                self.in_code = false;
                self.out.push('\n');
            }
            TagEnd::List(_) => {
                self.lists.pop();
                if self.lists.is_empty() {
                    self.out.push('\n');
                }
            }
            TagEnd::Link => {
                if let Some((start, target)) = self.marks.pop()
                    && !target.is_empty()
                    && self.out[start..] != target
                {
                    let _ = write!(self.out, " ({target})");
                }
            }
            _ => {}
        }
    }

    fn item(&mut self) {
        self.newline();
        let depth = self.lists.len().saturating_sub(1);
        let marker = match self.lists.last_mut() {
            Some(Some(n)) => {
                let marker = format!("{n}. ");
                *n += 1;
                marker
            }
            _ => "- ".to_owned(),
        };
        let _ = write!(self.out, "{}{marker}", "  ".repeat(depth));
    }

    fn underline(&mut self, level: HeadingLevel) {
        let start = self.marks.pop().map_or(self.out.len(), |(s, _)| s);
        let width = self.out[start..].chars().count();
        match level {
            HeadingLevel::H1 => {
                let _ = write!(self.out, "\n{}", "=".repeat(width));
            }
            HeadingLevel::H2 => {
                let _ = write!(self.out, "\n{}", "-".repeat(width));
            }
            _ => {}
        }
        self.out.push_str("\n\n");
    }

    fn newline(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    fn blank_line(&mut self) {
        self.newline();
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::Kind;

    struct Names;

    impl Lookup for Names {
        fn title_of(&self, id: &str) -> Option<&str> {
            (id == "concept/value").then_some("Value and amount")
        }

        fn error_name(&self, code: i64) -> Option<&str> {
            (code == 1000).then_some("no-book")
        }
    }

    fn page(body: &str) -> Page {
        Page {
            id: "method/book.info".into(),
            kind: Kind::Method,
            title: "book.info".into(),
            summary: "About the open book.".into(),
            related: vec!["concept/value".into()],
            from: None,
            body: body.into(),
            hand_written: None,
        }
    }

    const ONE: &str = "Say.\n\n```example\n{\"method\": \"book.info\"}\n{\"result\": {\"dirty\": false}}\n```\n";
    const FAILS: &str = "```example book=none\n{\"method\": \"book.info\"}\n{\"error\": {\"code\": 1000, \"message\": \"no book\"}}\n```\n";

    #[test]
    fn markdown_has_title_summary_related_and_footer() {
        let md = markdown(&page(ONE), Door::JsonRpc, &Names);
        assert!(
            md.starts_with("# book.info\n\nAbout the open book.\n\nSay.\n\n*Example: with a new, empty book open"),
            "{md}"
        );
        assert!(
            md.contains("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"book.info\",\"params\":{}}"),
            "{md}"
        );
        assert!(md.contains("- [Value and amount](concept/value)\n"), "{md}");
        assert!(md.contains("[the licence](legal/gfdl)"), "{md}");
    }

    #[test]
    fn the_cli_door_shows_exec() {
        let md = markdown(&page(ONE), Door::Cli, &Names);
        assert!(md.contains("```sh\nred-ale exec --book book.gnucash book.info\n```"), "{md}");
        assert!(md.contains("It prints:\n\n```json\n{\n  \"dirty\": false\n}\n```"), "{md}");
        let failing = markdown(&page(FAILS), Door::Cli, &Names);
        assert!(failing.contains("red-ale exec book.info\n"), "{failing}");
        assert!(failing.contains("exit status 2"), "{failing}");
    }

    #[test]
    fn the_cli_door_quotes_params_and_scripts_several_calls() {
        let e = Example {
            page: "p".into(),
            number: 1,
            fixture: None,
            book: BookState::Open,
            now: None,
            zone: None,
            calls: vec![
                Call {
                    method: "account.create".into(),
                    params: json!({ "name": "Bob's" }),
                    expected: Expected::Result(json!({ "a": 1 })),
                },
                Call {
                    method: "book.info".into(),
                    params: json!({}),
                    expected: Expected::Result(json!({ "b": 2 })),
                },
            ],
        };
        let md = example_markdown(&e, Door::Cli, &Names);
        assert!(
            md.contains("red-ale exec --book book.gnucash --script - <<'EOF'\naccount.create {\"name\":\"Bob's\"}\nbook.info {}\nEOF"),
            "{md}"
        );
        let one = Example {
            calls: e.calls[..1].to_vec(),
            ..e
        };
        let md = example_markdown(&one, Door::Cli, &Names);
        assert!(md.contains("account.create '{\"name\":\"Bob'\\''s\"}'"), "{md}");
    }

    #[test]
    fn the_mcp_door_names_the_error() {
        let md = markdown(&page(FAILS), Door::Mcp, &Names);
        assert!(md.contains("{\"name\":\"book.info\",\"arguments\":{}}"), "{md}");
        assert!(md.contains("\"isError\": true"), "{md}");
        assert!(md.contains("\"code\": 1000,\n      \"name\": \"no-book\""), "{md}");
    }

    #[test]
    fn text_has_no_markup() {
        let out = text(
            "# Title\n\nSome *words* and `code`, a [link](concept/value).\n\n- one\n- two\n\n```json\n{}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
        );
        assert_eq!(
            out,
            "Title\n=====\n\nSome words and `code`, a link (concept/value).\n\n- one\n- two\n\n    {}\n\na | b\n1 | 2\n"
        );
    }
}
