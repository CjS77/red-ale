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
use red_ale_import::csv::prices::PriceColumn;
use red_ale_import::csv::{ColumnType, DateFormat};
use red_ale_locale::Locale;
use serde_json::{Map, Value, json};

/// The keyword marking a setting `config.set` refuses through MCP.
pub const REFUSED_OVER_MCP: &str = "x-refusedOverMcp";

/// The values of the `locale` setting: every supported locale name, `C` among them, and `environment`.
pub static LOCALES: LazyLock<Vec<&'static str>> = LazyLock::new(|| Locale::supported().chain(["environment"]).collect());

/// The response formats a Quote service can have: one parser each.
pub const QUOTE_FORMATS: &[&str] = &["alphavantage", "twelvedata", "finnhub", "frankfurter", "yahoo-chart", "json"];

/// The largest request the JSON-RPC and MCP servers accept by default: 64 MiB, room for a few years of statements.
pub const DEFAULT_MAX_REQUEST_BYTES: u64 = 64 << 20;

/// The largest response the JSON-RPC server sends by default: 256 MiB, room for a CSV export of a large book
/// (`design/api/export.md`). jsonrpsee's own default, 10 MB, is a few years of transactions.
pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 256 << 20;

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
            "defaultCurrency": optional_text(
                Some("commodity-id"),
                "GnuCash's default currency (general/currency-other): what currencies are quoted in, and the currency of new \
                 Owners and QIF imports. Unset, the Locale's currency, else USD.",
            ),
            "jsonRpc": json_rpc(),
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
            "currency": optional_text(
                Some("commodity-id"),
                "The default of every \"Report's currency\" option. Unset, the Locale's currency if the book has it, else USD.",
            ),
            "reverseBalancedAccounts": choice("credit", &["none", "credit", "incomeExpense"], "Which accounts show reversed balances."),
            "dateFormat": choice("locale", &["us", "uk", "ce", "iso", "locale"], "How report dates print; locale is the Locale's date format."),
            "weekStart": {
                "type": ["string", "null"], "default": null,
                "enum": ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", null],
                "description": "The first day of the week. Unset, the Locale's (Sunday under C).",
            },
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
        &LOCALES,
        "The Locale reports format numbers and dates in, and unset report settings default from: a POSIX locale name such as \
         fr_FR, C, or environment, which takes each category as the C library does from LC_ALL, then LC_NUMERIC, LC_MONETARY, \
         LC_TIME or LC_MESSAGES, then LANG.",
    )
}

fn requests(server: &str) -> Value {
    let description = format!("The largest request the {server} accepts, in bytes. Read when the server starts.");
    let bytes = integer(DEFAULT_MAX_REQUEST_BYTES, 1 << 20, u64::from(u32::MAX), &description);
    group(&format!("The {server}."), json!({ "maxRequestBytes": bytes }))
}

/// The JSON-RPC server: the request limit, and the response limit a large export needs.
fn json_rpc() -> Value {
    let mut schema = requests("JSON-RPC server");
    let description = "The largest response the JSON-RPC server sends, in bytes; a larger result is an error. Read when the server starts.";
    schema["properties"]["maxResponseBytes"] = integer(DEFAULT_MAX_RESPONSE_BYTES, 1 << 20, u64::from(u32::MAX), description);
    schema
}

/// What imports keep outside the book (`design/api/import.md`): the Import settings, CSV import layouts and the QIF
/// account map. The import work declares the layouts' and the map's contents.
fn import() -> Value {
    group(
        "Import: the matcher's settings, CSV import layouts and the QIF account map (design/api/import.md).",
        json!({
            "settings": import_settings(),
            "csvLayouts": {
                "type": "object", "default": {},
                "description": "CSV transaction import layouts by name, beside the built-in ones (No Settings, GnuCash Export Format, \
                                GnuCash Export Format (4.x and older)), which always win over one of the same name.",
                "propertyNames": { "minLength": 1 },
                "additionalProperties": csv_layout(),
            },
            "csvPriceLayouts": {
                "type": "object", "default": {},
                "description": "CSV price import layouts by name, beside the built-in one (No Settings), which wins over one of the same name.",
                "propertyNames": { "minLength": 1 },
                "additionalProperties": csv_price_layout(),
            },
            "qifAccountMap": qif_account_map(),
        }),
    )
}

/// The account types a QIF map entry may list, as GnuCash names them.
const ACCOUNT_TYPES: [&str; 20] = [
    "NONE", "BANK", "CASH", "CREDIT", "ASSET", "LIABILITY", "STOCK", "MUTUAL", "CURRENCY", "INCOME", "EXPENSE", "EQUITY", "RECEIVABLE",
    "PAYABLE", "ROOT", "TRADING", "CHECKING", "SAVINGS", "MONEYMRKT", "CREDITLINE",
];

