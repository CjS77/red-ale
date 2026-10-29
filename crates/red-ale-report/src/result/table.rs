//! Tables of the report result: columns, rows with their roles, and cells (`design/reports.md` §7, "Tables").

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::OptionValues;
use crate::number::{Monetary, Rational};
use crate::wire::{CommodityId, Id, Timestamp};

/// A table: GnuCash's `<html-table>`, as data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Table {
    /// The table's title.
    #[serde(default)]
    pub title: Option<String>,
    /// The columns.
    pub columns: Vec<Column>,
    /// The rows; each has one cell per column, counting spans.
    pub rows: Vec<Row>,
    /// Notes that cells mark with a `footnote` number, numbered from 1 and listed after the table: GnuCash's footnote
    /// collector (`5.17:gnucash/report/reports/standard/budget.scm:240-259`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footnotes: Vec<String>,
}

/// A column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Column {
    /// The heading; empty for none.
    #[serde(default)]
    pub label: String,
    /// What the column holds.
    pub role: ColumnRole,
    /// Alignment; amounts default to the right.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    /// For an `amount` column of an account table: the staircase positions, a deeper account's amount further left
    /// and totals furthest right (`5.17:gnucash/report/html-acct-table.scm:870-923`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub levels: Option<u32>,
    /// The heading as stacked lines, top first, when it takes several header rows: GnuCash's multi-row column headers
    /// (`gnc:html-table-set-multirow-col-headers!`, `5.17:gnucash/report/reports/standard/txn-columns.scm:170-174`).
    /// `label` still names the column for a client that reads one line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_lines: Option<Vec<String>>,
}

/// What a column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ColumnRole {
    /// Account names and row labels.
    Label,
    /// Other text.
    Text,
    /// Figures.
    Amount,
    /// Dates.
    Date,
}

/// Horizontal alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum Align {
    Left,
    Right,
    Center,
}

/// A row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Row {
    /// What the row is.
    #[serde(default)]
    pub role: RowRole,
    /// The account the row is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<Id>,
    /// Indentation of the label, in levels (`indented-depth`, `5.17:gnucash/report/html-acct-table.scm:749-763`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    /// The amount's staircase position, 0 being the rightmost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// For a `data` or `account` row: whether it is shaded as GnuCash's `alternate-row` (`true`) or `normal-row`
    /// (`false`). Absent, rows alternate by position. The Transaction Report shades by split and keeps a
    /// transaction's lines alike (`5.17:gnucash/report/trep-engine.scm:1967-1977`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alternate: Option<bool>,
    /// The cells, `null` for an empty one.
    pub cells: Vec<Option<Cell>>,
}

/// What a row is. The roles stand for GnuCash's row and cell markup (`primary-subheading`, `total-label-cell`,
/// `total-number-cell`), which style sheets colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RowRole {
    /// Plain data.
    #[default]
    Data,
    /// A section label such as "Assets".
    Heading,
    /// An account's row.
    Account,
    /// A subtotal: "Total Bank1".
    Subtotal,
    /// A section total: "Total Assets".
    Total,
    /// A grand total: "Total Liabilities & Equity", "Net income".
    GrandTotal,
    /// A horizontal rule across the table (`gnc:html-table-append-ruler!`).
    Rule,
    /// An empty spacer row.
    Spacer,
    /// A warning inside a table, shown bold and in the negative colour: Investment Lots' warning rows
    /// (`total-label-cell neg`, `5.17:gnucash/report/reports/standard/investment-lots.scm:1077-1096`).
    Warning,
}

/// A cell. It holds exactly one of `text`, `monetary`, `commodities`, `number`, `price`, `date` and `anchors`; the
/// display fields are filled in by the host.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(extend("oneOf" = [
    { "required": ["text"] }, { "required": ["monetary"] }, { "required": ["commodities"] },
    { "required": ["number"] }, { "required": ["price"] }, { "required": ["date"] }, { "required": ["anchors"] }
]))]
pub struct Cell {
    /// Text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// An amount of a commodity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monetary: Option<Monetary>,
    /// GnuCash's nested commodity table: one line per commodity an account holds, with its value in the report
    /// currency (`5.17:gnucash/report/html-acct-table.scm:925-948`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commodities: Option<Vec<CommodityLine>>,
    /// A plain number, such as a percentage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<Rational>,
    /// A price or exchange rate in `currency`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<Rational>,
    /// An instant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<Timestamp>,
    /// Pieces of text, each linked, shown one per line: the Transaction Report's invoice cell
    /// (`5.17:gnucash/report/trep-engine.scm:1036-1049`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchors: Option<Vec<Anchor>>,
    /// For a `date`: the end of the period it starts, shown `<date> to <until>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<Timestamp>,
    /// For a `number`: decimals shown, default 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub places: Option<u32>,
    /// For a `number`: text after the figure, such as `" %"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    /// For a `number`: how it is rounded to `places`; banker's when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounding: Option<NumberRounding>,
    /// For a `number`: how it is printed; GnuCash's number renderer when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_format: Option<NumberFormat>,
    /// For a `price`: the currency it is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<CommodityId>,
    /// Host: for a `monetary`, the amount at the commodity's SCU, banker's rounding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounded: Option<Rational>,
    /// Host: for a `date`, its local date `YYYY-MM-DD` in the run's time zone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_date: Option<String>,
    /// Host: for a `date` with `until`, the local date of `until`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_until: Option<String>,
    /// Host: the figure as text, for `monetary`, `number`, `price` and `date` cells; `<date> to <until>` for a period.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// The account the cell names or links to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<Id>,
    /// Something a client can open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
    /// An accounting rule above the figure ("Show accounting-style rules",
    /// `5.17:gnucash/report/html-acct-table.scm:901-906`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rule: bool,
    /// Columns the cell covers, default 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<u32>,
    /// The number of the table's footnote this cell refers to, shown as a superscript after the content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footnote: Option<u32>,
}

