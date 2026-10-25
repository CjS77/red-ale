//! Turning command-line arguments into JSON-RPC params.

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value};

/// Params from arguments: either one JSON object, or `key=value` pairs (the value is a string) and `key:=json` pairs
/// (the value is JSON). No arguments give `{}`.
pub fn parse(args: &[String]) -> Result<Value> {
    if let [only] = args
        && only.trim_start().starts_with('{')
    {
        let value: Value = serde_json::from_str(only).context("params are not valid JSON")?;
        if !value.is_object() {
            bail!("params must be a JSON object");
        }
        return Ok(value);
    }
    args.iter().map(|a| pair(a)).collect::<Result<Map<_, _>>>().map(Value::Object)
}

fn pair(arg: &str) -> Result<(String, Value)> {
    if let Some((key, json)) = arg.split_once(":=")
        && !key.contains('=')
    {
        let value = serde_json::from_str(json).with_context(|| format!("{key}: not valid JSON: {json}"))?;
        return Ok((key.to_owned(), value));
    }
    match arg.split_once('=') {
        Some((key, value)) if !key.is_empty() => Ok((key.to_owned(), Value::String(value.to_owned()))),
        _ => bail!("expected key=value or key:=json, got {arg:?}"),
    }
}

/// One script line: `method {json}`, `method` alone, or `{"method": ..., "params": ...}`.
pub fn script_line(line: &str) -> Result<(String, Value)> {
    let line = line.trim();
    if line.starts_with('{') {
        let mut v: Value = serde_json::from_str(line).context("not valid JSON")?;
        let method = v.get("method").and_then(Value::as_str).context("missing \"method\"")?.to_owned();
        let params = v.get_mut("params").map_or(Value::Null, Value::take);
        return Ok((method, params));
    }
    let (method, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let params = if rest.trim().is_empty() { Value::Null } else { serde_json::from_str(rest.trim()).context("params are not valid JSON")? };
    Ok((method.to_owned(), params))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn pairs_and_objects() {
        assert_eq!(parse(&strings(&["name=Checking", "placeholder:=true", "amount=12.50"])).unwrap(), json!({ "name": "Checking", "placeholder": true, "amount": "12.50" }));
        assert_eq!(parse(&strings(&[r#"{"a": "b"}"#])).unwrap(), json!({ "a": "b" }));
        assert_eq!(parse(&[]).unwrap(), json!({}));
        assert_eq!(parse(&strings(&["memo=a:=b"])).unwrap(), json!({ "memo": "a:=b" }));
        assert!(parse(&strings(&["novalue"])).is_err());
    }

    #[test]
    fn script_lines() {
        assert_eq!(script_line(r#"account.create {"name": "A"}"#).unwrap(), ("account.create".into(), json!({ "name": "A" })));
        assert_eq!(script_line("book.info").unwrap(), ("book.info".into(), Value::Null));
        assert_eq!(script_line(r#"{"method": "book.save", "params": {}}"#).unwrap(), ("book.save".into(), json!({})));
    }
}
