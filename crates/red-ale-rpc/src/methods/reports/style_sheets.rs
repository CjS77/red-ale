//! Style sheets (`design/reports.md` §8, §9): the six predefined ones, always present, and user style sheets, kept
//! in red-ale's Configuration as GnuCash keeps them in `stylesheets-2.0`.
//!
//! The Configuration keeps whatever option values it was given, including those red-ale ignores (images), so a
//! style sheet edited here keeps the options GnuCash would use: an entry is merged with the changes, never replaced
//! by what red-ale renders with.

use red_ale_report::ReportError;
use red_ale_report::options::OptionInfo;
use red_ale_report::render::{StyleSheet, StyleSheetTemplate};
use red_ale_service::config::Config;
use red_ale_service::reports::StyleSheetRecord;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{ReportOptions, name, style_sheets, taken};
use crate::dispatch::Context;
use crate::error::RpcError;
use crate::wire::NoParams;

const WHAT: &str = "style sheet";
/// The `data.kind` of its errors.
const KIND: &str = "styleSheet";

/// A style sheet template and the options its style sheets have, at their defaults.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    /// `CSS`, `Easy`, `Fancy`, `Footer`, `Head or Tail` or `Plain`.
    pub name: String,
    /// Its options.
    pub options: Vec<OptionInfo>,
}

/// A style sheet with its options.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StyleSheetInfo {
    /// The style sheet's name.
    pub name: String,
    /// Its template's name.
    pub template: String,
    /// Whether it is one of GnuCash's six predefined style sheets, which can be changed and reset but not deleted.
    pub predefined: bool,
    /// Its options, with their defaults and values.
    pub options: Vec<OptionInfo>,
}

impl StyleSheetInfo {
    fn of(sheet: &StyleSheet) -> Self {
        Self {
            name: sheet.name().to_owned(),
            template: sheet.template().name().to_owned(),
            predefined: sheet.is_predefined(),
            options: sheet.options().describe(),
        }
    }
}

/// The result of `report.styleSheets`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListResult {
    /// The style sheet templates, sorted by name.
    pub templates: Vec<TemplateInfo>,
    /// Every style sheet, sorted by name bytewise.
    pub style_sheets: Vec<StyleSheetInfo>,
}

pub(super) fn list(ctx: &mut Context, _: NoParams) -> Result<ListResult, RpcError> {
    let sheets = style_sheets(&ctx.settings.config.load()?.style_sheets()?)?;
    let templates = StyleSheetTemplate::ALL.iter().map(|t| TemplateInfo { name: t.name().to_owned(), options: t.options().describe() });
    Ok(ListResult { templates: templates.collect(), style_sheets: sheets.iter().map(StyleSheetInfo::of).collect() })
}

/// Params of `report.createStyleSheet`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateParams {
    /// The new style sheet's name.
    pub name: String,
    /// The style sheet template: `CSS`, `Easy`, `Fancy`, `Footer`, `Head or Tail` or `Plain`.
    pub template: String,
    /// Option values over the template's defaults.
    #[serde(default)]
    pub options: Option<ReportOptions>,
}

pub(super) fn create(ctx: &mut Context, p: CreateParams) -> Result<StyleSheetInfo, RpcError> {
    let config = ctx.settings.config.load()?;
    let mut records = config.style_sheets()?;
    let name = name(&p.name, WHAT)?;
    if style_sheets(&records)?.get(&name).is_some() {
        return Err(taken(KIND, WHAT, &name));
    }
    let template = StyleSheetTemplate::from_name(&p.template).ok_or_else(|| RpcError::not_found("styleSheetTemplate", &p.template))?;
    let mut sheet = StyleSheet::new(name, template);
    let given = p.options.map_or_else(|| Value::Object(Map::new()), |o| o.0);
    sheet.apply(&given).map_err(ReportError::from)?;
    let raw = given.as_object().cloned().unwrap_or_default();
    records.push(record(&sheet, &raw, Map::new()));
    store(ctx, config, &records)?;
    Ok(StyleSheetInfo::of(&sheet))
}

/// Params of `report.updateStyleSheet`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateParams {
    /// The style sheet.
    pub name: String,
    /// The options to set; `null` returns one to its default.
    pub options: ReportOptions,
}