/// How a `number` cell is rounded to its `places`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NumberRounding {
    /// Ties to even, as GnuCash's number renderer rounds (`5.17:gnucash/report/html-style-info.scm:233-236`).
    Bankers,
    /// Ties away from zero, as Guile's `~,nf` rounds the rates GnuCash prints from doubles
    /// (`5.17:gnucash/report/reports/standard/advanced-portfolio.scm:914-922`).
    HalfUp,
}

/// How a `number` cell is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NumberFormat {
    /// GnuCash's share print info (`gnc_share_print_info_places`, `5.17:libgnucash/app-utils/gnc-ui-util.cpp:1013-1023`):
    /// exactly `places` decimals, rounded half away from zero (`:1105-1145`), grouped with the locale's numeric
    /// separators, the decimal point always printed: the portfolios' share counts, `100.` with no places.
    Share,
}

/// The content of a cell, borrowed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CellContent<'a> {
    /// Text.
    Text(&'a str),
    /// An amount of a commodity.
    Monetary(&'a Monetary),
    /// Lines of commodities.
    Commodities(&'a [CommodityLine]),
    /// A number.
    Number(&'a Rational),
    /// A price.
    Price(&'a Rational),
    /// An instant.
    Date(Timestamp),
    /// Linked pieces of text.
    Anchors(&'a [Anchor]),
}

impl Cell {
    /// A text cell.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }

    /// A monetary cell.
    pub fn monetary(m: Monetary) -> Self {
        Self {
            monetary: Some(m),
            ..Self::default()
        }
    }

    /// Every content the cell holds; a valid cell holds exactly one.
    pub fn contents(&self) -> Vec<CellContent<'_>> {
        [
            self.text.as_deref().map(CellContent::Text),
            self.monetary.as_ref().map(CellContent::Monetary),
            self.commodities.as_deref().map(CellContent::Commodities),
            self.number.as_ref().map(CellContent::Number),
            self.price.as_ref().map(CellContent::Price),
            self.date.map(CellContent::Date),
            self.anchors.as_deref().map(CellContent::Anchors),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// The cell's content, when it holds exactly one.
    pub fn content(&self) -> Option<CellContent<'_>> {
        match self.contents().as_slice() {
            [one] => Some(*one),
            _ => None,
        }
    }

    /// Columns the cell covers.
    pub fn columns(&self) -> u32 {
        self.span.unwrap_or(1)
    }
}

/// One commodity of a `commodities` cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommodityLine {
    /// The native amount.
    pub monetary: Monetary,
    /// Its value in the report currency, when converted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub converted: Option<Monetary>,
    /// Host: the native amount as text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// Host: the converted value as text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub converted_display: Option<String>,
}

/// One linked piece of text in an `anchors` cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Anchor {
    /// The text shown.
    pub text: String,
    /// Where it leads.
    pub link: Link,
}

/// Something a client can open, as GnuCash's anchors (`5.17:gnucash/report/html-utilities.scm:82-128`): an account,
/// split or transaction in a register, an invoice, a price, a linked document, or another report, GnuCash's
/// drill-down (`5.17:gnucash/report/reports/standard/category-barchart.scm:589-609`). GnuCash makes a report instance
/// for a report link at render time; red-ale only describes it, and a client runs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum Link {
    /// `{ "account": "<guid>" }`: the account's register (`gnc:account-anchor-text`, `:85-88`).
    Account(Id),
    /// `{ "report": { "template": "<guid>", "options": { … } } }`.
    Report(ReportLink),
    /// `{ "split": "<guid>" }`: the split in its account's register (`gnc:split-anchor-text`, `:90-91`).
    Split(Id),
    /// `{ "transaction": "<guid>" }`: the transaction in a register (`gnc:transaction-anchor-text`, `:93-94`).
    Transaction(Id),
    /// `{ "invoice": "<guid>" }`: an invoice, bill or voucher (`gnc:invoice-anchor-text`, `:127-128`).
    Invoice(Id),
    /// `{ "price": "<guid>" }`: a price in the price database (`gnc:price-anchor-text`, `:107-110`).
    Price(Id),
    /// `{ "docLink": "<uri>" }`: a transaction's or invoice's linked document (`gnc:transaction-doclink-anchor-text`,
    /// `:96-100`). Described only: a page shows its text unlinked, so an exported page points at no file.
    DocLink(String),
}

impl Link {
    /// Whether a page marks the link for a client to wire up; a linked document is described in the result only.
    pub fn is_live(&self) -> bool {
        !matches!(self, Self::DocLink(_))
    }
}

/// A link to a report run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportLink {
    /// The template to run.
    pub template: Id,
    /// A saved configuration of the template to run (v0.5); `options` are then changes over its values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<Id>,
    /// The options to change, in the form `report.run` takes.
    #[serde(default)]
    pub options: OptionValues,
}
