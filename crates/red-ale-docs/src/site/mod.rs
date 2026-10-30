//! The static site: the Manual as HTML files (`design/manual.md`, "The static site"). Nothing here does I/O; [`Manual::site`]
//! returns the files and the caller writes them.
//!
//! The landing page is the site's `index.html`; its `styles.css` and `script.js` dress every other page too. The pages
//! ask for their images at `/assets/…`; the caller copies the repository's `assets/` beside the site, so the site is
//! served from a domain's root.
//!
//! Layout: `index.html` is the landing page (`docs/landing-page/`); the Start Page is `start.html`; every other Page is
//! `<page id>.html`; each kind has `<kind>/index.html`; `search.html` with `search.js` and `search-index.js`;
//! `styles.css`; `script.js`; `schema.json`; `llms.txt`; `llms-full.txt`. Links between Pages are relative.

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

/// The landing page, which is the site's main page; it is a template whose `{{version}}` and `{{release-status}}` are filled in.
const LANDING: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/landing-page/index.html"));
/// The one stylesheet, shared by the landing page and every other page.
const STYLE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/landing-page/styles.css"));
/// The landing page's script: the wordmark's parts on every page and the walkthrough on the landing page.
const SCRIPT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/landing-page/script.js"));
const SEARCH_JS: &str = include_str!("search.js");

/// The fonts the landing page loads, and every page with it.
const FONTS: &str = "<link rel=\"preconnect\" href=\"https://fonts.googleapis.com\">\n\
<link rel=\"preconnect\" href=\"https://fonts.gstatic.com\" crossorigin>\n\
<link href=\"https://fonts.googleapis.com/css2?family=Big+Shoulders+Display:wght@700;800;900&amp;\
family=IBM+Plex+Mono:wght@400;500;600&amp;family=IBM+Plex+Sans:wght@400;500;600&amp;display=swap\" \
rel=\"stylesheet\">\n";

/// The landing page's, and the site's, home on the web.
const GITHUB: &str = "https://github.com/CjS77/red-ale";

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

/// The file that holds a Page, relative to the site's directory. The landing page has `index.html`, so the Start Page
/// is `start.html`.
pub fn page_path(id: &str) -> String {
    if id == START {
        "start.html".to_owned()
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
            .chain([search_page()])
            .chain([SiteFile::text("index.html", landing())])
            .chain([SiteFile::text("styles.css", STYLE.to_owned())])
            .chain([SiteFile::text("script.js", SCRIPT.to_owned())])
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

/// How mature the release is, shown beside its version: `Beta` until a release is declared stable.
const RELEASE_STATUS: &str = "Beta";

/// The landing page, its tokens replaced with red-ale's version and release status, so the site never shows a stale one.
fn landing() -> String {
    LANDING
        .replace("{{version}}", env!("CARGO_PKG_VERSION"))
        .replace("{{release-status}}", RELEASE_STATUS)
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
    let mut main = format!(
        "<article class=\"manual-page\">\n{}<h1>{}</h1>\n",
        kind_label(page.kind, &path),
        esc(&page.title)
    );
    if !page.summary.is_empty() {
        let _ = writeln!(main, "<p class=\"summary\">{}</p>", esc(&page.summary));
    }
    main.push_str(&md.html(&page.body));
    main.push_str(&related(manual, page, &path));
    if page.id == START {
        main.push_str(&browse(manual));
    }
    main.push_str("</article>\n");
    let licence = Renderer { page: None, ..md }.html(&render::footer(page));
    let title = if page.id == START {
        SITE_TITLE.to_owned()
    } else {
        format!("{} - {SITE_TITLE}", page.title)
    };
    SiteFile::text(&path, document(&path, &title, &page.summary, &main, &licence, ""))
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
    let main = format!(
        "<article class=\"manual-page\">\n<h1>{}</h1>\n{intro}{lists}</article>\n",
        plural(kind)
    );
    let title = format!("{} - {SITE_TITLE}", plural(kind));
    SiteFile::text(&path, document(&path, &title, blurb(kind), &main, &licence_footer(&path), ""))
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
        "<article class=\"manual-page\">\n<h1>Search</h1>\n<form id=\"search-form\" role=\"search\" action=\"search.html\">\n\
         <label for=\"q\">Search the Manual</label>\n<input id=\"q\" name=\"q\" type=\"search\" autocomplete=\"off\" autofocus>\n\
         <button type=\"submit\">Search</button>\n</form>\n<p id=\"search-status\" role=\"status\"></p>\n\
         <ol id=\"search-results\" class=\"pages\"></ol>\n\
         <noscript><p>Searching needs JavaScript. Without it, browse the Manual by part:</p>\n<ul>\n{browse}</ul></noscript>\n</article>\n"
    );
    let scripts = "<script src=\"search-index.js\"></script>\n<script src=\"search.js\"></script>\n";
    let title = format!("Search - {SITE_TITLE}");
    SiteFile::text(
        path,
        document(path, &title, "Search the Manual.", &main, &licence_footer(path), scripts),
    )
}

/// The licence notice of a file that is no Page: the site-wide one.
fn licence_footer(from: &str) -> String {
    let link = |id: &str, label: &str| format!("<a href=\"{}\">{label}</a>", relative(from, &page_path(id)));
    format!(
        "<p>{SITE_TITLE}. Licensed under the GNU Free Documentation License, version 1.1 or later, with no Invariant \
         Sections: see {} and {}. {}.</p>\n",
        link("legal/notices", "the notices"),
        link("legal/gfdl", "the licence"),
        link("legal/history", "History")
    )
}

/// A whole HTML document: header with navigation, `main`, the licence and the site's footer.
fn document(path: &str, title: &str, description: &str, main: &str, licence: &str, scripts: &str) -> String {
    let root = relative(path, "");
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n\
         <meta name=\"description\" content=\"{}\">\n{FONTS}<link rel=\"stylesheet\" href=\"{root}styles.css\">\n\
         </head>\n<body>\n<a class=\"skip\" href=\"#page-main\">Skip to the page</a>\n{}\n\
         <main id=\"page-main\" class=\"ruled\">\n<div class=\"section-inner container\">\n{main}</div>\n</main>\n\
         {}<script src=\"{root}script.js\"></script>\n{scripts}</body>\n</html>\n",
        esc(title),
        esc(description),
        nav(path),
        site_footer(path, licence)
    )
}

