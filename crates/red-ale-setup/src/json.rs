//! Editing one entry of a JSON or JSONC file through `jsonc-parser`'s syntax tree, which keeps every other byte:
//! other keys, their order, comments, blank lines and trailing commas (`design/setup.md`, "Editing a Harness config").

use jsonc_parser::ParseOptions;
use jsonc_parser::cst::{CstInputValue, CstObject, CstRootNode};
use serde_json::Value;

use crate::NAME;

/// How strictly a Harness reads its config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    /// Strict JSON: a comment or trailing comma makes the Harness reject the file, so `setup` rejects it too.
    Json,
    /// JSON with comments and trailing commas.
    Jsonc,
}

impl Dialect {
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Jsonc => "JSON with comments",
        }
    }

    fn options(self) -> ParseOptions {
        match self {
            Self::Jsonc => ParseOptions::default(),
            Self::Json => ParseOptions {
                allow_comments: false,
                allow_loose_object_property_names: false,
                allow_trailing_commas: false,
                allow_missing_commas: false,
                allow_single_quoted_strings: false,
                allow_hexadecimal_numbers: false,
                allow_unary_plus_numbers: false,
                allow_bare_decimal_point_numbers: false,
                allow_non_finite_numbers: false,
                allow_extended_string_escapes: false,
            },
        }
    }
}

/// Why a document can't be edited.
#[derive(Debug)]
pub(crate) enum DocError {
    /// It doesn't parse.
    Parse(String),
    /// This key, joined with `.` from the top (empty for the top level), holds something other than an object.
    NotAnObject(String),
}

/// A parsed Harness config. An empty file is a document with no value, which an edit makes an object.
pub(crate) struct Document {
    root: CstRootNode,
}

impl Document {
    pub(crate) fn parse(text: &str, dialect: Dialect) -> Result<Self, DocError> {
        let root = CstRootNode::parse(text, &dialect.options()).map_err(|e| DocError::Parse(e.to_string()))?;
        let document = Self { root };
        document.top()?;
        Ok(document)
    }

    /// The top-level object, if the document has a value.
    fn top(&self) -> Result<Option<CstObject>, DocError> {
        match self.root.value() {
            None => Ok(None),
            Some(value) => value.as_object().map(Some).ok_or_else(|| DocError::NotAnObject(String::new())),
        }
    }

    /// The object holding the servers, at `keys` under the top level, if it exists.
    fn servers(&self, keys: &[&str]) -> Result<Option<CstObject>, DocError> {
        keys.iter().enumerate().try_fold(self.top()?, |object, (depth, key)| {
            let Some(prop) = object.and_then(|o| o.get(key)) else { return Ok(None) };
            prop.value()
                .and_then(|v| v.as_object())
                .map(Some)
                .ok_or_else(|| DocError::NotAnObject(keys[..=depth].join(".")))
        })
    }

    /// Whether the top-level object has `key`.
    pub(crate) fn has_key(&self, key: &str) -> bool {
        self.top().ok().flatten().is_some_and(|o| o.get(key).is_some())
    }

    /// The red-ale entry under `keys`, if there is one.
    pub(crate) fn entry(&self, keys: &[&str]) -> Result<Option<Value>, DocError> {
        Ok(self.servers(keys)?.and_then(|s| s.get(NAME)).and_then(|p| p.to_serde_value()))
    }

    /// Make the red-ale entry under `keys` `value`, creating the objects on the way.
    pub(crate) fn set(&self, keys: &[&str], value: &Value) -> Result<(), DocError> {
        self.servers(keys)?;
        let top = self.root.object_value_or_create().ok_or_else(|| DocError::NotAnObject(String::new()))?;
        let servers = keys.iter().enumerate().try_fold(top, |object, (depth, key)| {
            object.object_value_or_create(key).ok_or_else(|| DocError::NotAnObject(keys[..=depth].join(".")))
        })?;
        match servers.get(NAME) {
            Some(prop) => prop.set_value(input(value)),
            None => drop(servers.append(NAME, input(value))),
        }
        Ok(())
    }

