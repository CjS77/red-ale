//! The tool list, generated from the dispatcher's method registry: one tool per JSON-RPC method.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use red_ale_rpc::{Dispatcher, Effect};
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::{Map, Value, json};

/// The method the dispatcher answers itself, outside its registry and so outside the schema document's method list.
const API_SCHEMA: &str = "api.schema";
const API_SCHEMA_SUMMARY: &str = "The API's schema document: every method with its params and result schemas, and the error codes.";

/// Where the schema document keeps its shared definitions, as a `$ref` prefix.
const DEFS: &str = "#/$defs/";

/// Every method of `dispatcher` as an MCP tool, `api.schema` included, in the registry's order.
pub fn tools(dispatcher: &Dispatcher) -> Vec<Tool> {
    let schema = dispatcher.schema();
    let defs = schema.get("$defs").and_then(Value::as_object).cloned().unwrap_or_default();
    let params: BTreeMap<&str, &Value> = schema
        .get("methods")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| Some((m.get("name")?.as_str()?, m.get("params")?)))
        .collect();
    dispatcher
        .methods()
        .map(|m| tool(m.name, m.summary, m.effect, input_schema(params.get(m.name).copied(), &defs)))
        .chain(std::iter::once(tool(API_SCHEMA, API_SCHEMA_SUMMARY, Effect::Read, no_params())))
        .collect()
}

fn tool(name: &'static str, summary: &'static str, effect: Effect, input_schema: JsonObject) -> Tool {
    Tool::new(name, summary, Arc::new(input_schema)).with_annotations(annotations(effect))
}

/// MCP's hints, from the Effect alone. `destructiveHint` means something only when `readOnlyHint` is false.
pub fn annotations(effect: Effect) -> ToolAnnotations {
    match effect {
        Effect::Read => ToolAnnotations::new().read_only(true),
        Effect::Write => ToolAnnotations::new().read_only(false).destructive(false),
        Effect::Destructive => ToolAnnotations::new().read_only(false).destructive(true),
    }
}

/// The schema MCP recommends for a tool that takes no params.
fn no_params() -> JsonObject {
    let Value::Object(schema) = json!({ "type": "object", "additionalProperties": false }) else { return JsonObject::new() };
    schema
}

/// A method's params schema made self-contained: the definition its top-level `$ref` names, plus the `$defs` it
/// reaches. `$ref`s cannot all be inlined, as some definitions refer to themselves.
fn input_schema(params: Option<&Value>, defs: &Map<String, Value>) -> JsonObject {
    let mut root = params.map(|p| dereference(p, defs)).and_then(|p| p.as_object().cloned()).unwrap_or_default();
    root.entry("type").or_insert_with(|| json!("object"));
    let reached = reachable(&Value::Object(root.clone()), defs);
    if !reached.is_empty() {
        let own: Map<String, Value> = defs.iter().filter(|(name, _)| reached.contains(*name)).map(|(n, d)| (n.clone(), d.clone())).collect();
        root.insert("$defs".to_owned(), Value::Object(own));
    }
    root
}

/// The definition a bare `{"$ref": "#/$defs/<Name>"}` names, or the schema itself.
fn dereference<'a>(schema: &'a Value, defs: &'a Map<String, Value>) -> &'a Value {
    let bare_ref = schema.as_object().filter(|o| o.len() == 1).and_then(|o| o.get("$ref")).and_then(Value::as_str);
    bare_ref.and_then(|r| r.strip_prefix(DEFS)).and_then(|name| defs.get(name)).unwrap_or(schema)
}

/// The names of every definition `schema` reaches, directly or through other definitions.
fn reachable(schema: &Value, defs: &Map<String, Value>) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut pending = refs(schema);
    while let Some(name) = pending.pop() {
        if let Some(def) = defs.get(&name)
            && seen.insert(name)
        {
            pending.extend(refs(def));
        }
    }
    seen
}

/// The definition names `schema` refers to directly.
fn refs(schema: &Value) -> Vec<String> {
    match schema {
        Value::Object(map) => map
            .iter()
            .flat_map(|(key, value)| match (key.as_str(), value) {
                ("$ref", Value::String(r)) => r.strip_prefix(DEFS).map(str::to_owned).into_iter().collect(),
                _ => refs(value),
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(refs).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn defs() -> Map<String, Value> {
        json!({
            "Params": { "type": "object", "properties": { "q": { "$ref": "#/$defs/Query" } } },
            "Query": { "anyOf": [{ "$ref": "#/$defs/Leaf" }, { "type": "array", "items": { "$ref": "#/$defs/Query" } }] },
            "Leaf": { "type": "string" },
            "Unused": { "type": "integer" }
        })
        .as_object()
        .unwrap()
        .clone()
    }

    #[test]
    fn a_bare_ref_becomes_the_root_and_carries_only_the_definitions_it_reaches() {
        let schema = input_schema(Some(&json!({ "$ref": "#/$defs/Params" })), &defs());
        assert_eq!(schema["properties"]["q"]["$ref"], "#/$defs/Query");
        let names: Vec<&String> = schema["$defs"].as_object().unwrap().keys().collect();
        assert_eq!(names, ["Query", "Leaf"], "recursive Query is kept once, Unused and Params are left out");
    }

    #[test]
    fn every_input_schema_is_an_object_schema() {
        let schema = input_schema(Some(&json!({ "properties": {} })), &defs());
        assert_eq!(schema["type"], "object");
        assert!(!schema.contains_key("$defs"));
        assert_eq!(input_schema(None, &defs())["type"], "object");
    }
}
