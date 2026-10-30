//! `red-ale-manual build` writes the Manual as a static website (`design/manual.md`, "The static site").

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::process::{Command, Output};
use std::sync::OnceLock;

use red_ale_docs::{SiteFile, page_path};
use red_ale_rpc::Dispatcher;
use red_ale_rpc::manual::manual;
use red_ale_service::Settings;
use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_red-ale-manual");

/// The site as `manual build` writes it, built once for the tests that only read it.
fn site() -> &'static BTreeMap<String, SiteFile> {
    static SITE: OnceLock<BTreeMap<String, SiteFile>> = OnceLock::new();
    SITE.get_or_init(|| files().into_iter().map(|f| (f.path.clone(), f)).collect())
}

/// One file of the site as text.
fn text(path: &str) -> &'static str {
    std::str::from_utf8(&site().get(path).unwrap_or_else(|| panic!("no file {path}")).bytes).unwrap()
}

fn files() -> Vec<SiteFile> {
    manual().site(&Dispatcher::new(Settings::default()).schema(), "/")
}

fn is_html(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("html"))
}

fn build(dir: &std::path::Path) -> Output {
    Command::new(BIN).arg("build").arg(dir).output().unwrap()
}

/// Every `name="value"` of `attribute` in `html`.
fn attribute_values<'a>(html: &'a str, attribute: &str) -> Vec<&'a str> {
    let needle = format!(" {attribute}=\"");
    html.match_indices(&needle)
        .filter_map(|(at, _)| {
            let rest = &html[at + needle.len()..];
            rest.find('"').map(|end| &rest[..end])
        })
        .collect()
}

/// The file a relative link from `from` names.
fn resolve(from: &str, link: &str) -> String {
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    link.split('/').for_each(|segment| match segment {
        ".." => {
            parts.pop();
        }
        "." => {}
        s => parts.push(s),
    });
    parts.join("/")
}

#[test]
fn building_twice_gives_the_same_bytes() {
    assert_eq!(files(), files());
    let paths: Vec<&String> = site().keys().collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);
    assert_eq!(files().len(), site().len(), "no two files share a path");
}

#[test]
fn the_command_writes_the_site_into_an_empty_or_new_directory() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("site");
    let out = build(&target);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read_to_string(target.join("index.html")).unwrap(), text("index.html"));
    assert_eq!(std::fs::read(target.join("llms.txt")).unwrap(), site()["llms.txt"].bytes);
    let again = build(&target);
    assert!(!again.status.success());
    assert!(
        String::from_utf8_lossy(&again.stderr).contains("is not empty"),
        "refuses a directory with files in it"
    );
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    assert!(build(&empty).status.success(), "an empty directory is fine");
}

#[test]
fn every_page_has_a_file_with_its_title_and_the_licence_line() {
    manual().pages().iter().for_each(|page| {
        let html = text(&page_path(&page.id));
        assert!(html.contains("<h1>"), "{}", page.id);
        assert!(html.contains("GNU Free Documentation License"), "{}", page.id);
        assert!(
            html.contains("legal/gfdl.html") && html.contains("legal/notices.html"),
            "{}",
            page.id
        );
    });
    assert!(text("start.html").contains("<title>red-ale Manual</title>"));
    assert!(
        text("index.html").contains("<title>red-ale — GnuCash books, kept by machines</title>"),
        "the landing page is the main page"
    );
}

