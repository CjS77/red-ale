//! `red-ale manual build` writes the Manual as a static website (`design/manual.md`, "The static site").

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::process::{Command, Output};
use std::sync::OnceLock;

use red_ale_docs::{SiteFile, page_path};
use red_ale_rpc::Dispatcher;
use red_ale_rpc::manual::manual;
use red_ale_service::Settings;
use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_red-ale");

/// The site as `manual build` writes it, built once for the tests that only read it.
fn site() -> &'static BTreeMap<String, String> {
    static SITE: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    SITE.get_or_init(|| files().into_iter().map(|f| (f.path, String::from_utf8(f.bytes).unwrap())).collect())
}

fn files() -> Vec<SiteFile> {
    manual().site(&Dispatcher::new(Settings::default()).schema())
}

fn is_html(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("html"))
}

fn build(dir: &std::path::Path) -> Output {
    Command::new(BIN).args(["manual", "build"]).arg(dir).output().unwrap()
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
    assert_eq!(std::fs::read_to_string(target.join("index.html")).unwrap(), site()["index.html"]);
    assert_eq!(std::fs::read(target.join("llms.txt")).unwrap(), site()["llms.txt"].as_bytes());
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
        let html = site()
            .get(&page_path(&page.id))
            .unwrap_or_else(|| panic!("no file for {}", page.id));
        assert!(html.contains("<h1>"), "{}", page.id);
        assert!(html.contains("GNU Free Documentation License"), "{}", page.id);
        assert!(
            html.contains("legal/gfdl.html") && html.contains("legal/notices.html"),
            "{}",
            page.id
        );
    });
    assert!(site()["index.html"].contains("<title>red-ale Manual</title>"));
}

#[test]
fn every_internal_link_resolves_to_a_file_and_anchor() {
    let missing: Vec<String> = site()
        .iter()
        .filter(|(path, _)| is_html(path))
        .flat_map(|(path, html)| {
            attribute_values(html, "href")
                .into_iter()
                .chain(attribute_values(html, "src"))
                .filter(|l| !l.contains(':'))
                .filter_map(|link| broken(path, link))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(missing.is_empty(), "{missing:#?}");
}

/// What is wrong with a link from `from`, if anything.
fn broken(from: &str, link: &str) -> Option<String> {
    let (file, anchor) = link.split_once('#').map_or((link, None), |(f, a)| (f, Some(a)));
    let target = if file.is_empty() { from.to_owned() } else { resolve(from, file) };
    let Some(html) = site().get(&target) else {
        return Some(format!("{from}: {link} has no file"));
    };
    anchor
        .filter(|a| !html.contains(&format!(" id=\"{a}\"")))
        .map(|a| format!("{from}: {link} has no anchor {a}"))
}

#[test]
fn llms_txt_lists_every_page_and_llms_full_holds_them() {
    let index = &site()["llms.txt"];
    let full = &site()["llms-full.txt"];
    assert!(index.starts_with("# red-ale Manual\n\n> "));
    manual().pages().iter().for_each(|p| {
        assert!(index.contains(&format!("[{}]({}): ", p.title, page_path(&p.id))), "{}", p.id);
        assert!(full.contains(&format!("red-ale Manual, `{}`.", p.id)), "{}", p.id);
    });
}

#[test]
fn the_schema_is_the_api_schema_document() {
    let schema: Value = serde_json::from_str(&site()["schema.json"]).unwrap();
    assert_eq!(schema, Dispatcher::new(Settings::default()).schema());
}

#[test]
fn the_search_index_parses_and_has_every_page() {
    let script = &site()["search-index.js"];
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
fn only_the_search_page_runs_a_script_and_nothing_is_fetched_from_elsewhere() {
    let with_script: BTreeSet<&str> = site()
        .iter()
        .filter(|(_, html)| html.contains("<script"))
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(with_script, BTreeSet::from(["search.html"]));
    site().iter().filter(|(p, _)| is_html(p)).for_each(|(path, html)| {
        assert!(
            !html.contains("src=\"http") && !html.contains("<link rel=\"stylesheet\" href=\"http"),
            "{path}"
        );
        assert!(!html.contains("<style") && !html.contains(" style=\""), "{path}: one stylesheet");
    });
    assert!(!site()["search.js"].contains("fetch(") && !site()["search.js"].contains("XMLHttpRequest"));
}

#[test]
fn examples_show_three_doors_and_pages_work_without_script() {
    let html = &site()["method/book.open.html"];
    ["<h4>JSON-RPC</h4>", "<h4>Command line</h4>", "<h4>MCP tool call</h4>"]
        .iter()
        .for_each(|label| assert!(html.contains(label), "{label}"));
    assert!(html.contains("red-ale exec book.open"));
    assert!(!html.contains("```example"));
}

#[test]
fn headings_have_ids_and_pages_use_relative_links() {
    let html = &site()["method/book.open.html"];
    assert!(html.contains("<h2 id=\"examples\">"));
    assert!(html.contains("href=\"../style.css\""));
    assert!(site()["index.html"].contains("href=\"style.css\""));
    assert!(site()["method/index.html"].contains("href=\"../method/book.open.html\""));
}
