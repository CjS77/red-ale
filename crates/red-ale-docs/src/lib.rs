//! red-ale's Manual: its user documentation, compiled into the binary and served through every door
//! (`design/manual.md`).
//!
//! This crate is the Manual's bounded context. It does no I/O and depends on no other red-ale crate: the registries
//! it documents reach it as data (the `api.schema` document and the report templates), and the hand-written Pages
//! of `docs/` are compiled in. [`Manual::build`] merges the two:
//!
//! - [`page`]: the [`Page`], its [`Kind`] and Page IDs;
//! - Reference pages generated from the schema and the report templates ([`ReportTemplate`]), completed by
//!   `docs/reference/<method>.md`, `docs/errors/<name>.md` and `docs/reports/<id>.md`;
//! - [`example`]: Examples, the fenced `example` blocks the test suite runs;
//! - rendering as Markdown or text with Examples for one [`Door`] ([`Manual::render`]);
//! - [`Manual::search`]: BM25 with a synonym table ([`synonyms`]);
//! - [`coverage`]: what the Manual still lacks.

#![forbid(unsafe_code)]

pub mod coverage;
pub mod example;
mod generate;
pub mod page;
mod render;
mod search;
mod source;
pub mod synonyms;

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

pub use example::Example;
pub use generate::{ReportOption, ReportTemplate};
pub use page::{Kind, Page, error_page_id, method_page_id, report_page_id};
pub use render::{Door, Format};
pub use search::{terms, words};
pub use source::SPDX_LINE;

use crate::render::Lookup;
use crate::search::Index;
use crate::source::Source;

mod sources {
    include!(concat!(env!("OUT_DIR"), "/sources.rs"));
}

/// The text of the GFDL 1.1, `docs/COPYING-DOCS`, served as the Page `legal/gfdl`.
pub const GFDL_TEXT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/COPYING-DOCS"));

/// The Page ID of the Start Page.
pub const START: &str = "start";

/// Something wrong in the Manual's sources: a malformed file of `docs/`, a hand-written part for a method that does
/// not exist, an Example that does not parse. The Manual is built without it; a test requires there be none.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{origin}: {message}")]
pub struct Problem {
    /// Where: `docs/<path>` or a Page ID.
    pub origin: String,
    /// What is wrong.
    pub message: String,
}

/// One Page found by [`Manual::search`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    /// The Page ID.
    pub id: String,
    /// The Page's kind.
    pub kind: Kind,
    /// The title.
    pub title: String,
    /// The summary.
    pub summary: String,
    /// The first line of the body that has a word of the query, or the summary.
    pub snippet: String,
}

/// The Manual: every Page, its Examples and its search index.
#[derive(Debug, Clone)]
pub struct Manual {
    /// Every Page, by Page ID.
    pages: Vec<Page>,
    examples: Vec<Example>,
    problems: Vec<Problem>,
    errors: Vec<(i64, String)>,
    index: Index,
}

impl Manual {
    /// Build the Manual from the `api.schema` document (as `Dispatcher::schema` gives it), the report templates and
    /// the compiled-in `docs/`. Never fails: whatever cannot be used is left out and listed in [`Manual::problems`].
    pub fn build(schema: &Value, reports: &[ReportTemplate]) -> Self {
        let mut problems = Vec::new();
        let generated = [
            generate::method_pages(schema),
            generate::error_pages(schema),
            generate::report_pages(reports),
        ];
        let mut pages: BTreeMap<String, Page> = BTreeMap::new();
        generated.into_iter().flatten().chain([gfdl_page()]).for_each(|p| {
            if pages.contains_key(&p.id) {
                problems.push(problem(&p.id, "two registry entries give this Page ID"));
            }
            pages.insert(p.id.clone(), p);
        });
        sources::SOURCES.iter().for_each(|(path, text)| match source::parse(path, text) {
            Ok(s) => merge(&mut pages, s, &mut problems),
            Err(p) => problems.push(p),
        });
        let pages: Vec<Page> = pages.into_values().collect();
        let examples = pages.iter().flat_map(|p| page_examples(p, &mut problems)).collect();
        let index = Index::build(&pages.iter().collect::<Vec<_>>(), synonyms::table());
        Self {
            pages,
            examples,
            problems,
            errors: generate::error_names(schema),
            index,
        }
    }

    /// Every Page, by Page ID.
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// The Pages of one kind, by Page ID.
    pub fn pages_of(&self, kind: Kind) -> impl Iterator<Item = &Page> {
        self.pages.iter().filter(move |p| p.kind == kind)
    }

    /// The Page with this Page ID.
    pub fn get(&self, id: &str) -> Option<&Page> {
        self.pages.binary_search_by(|p| p.id.as_str().cmp(id)).ok().map(|i| &self.pages[i])
    }

