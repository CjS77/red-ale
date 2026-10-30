//! Markdown to HTML for the static site: raw HTML escaped, headings given stable ids, links to Pages mapped to their
//! files, and `example` blocks shown for all three doors.

use std::collections::BTreeMap;

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd, html};

use super::{page_path, relative};
use crate::Manual;
use crate::example::{self, Block};
use crate::page::{Page, slug};
use crate::render::{self, Door};

/// The doors an Example is shown for, with the label of each block.
const DOORS: [(Door, &str); 3] = [
    (Door::JsonRpc, "JSON-RPC"),
    (Door::Cli, "Command line"),
    (Door::Mcp, "MCP tool call"),
];

/// Turns Markdown into the HTML of the file `from`.
#[derive(Clone, Copy)]
pub(super) struct Renderer<'a> {
    /// Resolves links to Pages.
    pub manual: &'a Manual,
    /// The path of the file being written, which links are relative to.
    pub from: &'a str,
    /// The Page whose `example` blocks are shown for each door; without it they stay code blocks.
    pub page: Option<&'a Page>,
}

impl Renderer<'_> {
    /// `markdown` as HTML.
    pub(super) fn html(&self, markdown: &str) -> String {
        let events: Vec<Event<'_>> = Parser::new_ext(markdown, Options::ENABLE_TABLES).map(escape_raw).collect();
        let events = heading_ids(self.examples(events));
        let mut out = String::new();
        html::push_html(&mut out, events.into_iter().map(|e| self.link(e)));
        out
    }

    /// Replace each `example` block that parses with the HTML of its three doors.
    fn examples<'e>(&self, events: Vec<Event<'e>>) -> Vec<Event<'e>> {
        let Some(page) = self.page else { return events };
        let mut out = Vec::with_capacity(events.len());
        let mut rest = events.into_iter();
        let mut number = 0;
        while let Some(event) = rest.next() {
            let Some(attributes) = example_attributes(&event) else {
                out.push(event);
                continue;
            };
            number += 1;
            let code = block_text(&mut rest);
            match self.example(page, number, &attributes, &code) {
                Some(html) => out.push(Event::Html(html.into())),
                None => out.extend([event, Event::Text(code.into()), Event::End(TagEnd::CodeBlock)]),
            }
        }
        out
    }

    /// One Example as a setting line and a labelled block per door.
    fn example(&self, page: &Page, number: usize, attributes: &str, code: &str) -> Option<String> {
        let block = Block {
            attributes,
            lines: code.lines().collect(),
        };
        let e = example::parse(&page.id, number, &block).ok()?;
        let plain = Renderer { page: None, ..*self };
        let doors = DOORS
            .iter()
            .map(|(door, label)| {
                let calls = plain.html(&render::calls_markdown(&e, *door, self.manual));
                format!("<section class=\"door\">\n<h4>{label}</h4>\n{calls}</section>\n")
            })
            .collect::<Vec<_>>()
            .concat();
        let setting = plain.html(&format!("*Example: {}.*", render::setting(&e)));
        Some(format!("<div class=\"example\">\n{setting}{doors}</div>\n"))
    }

    /// A link whose target is a Page ID points at that Page's file.
    fn link<'e>(&self, event: Event<'e>) -> Event<'e> {
        match event {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let dest_url = self.target(&dest_url).map_or(dest_url, CowStr::from);
                Event::Start(Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                })
            }
            other => other,
        }
    }

    fn target(&self, dest: &str) -> Option<String> {
        if dest.is_empty() || dest.contains(':') || dest.starts_with(['#', '/', '.']) {
            return None;
        }
        let (id, anchor) = dest.split_once('#').map_or((dest, String::new()), |(i, a)| (i, format!("#{a}")));
        self.manual.get(id)?;
        Some(format!("{}{anchor}", relative(self.from, &page_path(id))))
    }
}

