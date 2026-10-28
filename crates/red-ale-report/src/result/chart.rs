//! Charts of the report result (`design/reports.md` §7, "Charts" and "Hover labels"), standing for GnuCash's
//! `<html-chart>` (`5.17:gnucash/report/html-chart.scm`) as data.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

use super::table::Link;
use crate::number::Rational;
use crate::wire::CommodityId;

/// A chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Chart {
    /// The kind of chart (`set-type!`).
    #[serde(rename = "type")]
    pub chart_type: ChartType,
    /// Lines of the title (`set-title!`).
    #[serde(default)]
    pub title: Vec<String>,
    /// Width; default 100 % (`5.17:gnucash/report/html-chart.scm:190-193`).
    #[serde(default)]
    pub width: PlotSize,
    /// Height; default 100 %.
    #[serde(default)]
    pub height: PlotSize,
    /// Bars stack, or lines accumulate (`set-stacking?!`).
    #[serde(default)]
    pub stacked: bool,
    /// Grid lines (`set-grid?!`); on by default, as in GnuCash's chart (`:236-239`).
    #[serde(default = "yes")]
    pub grid: bool,
    /// Show the series (or slice) labels.
    #[serde(default = "yes")]
    pub legend: bool,
    /// The x axis.
    #[serde(default)]
    pub x_axis: Axis,
    /// The y axis.
    #[serde(default)]
    pub y_axis: Axis,
    /// Category labels (bar, line) or slice labels (pie).
    #[serde(default)]
    pub labels: Vec<String>,
    /// The data series (`add-data-series!`).
    pub series: Vec<Series>,
    /// Hover label behaviour.
    #[serde(default)]
    pub tooltip: Tooltip,
}

fn yes() -> bool {
    true
}

/// The kind of a chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Scatter,
}

/// A plot size: GnuCash's `(percent . n)` or `(pixels . n)` (`5.17:bindings/guile/gnc-optiondb.i:1094-1108`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum PlotSize {
    /// A percentage of the space available.
    Percent(u32),
    /// Pixels.
    Pixels(u32),
}

impl Default for PlotSize {
    fn default() -> Self {
        Self::Percent(100)
    }
}

impl PlotSize {
    /// The number, whatever the unit.
    pub fn value(self) -> u32 {
        match self {
            Self::Percent(n) | Self::Pixels(n) => n,
        }
    }
}

/// An axis.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Axis {
    /// The axis label; empty for none.
    #[serde(default)]
    pub label: String,
    /// How positions are laid out; `category` for bar and line, `linear` for scatter, by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<Scale>,
    /// How values show; `number` by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<AxisFormat>,
    /// The currency of a `currency` axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commodity: Option<CommodityId>,
}

/// How axis positions are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Scale {
    /// One slot per label.
    Category,
    /// Numeric x, for scatter.
    Linear,
}

/// How values on an axis show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AxisFormat {
    /// Amounts in the axis's `commodity`.
    Currency,
    /// Percentages: two decimals and `%`.
    Percent,
    /// Plain numbers, exact.
    #[default]
    Number,
}

/// How hover labels work (`set-tooltip-mode!`, `set-tooltip-non-zero-only!`, `5.17:gnucash/report/html-chart.scm:309-328`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tooltip {
    /// One label for all series at a category; GnuCash's default `index` mode (`:220-221`).
    #[serde(default = "yes")]
    pub grouped: bool,
    /// Leave zero values out of the label.
    #[serde(default)]
    pub non_zero_only: bool,
}

impl Default for Tooltip {
    fn default() -> Self {
        Self {
            grouped: true,
            non_zero_only: false,
        }
    }
}

