//! What makes a report result valid beyond its schema (`design/reports.md` §7).

use super::chart::AxisFormat;
use super::{
    Anchor, Block, Cell, CellContent, Chart, ChartType, Column, ColumnRole, GridBlock, Link, NumberFormat, PlotSize, Row, RowRole, Section,
    Series, Table,
};
use crate::error::ResultError;
use crate::options::SPAN_MAX;
use crate::render::check_css;

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
    sections_at("", sections, stage)
}

/// The sections at `prefix`: empty at the top, `….result.` inside a grid cell.
fn sections_at(prefix: &str, sections: &[Section], stage: Stage) -> Check {
    sections
        .iter()
        .enumerate()
        .try_for_each(|(i, s)| blocks(&s.blocks, &format!("{prefix}sections[{i}].blocks"), stage))
}

fn blocks(blocks: &[Block], path: &str, stage: Stage) -> Check {
    blocks
        .iter()
        .enumerate()
        .try_for_each(|(i, b)| block(b, &format!("{path}[{i}]"), stage))
}

/// A result's `css` passes the CSS template's rule (`design/reports.md` §8): the host leaves out any that does not.
pub(super) fn css(css: Option<&str>) -> Check {
    css.map_or(Ok(()), |c| check_css(c).or_else(|reason| fail("css", reason)))
}

