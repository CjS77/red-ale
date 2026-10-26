//! Bar, line and scatter charts: two axes, ticks at nice steps, grid lines, and the series drawn as bars, lines with
//! markers, or points.

use super::geom::{Hu, Scale, UNIT, fit, fmt, part, text_width};
use super::{Frame, PLOT_BACKGROUND, Rect, hover};
use crate::number::{Rational, display_exact};
use crate::render::text::escape;
use crate::result::{Axis, AxisFormat, Chart, ChartType, Marker, Series, SeriesKind};

const TICK_FONT: Hu = 12 * UNIT;
const LABEL_FONT: Hu = 13 * UNIT;
const MARKER: Hu = 4 * UNIT;
const GAP: Hu = 6 * UNIT;
/// The longest category label shown, in characters' width.
const MAX_LABEL: Hu = 24 * TICK_FONT * 3 / 5;
const GRID: &str = "#e3e3e3";
const AXIS: &str = "#8c8c8c";

pub(super) fn draw(chart: &Chart, frame: &Frame) -> String {
    if chart.chart_type == ChartType::Scatter { scatter(chart, frame) } else { categories(chart, frame) }
}

/// A tick's text under an axis's format: `percent` adds `%`; other values are exact, grouped numbers (a currency
/// axis names its currency in its label).
fn tick_text(axis: &Axis, v: &Rational) -> String {
    match axis.format {
        Some(AxisFormat::Percent) => format!("{}%", display_exact(v)),
        _ => display_exact(v),
    }
}

/// The plot rectangle inside the frame's area, leaving room for the y tick labels and both axis labels.
fn plot_rect(chart: &Chart, frame: &Frame, y_ticks: &[String]) -> Rect {
    let area = frame.area;
    let y_label = if chart.y_axis.label.is_empty() { 0 } else { LABEL_FONT + GAP };
    let x_label = if chart.x_axis.label.is_empty() { 0 } else { LABEL_FONT + GAP };
    let widest = y_ticks.iter().map(|t| text_width(t, TICK_FONT)).max().unwrap_or(0).min(area.width() / 4);
    let rect = Rect { x0: area.x0 + y_label + widest + GAP, y0: area.y0, x1: area.x1, y1: area.y1 - TICK_FONT - GAP - x_label };
    if rect.width() < 40 * UNIT || rect.height() < 40 * UNIT { area } else { rect }
}

/// Background, horizontal grid lines and y tick labels, the zero line and the axis labels.
fn axes(chart: &Chart, plot: Rect, y: &Scale, y_ticks: &[String], frame: &Frame) -> Vec<String> {
    let background = format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{PLOT_BACKGROUND}\"/>",
        fmt(plot.x0),
        fmt(plot.y0),
        fmt(plot.width()),
        fmt(plot.height())
    );
    let values = y.ticks();
    let ticks = values.iter().zip(y_ticks).flat_map(|(t, text)| {
        let at = y.map(t, plot.y1, plot.y0);
        let line = chart.grid.then(|| hline(plot, at, GRID));
        let label = format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"12\" text-anchor=\"end\">{}</text>",
            fmt(plot.x0 - GAP),
            fmt(at + 4 * UNIT),
            escape(text)
        );
        line.into_iter().chain([label])
    });
    let zero = (y.lo.is_negative() && y.hi.is_positive()).then(|| hline(plot, y.map(&Rational::zero(), plot.y1, plot.y0), AXIS));
    let frame_lines = [
        format!("<line x1=\"{0}\" y1=\"{1}\" x2=\"{0}\" y2=\"{2}\" stroke=\"{AXIS}\"/>", fmt(plot.x0), fmt(plot.y0), fmt(plot.y1)),
        hline(plot, plot.y1, AXIS),
    ];
    std::iter::once(background).chain(ticks).chain(zero).chain(frame_lines).chain(axis_labels(chart, plot, frame)).collect()
}

fn hline(plot: Rect, y: Hu, color: &str) -> String {
    let (x0, y, x1) = (fmt(plot.x0), fmt(y), fmt(plot.x1));
    format!("<line x1=\"{x0}\" y1=\"{y}\" x2=\"{x1}\" y2=\"{y}\" stroke=\"{color}\"/>")
}

