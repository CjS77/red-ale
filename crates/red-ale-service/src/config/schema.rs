//! The Configuration's declared settings, as the JSON Schema (2020-12) document `config.schema` returns
//! (`design/api/configuration.md`).
//!
//! The schema is the one declaration of every setting: its type, default and description. Reading with defaults,
//! validating and the rules for secrets all follow it, so adding a setting is adding one property below. Beyond
//! standard JSON Schema it uses:
//!
//! - `format` as an assertion, with red-ale's formats ([`check_format`]);
//! - `readOnly` for settings other methods own (saved report configurations, style sheets), which `config.set`
//!   refuses;
//! - `writeOnly` for secrets, which are never shown;
//! - `x-refusedOverMcp` for settings only a person may change: `config.set` called through MCP refuses them.

use std::sync::LazyLock;

use red_ale_core::ops::period::{FiscalYearEnd, PeriodBound};
use red_ale_core::{Guid, Numeric};
use serde_json::{Map, Value, json};

/// The keyword marking a setting `config.set` refuses through MCP.
pub const REFUSED_OVER_MCP: &str = "x-refusedOverMcp";

/// The values of the `locale` setting. The locale tables add their locale names here.
pub const LOCALES: &[&str] = &["C", "environment"];

/// The response formats a Quote service can have: one parser each.
pub const QUOTE_FORMATS: &[&str] = &["alphavantage", "twelvedata", "finnhub", "frankfurter", "yahoo-chart", "json"];

/// The largest request the JSON-RPC and MCP servers accept by default: 64 MiB, room for a few years of statements.
pub const DEFAULT_MAX_REQUEST_BYTES: u64 = 64 << 20;

/// The Configuration's JSON Schema document.
pub static SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "red-ale Configuration",
        "description": "red-ale's own settings: what GnuCash keeps outside the book. Keys red-ale does not know are kept.",
        "type": "object",
        "properties": {
            "accountingPeriod": accounting_period(),
            "docLinks": doc_links(),
            "reports": reports(),
            "locale": locale(),
            "network": network(),
            "jsonRpc": requests("JSON-RPC server"),
            "mcp": requests("MCP server"),
            "import": import(),
            "quotes": quotes(),
        }
    })
});

fn group(description: &str, properties: Value) -> Value {
    let mut schema = json!({ "type": "object", "description": description });
    schema["properties"] = properties;
    schema
}

fn boolean(default: bool, description: &str) -> Value {
    json!({ "type": "boolean", "default": default, "description": description })
}

fn integer(default: u64, minimum: u64, maximum: u64, description: &str) -> Value {
    json!({ "type": "integer", "default": default, "minimum": minimum, "maximum": maximum, "description": description })
}

fn choice(default: &str, choices: &[&str], description: &str) -> Value {
    json!({ "type": "string", "enum": choices, "default": default, "description": description })
}

/// A string that is unset (`null`) by default.
fn optional_text(format: Option<&str>, description: &str) -> Value {
    let mut schema = json!({ "type": ["string", "null"], "default": null, "minLength": 1, "description": description });
    if let Some(format) = format {
        schema["format"] = json!(format);
    }
    schema
}

fn accounting_period() -> Value {
    let bound = |default: &str, which: &str| {
        let description = format!(
            "The period's {which}: a relative choice (today, month, previous-month, quarter, previous-quarter, year, previous-year, \
             fiscal-year, previous-fiscal-year) or a YYYY-MM-DD date."
        );
        json!({ "type": "string", "format": "period-bound", "default": default, "description": description })
    };
    group(
        "The accounting period (GnuCash's window.pages.account-tree.summary preferences). Also set with period.set.",
        json!({
            "start": bound("year", "start"),
            "end": bound("year", "end"),
            "fiscalYearEnd": optional_text(Some("fiscal-year-end"), "The fiscal year end, MM-DD; unset uses the open book's fy_end slot."),
        }),
    )
}

fn doc_links() -> Value {
    group(
        "Document links. Also set with docLink.setPathHead.",
        json!({ "pathHead": optional_text(None, "The path head relative document links start from (GnuCash's general/assoc-head).") }),
    )
}

