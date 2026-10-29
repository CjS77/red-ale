//! Coverage checks: does the Manual cover the registries, and does every reference in it resolve
//! (`design/manual.md`, "Coverage is checked")? Each check returns the gaps it finds, sorted; none means covered.
//!
//! While the Manual is being written, a test compares the gaps with a committed list of known ones that may only
//! shrink (`docs/coverage-pending.txt`), so the checks can run before the content exists.

use std::collections::BTreeSet;
use std::fmt;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::page::{Kind, Page};
use crate::{Manual, synonyms, words};

/// One thing the Manual lacks. Shown as `<page-id> <what>`: `method/book.info text`, `method/book.info example`,
/// `concept/x link method/nothing`, `guide/y from guide/ch_none.docbook`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Gap {
    /// The Page with the gap.
    pub page: String,
    /// What is missing: `text` (no hand-written part), `example` (no Example), `link <id>` (a link to a Page that
    /// does not exist), `from <section>` (a source that is not a checklist row listing the Page), `checklist` (the
    /// checklist lists a Page that does not exist) or `term <words>` (a term the glossary does not define).
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

/// One row of the GnuCash documentation checklist (`design/gnucash-docs-checklist.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistRow {
    /// The section as a `from` names it: `<book>/<file>#<section id>`.
    pub section: String,
    /// The Page IDs the row lists; none when the row says `-`.
    pub pages: Vec<String>,
}

/// The rows of the checklist's table, whose columns are book, file, section id, title, classification and Page IDs.
/// A row is a line starting `|` whose first cell is a book's name in lower case, so the header and the separator are
/// not rows.
pub fn checklist(text: &str) -> Vec<ChecklistRow> {
    text.lines().filter_map(checklist_row).collect()
}

fn checklist_row(line: &str) -> Option<ChecklistRow> {
    let cells: Vec<&str> = line.strip_prefix('|')?.split('|').map(str::trim).collect();
    let [book, file, id, _title, _class, pages, ..] = cells.as_slice() else {
        return None;
    };
    let is_book = !book.is_empty() && book.chars().all(|c| c.is_ascii_lowercase());
    is_book.then(|| ChecklistRow {
        section: format!("{book}/{file}#{id}"),
        pages: pages
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty() && *p != "-")
            .map(str::to_owned)
            .collect(),
    })
}

/// Every `from` names a checklist row, and that row lists the Page.
pub fn sources(manual: &Manual, checklist: &[ChecklistRow]) -> Vec<Gap> {
    manual
        .pages()
        .iter()
        .filter_map(|p| {
            p.from
                .as_deref()
                .filter(|f| !checklist.iter().any(|r| r.section == *f && r.pages.contains(&p.id)))
                .map(|f| gap(p, format!("from {f}")))
        })
        .collect()
}

/// Every Page ID the checklist lists exists. The gap is the missing Page's: `guide/x checklist`.
pub fn checklist_pages(manual: &Manual, checklist: &[ChecklistRow]) -> Vec<Gap> {
    let missing: BTreeSet<&String> = checklist
        .iter()
        .flat_map(|r| &r.pages)
        .filter(|id| manual.get(id).is_none())
        .collect();
    missing
        .into_iter()
        .map(|id| Gap {
            page: id.clone(),
            what: "checklist".to_owned(),
        })
        .collect()
}

/// Every term a Concept page uses is a glossary term. A Concept page marks a term by writing it in bold; bold that
/// opens a list item (a label), holds code, ends with `.`, `!`, `?` or `:` (a sentence or a lead-in), or runs past
/// four words is not a term. A term is defined when its search words equal those of a phrase the glossary defines
/// ([`synonyms::terms_of_glossary`]), so case, plurals and small words like "the" don't count. The gap is
/// `term <bold text>`.
pub fn terms(manual: &Manual, glossary: &BTreeSet<Vec<String>>) -> Vec<Gap> {
    let mut gaps: Vec<Gap> = manual
        .pages_of(Kind::Concept)
        .flat_map(|p| {
            bold_terms(&p.body)
                .into_iter()
                .filter(|t| !glossary.contains(&words(t)) && !glossary.contains(&singular_es(words(t))))
                .map(|t| gap(p, format!("term {t}")))
                .collect::<Vec<_>>()
        })
        .collect();
    gaps.sort();
    gaps.dedup();
    gaps
}

