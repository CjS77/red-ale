//! The `grid` block: a Multicolumn View's children laid out in rows (`design/notes/report-statements-charts.md` §8.6,
//! `5.17:gnucash/report/reports/standard/view-column.scm:51-159`). The script lays out the cells and names each
//! child by its position in the view's report-list; the host runs the children and fills in their results.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Link, LogEntry, OptionValues, ReportIdentity, ReportResult, Section, WarningBlock};
use crate::number::Rational;

/// Children of a view in rows of cells.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridBlock {
    /// The view's "Number of columns", GnuCash's `table-width`: the width at which a row is closed.
    pub width: Rational,
    /// The rows, top to bottom. A cell spanning rows takes room in the rows below without being listed in them.
    pub rows: Vec<GridRow>,
}

/// One row of a grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridRow {
    /// The cells, left to right.
    pub cells: Vec<GridCell>,
}

/// One child in its cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridCell {
    /// The rows the cell spans, 0 to 100, GnuCash's `high`.
    pub rowspan: u32,
    /// The columns the cell spans, 0 to 100, GnuCash's `wide`; a page shows 0 as 1.
    pub colspan: u32,
    /// The child's position in the view's report-list, from 0.
    pub child: u32,
    /// *Host*: a report link that runs the child alone, GnuCash's "Single Report"; absent when the child names no
    /// report that is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
    /// *Host*: the child's result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Box<EmbeddedResult>>,
    /// *Host*: why the child did not run, in place of `result`: GnuCash's "Report error".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<WarningBlock>,
}

/// A child's report result: what `report.run` returns, without the view's `version` and `now`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddedResult {
    /// The template and saved configuration run.
    pub report: ReportIdentity,
    /// The child's title, which a page shows as the cell's heading.
    pub title: String,
    /// A heading when it differs from the title.
    pub headline: Option<String>,
    /// Every option's value, in wire form.
    pub options: OptionValues,
    /// The sections, in order.
    pub sections: Vec<Section>,
    /// The child's report log.
    pub log: Vec<LogEntry>,
}

impl From<ReportResult> for EmbeddedResult {
    fn from(r: ReportResult) -> Self {
        Self {
            report: r.report,
            title: r.title,
            headline: r.headline,
            options: r.options,
            sections: r.sections,
            log: r.log,
        }
    }
}
