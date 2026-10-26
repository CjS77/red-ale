//! Charts as inline SVG (`design/reports.md` §8, "Charts as SVG"): bar, line, pie and scatter charts drawn from a
//! chart block with integer and exact rational arithmetic only, hover labels as `<title>` elements, and no script.

mod cartesian;
mod cordic;
mod geom;
mod pie;

use geom::{Hu, UNIT, fit, fmt, text_width};
use red_ale_locale::Conventions;

use super::text::escape;
use crate::result::{Chart, ChartType, PlotSize};

/// The viewBox of a percent-sized chart (`design/reports.md` §8, "Size").
const PERCENT_WIDTH: i64 = 1000;
const PERCENT_HEIGHT: i64 = 600;
/// The smallest viewBox laid out; a smaller pixel size scales this down.
const MIN_WIDTH: i64 = 320;
const MIN_HEIGHT: i64 = 200;

const PAD: Hu = 16 * UNIT;
const TITLE_FONT: Hu = 16 * UNIT;
const LEGEND_FONT: Hu = 13 * UNIT;
const LEGEND_ROW: Hu = 20 * UNIT;
const SWATCH: Hu = 12 * UNIT;
/// The most title lines laid out.
const MAX_TITLE_LINES: usize = 3;

/// GnuCash's chart area background (`5.17:gnucash/report/html-chart.scm:203-204`).
const PLOT_BACKGROUND: &str = "#fffdf6";

/// A rectangle in hundredths of a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rect {
    pub x0: Hu,
    pub y0: Hu,
    pub x1: Hu,
    pub y1: Hu,
}

impl Rect {
    pub fn width(self) -> Hu {
        self.x1 - self.x0
    }

    pub fn height(self) -> Hu {
        self.y1 - self.y0
    }
}

/// An entry of the legend: a label and its colour.
pub(super) struct Entry<'a> {
    pub label: &'a str,
    pub color: &'a str,
}

/// The chart's box: its viewBox, the area left after the title and legend, and the legend's own area.
pub(super) struct Frame {
    pub width: Hu,
    /// What the plot may use, before axis labels.
    pub area: Rect,
    legend: Option<Rect>,
    /// The Locale's number format, for ticks.
    pub numbers: Conventions,
}