/// Search words with the plurals in `-es` that the search's stemmer only cuts to `-e` (`matches` to `matche`) made
/// singular (`match`). Stemming them in search itself would change its ranking.
fn singular_es(words: Vec<String>) -> Vec<String> {
    words
        .into_iter()
        .map(|w| match w.strip_suffix('e') {
            Some(base) if ["ch", "sh", "ss", "x"].iter().any(|e| base.ends_with(e)) => base.to_owned(),
            _ => w,
        })
        .collect()
}

/// The bold phrases of a Markdown body that are terms rather than emphasis or labels ([`terms`]).
pub fn bold_terms(body: &str) -> Vec<String> {
    bold_spans(body)
        .into_iter()
        .filter(|s| !s.has_code && !s.opens_item)
        .map(|s| s.text.trim().to_owned())
        .filter(|t| !t.ends_with(['.', '!', '?', ':']) && t.split_whitespace().count() <= 4 && !words(t).is_empty())
        .collect()
}

/// One outermost bold span of a Markdown body.
#[derive(Default)]
struct Bold {
    text: String,
    has_code: bool,
    /// The span is the first thing in a list item, so it labels the item.
    opens_item: bool,
}

fn bold_spans(body: &str) -> Vec<Bold> {
    let mut depth = 0_usize;
    let mut item_start = false;
    let mut spans: Vec<Bold> = Vec::new();
    Parser::new(body).for_each(|event| {
        let starts_item = matches!(event, Event::Start(Tag::Item)) || (item_start && matches!(event, Event::Start(Tag::Paragraph)));
        match event {
            Event::Start(Tag::Strong) => {
                if depth == 0 {
                    spans.push(Bold {
                        opens_item: item_start,
                        ..Bold::default()
                    });
                }
                depth += 1;
            }
            Event::End(TagEnd::Strong) => depth = depth.saturating_sub(1),
            Event::Text(t) if depth > 0 => spans.iter_mut().last().into_iter().for_each(|s| s.text.push_str(&t)),
            Event::SoftBreak | Event::HardBreak if depth > 0 => spans.iter_mut().last().into_iter().for_each(|s| s.text.push(' ')),
            Event::Code(_) if depth > 0 => spans.iter_mut().last().into_iter().for_each(|s| s.has_code = true),
            _ => {}
        }
        item_start = starts_item;
    });
    spans
}

/// Every check, sorted. The checklist's rows are passed in; the glossary is the compiled-in one.
pub fn all(manual: &Manual, checklist: &[ChecklistRow]) -> Vec<Gap> {
    let mut gaps: Vec<Gap> = [
        methods(manual),
        errors(manual),
        reports(manual),
        links(manual),
        sources(manual, checklist),
        checklist_pages(manual, checklist),
        terms(manual, &synonyms::glossary_terms()),
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

    #[test]
    fn checklist_rows_are_the_table_body() {
        let text = "Intro | not a row.\n\n| Book | File | Section id | Title | Classification | Page IDs |\n\
            |---|---|---|---|---|---|\n\
            | guide | ch_a.docbook | a-1 | A | concept | start, concept/a |\n\
            | manual | ch_b.docbook | (no id) | B | gnu-only | - |\n";
        let rows = checklist(text);
        assert_eq!(
            rows,
            [
                ChecklistRow {
                    section: "guide/ch_a.docbook#a-1".into(),
                    pages: vec!["start".into(), "concept/a".into()],
                },
                ChecklistRow {
                    section: "manual/ch_b.docbook#(no id)".into(),
                    pages: Vec::new(),
                },
            ]
        );
    }

    #[test]
    fn bold_terms_leave_out_emphasis() {
        let body = "A **Split** of **`code`** or **the `x` field**. **Locks.** **It refuses, rather than repairs.** \
            **Lead-in:** **one two three four five** **⚠** **unrealised *gain***, **Import\nproposal**.\n\n\
            - **Label**: an item with a **Lot**.\n- A **Price** in an item.\n";
        assert_eq!(bold_terms(body), ["Split", "unrealised gain", "Import proposal", "Lot", "Price"]);
    }

    #[test]
    fn es_plurals_are_singular() {
        assert_eq!(singular_es(words("Candidate matches, taxes")), ["candidate", "match", "tax"]);
    }
}