    /// The Page a user names: a Page ID, else a method's name (`transaction.post`), an error code's name
    /// (`unbalanced`) or a report template's name (`Balance Sheet`).
    pub fn find(&self, name: &str) -> Option<&Page> {
        self.get(name)
            .or_else(|| self.get(&method_page_id(name)))
            .or_else(|| self.get(&error_page_id(name)))
            .or_else(|| self.get(&report_page_id(name)))
    }

    /// The Start Page.
    pub fn start(&self) -> Option<&Page> {
        self.get(START)
    }

    /// Pages ranked for `query`, best first, of `kind` if given, at most `limit`. Deterministic: the same Manual and
    /// query always give the same list.
    pub fn search(&self, query: &str, kind: Option<Kind>, limit: usize) -> Vec<Hit> {
        let said = words(query);
        self.index
            .search(query, kind, limit)
            .into_iter()
            .map(|i| &self.pages[i])
            .map(|p| Hit {
                id: p.id.clone(),
                kind: p.kind,
                title: p.title.clone(),
                summary: p.summary.clone(),
                snippet: snippet(p, &said),
            })
            .collect()
    }

    /// A Page as Markdown or text, with its Examples shown for `door`, its related Pages and its licence notice.
    pub fn render(&self, page: &Page, format: Format, door: Door) -> String {
        let markdown = render::markdown(page, door, self);
        match format {
            Format::Markdown => markdown,
            Format::Text => render::text(&markdown),
        }
    }

    /// One Example as Markdown for `door`, as [`Manual::render`] shows it.
    pub fn render_example(&self, example: &Example, door: Door) -> String {
        render::example_markdown(example, door, self)
    }

    /// Every Example, in Page ID order, then in order within its Page.
    pub fn examples(&self) -> &[Example] {
        &self.examples
    }

    /// The Examples of one Page.
    pub fn examples_of<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Example> {
        self.examples.iter().filter(move |e| e.page == id)
    }

    /// What could not be used in building the Manual.
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }
}

impl Lookup for Manual {
    fn title_of(&self, id: &str) -> Option<&str> {
        self.get(id).map(|p| p.title.as_str())
    }

    fn error_name(&self, code: i64) -> Option<&str> {
        self.errors.iter().find(|(c, _)| *c == code).map(|(_, n)| n.as_str())
    }
}

fn problem(origin: &str, message: impl Into<String>) -> Problem {
    Problem {
        origin: origin.to_owned(),
        message: message.into(),
    }
}

/// The Page `legal/gfdl`: the licence's text.
fn gfdl_page() -> Page {
    Page {
        id: "legal/gfdl".to_owned(),
        kind: Kind::Legal,
        title: "GNU Free Documentation License".to_owned(),
        summary: "The licence of the Manual's text: the GNU Free Documentation License, version 1.1.".to_owned(),
        related: vec!["legal/notices".to_owned()],
        from: None,
        body: format!("```text\n{}\n```\n", GFDL_TEXT.trim_end()),
        hand_written: Some(String::new()),
    }
}

/// Add a hand-written file: complete its Reference page, or add it as a Page of its own.
fn merge(pages: &mut BTreeMap<String, Page>, s: Source, problems: &mut Vec<Problem>) {
    let Some(kind) = Kind::of_id(&s.id) else {
        problems.push(problem(&s.id, "not a Page ID"));
        return;
    };
    let reference = matches!(kind, Kind::Method | Kind::Error | Kind::Report);
    match pages.get_mut(&s.id) {
        Some(page) if reference => complete(page, s),
        Some(_) => problems.push(problem(&s.id, "this Page is generated and takes no hand-written part")),
        None if reference => problems.push(problem(&s.id, format!("no {} of that name in the registries", kind.name()))),
        None => match own_page(kind, s) {
            Ok(page) => {
                pages.insert(page.id.clone(), page);
            }
            Err(p) => problems.push(p),
        },
    }
}

/// A Reference page with its hand-written part: the hand-written text leads, then the generated reference.
fn complete(page: &mut Page, s: Source) {
    let hand = s.body.trim().to_owned();
    page.body = format!("{hand}\n\n{}", page.body);
    page.hand_written = Some(hand);
    page.title = s.title.unwrap_or_else(|| page.title.clone());
    page.summary = s.summary.unwrap_or_else(|| page.summary.clone());
    page.related = s.related;
    page.from = s.from;
}