#[test]
fn every_internal_link_resolves_to_a_file_and_anchor() {
    let missing: Vec<String> = site()
        .iter()
        .filter(|(path, _)| is_html(path))
        .flat_map(|(path, file)| {
            attribute_values(std::str::from_utf8(&file.bytes).unwrap(), "href")
                .into_iter()
                .chain(attribute_values(std::str::from_utf8(&file.bytes).unwrap(), "src"))
                .filter(|l| !l.contains(':') && !l.starts_with('/'))
                .filter_map(|link| broken(path, link))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(missing.is_empty(), "{missing:#?}");
}

/// What is wrong with a link from `from`, if anything. A link that starts with `/` is from the site's root.
fn broken(from: &str, link: &str) -> Option<String> {
    let (file, anchor) = link.split_once('#').map_or((link, None), |(f, a)| (f, Some(a)));
    let target = match (file.is_empty(), file.strip_prefix('/')) {
        (true, _) => from.to_owned(),
        (false, Some(rooted)) => rooted.to_owned(),
        (false, None) => resolve(from, file),
    };
    if !site().contains_key(&target) {
        return Some(format!("{from}: {link} has no file"));
    }
    if !is_html(&target) {
        return None;
    }
    let html = text(&target);
    anchor
        .filter(|a| !html.contains(&format!(" id=\"{a}\"")))
        .map(|a| format!("{from}: {link} has no anchor {a}"))
}

#[test]
fn llms_txt_lists_every_page_and_llms_full_holds_them() {
    let index = text("llms.txt");
    let full = text("llms-full.txt");
    assert!(index.starts_with("# red-ale Manual\n\n> "));
    manual().pages().iter().for_each(|p| {
        assert!(index.contains(&format!("[{}]({}): ", p.title, page_path(&p.id))), "{}", p.id);
        assert!(full.contains(&format!("red-ale Manual, `{}`.", p.id)), "{}", p.id);
    });
}

#[test]
fn the_schema_is_the_api_schema_document() {
    let schema: Value = serde_json::from_str(text("schema.json")).unwrap();
    assert_eq!(schema, Dispatcher::new(Settings::default()).schema());
}

#[test]
fn the_search_index_parses_and_has_every_page() {
    let script = text("search-index.js");
    let json = script
        .strip_prefix("window.redAleIndex=")
        .and_then(|s| s.trim_end().strip_suffix(';'))
        .unwrap();
    let entries: Vec<Value> = serde_json::from_str(json).unwrap();
    assert_eq!(entries.len(), manual().pages().len());
    let book_open = entries.iter().find(|e| e["id"] == "method/book.open").unwrap();
    assert_eq!(book_open["kind"], "method");
    assert!(book_open["terms"].as_str().unwrap().split(' ').any(|t| t == "book.open"));
}

#[test]
fn every_page_uses_the_landing_pages_stylesheet_and_script() {
    site().iter().filter(|(p, _)| is_html(p)).for_each(|(path, file)| {
        let html = std::str::from_utf8(&file.bytes).unwrap();
        assert!(html.contains("styles.css\""), "{path}: the one stylesheet");
        assert!(html.contains("script.js\"></script>"), "{path}: the shared script");
        assert!(!html.contains("<style") && !html.contains(" style=\""), "{path}: no inline style");
    });
    let search = text("search.html");
    assert!(search.contains("search-index.js") && search.contains("search.js"), "search");
    // Nothing but the fonts comes from another site.
    site().iter().filter(|(p, _)| is_html(p)).for_each(|(path, file)| {
        let html = std::str::from_utf8(&file.bytes).unwrap();
        assert!(!html.contains("src=\"http"), "{path}: a script or image off the site");
        let external = html.matches("<link rel=\"stylesheet\" href=\"http").count();
        assert!(external <= 1, "{path}: one stylesheet, the fonts beside it");
        if external == 1 {
            assert!(html.contains("fonts.googleapis.com/css2"), "{path}");
        }
    });
    assert!(!text("search.js").contains("fetch(") && !text("search.js").contains("XMLHttpRequest"));
}

#[test]
fn the_landing_page_carries_this_binaries_version_and_release_status() {
    let landing = text("index.html");
    let version = format!("v{} Beta", env!("CARGO_PKG_VERSION"));
    assert!(landing.contains(&version), "the landing page shows {version}");
    assert!(!landing.contains("{{version}}"), "no version placeholder is left");
    assert!(!landing.contains("{{release-status}}"), "no release status placeholder is left");
}

#[test]
fn the_landing_page_is_the_main_page() {
    let landing = text("index.html");
    assert!(landing.contains("class=\"hero-title\""), "the landing page");
    assert_eq!(page_path("start"), "start.html");
    assert!(text("start.html").contains("href=\"index.html\""), "the manual finds its way home");
}

#[test]
fn the_images_the_pages_ask_for_are_copied_beside_the_site() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("site");
    let out = build(&target);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(target.join("assets/wordmark.svg").is_file());
    assert!(target.join("assets/LICENSE").is_file());
    let wanted: BTreeSet<String> = site()
        .iter()
        .filter(|(p, _)| is_html(p))
        .flat_map(|(_, file)| {
            let html = std::str::from_utf8(&file.bytes).unwrap();
            attribute_values(html, "src")
                .into_iter()
                .chain(attribute_values(html, "href"))
                .filter(|link| link.starts_with("/assets/"))
                .map(|link| link[1..].to_owned())
                .chain(std::iter::empty())
                .collect::<Vec<String>>()
        })
        .collect();
    assert!(!wanted.is_empty());
    wanted
        .iter()
        .for_each(|link| assert!(target.join(link).is_file(), "{link} is not copied"));
}

#[test]
fn a_base_path_rewrites_the_asset_links_under_it() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("site");
    let out = Command::new(BIN)
        .arg("build")
        .arg("--base")
        .arg("/red-ale")
        .arg(&target)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(target.join("assets/logo.webp").is_file(), "assets are still copied beside the site");
    let landing = std::fs::read_to_string(target.join("index.html")).unwrap();
    assert!(landing.contains("src=\"/red-ale/assets/logo.webp\""), "the landing page");
    assert!(!landing.contains("src=\"/assets/"), "no root-absolute asset link is left");
    let script = std::fs::read_to_string(target.join("script.js")).unwrap();
    assert!(script.contains("fetch('/red-ale/assets/wordmark.svg')"), "script.js");
    let page = std::fs::read_to_string(target.join("method/book.open.html")).unwrap();
    assert!(page.contains("src=\"/red-ale/assets/logo.webp\""), "a page in a sub-directory");
}

#[test]
fn examples_show_three_doors_and_pages_work_without_script() {
    let html = text("method/book.open.html");
    ["<h4>JSON-RPC</h4>", "<h4>Command line</h4>", "<h4>MCP tool call</h4>"]
        .iter()
        .for_each(|label| assert!(html.contains(label), "{label}"));
    assert!(html.contains("red-ale exec book.open"));
    assert!(!html.contains("```example"));
}

#[test]
fn headings_have_ids_and_pages_use_relative_links() {
    let html = text("method/book.open.html");
    assert!(html.contains("<h2 id=\"examples\">"));
    assert!(html.contains("href=\"../styles.css\""));
    assert!(text("index.html").contains("href=\"styles.css\""));
    assert!(text("method/index.html").contains("href=\"../method/book.open.html\""));
}
