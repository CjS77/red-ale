//! What makes a report result valid beyond its schema (`design/reports.md` §7).

use super::{Block, Cell, CellContent, Chart, ChartType, ColumnRole, PlotSize, Row, Section, Series, Table};
use super::chart::AxisFormat;
use crate::error::ResultError;

/// Whether the host has filled in its fields yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    /// What a script returned: no host field may be set.
    Draft,
    /// A finished result: every host field is set.
    Finished,
}

type Check = Result<(), ResultError>;

/// The most decimals a number cell shows: as many as GnuCash prints (`max_leg_digits`).
const MAX_PLACES: u32 = 18;

fn fail(path: &str, reason: impl Into<String>) -> Check {
    Err(ResultError::new(path, reason))
}

fn ensure(ok: bool, path: &str, reason: impl Into<String>) -> Check {
    if ok { Ok(()) } else { fail(path, reason) }
}

/// A host field: absent in a draft, present in a finished result.
fn host_field(present: bool, stage: Stage, path: &str) -> Check {
    match (stage, present) {
        (Stage::Draft, true) => fail(path, "set by the host; a script must leave it out"),
        (Stage::Finished, false) => fail(path, "missing; the host fills it in"),
        _ => Ok(()),
    }
}

pub(super) fn sections(sections: &[Section], stage: Stage) -> Check {
    sections.iter().enumerate().try_for_each(|(i, s)| blocks(&s.blocks, &format!("sections[{i}].blocks"), stage))
}

fn blocks(blocks: &[Block], path: &str, stage: Stage) -> Check {
    blocks.iter().enumerate().try_for_each(|(i, b)| block(b, &format!("{path}[{i}]"), stage))
}

fn block(b: &Block, path: &str, stage: Stage) -> Check {
    match b {
        Block::Text(_) | Block::Warning(_) => Ok(()),
        Block::Table(t) => table(t, path, stage),
        Block::Chart(c) => chart(c, path, stage),
        Block::Group(g) => blocks(&g.blocks, &format!("{path}.blocks"), stage),
        Block::OptionsSummary(_) => ensure(stage == Stage::Finished, &format!("{path}.kind"), "the options summary is added by the host"),
    }
}

fn table(t: &Table, path: &str, stage: Stage) -> Check {
    t.columns.iter().enumerate().try_for_each(|(i, c)| {
        let at = format!("{path}.columns[{i}].levels");
        match c.levels {
            Some(0) => fail(&at, "levels must be at least 1"),
            Some(_) => ensure(c.role == ColumnRole::Amount, &at, "only an amount column has levels"),
            None => Ok(()),
        }
    })?;
    let levels = t.columns.iter().filter_map(|c| c.levels).max();
    t.rows.iter().enumerate().try_for_each(|(i, r)| row(r, t.columns.len(), levels, &format!("{path}.rows[{i}]"), stage))
}

fn row(r: &Row, width: usize, levels: Option<u32>, path: &str, stage: Stage) -> Check {
    let covered: u64 = r.cells.iter().map(|c| c.as_ref().map_or(1, |c| u64::from(c.columns()))).sum();
    let fits = covered == u64::try_from(width).unwrap_or(u64::MAX);
    ensure(fits, &format!("{path}.cells"), format!("covers {covered} columns; the table has {width}"))?;
    if let Some(level) = r.level {
        let at = format!("{path}.level");
        match levels {
            Some(n) => ensure(level < n, &at, format!("level {level} is outside the {n} levels of the table's amount columns"))?,
            None => fail(&at, "no column of the table has levels")?,
        }
    }
    r.cells.iter().enumerate().try_for_each(|(i, c)| c.as_ref().map_or(Ok(()), |c| cell(c, &format!("{path}.cells[{i}]"), stage)))
}

fn cell(c: &Cell, path: &str, stage: Stage) -> Check {
    let content = c.content().ok_or_else(|| {
        ResultError::new(path, "a cell holds exactly one of text, monetary, commodities, number, price and date")
    })?;
    ensure(c.span != Some(0), &format!("{path}.span"), "span must be at least 1")?;
    let is_number = matches!(content, CellContent::Number(_));
    let is_price = matches!(content, CellContent::Price(_));
    ensure(is_number || (c.places.is_none() && c.suffix.is_none()), path, "places and suffix belong to number cells")?;
    ensure(c.places.is_none_or(|p| p <= MAX_PLACES), &format!("{path}.places"), format!("at most {MAX_PLACES} decimals"))?;
    ensure(is_price == c.currency.is_some(), &format!("{path}.currency"), "a price cell, and only a price cell, names its currency")?;
    let (rounded, local_date, display) = match content {
        CellContent::Text(_) => (false, false, false),
        CellContent::Monetary(_) => (true, false, true),
        CellContent::Commodities(lines) => return commodity_lines(c, lines, path, stage),
        CellContent::Number(_) | CellContent::Price(_) => (false, false, true),
        CellContent::Date(_) => (false, true, true),
    };
    host_or_absent(c.rounded.is_some(), rounded, stage, &format!("{path}.rounded"))?;
    host_or_absent(c.local_date.is_some(), local_date, stage, &format!("{path}.localDate"))?;
    host_or_absent(c.display.is_some(), display, stage, &format!("{path}.display"))
}