/// The `<svg>` element for `chart`, identified by `id`, its ticks in the number format `numbers`.
pub(crate) fn chart(chart: &Chart, id: &str, numbers: Conventions) -> String {
    let (width, height) = (view_size(chart.width, PERCENT_WIDTH, MIN_WIDTH), view_size(chart.height, PERCENT_HEIGHT, MIN_HEIGHT));
    let entries = legend_entries(chart);
    let shown: &[Entry<'_>] = if chart.legend { &entries } else { &[] };
    let frame = Frame::new(width * UNIT, height * UNIT, title_lines(chart).len(), shown, numbers);
    let body = match chart.chart_type {
        ChartType::Pie => pie::draw(chart, &frame),
        ChartType::Bar | ChartType::Line | ChartType::Scatter => cartesian::draw(chart, &frame),
    };
    let title_text = chart.title.join(" ");
    let size = size_attributes(chart.width, chart.height, width, height);
    [
        format!("<svg id=\"{}\" class=\"chart\" viewBox=\"0 0 {width} {height}\" {size} role=\"img\">", escape(id)),
        format!("<title>{}</title>", escape(&title_text)),
        title(chart, &frame),
        body,
        frame.legend.map(|area| legend(&entries, area)).unwrap_or_default(),
        "</svg>".to_owned(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join("\n")
}

/// A pixel size is the viewBox (no smaller than the layout's minimum); a percent size uses the default viewBox,
/// a percent height shortening it.
fn view_size(size: PlotSize, percent_base: i64, min: i64) -> i64 {
    match size {
        PlotSize::Pixels(n) => i64::from(n).max(min),
        PlotSize::Percent(p) if percent_base == PERCENT_HEIGHT => (percent_base * i64::from(p.clamp(1, 100)) / 100).max(min),
        PlotSize::Percent(_) => percent_base,
    }
}

/// `width` and `height` in pixels, or a percentage of the container's width with the height following the aspect.
fn size_attributes(width: PlotSize, height: PlotSize, view_w: i64, view_h: i64) -> String {
    match (width, height) {
        (PlotSize::Pixels(w), PlotSize::Pixels(h)) => format!("width=\"{w}\" height=\"{h}\""),
        (PlotSize::Pixels(w), PlotSize::Percent(_)) => format!("width=\"{w}\" height=\"{}\"", i64::from(w) * view_h / view_w.max(1)),
        (PlotSize::Percent(p), _) => format!("style=\"width:{}%;height:auto\"", p.clamp(1, 100)),
    }
}

fn title_lines(chart: &Chart) -> &[String] {
    &chart.title[..chart.title.len().min(MAX_TITLE_LINES)]
}

impl Frame {
    fn new(width: Hu, height: Hu, title_lines: usize, entries: &[Entry<'_>], numbers: Conventions) -> Self {
        let lines = i64::try_from(title_lines).unwrap_or(0);
        let top = PAD + lines * (TITLE_FONT + 6 * UNIT);
        let widest = entries.iter().map(|e| text_width(e.label, LEGEND_FONT)).max();
        let legend_width = widest.map(|w| (w + SWATCH + 8 * UNIT).min(width * 3 / 10));
        let full = Rect { x0: PAD, y0: top, x1: width - PAD, y1: height - PAD };
        match legend_width {
            Some(lw) if full.width() - lw - PAD >= width / 3 => Self {
                width,
                area: Rect { x1: full.x1 - lw - PAD, ..full },
                legend: Some(Rect { x0: full.x1 - lw, ..full }),
                numbers,
            },
            _ => Self { width, area: full, legend: None, numbers },
        }
    }
}

fn title(chart: &Chart, frame: &Frame) -> String {
    let mid = frame.width / 2;
    title_lines(chart)
        .iter()
        .zip(1_i64..)
        .map(|(line, n)| {
            let y = PAD + n * (TITLE_FONT + 6 * UNIT) - 6 * UNIT;
            let shown = fit(line, frame.width - 2 * PAD, TITLE_FONT);
            let weight = if n == 1 { " font-weight=\"bold\"" } else { "" };
            format!("<text x=\"{}\" y=\"{}\" font-size=\"16\"{weight} text-anchor=\"middle\">{}</text>", fmt(mid), fmt(y), escape(&shown))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Series labels, or a pie's slice labels, with their colours.
fn legend_entries(chart: &Chart) -> Vec<Entry<'_>> {
    match (chart.chart_type, chart.series.first()) {
        (ChartType::Pie, Some(s)) => {
            let colors = s.colors.as_deref().unwrap_or_default();
            chart.labels.iter().enumerate().map(|(i, l)| Entry { label: l, color: colors.get(i).unwrap_or(&s.color).as_str() }).collect()
        }
        _ => chart.series.iter().map(|s| Entry { label: &s.label, color: s.color.as_str() }).collect(),
    }
}

/// The legend on the right, as GnuCash's chart has it (`5.17:gnucash/report/html-chart.scm:205-206`): one row per
/// entry, and a last row counting those that do not fit.
fn legend(entries: &[Entry<'_>], area: Rect) -> String {
    let rows = usize::try_from(area.height() / LEGEND_ROW).unwrap_or(0).max(1);
    let shown = if entries.len() > rows { rows - 1 } else { entries.len() };
    let row_y = |i: usize| area.y0 + i64::try_from(i).unwrap_or(0) * LEGEND_ROW;
    let room = area.width() - SWATCH - 6 * UNIT;
    let rows_svg = entries.iter().take(shown).enumerate().map(|(i, e)| {
        let y = row_y(i);
        format!(
            "<rect x=\"{}\" y=\"{}\" width=\"12\" height=\"12\" fill=\"{}\"/><text x=\"{}\" y=\"{}\" font-size=\"13\">{}</text>",
            fmt(area.x0),
            fmt(y),
            escape(e.color),
            fmt(area.x0 + SWATCH + 6 * UNIT),
            fmt(y + 10 * UNIT),
            escape(&fit(e.label, room, LEGEND_FONT))
        )
    });
    let more = (shown < entries.len()).then(|| {
        let y = row_y(shown) + 10 * UNIT;
        format!("<text x=\"{}\" y=\"{}\" font-size=\"13\">+{} more</text>", fmt(area.x0), fmt(y), entries.len() - shown)
    });
    let body: Vec<String> = rows_svg.chain(more).collect();
    format!("<g class=\"legend\">\n{}\n</g>", body.join("\n"))
}

/// A hover label as an SVG `<title>`.
pub(super) fn hover(chart: &Chart, index: usize, series: usize) -> String {
    format!("<title>{}</title>", escape(&chart.hover_label(index, Some(series))))
}
