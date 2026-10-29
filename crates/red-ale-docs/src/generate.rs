//! The generated part of Reference pages: from the `api.schema` document (methods and error codes) and from the
//! report templates, which reach this crate as data.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::page::{Kind, Page, error_page_id, method_page_id, report_page_id};

/// A report template as its Reference page needs it, or a built-in saved configuration of one (the Dashboard).
/// `red-ale-rpc` fills these from `red-ale-report`'s registry and built-in configurations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportTemplate {
    /// The template's GUID, or the saved configuration's.
    pub guid: String,
    /// The template's name, GnuCash's English one for a built-in report, or the saved configuration's.
    pub name: String,
    /// The script defining it: `standard/balance-sheet.rhai`; for a saved configuration, its template's.
    pub script: String,
    /// GnuCash's menu tip, if it has one.
    pub menu_tip: Option<String>,
    /// For a built-in saved configuration: the name of the template it configures, whose Page its Page links to.
    #[serde(default)]
    pub configures: Option<String>,
    /// Its options, in the order `report.options` lists them; a saved configuration's with its values as defaults.
    pub options: Vec<ReportOption>,
}

/// One option of a report template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportOption {
    /// The section (GnuCash's English string).
    pub section: String,
    /// The name (GnuCash's English string).
    pub name: String,
    /// The type, as `report.options` gives it: `boolean`, `multichoice`, `date`, …
    pub option_type: String,
    /// GnuCash's help text.
    pub help: String,
    /// The default, in wire form.
    pub default: Value,
}

/// A generated Page with no hand-written part yet.
fn page(id: String, kind: Kind, title: String, summary: String, body: String) -> Page {
    Page {
        id,
        kind,
        title,
        summary,
        related: Vec::new(),
        from: None,
        body,
        hand_written: None,
    }
}

/// The Reference pages of every method in the schema document.
pub(crate) fn method_pages(schema: &Value) -> Vec<Page> {
    let empty = Map::new();
    let defs = schema.get("$defs").and_then(Value::as_object).unwrap_or(&empty);
    entries(schema, "methods")
        .filter_map(|m| {
            let name = m.get("name")?.as_str()?;
            let summary = text(m, "summary");
            Some(page(
                method_page_id(name),
                Kind::Method,
                name.to_owned(),
                summary,
                method_body(m, defs),
            ))
        })
        .collect()
}

/// The Reference pages of every error code in the schema document.
pub(crate) fn error_pages(schema: &Value) -> Vec<Page> {
    entries(schema, "errors")
        .filter_map(|e| {
            let name = e.get("name")?.as_str()?;
            let code = e.get("code")?;
            let body = format!("**Code:** `{code}` · **Name:** `{name}`\n");
            Some(page(
                error_page_id(name),
                Kind::Error,
                name.to_owned(),
                format!("Error code {code}."),
                body,
            ))
        })
        .collect()
}

/// The Reference pages of the report templates.
pub(crate) fn report_pages(templates: &[ReportTemplate]) -> Vec<Page> {
    templates
        .iter()
        .map(|t| {
            let summary = t.menu_tip.clone().unwrap_or_else(|| format!("The {} report.", t.name));
            page(report_page_id(&t.name), Kind::Report, t.name.clone(), summary, report_body(t))
        })
        .collect()
}

/// The error codes of the schema document, by code, with their names.
pub(crate) fn error_names(schema: &Value) -> Vec<(i64, String)> {
    entries(schema, "errors")
        .filter_map(|e| Some((e.get("code")?.as_i64()?, e.get("name")?.as_str()?.to_owned())))
        .collect()
}

fn entries<'a>(schema: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    schema.get(key).and_then(Value::as_array).into_iter().flatten()
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn method_body(method: &Value, defs: &Map<String, Value>) -> String {
    let effect = method.get("effect").and_then(Value::as_str).unwrap_or("read");
    let target = match method.get("target").and_then(Value::as_str) {
        Some("book") => ", on the Book",
        Some("configuration") => ", on the Configuration only",
        _ => "",
    };
    let params = section("Params", method.get("params"), defs, "None.");
    let result = section("Result", method.get("result"), defs, "Nothing.");
    format!("**Effect:** `{effect}`{target}.\n\n{params}\n{result}")
}