/// Class names: one or more, separated by single spaces, each a letter then letters, digits, `-` and `_`.
fn class_list(class: Option<&str>, path: &str) -> Check {
    let name = |n: &str| {
        let mut chars = n.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    ensure(
        class.is_none_or(|c| c.split(' ').all(name)),
        path,
        "class names are separated by single spaces, each a letter then letters, digits, '-' and '_'",
    )
}

fn block(b: &Block, path: &str, stage: Stage) -> Check {
    class_list(b.class(), &format!("{path}.class"))?;
    match b {
        Block::Text(t) => t.link.as_ref().map_or(Ok(()), |l| link(l, &format!("{path}.link"))),
        Block::Warning(_) | Block::List(_) => Ok(()),
        Block::Table(t) => table(t, path, stage),
        Block::Chart(c) => chart(c, path, stage),
        Block::Group(g) => blocks(&g.blocks, &format!("{path}.blocks"), stage),
        Block::Grid(g) => grid(g, path, stage),
        Block::Image(i) => ensure(
            i.src.starts_with("file:///"),
            &format!("{path}.src"),
            "a picture's address starts with file:///",
        ),
        Block::OptionsSummary(_) => ensure(
            stage == Stage::Finished,
            &format!("{path}.kind"),
            "the options summary is added by the host",
        ),
    }
}

/// A grid's cells span 0 to 100 rows and columns. A script leaves each cell's host fields out; in a finished result
/// each cell holds its child's result or a warning, not both, and the child's result is valid in turn.
fn grid(g: &GridBlock, path: &str, stage: Stage) -> Check {
    g.rows.iter().enumerate().try_for_each(|(r, row)| {
        row.cells.iter().enumerate().try_for_each(|(c, cell)| {
            let at = format!("{path}.rows[{r}].cells[{c}]");
            let max = u32::from(SPAN_MAX);
            ensure(cell.rowspan <= max, &format!("{at}.rowspan"), "spans at most 100 rows")?;
            ensure(cell.colspan <= max, &format!("{at}.colspan"), "spans at most 100 columns")?;
            let host_only = "set by the host; a script must leave it out";
            ensure(stage == Stage::Finished || cell.link.is_none(), &format!("{at}.link"), host_only)?;
            match (&cell.result, &cell.warning, stage) {
                (None, None, Stage::Draft) | (None, Some(_), Stage::Finished) => Ok(()),
                (Some(_), _, Stage::Draft) => fail(&format!("{at}.result"), host_only),
                (_, Some(_), Stage::Draft) => fail(&format!("{at}.warning"), host_only),
                (Some(_), Some(_), Stage::Finished) => fail(&at, "holds a result or a warning, not both"),
                (None, None, Stage::Finished) => fail(&at, "missing its result or warning; the host fills it in"),
                (Some(child), None, Stage::Finished) => sections_at(&format!("{at}.result."), &child.sections, stage),
            }
        })
    })
}

/// A link to a linked document names one.
fn link(l: &Link, path: &str) -> Check {
    match l {
        Link::DocLink(uri) => ensure(!uri.is_empty(), &format!("{path}.docLink"), "names no document"),
        _ => Ok(()),
    }
}

fn column(c: &Column, path: &str) -> Check {
    let at = format!("{path}.levels");
    match c.levels {
        Some(0) => fail(&at, "levels must be at least 1"),
        Some(_) => ensure(c.role == ColumnRole::Amount, &at, "only an amount column has levels"),
        None => Ok(()),
    }?;
    ensure(
        c.label_lines.as_ref().is_none_or(|lines| !lines.is_empty()),
        &format!("{path}.labelLines"),
        "has no lines",
    )
}

fn table(t: &Table, path: &str, stage: Stage) -> Check {
    t.columns
        .iter()
        .enumerate()
        .try_for_each(|(i, c)| column(c, &format!("{path}.columns[{i}]")))?;
    let levels = t.columns.iter().filter_map(|c| c.levels).max();
    t.rows.iter().enumerate().try_for_each(|(i, r)| {
        let at = format!("{path}.rows[{i}]");
        row(r, t.columns.len(), levels, &at, stage)?;
        footnotes(r, t.footnotes.len(), &at)
    })
}

/// Every footnote mark of a row names one of the table's footnotes, counted from 1.
fn footnotes(r: &Row, count: usize, path: &str) -> Check {
    r.cells.iter().enumerate().try_for_each(|(i, c)| {
        let mark = c.as_ref().and_then(|c| c.footnote);
        ensure(
            mark.is_none_or(|n| n >= 1 && usize::try_from(n).is_ok_and(|n| n <= count)),
            &format!("{path}.cells[{i}].footnote"),
            format!("the table has {count} footnotes, numbered from 1"),
        )
    })
}

fn row(r: &Row, width: usize, levels: Option<u32>, path: &str, stage: Stage) -> Check {
    ensure(
        r.alternate.is_none() || matches!(r.role, RowRole::Data | RowRole::Account),
        &format!("{path}.alternate"),
        "only data and account rows are shaded",
    )?;
    let covered: u64 = r.cells.iter().map(|c| c.as_ref().map_or(1, |c| u64::from(c.columns()))).sum();
    let fits = covered == u64::try_from(width).unwrap_or(u64::MAX);
    ensure(
        fits,
        &format!("{path}.cells"),
        format!("covers {covered} columns; the table has {width}"),
    )?;
    if let Some(level) = r.level {
        let at = format!("{path}.level");
        match levels {
            Some(n) => ensure(
                level < n,
                &at,
                format!("level {level} is outside the {n} levels of the table's amount columns"),
            )?,
            None => fail(&at, "no column of the table has levels")?,
        }
    }
    r.cells
        .iter()
        .enumerate()
        .try_for_each(|(i, c)| c.as_ref().map_or(Ok(()), |c| cell(c, &format!("{path}.cells[{i}]"), stage)))
}

fn cell(c: &Cell, path: &str, stage: Stage) -> Check {
    let content = c.content().ok_or_else(|| {
        ResultError::new(
            path,
            "a cell holds exactly one of text, monetary, commodities, number, price, date and anchors",
        )
    })?;
    ensure(c.span != Some(0), &format!("{path}.span"), "span must be at least 1")?;
    class_list(c.class.as_deref(), &format!("{path}.class"))?;
    c.link.as_ref().map_or(Ok(()), |l| link(l, &format!("{path}.link")))?;
    period(c, content, path, stage)?;
    let is_number = matches!(content, CellContent::Number(_));
    let is_price = matches!(content, CellContent::Price(_));
    ensure(
        is_number || (c.places.is_none() && c.suffix.is_none() && c.rounding.is_none() && c.number_format.is_none()),
        path,
        "places, suffix, rounding and numberFormat belong to number cells",
    )?;
    number_format(c, path)?;
    ensure(
        c.places.is_none_or(|p| p <= MAX_PLACES),
        &format!("{path}.places"),
        format!("at most {MAX_PLACES} decimals"),
    )?;
    ensure(
        is_price == c.currency.is_some(),
        &format!("{path}.currency"),
        "a price cell, and only a price cell, names its currency",
    )?;
    let (rounded, local_date, display) = match content {
        CellContent::Text(_) => (false, false, false),
        CellContent::Monetary(_) => (true, false, true),
        CellContent::Commodities(lines) => return commodity_lines(c, lines, path, stage),
        CellContent::Anchors(anchors) => return self::anchors(c, anchors, path),
        CellContent::Number(_) | CellContent::Price(_) => (false, false, true),
        CellContent::Date(_) => (false, true, true),
    };
    host_or_absent(c.rounded.is_some(), rounded, stage, &format!("{path}.rounded"))?;
    host_or_absent(c.local_date.is_some(), local_date, stage, &format!("{path}.localDate"))?;
    host_or_absent(c.display.is_some(), display, stage, &format!("{path}.display"))
}

/// A share display rounds half up and takes no `rounding`; an exact display rounds nothing and takes no `places`
/// either.
fn number_format(c: &Cell, path: &str) -> Check {
    match c.number_format {
        Some(NumberFormat::Share) => ensure(
            c.rounding.is_none(),
            &format!("{path}.rounding"),
            "a share display rounds half up; it takes no rounding",
        ),
        Some(NumberFormat::Exact) => {
            ensure(c.places.is_none(), &format!("{path}.places"), "an exact display has no places")?;
            ensure(c.rounding.is_none(), &format!("{path}.rounding"), "an exact display rounds nothing")
        }
        None => Ok(()),
    }
}

/// `until` belongs to a date cell and is not before its date; the host fills `localUntil` for it.
fn period(c: &Cell, content: CellContent<'_>, path: &str, stage: Stage) -> Check {
    let at = format!("{path}.until");
    match (content, c.until) {
        (CellContent::Date(from), Some(until)) => ensure(until.0 >= from.0, &at, "is before the cell's date")?,
        (_, Some(_)) => fail(&at, "belongs to date cells")?,
        _ => {}
    }
    host_or_absent(c.local_until.is_some(), c.until.is_some(), stage, &format!("{path}.localUntil"))
}

/// An `anchors` cell has at least one anchor and carries its links in them, with no display of its own.
fn anchors(c: &Cell, anchors: &[Anchor], path: &str) -> Check {
    ensure(!anchors.is_empty(), &format!("{path}.anchors"), "has no anchors")?;
    ensure(
        c.link.is_none() && c.account.is_none(),
        path,
        "an anchors cell carries its links in its anchors",
    )?;
    [
        (c.rounded.is_some(), "rounded"),
        (c.local_date.is_some(), "localDate"),
        (c.display.is_some(), "display"),
    ]
    .iter()
    .try_for_each(|(present, name)| ensure(!present, &format!("{path}.{name}"), "does not apply to an anchors cell"))?;
    anchors
        .iter()
        .enumerate()
        .try_for_each(|(i, a)| link(&a.link, &format!("{path}.anchors[{i}].link")))
}

/// A host field the cell's kind has, or a field it must never carry.
fn host_or_absent(present: bool, applies: bool, stage: Stage, path: &str) -> Check {
    if applies {
        host_field(present, stage, path)
    } else {
        ensure(!present, path, "does not apply to this kind of cell")
    }
}

fn commodity_lines(c: &Cell, lines: &[super::CommodityLine], path: &str, stage: Stage) -> Check {
    [
        (c.rounded.is_some(), "rounded"),
        (c.local_date.is_some(), "localDate"),
        (c.display.is_some(), "display"),
    ]
    .iter()
    .try_for_each(|(present, name)| ensure(!present, &format!("{path}.{name}"), "does not apply to a commodities cell"))?;
    lines.iter().enumerate().try_for_each(|(i, l)| {
        let at = format!("{path}.commodities[{i}]");
        host_field(l.display.is_some(), stage, &format!("{at}.display"))?;
        host_or_absent(
            l.converted_display.is_some(),
            l.converted.is_some(),
            stage,
            &format!("{at}.convertedDisplay"),
        )
    })
}

fn chart(c: &Chart, path: &str, stage: Stage) -> Check {
    plot_size(c.width, &format!("{path}.width"))?;
    plot_size(c.height, &format!("{path}.height"))?;
    [(&c.x_axis, "xAxis"), (&c.y_axis, "yAxis")].iter().try_for_each(|(axis, name)| {
        let currency = axis.format == Some(AxisFormat::Currency);
        ensure(
            !currency || axis.commodity.is_some(),
            &format!("{path}.{name}.commodity"),
            "a currency axis names its commodity",
        )
    })?;
    ensure(
        c.chart_type != ChartType::Pie || c.series.len() <= 1,
        &format!("{path}.series"),
        "a pie chart has one series",
    )?;
    c.series
        .iter()
        .enumerate()
        .try_for_each(|(i, s)| series(c, s, &format!("{path}.series[{i}]"), stage))
}

fn plot_size(size: PlotSize, path: &str) -> Check {
    match size {
        PlotSize::Percent(n) => ensure((1..=100).contains(&n), path, format!("{n} % is not between 1 and 100")),
        PlotSize::Pixels(n) => ensure(n > 0, path, "a size in pixels is at least 1"),
    }
}

fn series(c: &Chart, s: &Series, path: &str, stage: Stage) -> Check {
    let count = if c.chart_type == ChartType::Scatter {
        scatter_points(s, path, stage)?
    } else {
        category_values(c, s, path, stage)?
    };
    let per_value = |len: Option<usize>, name: &str| {
        len.map_or(Ok(()), |n| {
            ensure(n == count, &format!("{path}.{name}"), format!("has {n} entries for {count} values"))
        })
    };
    per_value(s.colors.as_ref().map(Vec::len), "colors")?;
    per_value(s.links.as_ref().map(Vec::len), "links")?;
    s.link.as_ref().map_or(Ok(()), |l| link(l, &format!("{path}.link")))?;
    s.links
        .iter()
        .flatten()
        .enumerate()
        .try_for_each(|(i, l)| l.as_ref().map_or(Ok(()), |l| link(l, &format!("{path}.links[{i}]"))))?;
    s.line_width.as_ref().map_or(Ok(()), |w| {
        ensure(w.is_positive(), &format!("{path}.lineWidth"), "must be positive")
    })
}

fn scatter_points(s: &Series, path: &str, stage: Stage) -> Result<usize, ResultError> {
    ensure(
        s.values.is_none(),
        &format!("{path}.values"),
        "a scatter series has points, not values",
    )?;
    ensure(
        s.displays.is_none(),
        &format!("{path}.displays"),
        "a scatter series shows each point's display",
    )?;
    let points = s
        .points
        .as_ref()
        .ok_or_else(|| ResultError::new(format!("{path}.points"), "a scatter series needs points"))?;
    points
        .iter()
        .enumerate()
        .try_for_each(|(i, p)| host_field(p.display.is_some(), stage, &format!("{path}.points[{i}].display")))?;
    Ok(points.len())
}

fn category_values(c: &Chart, s: &Series, path: &str, stage: Stage) -> Result<usize, ResultError> {
    ensure(s.points.is_none(), &format!("{path}.points"), "only a scatter series has points")?;
    ensure(
        s.line.is_none(),
        &format!("{path}.line"),
        "only a scatter series has a joining line",
    )?;
    let values = s
        .values
        .as_ref()
        .ok_or_else(|| ResultError::new(format!("{path}.values"), "the series needs values"))?;
    let labels = c.labels.len();
    ensure(
        values.len() == labels,
        &format!("{path}.values"),
        format!("has {} values for {labels} labels", values.len()),
    )?;
    host_field(s.displays.is_some(), stage, &format!("{path}.displays"))?;
    s.displays.as_ref().map_or(Ok(()), |d| {
        ensure(d.len() == labels, &format!("{path}.displays"), "one display per value")
    })?;
    Ok(values.len())
}
