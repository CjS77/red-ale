//! Methods: red-ale's Configuration, read and changed by setting path against its declared schema. They work with no
//! book open and never make a book dirty. See `design/api/configuration.md`.

use red_ale_service::config::schema::SCHEMA;
use red_ale_service::config::{Problem, SettingError, SettingPath};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::dispatch::{Context, Effect, Method, Origin, method};
use crate::error::{RpcError, codes};
use crate::wire::NoParams;

const GET: &str = "The whole Configuration, or one setting by path, with defaults filled in; secrets shown only as set or unset.";
const SET: &str = "Change one or more settings by path, validated against the schema, all or none; null restores a default.";

/// The Configuration methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "config.schema",
            "The Configuration's JSON Schema: every setting, its type, default and description.",
            Effect::Read,
            schema,
        ),
        method("config.get", GET, Effect::Read, get),
        method("config.set", SET, Effect::Write, set).on_configuration(),
    ]
}

#[allow(clippy::unnecessary_wraps)] // Every handler returns a Result.
fn schema(_: &mut Context, _: NoParams) -> Result<Value, RpcError> {
    Ok(SCHEMA.clone())
}

/// A setting whose value in the Configuration does not match the schema.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ProblemInfo {
    /// The setting, with `[i]` for a list item.
    pub path: String,
    /// What is wrong.
    pub message: String,
}

impl From<Problem> for ProblemInfo {
    fn from(p: Problem) -> Self {
        Self {
            path: p.path,
            message: p.message,
        }
    }
}

fn setting_error(e: SettingError) -> RpcError {
    let problems: Vec<ProblemInfo> = e.problems.iter().cloned().map(ProblemInfo::from).collect();
    RpcError::invalid_params(e.to_string()).with_data(json!({ "problems": problems }))
}

fn path(text: &str) -> Result<SettingPath, RpcError> {
    text.parse().map_err(setting_error)
}

fn config_file(ctx: &Context) -> Option<String> {
    ctx.settings.config.path().map(|p| p.display().to_string())
}

/// Params of `config.get`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetParams {
    /// A setting path, keys joined with `.` (`import.settings.matchThreshold`, `quotes.services.alphavantage`); absent for the
    /// whole Configuration.
    #[serde(default)]
    pub path: Option<String>,
}

/// The result of `config.get`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetResult {
    /// The path asked for; `null` for the whole Configuration.
    pub path: Option<String>,
    /// The value, defaults filled in, each secret shown as `{"set": true|false}`. `null` for a map entry that does
    /// not exist.
    pub value: Value,
    /// Keys in the file, at or below the path, that declare no setting. They are kept and do nothing.
    pub unknown: Vec<String>,
    /// Settings, at or below the path, whose values in the file do not match the schema.
    pub problems: Vec<ProblemInfo>,
    /// The Configuration file; `null` for a process started with `--no-config`, whose Configuration is in memory.
    pub config_file: Option<String>,
}

/// True when `found` is `at` or lies below it.
fn below(found: &str, at: Option<&str>) -> bool {
    at.is_none_or(|at| {
        found
            .strip_prefix(at)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.') || rest.starts_with('['))
    })
}

fn get(ctx: &mut Context, p: GetParams) -> Result<GetResult, RpcError> {
    let config = ctx.settings.config.load()?;
    let at = p.path.as_deref().map(path).transpose()?;
    let value = config.view(at.as_ref()).map_err(setting_error)?;
    let shown = p.path.as_deref();
    Ok(GetResult {
        unknown: config.unknown_keys().into_iter().filter(|k| below(k, shown)).collect(),
        problems: config
            .problems()
            .into_iter()
            .filter(|q| below(&q.path, shown))
            .map(ProblemInfo::from)
            .collect(),
        path: p.path,
        value,
        config_file: config_file(ctx),
    })
}

/// Params of `config.set`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetParams {
    /// Setting paths to their new values; `null` restores a setting's default. No path may lie inside another.
    pub changes: Map<String, Value>,
}

/// The result of `config.set`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetResult {
    /// Each path changed, with its value now, as `config.get` shows it.
    pub values: Map<String, Value>,
    /// Keys cleared because their Quote service's URL moved to another host.
    pub cleared_keys: Vec<String>,
    /// The Configuration file; `null` for a process started with `--no-config`, whose Configuration is in memory.
    pub config_file: Option<String>,
}

fn set(ctx: &mut Context, p: SetParams) -> Result<SetResult, RpcError> {
    let lock = ctx.settings.config.lock()?;
    let mut config = ctx.settings.config.load()?;
    let changes: Vec<(String, Value)> = p.changes.into_iter().collect();
    let applied = config.apply(&changes).map_err(setting_error)?;
    if ctx.origin() == Origin::Mcp && !applied.refused_over_mcp.is_empty() {
        return Err(refused_over_mcp(applied.refused_over_mcp));
    }
    let values = changes
        .iter()
        .map(|(text, _)| Ok((text.clone(), config.view(Some(&path(text)?)).map_err(setting_error)?)));
    let values = values.collect::<Result<Map<String, Value>, RpcError>>()?;
    ctx.settings.config.save(&lock, &config)?;
    Ok(SetResult {
        values,
        cleared_keys: applied.cleared_keys,
        config_file: config_file(ctx),
    })
}

/// Keys and the network switch are a person's to change: through MCP they are refused, and nothing is written.
fn refused_over_mcp(paths: Vec<String>) -> RpcError {
    let message = format!(
        "{} can be changed only through the CLI, JSON-RPC or the Configuration file, not through MCP",
        paths.join(", ")
    );
    RpcError::new(codes::CONFLICT, message).with_data(json!({ "reason": "refused-over-mcp", "paths": paths }))
}
