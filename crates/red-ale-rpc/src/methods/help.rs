//! Methods: the Manual (`design/api/help.md`): its Pages listed, one Page rendered, and search. All read only the
//! Manual compiled into the binary, so they work with no book open.

use red_ale_docs::{Door, Format, Kind, Page};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::manual::manual;

/// The most Pages `help.search` returns.
const SEARCH_LIMIT_MAX: u32 = 100;
/// How many it returns without a `limit`.
const SEARCH_LIMIT_DEFAULT: u32 = 10;

/// The help methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "help.topics",
            "Every Page of red-ale's Manual, or those of one kind: Page ID, kind, title and summary.",
            Effect::Read,
            topics,
        ),
        method(
            "help.get",
            "One Page of the Manual, by Page ID or method name, as Markdown or text, with its Examples for one door.",
            Effect::Read,
            get,
        ),
        method(
            "help.search",
            "Pages of the Manual ranked for a question in your own words, best first, each with a snippet.",
            Effect::Read,
            search,
        ),
    ]
}

/// What a Page is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PageKind {
    /// A method's Reference page, `method/<name>`.
    Method,
    /// An error code's Reference page, `error/<name>`.
    Error,
    /// A report template's Reference page, `report/<name>`.
    Report,
    /// A Concept page, `concept/<name>`.
    Concept,
    /// A Guide, `guide/<name>`.
    Guide,
    /// A From GnuCash page, `gnucash/<name>`: GnuCash's menus and dialogs mapped to methods.
    Gnucash,
    /// The Start Page, `start`.
    Start,
    /// The licence and notices, `legal/<name>`.
    Legal,
}

impl From<PageKind> for Kind {
    fn from(k: PageKind) -> Self {
        match k {
            PageKind::Method => Kind::Method,
            PageKind::Error => Kind::Error,
            PageKind::Report => Kind::Report,
            PageKind::Concept => Kind::Concept,
            PageKind::Guide => Kind::Guide,
            PageKind::Gnucash => Kind::Gnucash,
            PageKind::Start => Kind::Start,
            PageKind::Legal => Kind::Legal,
        }
    }
}

impl From<Kind> for PageKind {
    fn from(k: Kind) -> Self {
        match k {
            Kind::Method => PageKind::Method,
            Kind::Error => PageKind::Error,
            Kind::Report => PageKind::Report,
            Kind::Concept => PageKind::Concept,
            Kind::Guide => PageKind::Guide,
            Kind::Gnucash => PageKind::Gnucash,
            Kind::Start => PageKind::Start,
            Kind::Legal => PageKind::Legal,
        }
    }
}

/// How `help.get` renders a Page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PageFormat {
    /// Markdown.
    #[default]
    Markdown,
    /// Plain text for a terminal.
    Text,
}

/// The door a Page's Examples are shown for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PageDoor {
    /// JSON-RPC 2.0 requests and responses.
    #[default]
    Jsonrpc,
    /// `red-ale exec` command lines and what they print.
    Cli,
    /// MCP tool calls and their results.
    Mcp,
}

/// Params of `help.topics`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HelpTopicsParams {
    /// Only Pages of this kind; every Page by default.
    #[serde(default)]
    pub kind: Option<PageKind>,
}

/// One Page, as lists give it.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelpTopic {
    /// The Page ID: `method/transaction.post`, `concept/value-and-amount`, `start`.
    pub id: String,
    /// What the Page is about.
    pub kind: PageKind,
    /// The title.
    pub title: String,
    /// One line on what it covers.
    pub summary: String,
}

impl From<&Page> for HelpTopic {
    fn from(p: &Page) -> Self {
        Self {
            id: p.id.clone(),
            kind: p.kind.into(),
            title: p.title.clone(),
            summary: p.summary.clone(),
        }
    }
}

/// Result of `help.topics`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelpTopics {
    /// The Pages, by Page ID.
    pub topics: Vec<HelpTopic>,
}