fn axis_labels(chart: &Chart, plot: Rect, frame: &Frame) -> Vec<String> {
    let x = (!chart.x_axis.label.is_empty()).then(|| {
        let shown = fit(&chart.x_axis.label, plot.width(), LABEL_FONT);
        let y = frame.area.y1 - 2 * UNIT;
        let x = fmt(plot.x0 + plot.width() / 2);
        format!("<text x=\"{x}\" y=\"{}\" font-size=\"13\" text-anchor=\"middle\">{}</text>", fmt(y), escape(&shown))
    });
    let y = (!chart.y_axis.label.is_empty()).then(|| {
        let shown = fit(&chart.y_axis.label, plot.height(), LABEL_FONT);
        let (cx, cy) = (frame.area.x0 + LABEL_FONT - 2 * UNIT, plot.y0 + plot.height() / 2);
        format!(
            "<text x=\"{0}\" y=\"{1}\" font-size=\"13\" text-anchor=\"middle\" transform=\"rotate(-90 {0} {1})\">{2}</text>",
            fmt(cx),
            fmt(cy),
            escape(&shown)
        )
    });
    x.into_iter().chain(y).collect()
}

/// How a series is drawn: by its own `kind`, else as the chart's type.
fn is_bar(chart: &Chart, s: &Series) -> bool {
    match s.kind {
        Some(SeriesKind::Bar) => true,
        Some(SeriesKind::Line) => false,
        None => chart.chart_type == ChartType::Bar,
    }
}

fn value(s: &Series, i: usize) -> Option<&Rational> {
    s.values.as_ref().and_then(|v| v.get(i)).and_then(Option::as_ref)
}

fn color_of(s: &Series, i: usize) -> &str {
    s.colors.as_ref().and_then(|c| c.get(i)).unwrap_or(&s.color).as_str()
}

/// One bar: a series' value at a label, from `from` to `to`, in a column of the label's slot.
struct Bar {
    series: usize,
    index: usize,
    column: usize,
    from: Rational,
    to: Rational,
}

/// The bar columns of each slot: every bar series its own, or, stacked, one per stack name in order of appearance
/// (Chart.js stacks bars with the same `stack` and sets stacks side by side).
fn columns(chart: &Chart) -> Vec<Vec<usize>> {
    let bars = chart.series.iter().enumerate().filter(|(_, s)| is_bar(chart, s));
    if !chart.stacked {
        return bars.map(|(i, _)| vec![i]).collect();
    }
    bars.fold(Vec::<(Option<&str>, Vec<usize>)>::new(), |mut groups, (i, s)| {
        let name = s.stack.as_deref();
        match groups.iter_mut().find(|(n, _)| *n == name) {
            Some((_, members)) => members.push(i),
            None => groups.push((name, vec![i])),
        }
        groups
    })
    .into_iter()
    .map(|(_, members)| members)
    .collect()
}

/// Every bar: stacked ones grow up from zero for positive values and down for negative, as Chart.js stacks them.
fn bars(chart: &Chart, columns: &[Vec<usize>]) -> Vec<Bar> {
    let labels = chart.labels.len();
    columns
        .iter()
        .enumerate()
        .flat_map(|(column, members)| {
            (0..labels).flat_map(move |index| {
                let start = (Rational::zero(), Rational::zero());
                members
                    .iter()
                    .scan(start, move |(up, down), &series| {
                        let bar = value(&chart.series[series], index).map(|v| {
                            let acc = if v.is_negative() { down } else { up };
                            let (from, to) = if chart.stacked { (acc.clone(), &*acc + v) } else { (Rational::zero(), v.clone()) };
                            acc.clone_from(&to);
                            Bar { series, index, column, from, to }
                        });
                        Some(bar)
                    })
                    .flatten()
            })
        })
        .collect()
}

/// A line series' points: for each label, the base and the top, when it has a value. Stacked lines add up in order
/// of the series.
struct Line {
    series: usize,
    points: Vec<Option<(Rational, Rational)>>,
}

fn lines(chart: &Chart) -> Vec<Line> {
    let labels = chart.labels.len();
    let line_series = chart.series.iter().enumerate().filter(|(_, s)| !is_bar(chart, s));
    line_series
        .scan(vec![Rational::zero(); labels], |acc, (series, s)| {
            let points = (0..labels)
                .map(|i| {
                    value(s, i).map(|v| {
                        let base = if chart.stacked { acc[i].clone() } else { Rational::zero() };
                        let top = &base + v;
                        if chart.stacked {
                            acc[i].clone_from(&top);
                        }
                        (base, top)
                    })
                })
                .collect();
            Some(Line { series, points })
        })
        .collect()
}

