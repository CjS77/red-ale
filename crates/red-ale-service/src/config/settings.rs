//! Settings by path: the Configuration read with its defaults filled in, checked against its schema
//! ([`super::schema`]), and changed atomically (`config.get`, `config.set`; `design/api/configuration.md`).
//!
//! A **setting path** names a setting by its keys joined with `.`: `import.settings.matchThreshold`,
//! `quotes.services.alphavantage.key`. Map entries (a Quote service, a quote source) are keys like any other.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde_json::{Map, Value, json};

use super::schema::{self, REFUSED_OVER_MCP, SCHEMA};
use super::{Config, invalid};
use crate::ServiceError;

/// A setting's place in the Configuration: its keys, in order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SettingPath(Vec<String>);

impl FromStr for SettingPath {
    type Err = SettingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let keys: Vec<String> = s.split('.').map(str::to_owned).collect();
        if keys.iter().any(String::is_empty) {
            return Err(SettingError::one(
                s,
                "is not a setting path: keys joined with \".\", such as import.settings.matchThreshold",
            ));
        }
        Ok(Self(keys))
    }
}

impl fmt::Display for SettingPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join("."))
    }
}

impl SettingPath {
    fn keys(&self) -> &[String] {
        &self.0
    }

    /// True when `self` is `other` or lies inside it.
    fn within(&self, other: &Self) -> bool {
        self.0.starts_with(&other.0)
    }
}

/// Something wrong with one setting. The message never holds a secret's value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Problem {
    /// Where: a setting path, with `[i]` for a list item.
    pub path: String,
    /// What is wrong.
    pub message: String,
}

/// A refused read or change of settings: every problem found.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", summary(.problems))]
pub struct SettingError {
    /// The problems, each with its setting.
    pub problems: Vec<Problem>,
}

fn summary(problems: &[Problem]) -> String {
    let first = problems.first().map_or_else(String::new, |p| format!("{} {}", p.path, p.message));
    match problems.len() {
        0 | 1 => first,
        n => format!("{first} (and {} more)", n - 1),
    }
}

impl SettingError {
    fn one(path: impl fmt::Display, message: impl Into<String>) -> Self {
        Self {
            problems: vec![Problem {
                path: path.to_string(),
                message: message.into(),
            }],
        }
    }
}

/// What a change did beyond setting the values asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// Settings only a person may change (`x-refusedOverMcp`) whose values the change altered.
    pub refused_over_mcp: Vec<String>,
    /// Keys cleared because their Quote service's URL moved to another host.
    pub cleared_keys: Vec<String>,
}

impl Config {
    fn root(&self) -> Value {
        Value::Object(self.raw.clone())
    }

    /// Every declared setting with its default filled in, secrets included: for red-ale's own use, never a result.
    pub fn effective(&self) -> Value {
        effective(&SCHEMA, Some(&self.root())).unwrap_or_default()
    }

    /// One setting with its default filled in, secrets included; `null` when unset with no default.
    pub fn setting(&self, path: &SettingPath) -> Result<Value, SettingError> {
        node(path)?;
        Ok(value_at(&self.effective(), path.keys()).cloned().unwrap_or_default())
    }

    /// A setting that must be valid, for red-ale's own use: `invalid` when the file holds a bad value for it.
    pub fn checked(&self, path: &str) -> Result<Value, ServiceError> {
        let path: SettingPath = path.parse().map_err(|e: SettingError| invalid(e.to_string()))?;
        let value = self.setting(&path).map_err(|e| invalid(e.to_string()))?;
        let wrong: Vec<Problem> = self.problems().into_iter().filter(|p| related(&p.path, &path)).collect();
        if wrong.is_empty() {
            Ok(value)
        } else {
            Err(invalid(format!("the Configuration's {}", SettingError { problems: wrong })))
        }
    }

    /// The Configuration, or one setting, as a client may see it: defaults filled in, and each secret shown only as
    /// `{"set": true|false}`.
    pub fn view(&self, path: Option<&SettingPath>) -> Result<Value, SettingError> {
        let Some(path) = path else {
            return Ok(mask(&SCHEMA, self.effective()));
        };
        let (schema, _) = node(path)?;
        Ok(mask(schema, self.setting(path)?))
    }