#[allow(clippy::unnecessary_wraps)] // Every handler returns a Result.
fn topics(_: &mut Context, p: HelpTopicsParams) -> Result<HelpTopics, RpcError> {
    let kind = p.kind.map(Kind::from);
    let topics = manual()
        .pages()
        .iter()
        .filter(|page| kind.is_none_or(|k| page.kind == k))
        .map(HelpTopic::from)
        .collect();
    Ok(HelpTopics { topics })
}

/// Params of `help.get`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HelpGetParams {
    /// The Page ID (`method/transaction.post`), or a method's, error code's or report template's name
    /// (`transaction.post`, `unbalanced`, `Balance Sheet`).
    pub id: String,
    /// `markdown` (the default) or `text`.
    #[serde(default)]
    pub format: PageFormat,
    /// Which door the Examples are shown for: `jsonrpc` (the default), `cli` or `mcp`.
    #[serde(default)]
    pub door: PageDoor,
}

/// Result of `help.get`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelpPage {
    /// The Page ID.
    pub id: String,
    /// What the Page is about.
    pub kind: PageKind,
    /// The title.
    pub title: String,
    /// One line on what it covers.
    pub summary: String,
    /// The Page IDs of related Pages.
    pub related: Vec<String>,
    /// The section of GnuCash's documentation the Page adapts, if any.
    pub from: Option<String>,
    /// The format of `content`.
    pub format: PageFormat,
    /// The door `content` shows Examples for.
    pub door: PageDoor,
    /// The Page, ending with its licence notice.
    pub content: String,
}

fn get(_: &mut Context, p: HelpGetParams) -> Result<HelpPage, RpcError> {
    let manual = manual();
    let page = manual.find(&p.id).ok_or_else(|| RpcError::not_found("page", &p.id))?;
    let format = match p.format {
        PageFormat::Markdown => Format::Markdown,
        PageFormat::Text => Format::Text,
    };
    let door = match p.door {
        PageDoor::Jsonrpc => Door::JsonRpc,
        PageDoor::Cli => Door::Cli,
        PageDoor::Mcp => Door::Mcp,
    };
    Ok(HelpPage {
        id: page.id.clone(),
        kind: page.kind.into(),
        title: page.title.clone(),
        summary: page.summary.clone(),
        related: page.related.clone(),
        from: page.from.clone(),
        format: p.format,
        door: p.door,
        content: manual.render(page, format, door),
    })
}

/// Params of `help.search`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HelpSearchParams {
    /// What to look for, in any words: a question, a GnuCash menu item ("Stock Split…"), a method name.
    pub query: String,
    /// Only Pages of this kind.
    #[serde(default)]
    pub kind: Option<PageKind>,
    /// The most Pages to return, 1 to 100; 10 by default.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// One Page `help.search` found.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelpHit {
    /// The Page ID.
    pub id: String,
    /// What the Page is about.
    pub kind: PageKind,
    /// The title.
    pub title: String,
    /// One line on what it covers.
    pub summary: String,
    /// A line of the Page with a word of the query, or its summary.
    pub snippet: String,
}

/// Result of `help.search`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelpSearchResult {
    /// The Pages found, best first; ties in Page ID order.
    pub hits: Vec<HelpHit>,
}

fn search(_: &mut Context, p: HelpSearchParams) -> Result<HelpSearchResult, RpcError> {
    let limit = p.limit.unwrap_or(SEARCH_LIMIT_DEFAULT);
    if !(1..=SEARCH_LIMIT_MAX).contains(&limit) {
        return Err(RpcError::invalid_params(format!(
            "limit must be 1 to {SEARCH_LIMIT_MAX}, not {limit}"
        )));
    }
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let hits = manual()
        .search(&p.query, p.kind.map(Kind::from), limit)
        .into_iter()
        .map(|h| HelpHit {
            id: h.id,
            kind: h.kind.into(),
            title: h.title,
            summary: h.summary,
            snippet: h.snippet,
        })
        .collect();
    Ok(HelpSearchResult { hits })
}