/// Change a style sheet's options. Predefined style sheets can be changed too, as GnuCash lets the user edit
/// `Default`.
pub(super) fn update(ctx: &mut Context, p: UpdateParams) -> Result<StyleSheetInfo, RpcError> {
    let config = ctx.settings.config.load()?;
    let mut records = config.style_sheets()?;
    let mut sheet = style_sheets(&records)?.get(&p.name).cloned().ok_or_else(|| RpcError::not_found(KIND, &p.name))?;
    sheet.apply(&p.options.0).map_err(ReportError::from)?;
    let previous = records.iter().rev().find(|r| r.name == p.name);
    let raw = merge(previous.map(|r| r.options.clone()).unwrap_or_default(), &p.options.0);
    let changed = record(&sheet, &raw, previous.map(|r| r.other.clone()).unwrap_or_default());
    let first = records.iter().position(|r| r.name == p.name);
    records.retain(|r| r.name != p.name);
    records.insert(first.unwrap_or(records.len()).min(records.len()), changed);
    store(ctx, config, &records)?;
    Ok(StyleSheetInfo::of(&sheet))
}

/// Params of `report.deleteStyleSheet`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeleteParams {
    /// The style sheet.
    pub name: String,
}

/// The result of `report.deleteStyleSheet`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    /// The style sheet as it was.
    pub deleted: StyleSheetInfo,
    /// True when it is predefined and so went back to its template's defaults rather than going away.
    pub reset: bool,
}

/// Delete a user style sheet, or reset a predefined one to its template's defaults, which is what GnuCash does at
/// its next start (`5.17:gnucash/report/gnc-report.cpp:81-111`).
pub(super) fn delete(ctx: &mut Context, p: DeleteParams) -> Result<DeleteResult, RpcError> {
    let config = ctx.settings.config.load()?;
    let mut records = config.style_sheets()?;
    let sheet = style_sheets(&records)?.get(&p.name).cloned().ok_or_else(|| RpcError::not_found(KIND, &p.name))?;
    records.retain(|r| r.name != p.name);
    store(ctx, config, &records)?;
    Ok(DeleteResult { deleted: StyleSheetInfo::of(&sheet), reset: sheet.is_predefined() })
}

/// The Configuration entry for `sheet`: the options that differ from its template's defaults, and from `raw` the
/// values of options red-ale leaves out, which `sheet` does not have.
fn record(sheet: &StyleSheet, raw: &Map<String, Value>, other: Map<String, Value>) -> StyleSheetRecord {
    let kept = raw.iter().filter_map(|(section, names)| {
        let names = names.as_object()?;
        let left_out = names.iter().filter(|(n, v)| !v.is_null() && sheet.options().get(section, n).is_none());
        let left_out: Map<String, Value> = left_out.map(|(n, v)| (n.clone(), v.clone())).collect();
        (!left_out.is_empty()).then(|| (section.clone(), Value::Object(left_out)))
    });
    let options = merge(sheet.changed(), &Value::Object(kept.collect()));
    StyleSheetRecord { name: sheet.name().to_owned(), template: sheet.template().name().to_owned(), options, other }
}

/// `base` with `changes` merged in by section and name; `null` removes a value.
fn merge(mut base: Map<String, Value>, changes: &Value) -> Map<String, Value> {
    changes.as_object().into_iter().flatten().for_each(|(section, names)| {
        let entry = base.entry(section.clone()).or_insert_with(|| Value::Object(Map::new()));
        if let (Value::Object(into), Some(names)) = (entry, names.as_object()) {
            names.iter().for_each(|(n, v)| {
                if v.is_null() {
                    into.remove(n);
                } else {
                    into.insert(n.clone(), v.clone());
                }
            });
        }
    });
    base.retain(|_, names| names.as_object().is_none_or(|n| !n.is_empty()));
    base
}

/// Write the style sheet entries to the Configuration, atomically.
fn store(ctx: &Context, mut config: Config, records: &[StyleSheetRecord]) -> Result<(), RpcError> {
    config.set_style_sheets(records);
    Ok(ctx.settings.config.save(&config)?)
}
