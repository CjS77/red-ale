//! The report result: the published language for reports (`design/reports.md` §7).
//!
//! A template's `render()` returns a [`ReportDraft`]: the title, an optional headline and the sections, without the
//! fields the host fills in. The host checks the draft ([`ReportDraft::from_json`], [`ReportDraft::validate`]) and
//! turns it into a [`ReportResult`] with [`ReportDraft::finish`], which adds the run's identity, time, options and log
//! and the display form of every figure. A mismatch is a `report-result` error naming the path.
//!
//! The result carries data, not markup: exact figures with their display form, and roles, not styles. It is part of
//! `redAleApi` and versioned with it ([`RESULT_VERSION`]); within a major version it changes only additively.

mod chart;
mod finish;
mod grid;
mod table;
mod validate;

use schemars::{JsonSchema, Schema, schema_for};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use chart::{Axis, AxisFormat, Chart, ChartType, Color, Marker, PlotSize, Point, Scale, Series, SeriesKind, Tooltip};
pub use finish::{DisplayContext, RunInfo};
pub use grid::{EmbeddedResult, GridBlock, GridCell, GridRow};
pub use table::{
    Align, Anchor, Cell, CellContent, Column, ColumnRole, CommodityLine, Link, NumberFormat, NumberRounding, ReportLink, Row, RowRole, Table,
};

use crate::error::ResultError;
use crate::number::Rational;
use crate::wire::{Id, Timestamp};

/// The version of the report result: the `redAleApi` version it was published with (`design/api.md`).
pub const RESULT_VERSION: &str = "0.2.0";

/// Option values by section and name, in wire form (`design/reports.md` §6): `{ "General": { "Start Date": … } }`.
/// Keys keep the order they were inserted in.
pub type OptionValues = Map<String, Value>;

/// The report a result came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportIdentity {
    /// The template's GUID.
    pub guid: Id,
    /// The template's name.
    pub name: String,
    /// The saved report configuration's GUID, when one was run.
    pub configuration: Option<Id>,
}

/// A finished report result, as `report.run` returns it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportResult {
    /// The result's version: [`RESULT_VERSION`].
    pub version: String,
    /// The template and saved configuration that produced it.
    pub report: ReportIdentity,
    /// The document title (`gnc:html-document-set-title!`): the HTML `<title>` and, without a headline, the heading.
    pub title: String,
    /// A heading when it differs from the title (`gnc:html-document-set-headline!`).
    pub headline: Option<String>,
    /// The time the report was run for.
    pub now: Timestamp,
    /// Every option's value, in wire form, so the result says exactly what produced it.
    pub options: OptionValues,
    /// The sections, in order.
    pub sections: Vec<Section>,
    /// The report log: what the script printed.
    pub log: Vec<LogEntry>,
    /// The rows of the report's export, when the run was for an export the template offers (`report.exportCsv`):
    /// GnuCash's export string before it is joined (`gnc:html-document-set-export-string`). Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export: Option<ExportRows>,
    /// Why the run for an export has nothing to export, as the script gave it (`gnc:html-document-set-export-error`,
    /// `5.17:gnucash/report/trep-engine.scm:2524`): "No splits found". Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_error: Option<String>,
}

/// The rows of an export: each a list of cells, as GnuCash's `gnc:lists->csv` takes them
/// (`5.17:gnucash/report/trep-engine.scm:463-512`).
pub type ExportRows = Vec<Vec<Option<ExportCell>>>;

/// One cell of an export row; `null` for an empty one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ExportCell {
    /// Text, written quoted.
    Text(String),
    /// An exact number, written as Guile prints it once inexact.
    Number(Rational),
}

/// What a template's `render()` returns: a report result without the fields the host fills in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportDraft {
    /// The document title.
    pub title: String,
    /// A heading when it differs from the title.
    #[serde(default)]
    pub headline: Option<String>,
    /// The sections, in order.
    pub sections: Vec<Section>,
    /// The export's rows, when the run context asks for an export (`ctx.export_type()`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export: Option<ExportRows>,
    /// Why there is nothing to export, when the run context asks for an export; not with `export`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_error: Option<String>,
}

/// A section: an optional title and ordered blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Section {
    /// The section's title.
    #[serde(default)]
    pub title: Option<String>,
    /// The blocks, in order.
    pub blocks: Vec<Block>,
}

/// A block of a section, told apart by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Block {
    /// Text: `gnc:make-html-text`, `gnc:html-markup-h3`, `gnc:html-markup-p`.
    Text(TextBlock),
    /// A warning (`gnc:html-make-generic-warning` and those built on it, `5.17:gnucash/report/html-utilities.scm:350-381`).
    Warning(WarningBlock),
    /// A table.
    Table(Table),
    /// A chart.
    Chart(Box<Chart>),
    /// Blocks laid out side by side: the one-row layout tables of the two-column Balance Sheet and Income Statement.
    Group(GroupBlock),
    /// A bulleted list (`gnc:html-markup-ul`, `5.17:gnucash/report/html-text.scm:210-215`).
    List(ListBlock),
    /// A Multicolumn View's children in rows of cells (`5.17:gnucash/report/reports/standard/view-column.scm:51-159`).
    Grid(GridBlock),
    /// Set by the host only: the options that differ from their defaults (`gnc:html-render-options-changed`).
    OptionsSummary(OptionsSummary),
}

/// A text block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBlock {
    /// The text.
    pub text: String,
    /// What the text is.
    #[serde(default)]
    pub role: TextRole,
    /// Something the text leads to, such as the Multicolumn statements' "Barchart" report
    /// (`5.17:gnucash/report/reports/standard/balsheet-pnl.scm:1079-1083`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
}