/// The QIF account map (`SavedMap`'s serde form): what GnuCash keeps in its `qif-accounts-map` file
/// (`5.17:gnucash/import-export/qif-imp/qif-guess-map.scm:296-344`). `import.apply` saves it.
fn qif_account_map() -> Value {
    let entry = json!({
        "type": "object", "required": ["account"], "additionalProperties": false,
        "properties": {
            "account": { "type": "string", "minLength": 1, "description": "The GnuCash account's full name." },
            "types": { "type": "array", "items": { "type": "string", "enum": ACCOUNT_TYPES }, "description": "The account types the QIF name may map to." },
            "description": { "type": ["string", "null"], "description": "The description, from the QIF account record." },
        },
    });
    let map = |what: &str| json!({ "type": "object", "default": {}, "description": what, "additionalProperties": entry });
    json!({
        "type": "object", "default": {},
        "description": "The QIF account map: QIF accounts, categories, payees and securities to accounts and commodities. Saved by import.apply.",
        "properties": {
            "accounts": map("QIF account names to accounts."),
            "categories": map("QIF categories to accounts."),
            "payees": map("QIF payees and memos to accounts."),
            "securities": {
                "type": "object", "default": {}, "description": "QIF security names to commodities.",
                "additionalProperties": {
                    "type": "object", "required": ["namespace", "mnemonic"], "additionalProperties": false,
                    "properties": {
                        "namespace": { "type": "string", "minLength": 1, "description": "The commodity's namespace." },
                        "mnemonic": { "type": "string", "minLength": 1, "description": "Its mnemonic." },
                    },
                },
            },
        },
    })
}

/// The fields every CSV import layout has: GnuCash's `CsvImportSettings`
/// (`5.17:gnucash/import-export/csv-imp/gnc-imp-settings-csv.cpp:50-66`).
fn csv_common() -> Map<String, Value> {
    let dates: Vec<&str> = DateFormat::ALL.iter().map(|f| f.name()).collect();
    let count = |what: &str| json!({ "type": "integer", "minimum": 0, "maximum": u32::MAX, "description": what });
    let flag = |what: &str| json!({ "type": "boolean", "description": what });
    let common = json!({
        "format": { "type": "string", "enum": ["csv", "fixedWidth"], "description": "Separated fields (csv, the default) or fixed widths." },
        "separators": { "type": "string", "description": "The separator characters, any of which ends a field; default \",\"." },
        "escape": flag("Backslash escapes in fields; default true."),
        "columnWidths": { "type": "array", "items": count("A width in characters."), "description": "Fixed width only: each column's width." },
        "skipStartLines": count("Lines to skip at the start, such as a header."),
        "skipEndLines": count("Lines to skip at the end."),
        "skipAltLines": flag("Skip every second line after the start lines."),
        "dateFormat": { "type": "string", "enum": dates, "description": "How dates are written; default y-m-d. locale is ICU's default date format for the locale setting." },
        "currencyFormat": { "type": "string", "enum": ["locale", "period", "comma"], "description": "How amounts are written: the Locale's, 123,456.78 (period) or 123.456,78 (comma)." },
        "encoding": { "type": "string", "format": "encoding", "description": "The file's encoding, a WHATWG label; default UTF-8." },
    });
    common.as_object().cloned().unwrap_or_default()
}

/// A CSV transaction import layout: GnuCash's preset, key for key (`TxLayout`'s serde form; `design/api/import.md`,
/// "CSV transactions"). Fields left out take GnuCash's defaults, so none is declared here.
pub fn csv_layout() -> Value {
    let columns: Vec<&str> = ColumnType::ALL.iter().map(|t| t.gnucash_name()).chain(["Deposit", "Withdrawal", "Num"]).collect();
    let mut properties = csv_common();
    properties.insert("multiSplit".to_owned(), json!({ "type": "boolean", "description": "Several lines make one transaction, one split per line." }));
    properties.insert(
        "baseAccount".to_owned(),
        json!({
            "type": ["string", "object", "null"],
            "description": "Single-line mode with no Account column: every line's account, by full name or as { guid, fullName }.",
            "properties": { "guid": { "type": "string", "format": "guid" }, "fullName": { "type": "string", "minLength": 1 } },
        }),
    );
    properties.insert("columns".to_owned(), json!({ "type": "array", "items": { "type": "string", "enum": columns }, "description": "Each column's type, by GnuCash's saved name." }));
    json!({
        "type": "object",
        "description": "A CSV transaction import layout (GnuCash's import preset). Fields left out take GnuCash's defaults.",
        "properties": properties,
    })
}