/// The site's footer: the brand, the site's links, and the licence of the file it ends.
fn site_footer(path: &str, licence: &str) -> String {
    let link = |label: &str, to: &str| format!("<a href=\"{}\">{label}</a>", relative(path, to));
    format!(
        "<footer class=\"site-footer\">\n<div class=\"footer-inner container\">\n\
         <div class=\"footer-brand\">\n<div class=\"footer-logo\">\n\
         <img class=\"brand-pint\" src=\"/assets/logo.webp\" alt=\"\" width=\"58\" height=\"84\">\n\
         <svg class=\"wordmark wordmark-footer\" viewBox=\"38 82 664 182\" role=\"img\" aria-label=\"red ale\"></svg>\n\
         </div>\n<span class=\"footer-meta\">Assets = Liabilities + Equity · GPL-3.0-or-later</span>\n</div>\n\
         <div class=\"footer-links\">\n{}\n{}\n{}\n<a href=\"{GITHUB}\">GitHub</a>\n\
         <a href=\"{GITHUB}/blob/main/CHANGELOG.md\">Changelog</a>\n</div>\n</div>\n\
         <div class=\"page-licence container\">\n{licence}</div>\n</footer>\n",
        link("Home", "index.html"),
        link("Manual", "start.html"),
        link("Search", "search.html")
    )
}

/// The header: the landing page's own, with the Manual's parts.
fn nav(path: &str) -> String {
    let link = |label: &str, to: &str| {
        let current = if to == path { " aria-current=\"page\"" } else { "" };
        format!("<a href=\"{}\"{current}>{label}</a>", relative(path, to))
    };
    let kind = |k: Kind| link(plural(k), &index_path(k));
    format!(
        "<header class=\"site-header\">\n<nav class=\"nav container\" aria-label=\"Main\">\n\
         <a class=\"brand\" href=\"{}\" aria-label=\"red ale — home\">\n\
         <img class=\"brand-pint\" src=\"/assets/logo.webp\" alt=\"\" width=\"33\" height=\"48\">\n\
         <svg class=\"wordmark wordmark-nav\" viewBox=\"38 82 664 182\" role=\"img\" aria-label=\"red ale\"></svg>\n\
         </a>\n<div class=\"nav-links\">\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n\
         <a class=\"nav-github\" href=\"{GITHUB}\">GitHub</a>\n</div>\n</nav>\n</header>\n",
        relative(path, "index.html"),
        link("Manual", "start.html"),
        kind(Kind::Concept),
        kind(Kind::Guide),
        kind(Kind::Method),
        kind(Kind::Error),
        kind(Kind::Report),
        kind(Kind::Gnucash),
        link("Search", "search.html"),
    )
}