/// A `## heading` with the fields of `schema`, or its type when it is not an object.
fn section(heading: &str, schema: Option<&Value>, defs: &Map<String, Value>, none: &str) -> String {
    let Some(schema) = schema else {
        return format!("## {heading}\n\n{none}\n");
    };
    let object = resolve(schema, defs);
    let fields = properties(object, defs);
    let described = description(schema, defs).map(|d| format!("{d}\n\n")).unwrap_or_default();
    if fields.is_empty() && object.get("properties").is_some() {
        return format!("## {heading}\n\n{described}{none}\n");
    }
    if fields.is_empty() {
        return format!("## {heading}\n\n{described}Type: {}.\n", type_name(schema, defs));
    }
    format!("## {heading}\n\n{described}{}", fields.concat())
}

/// One list item per property: name, type, whether required, description.
fn properties(object: &Value, defs: &Map<String, Value>) -> Vec<String> {
    let required: Vec<&str> = object
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    object
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(name, field)| {
            let need = if required.contains(&name.as_str()) { ", required" } else { "" };
            let what = description(field, defs).map(|d| format!(": {d}")).unwrap_or_default();
            format!("- `{name}` ({}{need}){what}\n", type_name(field, defs))
        })
        .collect()
}

/// The schema a `$ref` names, or the schema itself.
fn resolve<'a>(schema: &'a Value, defs: &'a Map<String, Value>) -> &'a Value {
    ref_name(schema).and_then(|name| defs.get(name)).unwrap_or(schema)
}

fn ref_name(schema: &Value) -> Option<&str> {
    schema.get("$ref")?.as_str()?.strip_prefix("#/$defs/")
}