/// Raw HTML in the source becomes text, so the site never carries markup the Manual's authors did not mean.
fn escape_raw(event: Event<'_>) -> Event<'_> {
    match event {
        Event::Html(s) | Event::InlineHtml(s) => Event::Text(s),
        other => other,
    }
}

/// The attributes of an `example` fence, if `event` opens one.
fn example_attributes(event: &Event<'_>) -> Option<String> {
    let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event else {
        return None;
    };
    let attributes = info.strip_prefix("example")?;
    (attributes.is_empty() || attributes.starts_with(' ')).then(|| attributes.trim().to_owned())
}

/// The text of a code block whose start has been read, up to and including its end.
fn block_text<'e>(events: &mut impl Iterator<Item = Event<'e>>) -> String {
    events
        .take_while(|e| !matches!(e, Event::End(TagEnd::CodeBlock)))
        .filter_map(|e| match e {
            Event::Text(t) => Some(t.to_string()),
            _ => None,
        })
        .collect()
}

/// Give each heading an id from its text: `Request parameters` is `request-parameters`, a repeat gets `-2`, `-3`.
fn heading_ids(mut events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let ids: Vec<(usize, String)> = (0..events.len())
        .filter(|&i| matches!(events[i], Event::Start(Tag::Heading { id: None, .. })))
        .map(|i| (i, unique(&mut seen, &slug(&heading_text(&events[i + 1..])))))
        .collect();
    ids.into_iter().for_each(|(i, new)| {
        if let Event::Start(Tag::Heading { id, .. }) = &mut events[i] {
            *id = Some(new.into());
        }
    });
    events
}

fn heading_text(events: &[Event<'_>]) -> String {
    events
        .iter()
        .take_while(|e| !matches!(e, Event::End(TagEnd::Heading(_))))
        .filter_map(|e| match e {
            Event::Text(t) | Event::Code(t) => Some(t.as_ref()),
            _ => None,
        })
        .collect()
}

fn unique(seen: &mut BTreeMap<String, usize>, base: &str) -> String {
    let base = if base.is_empty() { "section" } else { base };
    let count = seen.entry(base.to_owned()).or_insert(0);
    *count += 1;
    if *count == 1 { base.to_owned() } else { format!("{base}-{count}") }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn manual() -> Manual {
        Manual::build(&json!({ "redAleApi": "0.6.0", "methods": [], "errors": [], "$defs": {} }), &[])
    }

    fn html(markdown: &str, from: &str) -> String {
        let manual = manual();
        Renderer {
            manual: &manual,
            from,
            page: None,
        }
        .html(markdown)
    }

    #[test]
    fn raw_html_is_escaped() {
        let out = html("Hello <script>alert(1)</script> & <b>you</b>\n\n<div>block</div>\n", "index.html");
        assert!(!out.contains("<script>") && !out.contains("<b>") && !out.contains("<div>"), "{out}");
        assert!(out.contains("&lt;script&gt;") && out.contains("&amp;"), "{out}");
    }

    #[test]
    fn headings_get_stable_unique_ids() {
        let out = html(
            "## Request parameters\n\n## `book.open` result\n\n## Request parameters\n\n## !!!\n",
            "index.html",
        );
        assert!(out.contains("<h2 id=\"request-parameters\">"), "{out}");
        assert!(out.contains("<h2 id=\"book-open-result\">"), "{out}");
        assert!(out.contains("<h2 id=\"request-parameters-2\">"), "{out}");
        assert!(out.contains("<h2 id=\"section\">"), "{out}");
    }

    #[test]
    fn page_id_links_become_relative_files() {
        let out = html(
            "[a](start) [b](legal/gfdl#top) [c](https://example.org/x) [d](#here) [e](nothing/here)",
            "method/x.html",
        );
        assert!(out.contains("href=\"../start.html\""), "{out}");
        assert!(out.contains("href=\"../legal/gfdl.html#top\""), "{out}");
        assert!(out.contains("href=\"https://example.org/x\""), "{out}");
        assert!(out.contains("href=\"#here\"") && out.contains("href=\"nothing/here\""), "{out}");
    }
}
