//! A script's values as JSON: what `render()` returns becomes the draft of the report result (`design/reports.md`
//! §7), and a template's `params` become plain data the registry keeps between runs.
//!
//! Rationals take their wire form `{ "num", "denom" }`, Monetaries `{ "amount", "commodity" }`, handles their
//! identity (an Account, Transaction, Split or Price its GUID, a Commodity its id), and integers (Time64 included)
//! JSON integers, which the result reads as timestamps where it expects one. Anything else, such as a function
//! pointer, has no JSON form.

use rhai::{Array, Dynamic, Map};
use serde_json::Value;

use crate::error::ResultError;
use crate::number::{Monetary, Rational};
use crate::query::{AccountHandle, CommodityHandle, PriceHandle, SplitHandle, TransactionHandle};

/// How deep a value may nest. A report result nests a few levels (sections, blocks, groups, rows, cells); this
/// bounds the conversion's recursion.
const MAX_DEPTH: usize = 64;

/// The JSON form of `value`, or where it has none.
pub(crate) fn to_json(value: &Dynamic) -> Result<Value, ResultError> {
    json_at(value, "", 0)
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() { key.to_owned() } else { format!("{path}.{key}") }
}

fn json_at(value: &Dynamic, path: &str, depth: usize) -> Result<Value, ResultError> {
    if depth > MAX_DEPTH {
        return Err(ResultError::new(path, format!("nested more than {MAX_DEPTH} levels deep")));
    }
    let value = value.flatten_clone();
    if let Some(json) = scalar(&value) {
        return Ok(json);
    }
    if let Some(items) = value.read_lock::<Array>() {
        let items = items.iter().enumerate().map(|(i, v)| json_at(v, &format!("{path}[{i}]"), depth + 1));
        return items.collect::<Result<Vec<_>, _>>().map(Value::Array);
    }
    if let Some(map) = value.read_lock::<Map>() {
        let fields = map.iter().map(|(k, v)| Ok((k.to_string(), json_at(v, &join(path, k), depth + 1)?)));
        return fields.collect::<Result<serde_json::Map<_, _>, ResultError>>().map(Value::Object);
    }
    Err(ResultError::new(path, format!("a {} has no JSON form", value.type_name())))
}

/// The JSON form of a value that holds no other values.
fn scalar(value: &Dynamic) -> Option<Value> {
    if value.is_unit() {
        return Some(Value::Null);
    }
    if let Ok(b) = value.as_bool() {
        return Some(Value::Bool(b));
    }
    if let Ok(n) = value.as_int() {
        return Some(Value::from(n));
    }
    if let Ok(c) = value.as_char() {
        return Some(Value::String(c.to_string()));
    }
    if let Some(s) = value.read_lock::<rhai::ImmutableString>() {
        return Some(Value::String(s.to_string()));
    }
    if let Some(r) = value.read_lock::<Rational>() {
        return serde_json::to_value(&*r).ok();
    }
    if let Some(m) = value.read_lock::<Monetary>() {
        return serde_json::to_value(&*m).ok();
    }
    handle_id(value).map(Value::String)
}

fn handle_id(value: &Dynamic) -> Option<String> {
    let guid = |g: red_ale_core::Guid| Some(g.to_string());
    if let Some(a) = value.read_lock::<AccountHandle>() {
        return guid(a.guid());
    }
    if let Some(t) = value.read_lock::<TransactionHandle>() {
        return guid(t.guid());
    }
    if let Some(s) = value.read_lock::<SplitHandle>() {
        return guid(s.guid());
    }
    if let Some(p) = value.read_lock::<PriceHandle>() {
        return guid(p.guid());
    }
    value.read_lock::<CommodityHandle>().map(|c| c.id().to_string())
}

/// A JSON value as a script value. JSON from red-ale's own forms holds no floats; a float is refused.
pub(crate) fn from_json(value: &Value) -> Result<Dynamic, String> {
    rhai::serde::to_dynamic(value).map_err(|e| e.to_string())
}