    /// The paths of keys in the file that declare no setting. They are kept, and do nothing.
    pub fn unknown_keys(&self) -> Vec<String> {
        let mut found = Vec::new();
        unknown(&SCHEMA, &self.root(), "", &mut found);
        found
    }

    /// The settings whose values in the file do not match the schema.
    pub fn problems(&self) -> Vec<Problem> {
        problems(&self.effective())
    }

    /// Change settings: each path to its value, or back to its default with `null`. Every change is checked
    /// before any is made, so on error the Configuration is unchanged. A change may not introduce a problem, nor
    /// leave one in a setting it touches; problems elsewhere in the file are left alone.
    pub fn apply(&mut self, changes: &[(String, Value)]) -> Result<Applied, SettingError> {
        let changes = parse_changes(changes)?;
        let before = self.effective();
        let mut raw = self.root();
        changes.iter().try_for_each(|(path, value)| write(&mut raw, path, value))?;
        let cleared_keys = bind_keys(&before, &effective(&SCHEMA, Some(&raw)).unwrap_or_default(), &mut raw, &changes);
        let after = effective(&SCHEMA, Some(&raw)).unwrap_or_default();
        check_changes(&before, &after, &changes)?;
        let refused_over_mcp = refused_changes(&before, &after, &cleared_keys);
        if let Value::Object(map) = raw {
            self.raw = map;
        }
        Ok(Applied {
            refused_over_mcp,
            cleared_keys,
        })
    }
}

fn properties(schema: &Value) -> Option<&Map<String, Value>> {
    schema.get("properties").and_then(Value::as_object)
}

/// The schema of a map's entries: `additionalProperties` when it is a schema.
fn entries(schema: &Value) -> Option<&Value> {
    schema.get("additionalProperties").filter(|a| a.is_object())
}

fn child<'a>(schema: &'a Value, key: &str) -> Option<&'a Value> {
    properties(schema).and_then(|p| p.get(key)).or_else(|| entries(schema))
}

fn flag(schema: &Value, keyword: &str) -> bool {
    schema.get(keyword).and_then(Value::as_bool) == Some(true)
}

/// The schema of the setting `path` names, and whether another method owns it or something inside it.
fn node(path: &SettingPath) -> Result<(&'static Value, bool), SettingError> {
    let walked = path.keys().iter().try_fold((&*SCHEMA, false), |(schema, owned), key| {
        child(schema, key).map(|c| (c, owned || flag(c, "readOnly")))
    });
    let (schema, owned) = walked.ok_or_else(|| SettingError::one(path, "is not a setting"))?;
    Ok((schema, owned || owns_inside(schema)))
}

fn owns_inside(schema: &Value) -> bool {
    properties(schema)
        .into_iter()
        .flatten()
        .any(|(_, s)| flag(s, "readOnly") || owns_inside(s))
}

fn value_at<'a>(value: &'a Value, keys: &[String]) -> Option<&'a Value> {
    keys.iter().try_fold(value, |v, key| v.get(key))
}

/// `top` over `base`: objects merge key by key, anything else replaces; `null` in `top` leaves `base`.
fn overlay(base: Value, top: &Value) -> Value {
    match (base, top) {
        (base, Value::Null) => base,
        (Value::Object(mut base), Value::Object(top)) => {
            top.iter().for_each(|(key, value)| {
                let merged = overlay(base.remove(key).unwrap_or_default(), value);
                base.insert(key.clone(), merged);
            });
            Value::Object(base)
        }
        (_, top) => top.clone(),
    }
}

/// The stored value with defaults filled in, following `schema`: undeclared keys are left out, and a setting with
/// no value and no default is absent.
fn effective(schema: &Value, stored: Option<&Value>) -> Option<Value> {
    let stored = stored.filter(|v| !v.is_null());
    let declared = properties(schema);
    let is_object = declared.is_some() || entries(schema).is_some();
    if !is_object || stored.is_some_and(|v| !v.is_object()) {
        return stored.cloned().or_else(|| schema.get("default").cloned());
    }
    let base = schema
        .get("default")
        .cloned()
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Map::new()));
    let Value::Object(merged) = overlay(base, stored.unwrap_or(&Value::Null)) else {
        return None;
    };
    let own = declared
        .into_iter()
        .flatten()
        .filter_map(|(key, s)| Some((key.clone(), effective(s, merged.get(key))?)));
    let others = entries(schema).into_iter().flat_map(|entry| {
        let undeclared = merged.iter().filter(|(key, _)| !declared.is_some_and(|d| d.contains_key(*key)));
        undeclared.filter_map(move |(key, v)| Some((key.clone(), effective(entry, Some(v))?)))
    });
    Some(Value::Object(own.chain(others).collect()))
}