/// A CSV price import layout: GnuCash's price preset, key for key (`PriceLayout`'s serde form, `design/api/import.md`,
/// "CSV prices"; `5.17:gnucash/import-export/csv-imp/gnc-imp-settings-csv-price.cpp:120-228`).
pub fn csv_price_layout() -> Value {
    let columns: Vec<&str> = PriceColumn::ALL.iter().map(|t| t.gnucash_name()).collect();
    let mut properties = csv_common();
    let commodity = |what: &str| json!({ "type": "string", "format": "commodity-id", "description": what });
    properties.insert(
        "fromCommodity".to_owned(),
        commodity("GnuCash's Commodity From: the commodity of every price, NAMESPACE::MNEMONIC, in place of the From Symbol and From Namespace columns."),
    );
    properties.insert("toCurrency".to_owned(), commodity("GnuCash's Currency To: the currency of every price, in place of the Currency To column."));
    properties.insert("columns".to_owned(), json!({ "type": "array", "items": { "type": "string", "enum": columns }, "description": "Each column's type, by GnuCash's saved name." }));
    json!({
        "type": "object",
        "description": "A CSV price import layout (GnuCash's price import preset). Fields left out take GnuCash's defaults.",
        "properties": properties,
    })
}

/// GnuCash's generic import settings, with its defaults
/// (`5.17:gnucash/gschemas/org.gnucash.GnuCash.dialogs.import.generic.gschema.xml.in:3-47`).
fn import_settings() -> Value {
    let score = |default: u64, what: &str| integer(default, 0, 1000, what);
    let days = |default: u64, what: &str| integer(default, 0, 3650, what);
    group(
        "The Import settings: the matcher's rules and thresholds.",
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
    let mut network = boolean(false, "Let Quote fetches reach the network; nothing else does. Not changeable through MCP.");
    network[REFUSED_OVER_MCP] = json!(true);
    group(
        "Online quotes: where each Finance::Quote source is fetched from (design/api/quotes.md).",
        json!({
            "network": network,
            "timeoutSeconds": integer(30, 1, 600, "How long one request may take, in seconds."),
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

/// The URL settings of a Quote service. A key is bound to their hosts.
pub const SERVICE_URLS: [&str; 2] = ["securityUrl", "currencyUrl"];

fn quote_service() -> Value {
    let url = |what: &str| {
        let description = format!("The URL template for {what}, with {{symbol}}, {{base}}, {{quote}}, {{date}} and {{key}} filled in.");
        optional_text(Some("url-template"), &description)
    };
    let pointer = |what: &str| optional_text(Some("json-pointer"), &format!("The JSON Pointer to the {what}."));
    let mut key = optional_text(None, "The service's key, if it needs one. Never shown; not changeable through MCP.");
    key["writeOnly"] = json!(true);
    key[REFUSED_OVER_MCP] = json!(true);
    let mut key_env = optional_text(
        Some("env-name"),
        "An environment variable to take the key from when key is unset. Not changeable through MCP, since it hands a key on.",
    );
    key_env[REFUSED_OVER_MCP] = json!(true);
    json!({
        "type": "object",
        "required": ["format"],
        "properties": {
            "format": { "type": "string", "enum": QUOTE_FORMATS, "description": "The response format, which picks the parser." },
            "securityUrl": url("securities"),
            "currencyUrl": url("currency rates"),
            "key": key,
            "keyEnv": key_env,
            "minIntervalSeconds": integer(0, 0, 86_400, "The least time between two requests to the service, in seconds."),
            "currency": optional_text(Some("commodity-id"), "The currency of a price when the answer has none."),
            "pointers": group(
                "For the json format: JSON Pointers into the answer.",
                json!({
                    "price": pointer("price (required for the json format)"),
                    "currency": pointer("price's currency"),
                    "date": pointer("price's date"),
                }),
            ),
        },
    })
}

/// The Proposed defaults of `design/api/quotes.md`, "Quote services".
fn default_services() -> Value {
    json!({
        "frankfurter": { "format": "frankfurter", "currencyUrl": "https://api.frankfurter.dev/v1/latest?base={base}&symbols={quote}" },
        "alphavantage": {
            "format": "alphavantage",
            "securityUrl": "https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={symbol}&apikey={key}",
            "currencyUrl":
                "https://www.alphavantage.co/query?function=CURRENCY_EXCHANGE_RATE&from_currency={base}&to_currency={quote}&apikey={key}",
            "keyEnv": "ALPHAVANTAGE_API_KEY",
            "minIntervalSeconds": 15,
        },
        "twelvedata": {
            "format": "twelvedata",
            "securityUrl": "https://api.twelvedata.com/quote?symbol={symbol}&apikey={key}",
            "currencyUrl": "https://api.twelvedata.com/exchange_rate?symbol={base}/{quote}&apikey={key}",
            "keyEnv": "TWELVEDATA_API_KEY",
            "minIntervalSeconds": 8,
        },
        "finnhub": {
            "format": "finnhub",
            "securityUrl": "https://finnhub.io/api/v1/quote?symbol={symbol}&token={key}",
            "keyEnv": "FINNHUB_API_KEY",
            "minIntervalSeconds": 1,
        },
        "yahoo-chart": {
            "format": "yahoo-chart",
            "securityUrl": "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval=1d&range=1d",
            "currencyUrl": "https://query1.finance.yahoo.com/v8/finance/chart/{base}{quote}=X?interval=1d&range=1d",
            "minIntervalSeconds": 2,
        },
    })
}

fn default_sources() -> Value {
    let us = json!(["alphavantage", "twelvedata"]);
    json!({
        "currency": ["frankfurter", "alphavantage"],
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
        "url-template" => url_template(text),
        "json-pointer" => (text.is_empty() || text.starts_with('/')).then_some(()).ok_or_else(|| "must be a JSON Pointer".to_owned()),
        "name" => name(text),
        "env-name" => env_name(text),
        "encoding" => encoding_rs::Encoding::for_label(text.trim().as_bytes()).map(drop).ok_or_else(|| "must be a WHATWG encoding label".to_owned()),
        _ => Ok(()),
    }
}

/// The placeholders a URL template may hold.
pub const PLACEHOLDERS: [&str; 5] = ["symbol", "base", "quote", "date", "key"];

fn url_template(text: &str) -> Result<(), String> {
    origin(text).ok_or_else(|| "must be an http:// or https:// URL with a host".to_owned())?;
    let mut pieces = text.split('{').skip(1);
    let unknown = pieces.find_map(|piece| {
        let name = piece.split_once('}').map_or(piece, |(name, _)| name);
        (!PLACEHOLDERS.contains(&name)).then(|| name.to_owned())
    });
    match unknown {
        Some(name) => {
            let known = PLACEHOLDERS.map(|p| format!("{{{p}}}")).join(", ");
            Err(format!("has the placeholder {{{name}}}; only {known} are filled in"))
        }
        None => Ok(()),
    }
}

fn env_name(text: &str) -> Result<(), String> {
    let ok = text.bytes().next().is_some_and(|b| !b.is_ascii_digit()) && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    ok.then_some(()).ok_or_else(|| "must be an environment variable name: letters, digits and _".to_owned())
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

/// Problems the schema cannot express: every service a source names exists, every service has a URL, and a `json`
/// service says where its price is.
pub fn cross_checks(effective: &Value) -> Vec<(String, String)> {
    let services = effective.pointer("/quotes/services").and_then(Value::as_object);
    let sources = effective.pointer("/quotes/sources").and_then(Value::as_object).into_iter().flatten();
    let unknown = sources
        .flat_map(|(source, names)| names.as_array().into_iter().flatten().enumerate().map(move |(i, n)| (source, i, n)))
        .filter_map(|(source, i, n)| {
            let name = n.as_str()?;
            let known = services.is_some_and(|s| s.contains_key(name));
            (!known).then(|| (format!("quotes.sources.{source}[{i}]"), format!("names the quote service {name:?}, which does not exist")))
        });
    let incomplete = services.into_iter().flatten().filter_map(|(name, service)| {
        let has = |key: &str| service.get(key).is_some_and(|v| !v.is_null());
        let at = format!("quotes.services.{name}");
        if !SERVICE_URLS.iter().any(|url| has(url)) {
            return Some((at, "needs a securityUrl or a currencyUrl".to_owned()));
        }
        let json = service.get("format").and_then(Value::as_str) == Some("json");
        let price = service.pointer("/pointers/price").is_some_and(|p| !p.is_null());
        (json && !price).then(|| (format!("{at}.pointers.price"), "must be set for the json format".to_owned()))
    });
    unknown.chain(incomplete).collect()
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
        assert!(check_format("url-template", "https://q.example/{symbol}?k={key}&d={date}").is_ok());
        assert!(check_format("url-template", "https://q.example/{ticker}").is_err());
        assert!(check_format("url-template", "q.example/{symbol}").is_err());
        assert!(check_format("env-name", "ALPHAVANTAGE_API_KEY").is_ok());
        assert!(check_format("env-name", "1KEY").is_err());
        assert!(check_format("env-name", "A-B").is_err());
    }

    #[test]
    fn every_default_source_names_a_default_service() {
        let defaults = json!({ "quotes": { "services": default_services(), "sources": default_sources() } });
        assert_eq!(cross_checks(&defaults), Vec::new());
    }
}
