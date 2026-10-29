//! `llms.txt` and `llms-full.txt`, for language models reading the site (<https://llmstxt.org>).

use super::{page_path, plural};
use crate::page::Kind;
use crate::render::{Door, Format};
use crate::{Manual, SPDX_LINE};

/// `llms.txt`: the Manual's name and summary, then a link, title and summary for every Page, by kind.
pub(super) fn index(manual: &Manual) -> String {
    let summary = manual.start().map(|p| p.summary.as_str()).unwrap_or_default();
    let sections: String = Kind::ALL.into_iter().map(|kind| section(manual, kind)).collect();
    format!(
        "# red-ale Manual\n\n> {summary}\n\nEach link is a Page of the Manual as HTML; its Page ID is the path without `.html`. \
         `llms-full.txt` holds every Page as Markdown in one file, and `schema.json` is the API's schema document.\n\n\
         {sections}## Optional\n\n\
         - [llms-full.txt](llms-full.txt): every Page as Markdown, in one file\n\
         - [schema.json](schema.json): the API's schema document (`api.schema`)\n"
    )
}

fn section(manual: &Manual, kind: Kind) -> String {
    let items = manual
        .pages_of(kind)
        .map(|p| format!("- [{}]({}): {}\n", p.title, page_path(&p.id), p.summary))
        .collect::<Vec<_>>()
        .concat();
    format!("## {}\n\n{items}\n", plural(kind))
}

/// `llms-full.txt`: every Page as Markdown, Examples shown as JSON-RPC, in Page ID order.
pub(super) fn full(manual: &Manual) -> String {
    let pages: Vec<String> = manual
        .pages()
        .iter()
        .map(|p| manual.render(p, Format::Markdown, Door::JsonRpc))
        .collect();
    format!(
        "{SPDX_LINE}\n\nThis file holds every Page of the red-ale Manual as Markdown, in Page ID order, each ending with its \
         licence notice. A link whose target is a Page ID points at that Page in this file. Examples are shown as JSON-RPC \
         requests and responses; the same call is the method of the same name through `red-ale exec` or as an MCP tool.\n\n\
         ---\n\n{}",
        pages.join("\n---\n\n")
    )
}