/// Hide every secret's value: `{"set": true}` when it has one.
fn mask(schema: &Value, value: Value) -> Value {
    if flag(schema, "writeOnly") {
        let set = !value.is_null() && value.as_str() != Some("");
        return json!({ "set": set });
    }
    match value {
        Value::Object(map) => {
            let masked = map.into_iter().map(|(key, v)| {
                let v = match child(schema, &key) {
                    Some(s) => mask(s, v),
                    None => v,
                };
                (key, v)
            });
            Value::Object(masked.collect())
        }
        Value::Array(items) => match schema.get("items") {
            Some(item) => Value::Array(items.into_iter().map(|v| mask(item, v)).collect()),
            None => Value::Array(items),
        },
        other => other,
    }
}

fn join(at: &str, key: &str) -> String {
    if at.is_empty() { key.to_owned() } else { format!("{at}.{key}") }
}

fn unknown(schema: &Value, stored: &Value, at: &str, found: &mut Vec<String>) {
    let Value::Object(map) = stored else { return };
    if properties(schema).is_none() && entries(schema).is_none() {
        return;
    }
    map.iter().for_each(|(key, v)| match child(schema, key) {
        Some(s) => unknown(s, v, &join(at, key), found),
        None if !flag(schema, "additionalProperties") => found.push(join(at, key)),
        None => {}
    });
}

fn problems(effective: &Value) -> Vec<Problem> {
    let mut found = Vec::new();
    validate(&SCHEMA, effective, "", &mut found);
    let cross = schema::cross_checks(effective)
        .into_iter()
        .map(|(path, message)| Problem { path, message });
    found.extend(cross);
    found
}