/// A host field the cell's kind has, or a field it must never carry.
fn host_or_absent(present: bool, applies: bool, stage: Stage, path: &str) -> Check {
    if applies { host_field(present, stage, path) } else { ensure(!present, path, "does not apply to this kind of cell") }
}

fn commodity_lines(c: &Cell, lines: &[super::CommodityLine], path: &str, stage: Stage) -> Check {
    [(c.rounded.is_some(), "rounded"), (c.local_date.is_some(), "localDate"), (c.display.is_some(), "display")]
        .iter()
        .try_for_each(|(present, name)| ensure(!present, &format!("{path}.{name}"), "does not apply to a commodities cell"))?;
    lines.iter().enumerate().try_for_each(|(i, l)| {
        let at = format!("{path}.commodities[{i}]");
        host_field(l.display.is_some(), stage, &format!("{at}.display"))?;
        host_or_absent(l.converted_display.is_some(), l.converted.is_some(), stage, &format!("{at}.convertedDisplay"))
    })
}

fn chart(c: &Chart, path: &str, stage: Stage) -> Check {
    plot_size(c.width, &format!("{path}.width"))?;
    plot_size(c.height, &format!("{path}.height"))?;
    [(&c.x_axis, "xAxis"), (&c.y_axis, "yAxis")].iter().try_for_each(|(axis, name)| {
        let currency = axis.format == Some(AxisFormat::Currency);
        ensure(!currency || axis.commodity.is_some(), &format!("{path}.{name}.commodity"), "a currency axis names its commodity")
    })?;
    ensure(c.chart_type != ChartType::Pie || c.series.len() <= 1, &format!("{path}.series"), "a pie chart has one series")?;
    c.series.iter().enumerate().try_for_each(|(i, s)| series(c, s, &format!("{path}.series[{i}]"), stage))
}

fn plot_size(size: PlotSize, path: &str) -> Check {
    match size {
        PlotSize::Percent(n) => ensure((1..=100).contains(&n), path, format!("{n} % is not between 1 and 100")),
        PlotSize::Pixels(n) => ensure(n > 0, path, "a size in pixels is at least 1"),
    }
}

fn series(c: &Chart, s: &Series, path: &str, stage: Stage) -> Check {
    let count = if c.chart_type == ChartType::Scatter { scatter_points(s, path, stage)? } else { category_values(c, s, path, stage)? };
    let per_value = |len: Option<usize>, name: &str| {
        len.map_or(Ok(()), |n| ensure(n == count, &format!("{path}.{name}"), format!("has {n} entries for {count} values")))
    };
    per_value(s.colors.as_ref().map(Vec::len), "colors")?;
    per_value(s.links.as_ref().map(Vec::len), "links")?;
    s.line_width.as_ref().map_or(Ok(()), |w| ensure(w.is_positive(), &format!("{path}.lineWidth"), "must be positive"))
}

fn scatter_points(s: &Series, path: &str, stage: Stage) -> Result<usize, ResultError> {
    ensure(s.values.is_none(), &format!("{path}.values"), "a scatter series has points, not values")?;
    ensure(s.displays.is_none(), &format!("{path}.displays"), "a scatter series shows each point's display")?;
    let points = s.points.as_ref().ok_or_else(|| ResultError::new(format!("{path}.points"), "a scatter series needs points"))?;
    points.iter().enumerate().try_for_each(|(i, p)| host_field(p.display.is_some(), stage, &format!("{path}.points[{i}].display")))?;
    Ok(points.len())
}

fn category_values(c: &Chart, s: &Series, path: &str, stage: Stage) -> Result<usize, ResultError> {
    ensure(s.points.is_none(), &format!("{path}.points"), "only a scatter series has points")?;
    ensure(s.line.is_none(), &format!("{path}.line"), "only a scatter series has a joining line")?;
    let values = s.values.as_ref().ok_or_else(|| ResultError::new(format!("{path}.values"), "the series needs values"))?;
    let labels = c.labels.len();
    ensure(values.len() == labels, &format!("{path}.values"), format!("has {} values for {labels} labels", values.len()))?;
    host_field(s.displays.is_some(), stage, &format!("{path}.displays"))?;
    s.displays.as_ref().map_or(Ok(()), |d| ensure(d.len() == labels, &format!("{path}.displays"), "one display per value"))?;
    Ok(values.len())
}