fn reports() -> Value {
    group(
        "Report settings, the report directory, limits, saved report configurations and style sheets (design/reports.md §4, §9).",
        json!({
            "directory": optional_text(None, "The report directory user report scripts load from; relative to the Configuration file."),
            "currency": {
                "type": "string", "format": "commodity-id", "default": "CURRENCY::USD",
                "description": "The default of every \"Report's currency\" option.",
            },
            "reverseBalancedAccounts": choice("credit", &["none", "credit", "incomeExpense"], "Which accounts show reversed balances."),
            "dateFormat": choice("locale", &["us", "uk", "ce", "iso", "locale"], "How report dates print."),
            "weekStart": choice(
                "sunday",
                &["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"],
                "The first day of the week.",
            ),
            "limits": report_limits(),
            "saved": owned_list(
                "Saved report configurations. Changed with report.saveConfiguration and the other report.*Configuration methods.",
                &[("guid", Some("guid")), ("name", None), ("template", Some("guid"))],
            ),
            "styleSheets": owned_list(
                "Style sheet entries. Changed with report.createStyleSheet, report.updateStyleSheet and report.deleteStyleSheet.",
                &[("name", None), ("template", None)],
            ),
        }),
    )
}

/// The report script limits, under the names `data.limit` uses (`seconds` for `time`), with their defaults.
fn report_limits() -> Value {
    let limit = |default: u64, what: &str| integer(default, 1, u64::MAX, what);
    group(
        "Report script limits (design/reports.md §2).",
        json!({
            "operations": limit(1_000_000_000, "Rhai operations per run."),
            "seconds": limit(120, "Run time, in seconds."),
            "call-depth": limit(64, "Function call depth."),
            "expression-depth": limit(128, "Expression nesting at the top level; half of it inside functions."),
            "string-size": limit(1 << 20, "String length, in bytes."),
            "array-size": limit(10_000_000, "Array length."),
            "map-size": limit(1_000_000, "Map size."),
            "modules": limit(64, "Modules loaded."),
            "variables": limit(10_000, "Variables in scope."),
            "functions": limit(1_000, "Functions one script defines."),
            "number-size": limit(4096, "Bits in a Rational's numerator or denominator."),
        }),
    )
}

/// A list other methods own: shown by `config.get`, refused by `config.set`. Each item has the required string
/// `fields`, with their formats.
fn owned_list(description: &str, fields: &[(&str, Option<&str>)]) -> Value {
    let mut properties: Map<String, Value> = fields
        .iter()
        .map(|(name, format)| {
            let mut schema = json!({ "type": "string", "minLength": 1 });
            if let Some(format) = format {
                schema["format"] = json!(format);
            }
            ((*name).to_owned(), schema)
        })
        .collect();
    properties.insert("options".to_owned(), json!({ "type": "object", "additionalProperties": true }));
    let required: Vec<&str> = fields.iter().map(|(name, _)| *name).collect();
    json!({
        "type": "array", "readOnly": true, "default": [], "description": description,
        "items": { "type": "object", "required": required, "properties": properties, "additionalProperties": true },
    })
}

fn locale() -> Value {
    choice(
        "C",
        LOCALES,
        "The locale reports format numbers and dates in: a POSIX locale name, C, or environment (LC_ALL, LC_NUMERIC, LC_TIME, LANG).",
    )
}

fn network() -> Value {
    let mut enabled = boolean(false, "Let red-ale reach the network. Only Quote fetches use it. Not changeable through MCP.");
    enabled[REFUSED_OVER_MCP] = json!(true);
    group("Network access.", json!({ "enabled": enabled }))
}

fn requests(server: &str) -> Value {
    let description = format!("The largest request the {server} accepts, in bytes. Read when the server starts.");
    let bytes = integer(DEFAULT_MAX_REQUEST_BYTES, 1 << 20, u64::from(u32::MAX), &description);
    group(&format!("The {server}."), json!({ "maxRequestBytes": bytes }))
}

