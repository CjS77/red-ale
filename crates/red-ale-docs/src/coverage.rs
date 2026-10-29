//! Coverage checks: does the Manual cover the registries, and does every reference in it resolve
//! (`design/manual.md`, "Coverage is checked")? Each check returns the gaps it finds, sorted; none means covered.
//!
//! While the Manual is being written, a test compares the gaps with a committed list of known ones that may only
//! shrink (`docs/coverage-pending.txt`), so the checks can run before the content exists.

use std::fmt;

use pulldown_cmark::{Event, Parser, Tag};

use crate::Manual;
use crate::page::{Kind, Page};

/// One thing the Manual lacks. Shown as `<page-id> <what>`: `method/book.info text`, `method/book.info example`,
/// `concept/x link method/nothing`, `guide/y from guide/ch_none.docbook`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Gap {
    /// The Page with the gap.
    pub page: String,
    /// What is missing: `text` (no hand-written part), `example` (no Example), `link <id>` (a link to a Page that
    /// does not exist), `from <section>` (a source not in the checklist).
    pub what: String,
}

impl fmt::Display for Gap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.page, self.what)
    }
}

fn gap(page: &Page, what: impl Into<String>) -> Gap {
    Gap {
        page: page.id.clone(),
        what: what.into(),
    }
}

/// Every method has a Reference page with a hand-written part and at least one Example.
pub fn methods(manual: &Manual) -> Vec<Gap> {
    text_and_examples(manual, Kind::Method)
}

/// Every error code has a hand-written cause and fix.
pub fn errors(manual: &Manual) -> Vec<Gap> {
    manual
        .pages_of(Kind::Error)
        .filter(|p| p.hand_written.is_none())
        .map(|p| gap(p, "text"))
        .collect()
}

/// Every report template has a Page with a hand-written part and at least one Example.
pub fn reports(manual: &Manual) -> Vec<Gap> {
    text_and_examples(manual, Kind::Report)
}

fn text_and_examples(manual: &Manual, kind: Kind) -> Vec<Gap> {
    manual
        .pages_of(kind)
        .flat_map(|p| {
            let text = p.hand_written.is_none().then(|| gap(p, "text"));
            let example = (manual.examples_of(&p.id).next().is_none()).then(|| gap(p, "example"));
            text.into_iter().chain(example)
        })
        .collect()
}

/// Every Page ID a Page links to, in its body or its `related`, exists.
pub fn links(manual: &Manual) -> Vec<Gap> {
    let mut gaps: Vec<Gap> = manual
        .pages()
        .iter()
        .flat_map(|p| {
            let targets = p.related.iter().cloned().chain(page_links(&p.body));
            targets
                .filter(|t| manual.get(t).is_none())
                .map(|t| gap(p, format!("link {t}")))
                .collect::<Vec<_>>()
        })
        .collect();
    gaps.sort();
    gaps.dedup();
    gaps
}

/// Every `from` names an entry of the GnuCash documentation checklist.
pub fn sources(manual: &Manual, checklist: &[&str]) -> Vec<Gap> {
    manual
        .pages()
        .iter()
        .filter_map(|p| {
            p.from
                .as_deref()
                .filter(|f| !checklist.contains(f))
                .map(|f| gap(p, format!("from {f}")))
        })
        .collect()
}

/// Every check, sorted.
pub fn all(manual: &Manual, checklist: &[&str]) -> Vec<Gap> {
    let mut gaps: Vec<Gap> = [
        methods(manual),
        errors(manual),
        reports(manual),
        links(manual),
        sources(manual, checklist),
    ]
    .into_iter()
    .flatten()
    .collect();
    gaps.sort();
    gaps
}

/// The Page IDs a Markdown body links to: every link target with no scheme that does not start with `#`, `/` or
/// `.`, without its `#anchor`.
pub fn page_links(body: &str) -> Vec<String> {
    Parser::new(body)
        .filter_map(|event| match event {
            Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.to_string()),
            _ => None,
        })
        .filter(|d| !d.contains(':') && !d.starts_with(['#', '/', '.']) && !d.is_empty())
        .map(|d| d.split('#').next().unwrap_or_default().to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_page_ids() {
        let body = "See [a](concept/a#part), [b](https://x.org), [c](#here), [d](method/x.y) and `[e](no)`.";
        assert_eq!(page_links(body), ["concept/a", "method/x.y"]);
    }
}
