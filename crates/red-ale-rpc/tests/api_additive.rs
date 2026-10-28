//! The API stays additive over every released `redAleApi` until v1.0 freezes it (`design/api.md`, "Versioning"): every
//! method of an earlier version is kept with the same Effect and Effect target, its params take no new required field
//! and lose or retype none, its result loses or retypes no field, and every earlier error code keeps its name.
//!
//! `schemas/red-ale-api-0.2.0.json`, `-0.3.0.json` and `-0.4.0.json` are `red-ale schema` as v0.2, v0.3 and v0.4
//! shipped it. Schemas are compared by what they let a value hold, place by place, not by their `$defs` names
//! (schemars numbers clashing names, so they shift) or their descriptions.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use red_ale_rpc::Dispatcher;
use red_ale_service::Settings;
use serde_json::{Map, Value, json};

const V0_2: &str = include_str!("schemas/red-ale-api-0.2.0.json");
const V0_3: &str = include_str!("schemas/red-ale-api-0.3.0.json");
const V0_4: &str = include_str!("schemas/red-ale-api-0.4.0.json");

/// What a schema says about one place in a value: the JSON types it may take (with their `format`), the values an
/// enum or const allows, and whether it must be present.
#[derive(Debug, Default)]
struct Slot {
    types: BTreeSet<String>,
    values: BTreeSet<String>,
    required: bool,
}

/// Every place in a params or result value, by path: `params.splits[].account`, `result.accounts[]`, `map{}`.
type Shape = BTreeMap<String, Slot>;

/// A client sends params and reads results, so each may change only one way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Params,
    Result,
}

/// The [`Shape`] of `schema`, resolving `$ref`s in `defs`. A type that contains itself is followed once.
fn shape(schema: &Value, defs: &Map<String, Value>, root: &str) -> Shape {
    let mut out = Shape::new();
    walk(schema, defs, root, &mut Vec::new(), &mut out);
    out
}

fn walk<'a>(schema: &'a Value, defs: &'a Map<String, Value>, path: &str, refs: &mut Vec<&'a str>, out: &mut Shape) {
    let slot = out.entry(path.to_owned()).or_default();
    slot.types.extend(types(schema));
    slot.values
        .extend(list(schema, "enum").chain(schema.get("const")).map(Value::to_string));
    let target = schema.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix("#/$defs/"));
    if let Some(name) = target.filter(|name| !refs.contains(name)) {
        refs.push(name);
        walk(&defs[name], defs, path, refs, out);
        refs.pop();
    }
    let variants = ["anyOf", "oneOf", "allOf"].into_iter().flat_map(|k| list(schema, k));
    variants.for_each(|v| walk(v, defs, path, refs, out));
    if let Some(items) = schema.get("items") {
        walk(items, defs, &format!("{path}[]"), refs, out);
    }
    if let Some(values) = schema.get("additionalProperties").filter(|v| v.is_object()) {
        walk(values, defs, &format!("{path}{{}}"), refs, out);
    }
    walk_properties(schema, defs, path, refs, out);
}

fn walk_properties<'a>(schema: &'a Value, defs: &'a Map<String, Value>, path: &str, refs: &mut Vec<&'a str>, out: &mut Shape) {
    let required: BTreeSet<&str> = list(schema, "required").filter_map(Value::as_str).collect();
    schema
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .for_each(|(name, v)| {
            let field = format!("{path}.{name}");
            walk(v, defs, &field, refs, out);
            out.get_mut(&field).unwrap().required |= required.contains(name.as_str());
        });
}

/// The members of the array at `key`, none when there is none.
fn list<'a>(schema: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    schema.get(key).and_then(Value::as_array).into_iter().flatten()
}

/// `type`, one or a list, each with the schema's `format` when it has one: `integer:int64`, `string:date`.
fn types(schema: &Value) -> Vec<String> {
    let format = schema.get("format").and_then(Value::as_str);
    let names: Vec<&str> = match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    names
        .into_iter()
        .map(|t| format.filter(|_| t != "null").map_or_else(|| t.to_owned(), |f| format!("{t}:{f}")))
        .collect()
}

/// How `new` breaks a client of `old`: params may only widen and add optional fields; results may only narrow their
/// types and add fields.
fn breaks(old: &Shape, new: &Shape, side: Side) -> Vec<String> {
    let changed = old.iter().filter_map(|(path, o)| match new.get(path) {
        None => Some(format!("{path}: removed")),
        Some(n) if !still_fits(o, n, side) => Some(format!("{path}: {o:?} became {n:?}")),
        Some(_) => None,
    });
    let newly_required = new
        .iter()
        .filter(|(path, n)| side == Side::Params && n.required && !old.get(*path).is_some_and(|o| o.required))
        .filter(|(path, _)| path.rsplit_once('.').is_some_and(|(parent, _)| old.contains_key(parent)))
        .map(|(path, _)| format!("{path}: newly required"));
    changed.chain(newly_required).collect()
}

