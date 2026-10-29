//! What `report.options` publishes about a template's options: each option with its constraints, default and value,
//! and the JSON Schema of the `options` param (`design/reports.md` §6, "The published schema and setting options").

use red_ale_core::time::RelativeDate;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Map, Value, json};

use super::db::OptionDb;
use super::model::{
    Choice, ChoiceKey, DateUi, OptionKind, OptionValue, PLOT_SIZE_MAX, PLOT_SIZE_MIN, PLOT_SIZE_PERCENT_MAX, ReportOption, SPAN_MAX,
    relative_name,
};
use super::value::write;
use crate::number::{Rational, Rounding};

/// One option as `report.options` lists it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptionInfo {
    /// The section.
    pub section: String,
    /// The name.
    pub name: String,
    /// The sort tag.
    pub key: String,
    /// The type: `boolean`, `string`, `text`, `font`, `color`, `multichoice`, `list`, `numberRange`, `plotSize`,
    /// `date`, `accountList`, `accountSel`, `currency`, `commodity`, `budget`, `internal` or `reportPlacement`.
    #[serde(rename = "type")]
    pub option_type: String,
    /// Help text.
    pub help: String,
    /// The default, in wire form.
    pub default: Value,
    /// The configured value, in wire form.
    pub value: Value,
    /// For multichoice and list options: the choices.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<Choice>>,
    /// For number ranges: the smallest value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<Rational>,
    /// For number ranges: the largest value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Rational>,
    /// For number ranges: the step a UI moves by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<Rational>,
    /// For dates: which values are offered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui: Option<DateUi>,
    /// For dates: the relative dates offered; absent when any is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative: Option<Vec<String>>,
    /// For account options: the account types allowed; absent when any is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_types: Option<Vec<String>>,
    /// For account lists: whether more than one account may be chosen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple: Option<bool>,
}

impl OptionInfo {
    fn of(o: &ReportOption) -> Self {
        let mut info = Self {
            section: o.section().to_owned(),
            name: o.name().to_owned(),
            key: o.classifier.key.clone(),
            option_type: o.kind.type_name().to_owned(),
            help: o.classifier.help.clone(),
            default: write(o.default_value()),
            value: write(o.value()),
            choices: o.kind.choices().map(<[Choice]>::to_vec),
            min: None,
            max: None,
            step: None,
            ui: None,
            relative: None,
            account_types: None,
            multiple: None,
        };
        match &o.kind {
            OptionKind::NumberRange { min, max, step } => {
                (info.min, info.max, info.step) = (Some(min.clone()), Some(max.clone()), Some(step.clone()));
            }
            OptionKind::Date { ui, relative } => {
                info.ui = Some(*ui);
                info.relative = (!relative.is_empty()).then(|| relative.iter().map(|r| relative_name(*r).to_owned()).collect());
            }
            OptionKind::AccountList { types, multiple } => (info.account_types, info.multiple) = (type_names(types), Some(*multiple)),
            OptionKind::AccountSel { types } => info.account_types = type_names(types),
            _ => {}
        }
        info
    }
}

fn type_names(types: &[red_ale_core::AccountType]) -> Option<Vec<String>> {
    (!types.is_empty()).then(|| types.iter().map(|t| t.as_str().to_owned()).collect())
}

impl OptionDb {
    /// Every option in order, with its constraints, default and value; options made internal are left out.
    pub fn describe(&self) -> Vec<OptionInfo> {
        self.published().map(OptionInfo::of).collect()
    }

    fn published(&self) -> impl Iterator<Item = &ReportOption> {
        self.options().into_iter().filter(|o| !o.is_made_internal())
    }

    /// The JSON Schema (2020-12) of the `options` param for these options: an object by section and name, each
    /// value in its option's wire form. A client may check its call against it before making it. `null`, which
    /// returns an option to its default, is left out.
    pub fn params_schema(&self) -> Value {
        let sections = self.published().fold(Map::new(), |mut sections, o| {
            let empty = || json!({ "type": "object", "additionalProperties": false, "properties": {} });
            let entry = sections.entry(o.section().to_owned()).or_insert_with(empty);
            if let Some(props) = entry.get_mut("properties").and_then(Value::as_object_mut) {
                props.insert(o.name().to_owned(), value_schema(o));
            }
            sections
        });
        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "additionalProperties": false,
            "properties": sections
        })
    }
}

/// The schema of one option's value, with its help and default.
fn value_schema(o: &ReportOption) -> Value {
    let mut schema = kind_schema(&o.kind, o.default_value());
    if let Some(fields) = schema.as_object_mut() {
        fields.insert("description".into(), Value::String(o.classifier.help.clone()));
        fields.insert("default".into(), write(o.default_value()));
    }
    schema
}