fn own_page(kind: Kind, s: Source) -> Result<Page, Problem> {
    let (Some(title), Some(summary)) = (s.title, s.summary) else {
        return Err(problem(&s.id, "a Page needs a title and a summary"));
    };
    let body = s.body.trim().to_owned();
    Ok(Page {
        id: s.id,
        kind,
        title,
        summary,
        related: s.related,
        from: s.from,
        body: format!("{body}\n"),
        hand_written: Some(body),
    })
}

/// The Examples of a Page; those that do not parse become problems.
fn page_examples(page: &Page, problems: &mut Vec<Problem>) -> Vec<Example> {
    example::pieces(&page.body)
        .into_iter()
        .filter_map(|p| match p {
            example::Piece::Example(block) => Some(block),
            example::Piece::Text(_) => None,
        })
        .zip(1..)
        .filter_map(|(block, n)| {
            example::parse(&page.id, n, &block)
                .map_err(|m| problems.push(problem(&page.id, format!("Example {n}: {m}"))))
                .ok()
        })
        .collect()
}

/// The first line of the body outside code that has one of `said`, without its Markdown markers; else the summary.
fn snippet(page: &Page, said: &[String]) -> String {
    let mut in_code = false;
    let line = page.body.lines().find(|line| {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            return false;
        }
        !in_code && words(line).iter().any(|w| said.contains(w))
    });
    let text = line.map_or(page.summary.as_str(), |l| l.trim().trim_start_matches(['#', '-', '*', '|', ' ']));
    shorten(&text.replace("**", ""), 200)
}

fn shorten(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "redAleApi": "0.6.0",
            "methods": [
                { "name": "book.open", "summary": "Open a book.", "effect": "read", "target": null,
                  "params": { "type": "object", "properties": {} }, "result": { "type": "object" } },
                { "name": "zz.unwritten", "summary": "Nobody wrote this up.", "effect": "write", "target": "book",
                  "params": { "type": "object" }, "result": { "type": "object" } },
            ],
            "errors": [{ "code": 1000, "name": "no-book" }],
            "$defs": {},
        })
    }

    #[test]
    fn the_compiled_manual_builds() {
        let manual = Manual::build(&schema(), &[]);
        let unexpected: Vec<&Problem> = manual.problems().iter().filter(|p| !p.message.contains("registries")).collect();
        assert!(unexpected.is_empty(), "{unexpected:?}");
        assert!(manual.start().is_some());
        assert!(manual.get("legal/gfdl").unwrap().body.contains("GNU Free Documentation License"));
        let ids: Vec<&str> = manual.pages().iter().map(|p| p.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn reference_pages_merge_their_hand_written_part() {
        let manual = Manual::build(&schema(), &[]);
        let open = manual.find("book.open").unwrap();
        assert_eq!(open.id, "method/book.open");
        assert!(open.hand_written.is_some());
        assert!(
            open.body.find("## Params").unwrap() > open.body.find("```example").unwrap(),
            "{}",
            open.body
        );
        assert!(manual.examples_of("method/book.open").next().is_some());
        assert!(manual.find("zz.unwritten").unwrap().hand_written.is_none());
        let gaps: Vec<String> = coverage::methods(&manual).iter().map(ToString::to_string).collect();
        assert_eq!(gaps, ["method/zz.unwritten text", "method/zz.unwritten example"]);
    }

    #[test]
    fn search_finds_the_start_page_and_methods() {
        let manual = Manual::build(&schema(), &[]);
        let hits = manual.search("open a book", None, 5);
        assert_eq!(hits.first().map(|h| h.id.as_str()), Some("method/book.open"), "{hits:?}");
        let methods = manual.search("book", Some(Kind::Method), 5);
        assert!(methods.iter().all(|h| h.kind == Kind::Method));
        assert_eq!(manual.search("open a book", None, 5), hits, "deterministic");
    }

    #[test]
    fn pages_render_as_text() {
        let manual = Manual::build(&schema(), &[]);
        let start = manual.start().unwrap();
        let text = manual.render(start, Format::Text, Door::Cli);
        assert!(text.starts_with(&format!("{}\n=", start.title)), "{text}");
        assert!(!text.contains("```"), "{text}");
    }

    #[test]
    fn snippets_are_short_lines_with_the_query() {
        let page = Page {
            id: "concept/x".into(),
            kind: Kind::Concept,
            title: "X".into(),
            summary: "Sum.".into(),
            related: Vec::new(),
            from: None,
            body: "```json\n{\"amount\": 1}\n```\n\n## The **amount**\n".into(),
            hand_written: None,
        };
        assert_eq!(snippet(&page, &["amount".to_owned()]), "The amount");
        assert_eq!(snippet(&page, &["zebra".to_owned()]), "Sum.");
        assert_eq!(shorten("abcdef", 3), "abc…");
    }
}
