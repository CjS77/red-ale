//! The static site: the Manual as HTML files (`design/manual.md`, "The static site"). Nothing here does I/O; [`Manual::site`]
//! returns the files and the caller writes them.
//!
//! The rules are the report renderer's (`design/reports.md` §8): one stylesheet, no script except the search page's,
//! no request outside the site, and the same bytes on every build.
//!
//! Layout: the Start Page is `index.html`; every other Page is `<page id>.html`; each kind has `<kind>/index.html`;
//! `search.html` with `search.js` and `search-index.js`; `style.css`; `schema.json`; `llms.txt`; `llms-full.txt`.
//! Links are relative, so the site works from `file://` and from any sub-path.

mod llms;
mod markdown;
mod search;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::Value;

use crate::page::{Kind, Page};
use crate::render;
use crate::{Manual, START};
use markdown::Renderer;

const STYLE: &str = include_str!("style.css");
const SEARCH_JS: &str = include_str!("search.js");

/// The Manual's name on the site.
const SITE_TITLE: &str = "red-ale Manual";

/// One file of the site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteFile {
    /// Where it goes, relative to the site's directory, with `/` separators.
    pub path: String,
    /// What it holds.
    pub bytes: Vec<u8>,
}

impl SiteFile {
    fn text(path: impl Into<String>, text: String) -> Self {
        Self {
            path: path.into(),
            bytes: text.into_bytes(),
        }
    }
}

/// The file that holds a Page, relative to the site's directory.
pub fn page_path(id: &str) -> String {
    if id == START {
        "index.html".to_owned()
    } else {
        format!("{id}.html")
    }
}

/// The index file of a kind.
fn index_path(kind: Kind) -> String {
    format!("{}/index.html", kind.name())
}

/// `to` as a link from the file `from`: up one level for each directory `from` is in.
fn relative(from: &str, to: &str) -> String {
    format!("{}{to}", "../".repeat(from.matches('/').count()))
}

impl Manual {
    /// The static site: every file, sorted by path. `schema` is the `api.schema` document, published as `schema.json`.
    pub fn site(&self, schema: &Value) -> Vec<SiteFile> {
        let mut files: Vec<SiteFile> = self
            .pages()
            .iter()
            .map(|p| page_file(self, p))
            .chain(Kind::ALL.into_iter().filter(|k| *k != Kind::Start).map(|k| kind_index(self, k)))
            .chain([search_page(), SiteFile::text("style.css", STYLE.to_owned())])
            .chain([SiteFile::text("search.js", SEARCH_JS.to_owned())])
            .chain([SiteFile::text("search-index.js", search::index_script(self))])
            .chain([SiteFile::text("schema.json", schema_json(schema))])
            .chain([
                SiteFile::text("llms.txt", llms::index(self)),
                SiteFile::text("llms-full.txt", llms::full(self)),
            ])
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files
    }
}

fn schema_json(schema: &Value) -> String {
    format!("{}\n", serde_json::to_string_pretty(schema).unwrap_or_default())
}

/// The singular name of a kind, for the label above a Page's title.
fn singular(kind: Kind) -> &'static str {
    match kind {
        Kind::Method => "Method",
        Kind::Error => "Error code",
        Kind::Report => "Report",
        Kind::Concept => "Concept",
        Kind::Guide => "Guide",
        Kind::Gnucash => "From GnuCash",
        Kind::Start => "Start",
        Kind::Legal => "Legal",
    }
}

/// The plural name of a kind, for its index and headings.
fn plural(kind: Kind) -> &'static str {
    match kind {
        Kind::Method => "Methods",
        Kind::Error => "Errors",
        Kind::Report => "Reports",
        Kind::Concept => "Concepts",
        Kind::Guide => "Guides",
        Kind::Gnucash => "From GnuCash",
        Kind::Start => "Start",
        Kind::Legal => "Notices, History and licence",
    }
}

/// What a kind's index says it holds.
fn blurb(kind: Kind) -> &'static str {
    match kind {
        Kind::Method => "Every JSON-RPC method, which is also an MCP tool and a `red-ale exec` command.",
        Kind::Error => "Every error code the API gives, with what to do about it.",
        Kind::Report => "Every report template, with its options.",
        Kind::Concept => "What red-ale's words mean and how its books behave.",
        Kind::Guide => "Step-by-step ways to get things done.",
        Kind::Gnucash => "GnuCash's menus and dialogs, mapped to red-ale methods.",
        Kind::Start => "Where to begin.",
        Kind::Legal => "The notices, History and licence of the Manual's text.",
    }
}

