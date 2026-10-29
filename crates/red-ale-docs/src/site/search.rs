//! The search page's index: one entry per Page, read by `search.js`. A script file rather than a JSON file, because a
//! browser will not `fetch` a file from a `file://` page.

use std::collections::BTreeSet;

use serde_json::json;

use crate::{Manual, terms};

/// Longest term kept: longer ones are hashes and encoded data in Examples.
const MAX_TERM: usize = 24;

/// `search-index.js`: `window.redAleIndex = [...]`, one object per Page with its `id`, `title`, `summary`, `kind` and
/// `terms`, the stemmed words of the whole Page in one space-separated string.
pub(super) fn index_script(manual: &Manual) -> String {
    format!("window.redAleIndex={};\n", serde_json::Value::Array(entries(manual)))
}

fn entries(manual: &Manual) -> Vec<serde_json::Value> {
    manual
        .pages()
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "title": p.title,
                "summary": p.summary,
                "kind": p.kind.name(),
                "terms": page_terms(&format!("{} {} {}", p.id, p.summary, p.body)),
            })
        })
        .collect()
}

/// The distinct terms of a text, sorted, leaving out numbers and long strings.
fn page_terms(text: &str) -> String {
    let distinct: BTreeSet<String> = terms(text)
        .into_iter()
        .filter(|t| t.chars().next().is_some_and(char::is_alphabetic) && t.len() <= MAX_TERM)
        .collect();
    distinct.into_iter().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_are_distinct_sorted_and_without_numbers() {
        assert_eq!(
            page_terms("Books, book 12345 and post transaction.post"),
            "book post transaction transaction.post"
        );
    }
}