fn categories(chart: &Chart, frame: &Frame) -> String {
    let columns = columns(chart);
    let bars = bars(chart, &columns);
    let lines = lines(chart);
    let filled = lines.iter().any(|l| chart.series[l.series].fill);
    let bar_ends = bars.iter().flat_map(|b| [&b.from, &b.to]);
    let line_ends = lines.iter().flat_map(|l| l.points.iter().flatten()).flat_map(|(base, top)| [base, top]);
    let y = Scale::covering(bar_ends.chain(line_ends), !bars.is_empty() || filled);
    let y_ticks: Vec<String> = y.ticks().iter().map(|t| tick_text(&chart.y_axis, t)).collect();
    let plot = plot_rect(chart, frame, &y_ticks);
    let slots = Slots { plot, count: chart.labels.len().max(1) };
    let at = |v: &Rational| y.map(v, plot.y1, plot.y0);
    let bars_svg = bars.iter().map(|b| bar(chart, b, &slots, columns.len(), &at));
    let lines_svg = lines.iter().map(|l| line(chart, l, &slots, &at));
    axes(chart, plot, &y, &y_ticks, frame)
        .into_iter()
        .chain(category_labels(chart, &slots))
        .chain(bars_svg)
        .chain(lines_svg)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The label slots across the plot.
struct Slots {
    plot: Rect,
    count: usize,
}

impl Slots {
    fn n(&self) -> i64 {
        i64::try_from(self.count).unwrap_or(1)
    }

    fn start(&self, i: usize) -> Hu {
        self.plot.x0 + part(self.plot.width(), i64::try_from(i).unwrap_or(0), self.n())
    }

    fn centre(&self, i: usize) -> Hu {
        self.plot.x0 + part(self.plot.width(), 2 * i64::try_from(i).unwrap_or(0) + 1, 2 * self.n())
    }

    fn width(&self) -> Hu {
        part(self.plot.width(), 1, self.n())
    }
}

/// Category labels under the plot, every k-th one when they would overlap.
fn category_labels(chart: &Chart, slots: &Slots) -> Vec<String> {
    let shown: Vec<String> = chart.labels.iter().map(|l| fit(l, MAX_LABEL, TICK_FONT)).collect();
    let widest = shown.iter().map(|l| text_width(l, TICK_FONT)).max().unwrap_or(0) + GAP;
    let every = usize::try_from((widest + slots.width() - 1) / slots.width().max(1)).unwrap_or(1).max(1);
    let y = slots.plot.y1 + GAP + TICK_FONT;
    shown
        .iter()
        .enumerate()
        .filter(|(i, _)| i % every == 0)
        .map(|(i, l)| {
            format!("<text x=\"{}\" y=\"{}\" font-size=\"12\" text-anchor=\"middle\">{}</text>", fmt(slots.centre(i)), fmt(y), escape(l))
        })
        .collect()
}

fn bar(chart: &Chart, b: &Bar, slots: &Slots, columns: usize, at: &impl Fn(&Rational) -> Hu) -> String {
    let (start, width) = (slots.start(b.index), slots.width());
    let (group_x, group_w) = (start + part(width, 1, 10), part(width, 8, 10));
    let cols = i64::try_from(columns.max(1)).unwrap_or(1);
    let column = i64::try_from(b.column).unwrap_or(0);
    let (x0, x1) = (group_x + part(group_w, column, cols), group_x + part(group_w, column + 1, cols));
    let (ya, yb) = (at(&b.from), at(&b.to));
    let s = &chart.series[b.series];
    format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\">{}</rect>",
        fmt(x0),
        fmt(ya.min(yb)),
        fmt(x1 - x0),
        fmt((ya - yb).abs()),
        escape(color_of(s, b.index)),
        hover(chart, b.index, b.series)
    )
}

/// The stroke width of a series' line, two units by default.
fn stroke_width(s: &Series) -> String {
    let width = s.line_width.as_ref().map_or(2 * UNIT, |w| {
        let hundredths = (w * &Rational::from_int(UNIT)).round();
        num_traits::ToPrimitive::to_i64(hundredths.numer()).unwrap_or(20 * UNIT).clamp(1, 20 * UNIT)
    });
    fmt(width)
}

/// A line series: a filled area under each run of values when `fill` is set, the line, then a marker per point.
fn line(chart: &Chart, l: &Line, slots: &Slots, at: &impl Fn(&Rational) -> Hu) -> String {
    let s = &chart.series[l.series];
    let points: Vec<Option<(Hu, Hu, Hu)>> =
        l.points.iter().enumerate().map(|(i, p)| p.as_ref().map(|(base, top)| (slots.centre(i), at(base), at(top)))).collect();
    let runs: Vec<Vec<(Hu, Hu, Hu)>> =
        points.split(Option::is_none).filter(|run| !run.is_empty()).map(|run| run.iter().flatten().copied().collect()).collect();
    let color = escape(s.color.as_str());
    let fills = runs.iter().filter(|_| s.fill).map(|run| {
        let tops = run.iter().map(|(x, _, top)| format!("{},{}", fmt(*x), fmt(*top)));
        let bases = run.iter().rev().map(|(x, base, _)| format!("{},{}", fmt(*x), fmt(*base)));
        format!("<polygon points=\"{}\" fill=\"{color}\" fill-opacity=\"0.25\"/>", tops.chain(bases).collect::<Vec<_>>().join(" "))
    });
    let strokes = runs.iter().map(|run| {
        let pts: Vec<String> = run.iter().map(|(x, _, top)| format!("{},{}", fmt(*x), fmt(*top))).collect();
        format!("<polyline points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\"/>", pts.join(" "), stroke_width(s))
    });
    let markers = points.iter().enumerate().filter_map(|(i, p)| {
        p.map(|(x, _, top)| marker(s.marker.unwrap_or(Marker::Circle), x, top, color_of(s, i), &hover(chart, i, l.series)))
    });
    fills.chain(strokes).chain(markers).collect::<Vec<_>>().join("\n")
}