/// True when the problem at `problem` is in, or contains, the setting `path`.
fn related(problem: &str, path: &SettingPath) -> bool {
    let path = path.to_string();
    let inside = |outer: &str, inner: &str| {
        inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.') || rest.starts_with('['))
    };
    inside(&path, problem) || inside(problem, &path)
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn types(schema: &Value) -> Vec<&str> {
    match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

/// Check `value` against `schema`, the subset of JSON Schema the Configuration's schema uses.
fn validate(schema: &Value, value: &Value, at: &str, found: &mut Vec<Problem>) {
    let message = match value {
        _ if !kind_allowed(schema, value) => Some(format!("must be {}{}", types(schema).join(" or "), shown(schema, value))),
        _ if !choice_allowed(schema, value) => Some(format!("must be one of {}{}", choices(schema), shown(schema, value))),
        Value::Number(n) => bounds(schema, n),
        Value::String(s) => text(schema, s),
        Value::Object(map) => return object(schema, map, at, found),
        Value::Array(items) => return array(schema, items, at, found),
        _ => None,
    };
    found.extend(message.map(|message| Problem {
        path: at.to_owned(),
        message,
    }));
}

/// `, not <value>` for a message, or nothing for a secret.
fn shown(schema: &Value, value: &Value) -> String {
    if flag(schema, "writeOnly") {
        String::new()
    } else {
        format!(", not {value}")
    }
}

fn kind_allowed(schema: &Value, value: &Value) -> bool {
    let allowed = types(schema);
    allowed.is_empty() || allowed.contains(&type_name(value))
}

fn choice_allowed(schema: &Value, value: &Value) -> bool {
    schema.get("enum").and_then(Value::as_array).is_none_or(|c| c.contains(value))
}

fn choices(schema: &Value) -> String {
    let names: Vec<String> = schema
        .get("enum")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(Value::to_string)
        .collect();
    names.join(", ")
}

fn bounds(schema: &Value, n: &serde_json::Number) -> Option<String> {
    let (min, max) = (
        schema.get("minimum").and_then(Value::as_u64),
        schema.get("maximum").and_then(Value::as_u64),
    );
    match (n.as_u64(), min, max) {
        (None, Some(min), _) => Some(format!("must be at least {min}")),
        (Some(v), Some(min), _) if v < min => Some(format!("must be at least {min}")),
        (Some(v), _, Some(max)) if v > max => Some(format!("must be at most {max}")),
        _ => None,
    }
}

fn text(schema: &Value, s: &str) -> Option<String> {
    let min = schema.get("minLength").and_then(Value::as_u64).unwrap_or(0);
    if u64::try_from(s.chars().count()).unwrap_or(u64::MAX) < min {
        return Some(if min == 1 {
            "must not be empty".to_owned()
        } else {
            format!("must be at least {min} characters")
        });
    }
    let format = schema.get("format").and_then(Value::as_str)?;
    schema::check_format(format, s).err()
}

fn object(schema: &Value, map: &Map<String, Value>, at: &str, found: &mut Vec<Problem>) {
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str);
    let missing: Vec<Problem> = required
        .filter(|r| !map.contains_key(*r))
        .map(|r| Problem {
            path: join(at, r),
            message: "must be set".to_owned(),
        })
        .collect();
    found.extend(missing);
    let names = schema.get("propertyNames");
    map.iter().for_each(|(key, v)| {
        if let Some(names) = names {
            validate(names, &Value::String(key.clone()), &join(at, key), found);
        }
        if let Some(s) = child(schema, key) {
            validate(s, v, &join(at, key), found);
        }
    });
}

fn array(schema: &Value, items: &[Value], at: &str, found: &mut Vec<Problem>) {
    let min = schema.get("minItems").and_then(Value::as_u64).unwrap_or(0);
    if u64::try_from(items.len()).unwrap_or(u64::MAX) < min {
        found.push(Problem {
            path: at.to_owned(),
            message: format!("must have at least {min} item(s)"),
        });
    }
    if let Some(item) = schema.get("items") {
        items
            .iter()
            .enumerate()
            .for_each(|(i, v)| validate(item, v, &format!("{at}[{i}]"), found));
    }
}

/// The changes by path: each a setting, none owned by another method, none inside another.
fn parse_changes(changes: &[(String, Value)]) -> Result<Vec<(SettingPath, Value)>, SettingError> {
    if changes.is_empty() {
        return Err(SettingError::one("changes", "must name at least one setting"));
    }
    let mut problems = Vec::new();
    let parsed: Vec<(SettingPath, Value)> = changes
        .iter()
        .filter_map(
            |(text, value)| match text.parse::<SettingPath>().and_then(|p| node(&p).map(|n| (p, n.1))) {
                Ok((path, true)) => {
                    let message = "is kept by its own methods (report.*Configuration, report.*StyleSheet); config.set does not change it";
                    problems.push(Problem {
                        path: path.to_string(),
                        message: message.to_owned(),
                    });
                    None
                }
                Ok((path, false)) => Some((path, value.clone())),
                Err(e) => {
                    problems.extend(e.problems);
                    None
                }
            },
        )
        .collect();
    let overlaps = parsed
        .iter()
        .enumerate()
        .flat_map(|(i, (a, _))| parsed[i + 1..].iter().map(move |(b, _)| (a, b)));
    problems.extend(overlaps.filter(|(a, b)| a.within(b) || b.within(a)).map(|(a, b)| Problem {
        path: a.to_string(),
        message: format!("overlaps {b}; change each setting once"),
    }));
    if problems.is_empty() {
        Ok(parsed)
    } else {
        Err(SettingError { problems })
    }
}

/// Set `path` in the stored Configuration, or remove it (restoring its default) for `null`.
fn write(raw: &mut Value, path: &SettingPath, value: &Value) -> Result<(), SettingError> {
    let Value::Object(map) = raw else {
        return Err(SettingError::one(path, "cannot be set: the Configuration is not an object"));
    };
    if value.is_null() {
        remove(map, path.keys());
        return Ok(());
    }
    let (last, parents) = path.keys().split_last().ok_or_else(|| SettingError::one(path, "is empty"))?;
    let parent = parents.iter().try_fold(map, |map, key| {
        let entry = map.entry(key.clone()).or_insert_with(|| Value::Object(Map::new()));
        if entry.is_null() {
            *entry = Value::Object(Map::new());
        }
        let message = || format!("cannot be set: {key} in the Configuration file is not an object");
        entry.as_object_mut().ok_or_else(|| SettingError::one(path, message()))
    })?;
    parent.insert(last.clone(), value.clone());
    Ok(())
}

/// Remove the value at `keys`, and every object the removal leaves empty.
fn remove(map: &mut Map<String, Value>, keys: &[String]) {
    match keys {
        [last] => {
            map.remove(last);
        }
        [first, rest @ ..] => {
            if let Some(Value::Object(inner)) = map.get_mut(first) {
                remove(inner, rest);
                if inner.is_empty() {
                    map.remove(first);
                }
            }
        }
        [] => {}
    }
}

/// The origins of a service's URLs, or `None` when there is no such service.
fn service_origins(effective: &Value, name: &str) -> Option<Vec<String>> {
    let service = effective.get("quotes")?.get("services")?.get(name)?;
    let urls = schema::SERVICE_URLS.iter().filter_map(|url| service.get(url)?.as_str());
    Some(urls.filter_map(schema::origin).collect())
}

/// What a key is bound with: the key itself, and the environment variable it may come from.
const BOUND: [&str; 2] = ["key", "keyEnv"];

/// A key is bound to its service's host: when a change gives a service a URL on an origin none of its URLs had, clear
/// its stored key and key variable, unless the same change sets them. Returns the settings cleared.
fn bind_keys(before: &Value, after: &Value, raw: &mut Value, changes: &[(SettingPath, Value)]) -> Vec<String> {
    let services = after.get("quotes").and_then(|q| q.get("services")).and_then(Value::as_object);
    let moved = services.into_iter().flat_map(Map::keys).filter(|name| {
        let (old, new) = (service_origins(before, name), service_origins(after, name).unwrap_or_default());
        old.is_some_and(|old| new.iter().any(|o| !old.contains(o)))
    });
    let bound = moved.flat_map(|name| BOUND.map(|field| SettingPath(["quotes", "services", name, field].map(str::to_owned).to_vec())));
    let cleared: Vec<SettingPath> = bound
        .filter(|key| !sets(changes, key) && value_at(raw, key.keys()).is_some_and(|k| !k.is_null()))
        .collect();
    if let Value::Object(map) = raw {
        cleared.iter().for_each(|key| remove(map, key.keys()));
    }
    cleared.iter().map(ToString::to_string).collect()
}

/// True when one of `changes` gives `target` a value.
fn sets(changes: &[(SettingPath, Value)], target: &SettingPath) -> bool {
    changes
        .iter()
        .any(|(path, value)| target.within(path) && value_at(value, &target.keys()[path.keys().len()..]).is_some_and(|v| !v.is_null()))
}

/// Refuse a change that introduces a problem, or leaves one in a setting it touches.
fn check_changes(before: &Value, after: &Value, changes: &[(SettingPath, Value)]) -> Result<(), SettingError> {
    let old = problems(before);
    let wrong: Vec<Problem> = problems(after)
        .into_iter()
        .filter(|p| !old.contains(p) || changes.iter().any(|(path, _)| related(&p.path, path)))
        .collect();
    if wrong.is_empty() {
        Ok(())
    } else {
        Err(SettingError { problems: wrong })
    }
}

/// The values of every setting only a person may change, by path.
fn refused_values(schema: &Value, value: &Value, at: &str, found: &mut BTreeMap<String, Value>) {
    if flag(schema, REFUSED_OVER_MCP) {
        found.insert(at.to_owned(), value.clone());
        return;
    }
    if let Value::Object(map) = value {
        let declared = map.iter().filter_map(|(key, v)| Some((key, v, child(schema, key)?)));
        declared.for_each(|(key, v, s)| refused_values(s, v, &join(at, key), found));
    }
}

/// The settings only a person may change whose values differ, leaving out keys cleared by a move to another host.
fn refused_changes(before: &Value, after: &Value, cleared: &[String]) -> Vec<String> {
    let (mut old, mut new) = (BTreeMap::new(), BTreeMap::new());
    refused_values(&SCHEMA, before, "", &mut old);
    refused_values(&SCHEMA, after, "", &mut new);
    let paths: std::collections::BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    paths
        .into_iter()
        .filter(|p| old.get(*p).unwrap_or(&Value::Null) != new.get(*p).unwrap_or(&Value::Null))
        .filter(|p| !cleared.contains(p))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;