/// GnuCash's generic import settings, with its defaults
/// (`5.17:gnucash/gschemas/org.gnucash.GnuCash.dialogs.import.generic.gschema.xml.in:3-47`).
fn import() -> Value {
    let score = |default: u64, what: &str| integer(default, 0, 1000, what);
    let days = |default: u64, what: &str| integer(default, 0, 3650, what);
    group(
        "Import settings: the matcher's rules and thresholds (design/milestone3.md, \"Import\").",
        json!({
            "enableSkip": boolean(true, "\"Skip\" may be chosen for a transaction (enable-skip)."),
            "enableUpdate": boolean(true, "\"Update and clear\" may be chosen (enable-update)."),
            "useBayes": boolean(true, "Bayesian matching instead of the plain account map (use-bayes)."),
            "matchThreshold": score(1, "Lowest score a candidate needs to be offered as a match (match-threshold)."),
            "matchDateThreshold": days(
                4,
                "Dates this many days apart or closer add 2 to the score; the same day adds 3 (match-date-threshold).",
            ),
            "matchDateNotThreshold": days(14, "Dates further apart than this many days take 5 off the score (match-date-not-threshold)."),
            "autoAddThreshold": score(3, "At or below this best score, or with no match, the transaction is added (auto-add-threshold)."),
            "autoClearThreshold": score(
                6,
                "At or above this best score, the match is cleared, or updated and cleared (auto-clear-threshold).",
            ),
            "atmFeeThreshold": {
                "type": "string", "format": "decimal", "default": "2.00",
                "description": "Amount difference still matched, for ATM fees (atm-fee-threshold). A decimal string.",
            },
        }),
    )
}

fn quotes() -> Value {
    group(
        "Online quotes: where each Finance::Quote source is fetched from (design/milestone3.md, \"Quote services\").",
        json!({
            "services": {
                "type": "object",
                "description": "Quote services by name. Changing a service's URL to another host clears its key.",
                "propertyNames": { "format": "name" },
                "additionalProperties": quote_service(),
                "default": default_services(),
            },
            "sources": {
                "type": "object",
                "description": "Finance::Quote source names to the services that answer them, tried in order.",
                "propertyNames": { "format": "name" },
                "additionalProperties": {
                    "type": "array", "minItems": 1,
                    "items": { "type": "string", "format": "name" },
                },
                "default": default_sources(),
            },
        }),
    )
}

fn quote_service() -> Value {
    let pointer = |what: &str| optional_text(Some("json-pointer"), &format!("For the json format: the JSON Pointer to the {what}."));
    let mut key = optional_text(None, "The service's key, if it needs one. Never shown; not changeable through MCP.");
    key["writeOnly"] = json!(true);
    key[REFUSED_OVER_MCP] = json!(true);
    json!({
        "type": "object",
        "required": ["urlTemplate", "format"],
        "properties": {
            "urlTemplate": {
                "type": "string", "format": "url-template",
                "description": "The URL fetched, with {symbol}, {base}, {quote}, {date} and {key} filled in.",
            },
            "format": { "type": "string", "enum": QUOTE_FORMATS, "description": "The response format, which picks the parser." },
            "key": key,
            "minIntervalMs": integer(0, 0, 86_400_000, "The least time between two requests, in milliseconds."),
            "timeoutMs": integer(30_000, 1, 600_000, "How long one request may take, in milliseconds."),
            "pricePointer": pointer("price"),
            "currencyPointer": pointer("price's currency"),
            "datePointer": pointer("price's date"),
        },
    })
}

/// The Proposed defaults of `design/milestone3.md`, "Quote services".
fn default_services() -> Value {
    let service = |url: &str, format: &str, min_interval_ms: u64| {
        json!({ "urlTemplate": url, "format": format, "minIntervalMs": min_interval_ms, "timeoutMs": 30_000 })
    };
    json!({
        "frankfurter": service("https://api.frankfurter.dev/v1/latest?base={base}&symbols={quote}", "frankfurter", 0),
        "alphavantage": service(
            "https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={symbol}&apikey={key}",
            "alphavantage",
            12_000,
        ),
        "alphavantage-currency": service(
            "https://www.alphavantage.co/query?function=CURRENCY_EXCHANGE_RATE&from_currency={base}&to_currency={quote}&apikey={key}",
            "alphavantage",
            12_000,
        ),
        "twelvedata": service("https://api.twelvedata.com/quote?symbol={symbol}&apikey={key}", "twelvedata", 7_500),
        "finnhub": service("https://finnhub.io/api/v1/quote?symbol={symbol}&token={key}", "finnhub", 1_000),
        "yahoo-chart": service("https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval=1d&range=5d", "yahoo-chart", 2_000),
    })
}

fn default_sources() -> Value {
    let us = json!(["alphavantage", "twelvedata"]);
    json!({
        "currency": ["frankfurter", "alphavantage-currency"],
        "alphavantage": ["alphavantage"],
        "twelvedata": ["twelvedata"],
        "finnhub": ["finnhub"],
        "yahoo_json": ["yahoo-chart"],
        "nasdaq": us,
        "nyse": us,
        "usa": us,
    })
}

