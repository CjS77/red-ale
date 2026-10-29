//! The hand-written files of `docs/`: an SPDX line, a short front matter and a Markdown body.
//!
//! ```text
//! <!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
//! ---
//! id: concept/value-and-amount
//! title: Value and amount
//! summary: A split's two sizes, and why both are kept.
//! related: method/transaction.post, concept/commodities
//! from: guide/ch_txns.docbook
//! ---
//! Markdown…
//! ```
//!
//! The SPDX line is an HTML comment, so Markdown viewers hide it. `id` is required and must be the Page ID the file's
//! path gives; `title` and `summary` are required except on Reference pages, which take them from the registries
//! unless the file overrides them. `related` is a comma-separated list of Page IDs, optionally in `[` `]`.

use crate::Problem;
use crate::page::id_for_path;

/// The licence line every file of `docs/` starts with (`design/manual.md`, "Licence").
pub const SPDX_LINE: &str = "<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->";

/// One hand-written file, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Source {
    /// The Page ID.
    pub id: String,
    /// The title, if given.
    pub title: Option<String>,
    /// The summary, if given.
    pub summary: Option<String>,
    /// Related Page IDs.
    pub related: Vec<String>,
    /// The section of GnuCash's documentation it adapts.
    pub from: Option<String>,
    /// The Markdown after the front matter.
    pub body: String,
}

/// Parse the file at `path` (relative to `docs/`).
pub(crate) fn parse(path: &str, text: &str) -> Result<Source, Problem> {
    let problem = |message: String| Problem {
        origin: format!("docs/{path}"),
        message,
    };
    let expected = id_for_path(path).ok_or_else(|| problem("not a place a Page can live in docs/".to_owned()))?;
    let rest = text
        .strip_prefix(SPDX_LINE)
        .and_then(|r| r.strip_prefix('\n'))
        .ok_or_else(|| problem(format!("the first line must be `{SPDX_LINE}`")))?;
    let rest = rest
        .strip_prefix("---\n")
        .ok_or_else(|| problem("no front matter after the SPDX line".to_owned()))?;
    let (front, body) = rest
        .split_once("\n---\n")
        .or_else(|| rest.strip_suffix("\n---").map(|f| (f, "")))
        .ok_or_else(|| problem("the front matter does not end with `---`".to_owned()))?;
    let source = front_matter(front, body).map_err(problem)?;
    if source.id != expected {
        return Err(problem(format!("id is {}, but the file's place makes it {expected}", source.id)));
    }
    Ok(source)
}

fn front_matter(front: &str, body: &str) -> Result<Source, String> {
    let mut source = Source {
        id: String::new(),
        title: None,
        summary: None,
        related: Vec::new(),
        from: None,
        body: body.to_owned(),
    };
    front.lines().filter(|l| !l.trim().is_empty()).try_for_each(|line| {
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| format!("front matter line without `key:`: {line}"))?;
        let value = unquote(value.trim());
        match key.trim() {
            "id" => value.clone_into(&mut source.id),
            "title" => source.title = Some(value.to_owned()),
            "summary" => source.summary = Some(value.to_owned()),
            "related" => source.related = list(value),
            "from" => source.from = Some(value.to_owned()),
            other => return Err(format!("unknown front matter key {other}")),
        }
        Ok(())
    })?;
    if source.id.is_empty() {
        return Err("the front matter has no id".to_owned());
    }
    Ok(source)
}

fn unquote(value: &str) -> &str {
    ['"', '\'']
        .into_iter()
        .find_map(|q| value.strip_prefix(q).and_then(|v| v.strip_suffix(q)))
        .unwrap_or(value)
}

fn list(value: &str) -> Vec<String> {
    let inner = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')).unwrap_or(value);
    inner
        .split(',')
        .map(|s| unquote(s.trim()).to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(front: &str, body: &str) -> String {
        format!("{SPDX_LINE}\n---\n{front}\n---\n{body}")
    }

    #[test]
    fn a_file_parses() {
        let text = file(
            "id: concept/value\ntitle: \"Value: two sizes\"\nsummary: Why.\nrelated: [method/a.b, start]\nfrom: guide/x.docbook",
            "Body.\n",
        );
        let s = parse("concepts/value.md", &text).unwrap();
        assert_eq!(s.id, "concept/value");
        assert_eq!(s.title.as_deref(), Some("Value: two sizes"));
        assert_eq!(s.related, ["method/a.b", "start"]);
        assert_eq!(s.from.as_deref(), Some("guide/x.docbook"));
        assert_eq!(s.body, "Body.\n");
    }

    #[test]
    fn mistakes_are_problems() {
        let wrong_id = parse("concepts/value.md", &file("id: concept/other", "")).unwrap_err();
        assert!(wrong_id.message.contains("concept/value"), "{wrong_id}");
        let no_spdx = parse("concepts/value.md", "---\nid: concept/value\n---\n").unwrap_err();
        assert!(no_spdx.message.contains("SPDX"), "{no_spdx}");
        let unknown = parse("concepts/value.md", &file("id: concept/value\ncolour: red", "")).unwrap_err();
        assert!(unknown.message.contains("colour"), "{unknown}");
        assert!(parse("misc/value.md", &file("id: x", "")).is_err());
    }
}