/// A point marker centred on (x, y), holding its hover label.
fn marker(shape: Marker, cx: Hu, cy: Hu, color: &str, title: &str) -> String {
    let (c, r) = (escape(color), MARKER);
    let at = |dx: Hu, dy: Hu| format!("{} {}", fmt(cx + dx), fmt(cy + dy));
    match shape {
        Marker::Circle => format!("<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{c}\">{title}</circle>", fmt(cx), fmt(cy), fmt(r)),
        Marker::Square => {
            let (x0, y0, side) = (fmt(cx - r), fmt(cy - r), fmt(2 * r));
            format!("<rect x=\"{x0}\" y=\"{y0}\" width=\"{side}\" height=\"{side}\" fill=\"{c}\">{title}</rect>")
        }
        Marker::Diamond => format!("<path d=\"M{} L{} L{} L{} Z\" fill=\"{c}\">{title}</path>", at(0, -r), at(r, 0), at(0, r), at(-r, 0)),
        Marker::Cross => {
            let (m1, l1, m2, l2) = (at(-r, -r), at(r, r), at(r, -r), at(-r, r));
            format!("<path d=\"M{m1} L{l1} M{m2} L{l2}\" stroke=\"{c}\" stroke-width=\"2\">{title}</path>")
        }
        Marker::Plus => {
            let (m1, l1, m2, l2) = (at(-r, 0), at(r, 0), at(0, -r), at(0, r));
            format!("<path d=\"M{m1} L{l1} M{m2} L{l2}\" stroke=\"{c}\" stroke-width=\"2\">{title}</path>")
        }
    }
}

/// A scatter chart: linear x and y axes over the points, a marker per point, joined in data order by a line of the
/// series' `line` colour when it has one (`5.17:gnucash/report/reports/standard/price-scatter.scm:269-274`).
fn scatter(chart: &Chart, frame: &Frame) -> String {
    let all = || chart.series.iter().flat_map(|s| s.points.iter().flatten());
    let x = Scale::covering(all().map(|p| &p.x), false);
    let y = Scale::covering(all().map(|p| &p.y), false);
    let y_ticks: Vec<String> = y.ticks().iter().map(|t| tick_text(&chart.y_axis, t)).collect();
    let plot = plot_rect(chart, frame, &y_ticks);
    let to_x = |v: &Rational| x.map(v, plot.x0, plot.x1);
    let to_y = |v: &Rational| y.map(v, plot.y1, plot.y0);
    let x_ticks = x.ticks().into_iter().flat_map(|t| {
        let at = to_x(&t);
        let grid = chart.grid.then(|| {
            format!("<line x1=\"{0}\" y1=\"{1}\" x2=\"{0}\" y2=\"{2}\" stroke=\"{GRID}\"/>", fmt(at), fmt(plot.y0), fmt(plot.y1))
        });
        let label = format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"12\" text-anchor=\"middle\">{}</text>",
            fmt(at),
            fmt(plot.y1 + GAP + TICK_FONT),
            escape(&tick_text(&chart.x_axis, &t))
        );
        grid.into_iter().chain([label])
    });
    let series = chart.series.iter().enumerate().flat_map(|(si, s)| {
        let points: Vec<(Hu, Hu)> = s.points.iter().flatten().map(|p| (to_x(&p.x), to_y(&p.y))).collect();
        let joined = s.line.as_ref().filter(|_| points.len() > 1).map(|c| {
            let pts: Vec<String> = points.iter().map(|(px, py)| format!("{},{}", fmt(*px), fmt(*py))).collect();
            let (stroke, width) = (escape(c.as_str()), stroke_width(s));
            format!("<polyline points=\"{}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{width}\"/>", pts.join(" "))
        });
        let markers: Vec<String> = points
            .iter()
            .enumerate()
            .map(|(i, (px, py))| marker(s.marker.unwrap_or(Marker::Circle), *px, *py, color_of(s, i), &hover(chart, i, si)))
            .collect();
        joined.into_iter().chain(markers)
    });
    axes(chart, plot, &y, &y_ticks, frame).into_iter().chain(x_ticks).chain(series).collect::<Vec<_>>().join("\n")
}