/// A schema's description, or that of the definition it refers to, on one line.
fn description(schema: &Value, defs: &Map<String, Value>) -> Option<String> {
    let text = schema
        .get("description")
        .or_else(|| resolve(schema, defs).get("description"))
        .and_then(Value::as_str)?;
    Some(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// A short name for a schema's type: `string`, `string (date)`, `Amount`, `array of Split`, `"a" or "b"`.
fn type_name(schema: &Value, defs: &Map<String, Value>) -> String {
    if let Some(name) = ref_name(schema) {
        return name.to_owned();
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values.iter().map(Value::to_string).collect::<Vec<_>>().join(" or ");
    }
    if let Some(value) = schema.get("const") {
        return value.to_string();
    }
    if let Some(variants) = ["anyOf", "oneOf"].iter().find_map(|k| schema.get(*k).and_then(Value::as_array)) {
        return variants.iter().map(|v| type_name(v, defs)).collect::<Vec<_>>().join(" or ");
    }
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => vec!["any"],
    };
    types
        .into_iter()
        .map(|t| one_type(t, schema, defs))
        .collect::<Vec<_>>()
        .join(" or ")
}

fn one_type(t: &str, schema: &Value, defs: &Map<String, Value>) -> String {
    match (t, schema.get("items"), schema.get("additionalProperties").filter(|v| v.is_object())) {
        ("array", Some(items), _) => format!("array of {}", type_name(items, defs)),
        ("object", _, Some(values)) => format!("map of {}", type_name(values, defs)),
        _ => match schema.get("format").and_then(Value::as_str) {
            Some(format) if t != "null" => format!("{t} ({format})"),
            _ => t.to_owned(),
        },
    }
}

fn report_body(t: &ReportTemplate) -> String {
    let head = match &t.configures {
        None => format!("**Template GUID:** `{}` · **Script:** `{}`\n", t.guid, t.script),
        Some(template) => format!(
            "**Built-in saved configuration GUID:** `{}`, read-only · **Configures:** [{template}]({}) · **Script:** `{}`\n",
            t.guid,
            report_page_id(template),
            t.script
        ),
    };
    if t.options.is_empty() {
        return head;
    }
    let mut sections: Vec<&str> = Vec::new();
    t.options.iter().for_each(|o| {
        if !sections.contains(&o.section.as_str()) {
            sections.push(&o.section);
        }
    });
    let options = sections.iter().fold(String::new(), |mut out, s| {
        let items: String = t.options.iter().filter(|o| o.section == *s).map(option_item).collect();
        let _ = write!(out, "\n### {s}\n\n{items}");
        out
    });
    format!("{head}\n## Options\n{options}")
}

fn option_item(o: &ReportOption) -> String {
    let help = o.help.split_whitespace().collect::<Vec<_>>().join(" ");
    let help = if help.is_empty() { String::new() } else { format!(": {help}") };
    format!("- **{}** ({}, default `{}`){help}\n", o.name, o.option_type, o.default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "redAleApi": "0.6.0",
            "methods": [{
                "name": "account.balance",
                "summary": "An account's balance.",
                "effect": "read",
                "target": null,
                "params": { "$ref": "#/$defs/BalanceParams" },
                "result": { "type": "array", "items": { "$ref": "#/$defs/Balance" } },
            }],
            "errors": [{ "code": 1000, "name": "no-book" }],
            "$defs": {
                "BalanceParams": {
                    "type": "object",
                    "properties": {
                        "account": { "type": "string", "description": "The account:\n a GUID or full name." },
                        "asOf": { "type": ["string", "null"], "format": "date" },
                        "kind": { "enum": ["a", "b"] },
                    },
                    "required": ["account"],
                },
                "Balance": { "type": "object" },
            },
        })
    }

    #[test]
    fn method_pages_list_params_and_result() {
        let pages = method_pages(&schema());
        assert_eq!(pages.len(), 1);
        let page = &pages[0];
        assert_eq!(page.id, "method/account.balance");
        assert_eq!(page.summary, "An account's balance.");
        assert!(page.body.contains("**Effect:** `read`."), "{}", page.body);
        assert!(
            page.body
                .contains("- `account` (string, required): The account: a GUID or full name.\n"),
            "{}",
            page.body
        );
        assert!(page.body.contains("- `asOf` (string (date) or null)\n"), "{}", page.body);
        assert!(page.body.contains("- `kind` (\"a\" or \"b\")\n"), "{}", page.body);
        assert!(page.body.contains("## Result\n\nType: array of Balance.\n"), "{}", page.body);
    }

    #[test]
    fn error_pages_name_their_code() {
        let pages = error_pages(&schema());
        assert_eq!(pages[0].id, "error/no-book");
        assert_eq!(pages[0].summary, "Error code 1000.");
        assert_eq!(error_names(&schema()), [(1000, "no-book".to_owned())]);
    }

    #[test]
    fn report_pages_list_options_by_section() {
        let option = |section: &str, name: &str| ReportOption {
            section: section.into(),
            name: name.into(),
            option_type: "boolean".into(),
            help: "Show it?".into(),
            default: json!(true),
        };
        let template = ReportTemplate {
            guid: "c4173ac99b2b448289bf4d11c731af13".into(),
            name: "Balance Sheet".into(),
            script: "standard/balance-sheet.rhai".into(),
            menu_tip: None,
            configures: None,
            options: vec![option("General", "A"), option("Display", "B"), option("General", "C")],
        };
        let pages = report_pages(&[template]);
        assert_eq!(pages[0].id, "report/balance-sheet");
        assert_eq!(pages[0].summary, "The Balance Sheet report.");
        let body = &pages[0].body;
        let general = body.find("### General").unwrap();
        assert!(general < body.find("### Display").unwrap(), "{body}");
        assert!(body.contains("- **C** (boolean, default `true`): Show it?\n"), "{body}");
    }

    #[test]
    fn a_built_in_configuration_links_to_its_template() {
        let dashboard = ReportTemplate {
            guid: "ad80271c890b11dfa79f2dcedfd72085".into(),
            name: "Dashboard".into(),
            script: "standard/view-column.rhai".into(),
            menu_tip: Some("A basic dashboard for your accounting data".into()),
            configures: Some("Multicolumn View".into()),
            options: Vec::new(),
        };
        let pages = report_pages(&[dashboard]);
        assert_eq!(pages[0].id, "report/dashboard");
        assert!(
            pages[0]
                .body
                .contains("**Configures:** [Multicolumn View](report/multicolumn-view)"),
            "{}",
            pages[0].body
        );
    }
}