/// Whether a client written for `o` still works with `n`.
fn still_fits(o: &Slot, n: &Slot, side: Side) -> bool {
    match side {
        Side::Params => {
            let values_kept = n.values.is_empty() || (!o.values.is_empty() && o.values.is_subset(&n.values));
            o.types.is_subset(&n.types) && values_kept
        }
        Side::Result => n.types.is_subset(&o.types) && (!o.required || n.required),
    }
}

/// Every way `new` is not additive over `old`, as one line each.
fn not_additive(old: &Value, new: &Value) -> Vec<String> {
    let by_name = |s: &Value| -> BTreeMap<String, Value> {
        s["methods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| (m["name"].as_str().unwrap().to_owned(), m.clone()))
            .collect()
    };
    let (old_methods, new_methods) = (by_name(old), by_name(new));
    let (old_defs, new_defs) = (old["$defs"].as_object().unwrap(), new["$defs"].as_object().unwrap());
    let methods = old_methods.iter().flat_map(|(name, o)| {
        let Some(n) = new_methods.get(name) else {
            return vec![format!("{name}: removed")];
        };
        let effect = ["effect", "target"]
            .into_iter()
            .filter(|k| o[k] != n[k])
            .map(|k| format!("{k} {} became {}", o[k], n[k]));
        let side = |key: &str, side: Side| breaks(&shape(&o[key], old_defs, key), &shape(&n[key], new_defs, key), side);
        let sides = side("params", Side::Params).into_iter().chain(side("result", Side::Result));
        effect.chain(sides).map(|b| format!("{name}: {b}")).collect()
    });
    let new_errors: &Vec<Value> = new["errors"].as_array().unwrap();
    let errors = list(old, "errors")
        .filter(|e| !new_errors.contains(e))
        .map(|e| format!("error {e} removed or renamed"));
    methods.chain(errors).collect()
}

#[test]
fn the_api_is_additive_over_0_2_0() {
    let old: Value = serde_json::from_str(V0_2).unwrap();
    assert_eq!(old["redAleApi"], "0.2.0");
    let current = Dispatcher::new(Settings::default()).schema();
    let found = not_additive(&old, &current);
    assert!(found.is_empty(), "the API is no longer additive over 0.2.0:\n{}", found.join("\n"));
}

#[test]
fn the_api_is_additive_over_0_3_0() {
    let old: Value = serde_json::from_str(V0_3).unwrap();
    assert_eq!(old["redAleApi"], "0.3.0");
    let current = Dispatcher::new(Settings::default()).schema();
    let found = not_additive(&old, &current);
    assert!(found.is_empty(), "the API is no longer additive over 0.3.0:\n{}", found.join("\n"));
}

#[test]
fn the_api_is_additive_over_0_4_0() {
    let old: Value = serde_json::from_str(V0_4).unwrap();
    assert_eq!(old["redAleApi"], "0.4.0");
    let current = Dispatcher::new(Settings::default()).schema();
    let found = not_additive(&old, &current);
    assert!(found.is_empty(), "the API is no longer additive over 0.4.0:\n{}", found.join("\n"));
}

#[test]
fn the_check_finds_breaking_changes() {
    let old: Value = serde_json::from_str(V0_2).unwrap();
    let broken = |edit: &dyn Fn(&mut Value)| {
        let mut new = old.clone();
        edit(&mut new);
        not_additive(&old, &new)
    };
    let info = old["methods"]
        .as_array()
        .unwrap()
        .iter()
        .position(|m| m["name"] == "book.info")
        .unwrap();
    let found = broken(&|s| drop(s["methods"].as_array_mut().unwrap().remove(info)));
    assert_eq!(found, ["book.info: removed"]);
    let found = broken(&|s| s["methods"][info]["effect"] = json!("write"));
    assert_eq!(found, [r#"book.info: effect "read" became "write""#]);
    let found = broken(&|s| s["$defs"]["CheckParams"]["required"] = json!(["account", "asOf"]));
    assert_eq!(found, ["account.checkBalanceLimit: params.asOf: newly required"]);
    let found = broken(&|s| s["$defs"]["CheckResult"]["properties"]["includesSubaccounts"] = json!({ "type": "string" }));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("account.checkBalanceLimit: result.includesSubaccounts: "),
        "{found:?}"
    );
    let found = broken(&|s| drop(s["errors"].as_array_mut().unwrap().pop()));
    assert_eq!(found.len(), 1, "{found:?}");
    let found = broken(&|s| s["$defs"]["CheckParams"]["properties"]["extra"] = json!({ "type": "string" }));
    assert!(found.is_empty(), "an optional param is additive: {found:?}");
}
