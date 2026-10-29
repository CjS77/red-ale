//! The Manual through MCP's own ways of offering context (`design/manual.md`, "The doors"): every Page as a
//! resource, a resource template for methods' Reference pages, the server's instructions from the Start Page, and
//! tool descriptions from the Reference pages.

use red_ale_docs::{Door, Format, Manual, Page};
use rmcp::model::{Resource, ResourceContents, ResourceTemplate};

/// Every Page's resource URI is this followed by its Page ID.
pub const URI_PREFIX: &str = "red-ale://manual/";

/// The resource template of methods' Reference pages, `{name}` being the method's name.
pub const METHOD_TEMPLATE: &str = "red-ale://manual/method/{name}";

/// The media type of every Page's resource: the Page rendered as Markdown.
pub const MIME_TYPE: &str = "text/markdown";

/// The most words a tool description takes, the Page ID included.
pub const DESCRIPTION_BUDGET: usize = 60;

/// How much each tool's description says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolDescriptions {
    /// The method's summary, the lead paragraph of its Reference page within [`DESCRIPTION_BUDGET`] words, and its
    /// Page ID.
    #[default]
    Full,
    /// The method's one-line summary only, for clients short of context.
    Short,
}

/// A Page's resource URI.
pub fn uri(page_id: &str) -> String {
    format!("{URI_PREFIX}{page_id}")
}

/// Every Page as a resource, by Page ID.
pub fn resources(manual: &Manual) -> Vec<Resource> {
    manual
        .pages()
        .iter()
        .map(|p| {
            Resource::new(uri(&p.id), p.id.clone())
                .with_title(p.title.clone())
                .with_description(p.summary.clone())
                .with_mime_type(MIME_TYPE)
        })
        .collect()
}

/// The resource templates: one, for methods' Reference pages.
pub fn templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(METHOD_TEMPLATE, "method")
            .with_title("Method reference")
            .with_description("The Reference page of the method `name`: what it is for, its params, result and Examples.")
            .with_mime_type(MIME_TYPE),
    ]
}

/// The contents of the resource at `uri`: its Page as Markdown, with Examples shown as MCP tool calls. `None` when
/// `uri` names no Page.
pub fn read(manual: &Manual, uri: &str) -> Option<ResourceContents> {
    let page = manual.get(uri.strip_prefix(URI_PREFIX)?)?;
    let text = manual.render(page, Format::Markdown, Door::Mcp);
    Some(ResourceContents::text(text, uri).with_mime_type(MIME_TYPE))
}

/// The server's instructions: the Start Page's summary, and where the rest of the Manual is. `None` without a Start
/// Page.
pub fn instructions(manual: &Manual) -> Option<String> {
    let start = manual.start()?;
    Some(format!(
        "{} Every Page of the Manual is also a resource, {}, starting at {}.",
        start.summary,
        uri("<page-id>"),
        uri(&start.id)
    ))
}

/// A tool's description: `summary` alone when `descriptions` is short or there is no `page`; else the summary,
/// as much of the page's lead paragraph as fits the budget in whole sentences, and the Page ID.
pub fn tool_description(summary: &str, page: Option<&Page>, descriptions: ToolDescriptions) -> String {
    let Some(page) = page.filter(|_| descriptions == ToolDescriptions::Full) else {
        return summary.to_owned();
    };
    let ending = format!("Manual: {}", page.id);
    let room = DESCRIPTION_BUDGET.saturating_sub(word_count(summary) + word_count(&ending));
    let lead = page.lead().map(|l| fit(l, room)).filter(|l| !l.is_empty());
    [Some(summary.to_owned()), lead, Some(ending)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// `text` on one line, cut to at most `room` words: the most whole sentences that fit, else the first words and `…`.
fn fit(text: &str, room: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= room {
        return words.join(" ");
    }
    let sentence_end = (1..=room).rev().find(|&n| words[n - 1].ends_with(['.', '!', '?']));
    match sentence_end {
        Some(n) => words[..n].join(" "),
        None if room == 0 => String::new(),
        None => format!("{}…", words[..room].join(" ")),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use red_ale_docs::Kind;

    use super::*;

    fn page(hand_written: Option<&str>) -> Page {
        Page {
            id: "method/x.do".into(),
            kind: Kind::Method,
            title: "x.do".into(),
            summary: "Do x.".into(),
            related: Vec::new(),
            from: None,
            body: String::new(),
            hand_written: hand_written.map(str::to_owned),
        }
    }

    #[test]
    fn whole_sentences_fit_the_room_or_words_are_cut() {
        assert_eq!(fit("One two.\nThree four.", 5), "One two. Three four.");
        assert_eq!(fit("One two. Three four five.", 4), "One two.");
        assert_eq!(fit("One two three four five.", 3), "One two three…");
        assert_eq!(fit("One two three.", 0), "");
    }

    #[test]
    fn descriptions_join_summary_lead_and_page_id() {
        let full = page(Some("Use it to do x.\nIt does x.\n\n## Examples\n"));
        assert_eq!(
            tool_description("Do x.", Some(&full), ToolDescriptions::Full),
            "Do x. Use it to do x. It does x. Manual: method/x.do"
        );
        assert_eq!(tool_description("Do x.", Some(&full), ToolDescriptions::Short), "Do x.");
        assert_eq!(
            tool_description("Do x.", Some(&page(None)), ToolDescriptions::Full),
            "Do x. Manual: method/x.do"
        );
        assert_eq!(tool_description("Do x.", None, ToolDescriptions::Full), "Do x.");
    }

    #[test]
    fn a_long_lead_stays_within_the_budget() {
        let long = "Word word word word word. ".repeat(30);
        let description = tool_description("Do x.", Some(&page(Some(&long))), ToolDescriptions::Full);
        assert!(word_count(&description) <= DESCRIPTION_BUDGET, "{description}");
        assert!(description.ends_with("word. Manual: method/x.do"), "{description}");
    }
}