/// Escape text for HTML content and attribute values.
fn esc(text: &str) -> String {
    text.chars().fold(String::with_capacity(text.len()), |mut out, c| {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
        out
    })
}

/// A Page's file: kind, title, summary, body, related Pages and the licence footer.
fn page_file(manual: &Manual, page: &Page) -> SiteFile {
    let path = page_path(&page.id);
    let md = Renderer {
        manual,
        from: &path,
        page: Some(page),
    };
    let mut main = format!("<article>\n{}<h1>{}</h1>\n", kind_label(page.kind, &path), esc(&page.title));
    if !page.summary.is_empty() {
        let _ = writeln!(main, "<p class=\"summary\">{}</p>", esc(&page.summary));
    }
    main.push_str(&md.html(&page.body));
    main.push_str(&related(manual, page, &path));
    if page.id == START {
        main.push_str(&browse(manual));
    }
    main.push_str("</article>\n");
    let footer = Renderer { page: None, ..md }.html(&render::footer(page));
    let title = if page.id == START {
        SITE_TITLE.to_owned()
    } else {
        format!("{} - {SITE_TITLE}", page.title)
    };
    SiteFile::text(&path, document(&path, &title, &page.summary, &main, &footer, ""))
}

fn kind_label(kind: Kind, from: &str) -> String {
    if kind == Kind::Start {
        return String::new();
    }
    format!(
        "<p class=\"kind\"><a href=\"{}\">{}</a></p>\n",
        relative(from, &index_path(kind)),
        singular(kind)
    )
}

fn related(manual: &Manual, page: &Page, from: &str) -> String {
    let items = page
        .related
        .iter()
        .filter_map(|id| manual.get(id))
        .map(|p| format!("<li><a href=\"{}\">{}</a></li>\n", relative(from, &page_path(&p.id)), esc(&p.title)))
        .collect::<Vec<_>>()
        .concat();
    if items.is_empty() {
        return String::new();
    }
    format!("<nav class=\"related\" aria-labelledby=\"related\">\n<h2 id=\"related\">Related</h2>\n<ul>\n{items}</ul>\n</nav>\n")
}

/// The home page's list of the Manual's parts, and of the files meant for tools.
fn browse(manual: &Manual) -> String {
    let parts = Kind::ALL
        .into_iter()
        .filter(|k| *k != Kind::Start)
        .map(|k| {
            format!(
                "<li><a href=\"{}\">{}</a> ({}): {}</li>\n",
                index_path(k),
                plural(k),
                manual.pages_of(k).count(),
                esc(blurb(k).replace('`', "").as_str())
            )
        })
        .collect::<Vec<_>>()
        .concat();
    format!(
        "<section class=\"browse\" aria-labelledby=\"browse\">\n<h2 id=\"browse\">Browse the Manual</h2>\n<ul>\n{parts}</ul>\n\
         <p>For tools: <a href=\"llms.txt\">llms.txt</a> (an index), <a href=\"llms-full.txt\">llms-full.txt</a> (every Page \
         as Markdown) and <a href=\"schema.json\">schema.json</a> (the API's schema).</p>\n</section>\n"
    )
}

/// The index of one kind. Methods are grouped by namespace (`transaction.post` is under `transaction`).
fn kind_index(manual: &Manual, kind: Kind) -> SiteFile {
    let path = index_path(kind);
    let mut groups: BTreeMap<String, Vec<&Page>> = BTreeMap::new();
    manual.pages_of(kind).for_each(|p| groups.entry(group_of(p)).or_default().push(p));
    let lists: String = groups.iter().map(|(name, pages)| group_html(&path, kind, name, pages)).collect();
    let intro = Renderer {
        manual,
        from: &path,
        page: None,
    }
    .html(blurb(kind));
    let main = format!("<article>\n<h1>{}</h1>\n{intro}{lists}</article>\n", plural(kind));
    let title = format!("{} - {SITE_TITLE}", plural(kind));
    SiteFile::text(&path, document(&path, &title, blurb(kind), &main, &site_footer(&path), ""))
}

/// The namespace of a method (`transaction`), empty for every other kind.
fn group_of(page: &Page) -> String {
    match (page.kind, page.id.strip_prefix("method/")) {
        (Kind::Method, Some(name)) => name.split('.').next().unwrap_or_default().to_owned(),
        _ => String::new(),
    }
}