    /// Remove the red-ale entry under `keys`, and then each object on the way that it leaves empty, so removing
    /// what [`Document::set`] added gives back the original.
    pub(crate) fn remove(&self, keys: &[&str]) -> Result<(), DocError> {
        let Some(entry) = self.servers(keys)?.and_then(|s| s.get(NAME)) else { return Ok(()) };
        entry.remove();
        (1..=keys.len()).rev().try_for_each(|depth| {
            let empty = self.servers(&keys[..depth])?.is_some_and(|o| o.properties().is_empty());
            let parent = self.servers(&keys[..depth - 1])?;
            if let (true, Some(prop)) = (empty, parent.and_then(|p| p.get(keys[depth - 1]))) {
                prop.remove();
            }
            Ok(())
        })
    }

    /// The document's text, with the edits made.
    pub(crate) fn text(&self) -> String {
        self.root.to_string()
    }
}

fn input(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(b) => CstInputValue::Bool(*b),
        Value::Number(n) => CstInputValue::Number(n.to_string()),
        Value::String(s) => CstInputValue::String(s.clone()),
        Value::Array(items) => CstInputValue::Array(items.iter().map(input).collect()),
        Value::Object(map) => CstInputValue::Object(map.iter().map(|(k, v)| (k.clone(), input(v))).collect()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const KEYS: &[&str] = &["mcp", "servers"];

    fn edited(text: &str, edit: impl Fn(&Document)) -> String {
        let document = Document::parse(text, Dialect::Jsonc).unwrap();
        edit(&document);
        document.text()
    }

    #[test]
    fn set_keeps_comments_and_other_keys() {
        let text = "{\n  // keep\n  \"a\": 1, /* and this */\n  \"mcp\": { \"servers\": { \"x\": {} } },\n}\n";
        let out = edited(text, |d| d.set(KEYS, &json!({ "command": ["/r", "mcp"] })).unwrap());
        assert!(out.starts_with("{\n  // keep\n  \"a\": 1, /* and this */\n"), "{out}");
        assert!(out.contains("\"x\": {}"), "{out}");
        assert_eq!(Document::parse(&out, Dialect::Jsonc).unwrap().entry(KEYS).unwrap(), Some(json!({ "command": ["/r", "mcp"] })));
    }

    #[test]
    fn remove_after_set_gives_back_the_original() {
        ["", "{}\n", "{\n  \"a\": [1, 2] // c\n}\n", "{\n\t\"mcp\": {\n\t\t\"other\": true\n\t}\n}"].iter().for_each(|original| {
            let added = edited(original, |d| d.set(KEYS, &json!({ "type": "local" })).unwrap());
            let removed = edited(&added, |d| d.remove(KEYS).unwrap());
            let expected = if original.is_empty() { "{}\n" } else { original };
            assert_eq!(removed, expected, "from {original:?} through {added:?}");
        });
    }

    #[test]
    fn a_key_that_is_not_an_object_is_refused() {
        let document = Document::parse("{\"mcp\": {\"servers\": []}}", Dialect::Jsonc).unwrap();
        assert!(matches!(document.entry(KEYS), Err(DocError::NotAnObject(k)) if k == "mcp.servers"));
        assert!(matches!(document.set(KEYS, &json!({})), Err(DocError::NotAnObject(_))));
        assert!(matches!(Document::parse("[1]", Dialect::Jsonc), Err(DocError::NotAnObject(k)) if k.is_empty()));
    }

    #[test]
    fn strict_json_refuses_comments_and_trailing_commas() {
        assert!(Document::parse("{\"a\": 1, // c\n}", Dialect::Json).is_err());
        assert!(Document::parse("{\"a\": 1,}", Dialect::Json).is_err());
        assert!(Document::parse("{\"a\": 1}", Dialect::Json).is_ok());
    }
}