/// A data series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Series {
    /// The series label.
    pub label: String,
    /// Its colour.
    pub color: Color,
    /// One value per label, `null` for none; every chart but `scatter`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<Option<Rational>>>,
    /// The points of a `scatter` series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<Point>>,
    /// One colour per value: a pie's slices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<Vec<Color>>,
    /// Fill the area under a line, or fill a bar.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fill: bool,
    /// Line width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_width: Option<Rational>,
    /// Series with the same stack name stack together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    /// Draw this series as bars or a line, to mix them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<SeriesKind>,
    /// A link for the whole series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
    /// One link per value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<Option<Link>>>,
    /// For `scatter`: join the points with a line of this colour (`5.17:gnucash/report/reports/standard/price-scatter.scm:269-274`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<Color>,
    /// The point marker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<Marker>,
    /// The points' radius, GnuCash's `pointRadius` (`5.17:gnucash/report/reports/standard/balance-forecast.scm:153-154`):
    /// 0 hides a line's markers, which keep their hover labels; any other radius draws them as usual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point_radius: Option<Rational>,
    /// Host: each value as its hover label shows it, under the y axis's format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub displays: Option<Vec<Option<String>>>,
}

/// How a series is drawn when a chart mixes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum SeriesKind {
    Bar,
    Line,
}

/// A point marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum Marker {
    Circle,
    Square,
    Diamond,
    Cross,
    Plus,
}

/// A point of a scatter series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Point {
    /// x.
    pub x: Rational,
    /// y.
    pub y: Rational,
    /// The hover label's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Host: y as its hover label shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

/// A colour, `#rrggbb` in lowercase.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Color(String);

impl Color {
    /// A colour from `#rrggbb`, either case.
    pub fn parse(s: &str) -> Result<Self, String> {
        let hex = s
            .strip_prefix('#')
            .filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()));
        hex.map(|h| Self(format!("#{}", h.to_ascii_lowercase())))
            .ok_or_else(|| format!("not a colour #rrggbb: {s:?}"))
    }

    /// The text, `#rrggbb`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = Cow::<str>::deserialize(d)?;
        Self::parse(&text).map_err(de::Error::custom)
    }
}

impl JsonSchema for Color {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "Color".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "pattern": "^#[0-9a-fA-F]{6}$", "description": "A colour, #rrggbb." })
    }
}

impl Chart {
    /// The hover label of value `index`, following GnuCash's tooltip (`5.17:gnucash/report/html-chart.scm:433-449`):
    /// the label, then a `series : value` line for each series (only `series`, when given and the tooltip is not
    /// grouped), leaving out zero values when `nonZeroOnly` is set. Uses the display forms the host filled in.
    pub fn hover_label(&self, index: usize, series: Option<usize>) -> String {
        let title = self.point_title(index, series);
        let chosen: Vec<&Series> = match series {
            Some(s) if !self.tooltip.grouped => self.series.get(s).into_iter().collect(),
            _ => self.series.iter().collect(),
        };
        let lines = chosen.into_iter().filter_map(|s| self.hover_line(s, index));
        std::iter::once(title).chain(lines).collect::<Vec<_>>().join("\n")
    }

    fn point_title(&self, index: usize, series: Option<usize>) -> String {
        let point = series
            .and_then(|s| self.series.get(s))
            .and_then(|s| s.points.as_ref())
            .and_then(|p| p.get(index));
        match point {
            Some(p) => p.label.clone().unwrap_or_else(|| p.x.to_string()),
            None => self.labels.get(index).cloned().unwrap_or_default(),
        }
    }

    fn hover_line(&self, s: &Series, index: usize) -> Option<String> {
        let (value, display) = if let Some(points) = &s.points {
            points.get(index).map(|p| (Some(&p.y), p.display.clone()))?
        } else {
            let value = s.values.as_ref().and_then(|v| v.get(index)).and_then(Option::as_ref);
            (value, s.displays.as_ref().and_then(|d| d.get(index)).cloned().flatten())
        };
        let value = value?;
        if self.tooltip.non_zero_only && value.is_zero() {
            return None;
        }
        Some(format!("{} : {}", s.label, display.unwrap_or_else(|| value.to_string())))
    }
}