fn group_html(from: &str, kind: Kind, name: &str, pages: &[&Page]) -> String {
    let heading = if name.is_empty() {
        String::new()
    } else {
        format!("<h2 id=\"{0}\">{0}</h2>\n", esc(name))
    };
    let items = pages
        .iter()
        .map(|p| {
            format!(
                "<li><a href=\"{}\">{}</a><span class=\"about\">{}</span></li>\n",
                relative(from, &page_path(&p.id)),
                esc(&p.title),
                esc(&p.summary)
            )
        })
        .collect::<Vec<_>>()
        .concat();
    let class = if kind == Kind::Method {
        " class=\"pages compact\""
    } else {
        " class=\"pages\""
    };
    format!("{heading}<ul{class}>\n{items}</ul>\n")
}

/// The search page: a form and the script that answers it; without the script it points to the indexes.
fn search_page() -> SiteFile {
    let path = "search.html";
    let browse = Kind::ALL
        .into_iter()
        .filter(|k| *k != Kind::Start)
        .map(|k| format!("<li><a href=\"{}\">{}</a></li>\n", index_path(k), plural(k)))
        .collect::<Vec<_>>()
        .concat();
    let main = format!(
        "<article>\n<h1>Search</h1>\n<form id=\"search-form\" role=\"search\" action=\"search.html\">\n\
         <label for=\"q\">Search the Manual</label>\n<input id=\"q\" name=\"q\" type=\"search\" autocomplete=\"off\" autofocus>\n\
         <button type=\"submit\">Search</button>\n</form>\n<p id=\"search-status\" role=\"status\"></p>\n\
         <ol id=\"search-results\" class=\"pages\"></ol>\n\
         <noscript><p>Searching needs JavaScript. Without it, browse the Manual by part:</p>\n<ul>\n{browse}</ul></noscript>\n</article>\n"
    );
    let scripts = "<script src=\"search-index.js\"></script>\n<script src=\"search.js\"></script>\n";
    let title = format!("Search - {SITE_TITLE}");
    SiteFile::text(
        path,
        document(path, &title, "Search the Manual.", &main, &site_footer(path), scripts),
    )
}

/// The footer of a file that is no Page: the site-wide links.
fn site_footer(from: &str) -> String {
    let link = |id: &str, label: &str| format!("<a href=\"{}\">{label}</a>", relative(from, &page_path(id)));
    format!(
        "<p>{SITE_TITLE}. Licensed under the GNU Free Documentation License, version 1.1 or later, with no Invariant \
         Sections: see {} and {}. {}.</p>\n",
        link("legal/notices", "the notices"),
        link("legal/gfdl", "the licence"),
        link("legal/history", "History")
    )
}

/// A whole HTML document: header with navigation, `main`, footer.
fn document(path: &str, title: &str, description: &str, main: &str, footer: &str, scripts: &str) -> String {
    let root = relative(path, "");
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n\
         <meta name=\"description\" content=\"{}\">\n<link rel=\"stylesheet\" href=\"{root}style.css\">\n</head>\n<body>\n\
         <a class=\"skip\" href=\"#page-main\">Skip to the page</a>\n<header>\n{}</header>\n\
         <main id=\"page-main\">\n{main}</main>\n<footer>\n{footer}</footer>\n{scripts}</body>\n</html>\n",
        esc(title),
        esc(description),
        nav(path)
    )
}

/// The header: the Manual's name and its parts. Reference is methods, errors and reports.
fn nav(path: &str) -> String {
    let link = |label: &str, to: &str| {
        let current = if to == path { " aria-current=\"page\"" } else { "" };
        format!("<a href=\"{}\"{current}>{label}</a>", relative(path, to))
    };
    let kind = |k: Kind| link(plural(k), &index_path(k));
    format!(
        "<nav aria-label=\"The Manual\">\n<a class=\"brand\" href=\"{}\">{SITE_TITLE}</a>\n<ul>\n\
         <li>{}</li>\n<li>{}</li>\n<li>{}</li>\n<li class=\"group\"><span>Reference:</span> {} {} {}</li>\n<li>{}</li>\n<li>{}</li>\n\
         </ul>\n</nav>\n",
        relative(path, "index.html"),
        link("Start", "index.html"),
        kind(Kind::Concept),
        kind(Kind::Guide),
        kind(Kind::Method),
        kind(Kind::Error),
        kind(Kind::Report),
        kind(Kind::Gnucash),
        link("Search", "search.html"),
    )
}