const DECIMAL_PATTERN: &str = r"^\s*[-+]?(\d+(\.\d*)?|\.\d+)\s*$|^\s*[-+]?\d+\s*/\s*\d+\s*$";

fn kind_schema(kind: &OptionKind, default: &OptionValue) -> Value {
    match kind {
        OptionKind::Boolean => json!({ "type": "boolean" }),
        OptionKind::String
        | OptionKind::Text
        | OptionKind::Font
        | OptionKind::Currency
        | OptionKind::Commodity
        | OptionKind::Budget
        | OptionKind::AccountSel { .. } => json!({ "type": "string" }),
        OptionKind::Color => json!({ "type": "string", "pattern": "^#?([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$" }),
        OptionKind::Internal => json!({ "type": if matches!(default, OptionValue::Bool(_)) { "boolean" } else { "string" } }),
        OptionKind::Multichoice { choices } => json!({ "enum": keys(choices) }),
        OptionKind::List { choices } => json!({ "type": "array", "items": { "enum": keys(choices) }, "uniqueItems": true }),
        OptionKind::NumberRange { min, max, .. } => number_schema(min, max),
        OptionKind::PlotSize => plot_size_schema(),
        OptionKind::Date { ui, relative } => date_schema(*ui, relative),
        OptionKind::AccountList { multiple, .. } => {
            let mut schema = json!({ "type": "array", "items": { "type": "string" }, "uniqueItems": true });
            if !multiple {
                schema["maxItems"] = json!(1);
            }
            schema
        }
        OptionKind::ReportPlacement => placement_schema(),
    }
}

fn placement_schema() -> Value {
    let span = |what: &str| json!({ "type": "integer", "minimum": 0, "maximum": SPAN_MAX, "default": 1, "description": format!("The {what} it spans.") });
    json!({ "type": "array", "items": {
        "type": "object",
        "properties": {
            "report": { "type": "string", "description": "The GUID of a report template or saved report configuration." },
            "options": {
                "type": "object", "additionalProperties": { "type": "object" },
                "description": "Only with a template's GUID: changes over its defaults, by section and name, as report.run takes them."
            },
            "colspan": span("columns"),
            "rowspan": span("rows")
        },
        "required": ["report"],
        "additionalProperties": false
    } })
}

fn keys(choices: &[Choice]) -> Vec<Value> {
    choices
        .iter()
        .map(|c| match &c.key {
            ChoiceKey::Text(s) => Value::String(s.clone()),
            ChoiceKey::Number(n) => Value::from(*n),
        })
        .collect()
}

fn number_schema(min: &Rational, max: &Rational) -> Value {
    let whole = |r: &Rational, mode| r.round_to(1, mode).ok().and_then(|w| w.to_numeric()).map(|n| Value::from(n.num()));
    let mut integer = json!({ "type": "integer" });
    if let Some(lo) = whole(min, Rounding::Ceiling) {
        integer["minimum"] = lo;
    }
    if let Some(hi) = whole(max, Rounding::Floor) {
        integer["maximum"] = hi;
    }
    json!({ "oneOf": [integer, { "type": "string", "pattern": DECIMAL_PATTERN }] })
}

fn plot_size_schema() -> Value {
    let unit = |name: &str, lo: u32, hi: u32| {
        json!({
            "type": "object",
            "properties": { name: { "type": "integer", "minimum": lo, "maximum": hi } },
            "required": [name],
            "additionalProperties": false
        })
    };
    json!({ "oneOf": [
        unit("percent", PLOT_SIZE_MIN, PLOT_SIZE_PERCENT_MAX),
        unit("pixels", PLOT_SIZE_PERCENT_MAX + 1, PLOT_SIZE_MAX),
        { "type": "integer", "minimum": PLOT_SIZE_MIN, "maximum": PLOT_SIZE_MAX }
    ] })
}

fn date_schema(ui: DateUi, relative: &[RelativeDate]) -> Value {
    let names: Vec<&str> = if relative.is_empty() {
        RelativeDate::all().map(relative_name).collect()
    } else {
        relative.iter().map(|r| relative_name(*r)).collect()
    };
    let absolute = json!({
        "type": "object",
        "properties": {
            "absolute": { "type": "string", "description": "A date YYYY-MM-DD (local 10:59:00 that day) or an RFC 3339 timestamp." }
        },
        "required": ["absolute"], "additionalProperties": false
    });
    let rel = json!({
        "type": "object", "properties": { "relative": { "enum": names } }, "required": ["relative"], "additionalProperties": false
    });
    match ui {
        DateUi::Absolute => absolute,
        DateUi::Relative => rel,
        DateUi::Both => json!({ "oneOf": [absolute, rel] }),
    }
}