/// Check `text` against one of red-ale's formats. Unknown formats pass, as JSON Schema's annotations do.
pub fn check_format(format: &str, text: &str) -> Result<(), String> {
    match format {
        "period-bound" => text.parse::<PeriodBound>().map(drop).map_err(|e| e.to_string()),
        "fiscal-year-end" => text.parse::<FiscalYearEnd>().map(drop).map_err(|e| e.to_string()),
        "guid" => Guid::parse(text).map(drop).map_err(|e| e.to_string()),
        "commodity-id" => commodity_id(text),
        "decimal" => decimal(text),
        "url-template" => origin(text).map(drop).ok_or_else(|| "must be an http:// or https:// URL with a host".to_owned()),
        "json-pointer" => (text.is_empty() || text.starts_with('/')).then_some(()).ok_or_else(|| "must be a JSON Pointer".to_owned()),
        "name" => name(text),
        _ => Ok(()),
    }
}

fn commodity_id(text: &str) -> Result<(), String> {
    let ok = match text.split_once("::") {
        Some((space, mnemonic)) => !space.is_empty() && !mnemonic.is_empty(),
        None => !text.trim().is_empty(),
    };
    ok.then_some(()).ok_or_else(|| "must be a commodity, NAMESPACE::MNEMONIC or a currency code".to_owned())
}

fn decimal(text: &str) -> Result<(), String> {
    match Numeric::parse_decimal(text) {
        Ok(n) if !n.is_negative() => Ok(()),
        _ => Err("must be a decimal string, not negative, such as \"2.00\"".to_owned()),
    }
}

fn name(text: &str) -> Result<(), String> {
    let ok = !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    ok.then_some(()).ok_or_else(|| "must be letters, digits, _ and - only".to_owned())
}

/// The origin of an `http` or `https` URL template: scheme, host and port, lower-cased. A key is bound to it.
pub fn origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    (!host.is_empty()).then(|| format!("{scheme}://{}", host.to_ascii_lowercase()))
}

/// Problems the schema cannot express: every service a source names must exist.
pub fn cross_checks(effective: &Value) -> Vec<(String, String)> {
    let services = effective.pointer("/quotes/services").and_then(Value::as_object);
    let sources = effective.pointer("/quotes/sources").and_then(Value::as_object).into_iter().flatten();
    sources
        .flat_map(|(source, names)| names.as_array().into_iter().flatten().enumerate().map(move |(i, n)| (source, i, n)))
        .filter_map(|(source, i, n)| {
            let name = n.as_str()?;
            let known = services.is_some_and(|s| s.contains_key(name));
            (!known).then(|| (format!("quotes.sources.{source}[{i}]"), format!("names the quote service {name:?}, which does not exist")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_are_scheme_host_and_port() {
        assert_eq!(origin("https://API.example.com:8443/q?x={key}").as_deref(), Some("https://api.example.com:8443"));
        assert_eq!(origin("https://user@example.com/q").as_deref(), Some("https://example.com"));
        assert_ne!(origin("http://example.com/"), origin("https://example.com/"));
        assert_eq!(origin("ftp://example.com/"), None);
        assert_eq!(origin("https:///path"), None);
    }

    #[test]
    fn formats_check_their_text() {
        assert!(check_format("period-bound", "previous-month").is_ok());
        assert!(check_format("period-bound", "soon").is_err());
        assert!(check_format("fiscal-year-end", "06-30").is_ok());
        assert!(check_format("decimal", "2.00").is_ok());
        assert!(check_format("decimal", "-1").is_err());
        assert!(check_format("decimal", "1e3").is_err());
        assert!(check_format("commodity-id", "CURRENCY::EUR").is_ok());
        assert!(check_format("commodity-id", "::EUR").is_err());
        assert!(check_format("name", "yahoo_json").is_ok());
        assert!(check_format("name", "a.b").is_err());
        assert!(check_format("json-pointer", "/data/0/price").is_ok());
    }

    #[test]
    fn every_default_source_names_a_default_service() {
        let defaults = json!({ "quotes": { "services": default_services(), "sources": default_sources() } });
        assert_eq!(cross_checks(&defaults), Vec::new());
    }
}