/// The role of a text block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TextRole {
    /// A paragraph.
    #[default]
    Paragraph,
    /// A heading.
    Heading,
    /// A note.
    Note,
}

/// A warning block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WarningBlock {
    /// The warning's title, such as "No accounts selected".
    pub title: String,
    /// What it says.
    pub text: String,
}

/// Blocks side by side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupBlock {
    /// The blocks, left to right.
    pub blocks: Vec<Block>,
}

/// A bulleted list: the Cash Flow's selected accounts (`5.17:gnucash/report/reports/standard/cash-flow.scm:253-262`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListBlock {
    /// The items, in order.
    pub items: Vec<ListItem>,
}

/// One item of a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListItem {
    /// The text.
    pub text: String,
    /// The account the item names, which links to its register (`gnc:html-account-anchor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<Id>,
}

/// The options summary the host appends when "Add options summary" is `always`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptionsSummary {
    /// One row per changed option, in option order.
    pub rows: Vec<OptionsSummaryRow>,
}

/// One changed option in the options summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptionsSummaryRow {
    /// The option's section; empty for a view's `Embedded Report` rows (v0.5), which name no option.
    pub section: String,
    /// The option's name, or `Embedded Report`.
    pub name: String,
    /// Its value as text; for an `Embedded Report` row, the child's "Report name".
    pub value: String,
}

/// An entry of the report log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogEntry {
    /// Which call wrote it.
    pub level: LogLevel,
    /// The text.
    pub text: String,
    /// The script it came from, when known.
    pub source: Option<String>,
    /// The line, from 1, when known.
    pub line: Option<u32>,
}

/// The call that wrote a log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    /// Rhai's `print`.
    Print,
    /// Rhai's `debug`.
    Debug,
}

/// Deserialize `value`, naming the path of the first mismatch.
fn from_json<T: DeserializeOwned>(value: &Value) -> Result<T, ResultError> {
    from_json_at(value, "")
}

/// Deserialize `value`, found at `prefix`. Serde reads a block whole before looking at its `kind`, so a mismatch
/// inside a block is reported at the block; it is then located by reading the block again as its own kind.
fn from_json_at<T: DeserializeOwned>(value: &Value, prefix: &str) -> Result<T, ResultError> {
    serde_path_to_error::deserialize(value).map_err(|e| {
        let segments: Vec<String> = e.path().iter().map(ToString::to_string).collect();
        let path = join_path(prefix, &segments);
        let at = segments.iter().try_fold(value, |v, s| step(v, s));
        at.and_then(|block| locate_in_block(block, &path))
            .unwrap_or_else(|| ResultError::new(path, e.into_inner().to_string()))
    })
}

fn join_path(prefix: &str, segments: &[String]) -> String {
    segments.iter().fold(prefix.to_owned(), |path, s| match s.strip_prefix('[') {
        Some(_) => format!("{path}{s}"),
        None if path.is_empty() => s.clone(),
        None => format!("{path}.{s}"),
    })
}

/// The value one path segment (`name` or `[3]`) below `v`.
fn step<'a>(v: &'a Value, segment: &str) -> Option<&'a Value> {
    match segment.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        Some(index) => v.get(index.parse::<usize>().ok()?),
        None => v.get(segment),
    }
}

/// The mismatch inside a block, read as the kind it names.
fn locate_in_block(block: &Value, path: &str) -> Option<ResultError> {
    let mut fields = block.as_object()?.clone();
    let kind = fields.remove("kind")?;
    let inner = &Value::Object(fields);
    match kind.as_str()? {
        "text" => from_json_at::<TextBlock>(inner, path).err(),
        "warning" => from_json_at::<WarningBlock>(inner, path).err(),
        "table" => from_json_at::<Table>(inner, path).err(),
        "chart" => from_json_at::<Chart>(inner, path).err(),
        "group" => from_json_at::<GroupBlock>(inner, path).err(),
        "list" => from_json_at::<ListBlock>(inner, path).err(),
        "grid" => from_json_at::<GridBlock>(inner, path).err(),
        "optionsSummary" => from_json_at::<OptionsSummary>(inner, path).err(),
        _ => None,
    }
}

impl ReportDraft {
    /// Read a draft from its JSON form, naming the path of the first mismatch with the schema.
    pub fn from_json(value: &Value) -> Result<Self, ResultError> {
        from_json(value)
    }

    /// Check what the schema cannot (`design/reports.md` §7): one content per cell, rows as wide as their table,
    /// staircase levels within the columns' levels, chart series that fit their labels, and no field only the host
    /// may set.
    pub fn validate(&self) -> Result<(), ResultError> {
        if self.export.is_some() && self.export_error.is_some() {
            return Err(ResultError::new("exportError", "a draft has export rows or an export error, not both"));
        }
        validate::sections(&self.sections, validate::Stage::Draft)
    }
}

impl ReportResult {
    /// Read a result from its JSON form, naming the path of the first mismatch.
    pub fn from_json(value: &Value) -> Result<Self, ResultError> {
        from_json(value)
    }

    /// Check a finished result: everything [`ReportDraft::validate`] checks, with every display field present.
    pub fn validate(&self) -> Result<(), ResultError> {
        if self.version != RESULT_VERSION {
            return Err(ResultError::new(
                "version",
                format!("expected {RESULT_VERSION}, got {}", self.version),
            ));
        }
        validate::sections(&self.sections, validate::Stage::Finished)
    }

    /// The JSON Schema of the report result, listed in `api.schema` as `ReportResult`.
    pub fn json_schema() -> Schema {
        schema_for!(ReportResult)
    }
}

#[cfg(test)]
mod tests;
