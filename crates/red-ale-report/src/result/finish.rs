//! The host's half of the report result: the run's identity, time, options and log, and the display form of every
//! figure (`design/reports.md` §3, "Display rounding and formatting"; §7).

use red_ale_core::{Book, CommodityRef, Date, Time64, TimeZone};
use red_ale_locale::Locale;

use super::chart::{Axis, AxisFormat};
use super::validate::Stage;
use super::{
    Block, Cell, Chart, CommodityLine, LogEntry, OptionValues, RESULT_VERSION, ReportDraft, ReportIdentity, ReportResult, Section,
};
use crate::error::ResultError;
use crate::number::{CommodityFormat, Monetary, Rational, display_exact, display_monetary, display_number, display_price};
use crate::settings::DateFormat;
use crate::wire::Timestamp;

/// What the host knows about a run.
#[derive(Debug, Clone, PartialEq)]
pub struct RunInfo {
    /// The template and saved configuration run.
    pub report: ReportIdentity,
    /// The time the report was run for.
    pub now: Time64,
    /// Every option's value, in wire form.
    pub options: OptionValues,
    /// The report log.
    pub log: Vec<LogEntry>,
}

/// What display forms need: the Book's commodities, the run's local time zone, `reports.dateFormat` and the Locale.
#[derive(Debug, Clone, Copy)]
pub struct DisplayContext<'a> {
    /// The Book the report read.
    pub book: &'a Book,
    /// The run's local time zone.
    pub zone: &'a TimeZone,
    /// How dates print.
    pub date_format: DateFormat,
    /// The Locale numbers and `locale` dates follow.
    pub locale: Locale,
}

impl DisplayContext<'_> {
    fn commodity(&self, id: &CommodityRef, path: &str) -> Result<CommodityFormat, ResultError> {
        CommodityFormat::from_book(self.book, id).ok_or_else(|| ResultError::new(path, format!("unknown commodity {id}")))
    }

    fn monetary(&self, m: &Monetary, path: &str) -> Result<(Rational, String), ResultError> {
        let shown = display_monetary(&m.amount, &self.commodity(&m.commodity, path)?, &self.locale.conventions());
        Ok((shown.rounded, shown.display))
    }

    /// A date as `reports.dateFormat` prints it under the Locale.
    pub(crate) fn print_date(&self, day: Date) -> String {
        self.date_format.print(day, &self.locale)
    }
}

type Fill = Result<(), ResultError>;

/// How a chart value is shown.
type Show = Box<dyn Fn(&Rational) -> String>;

impl ReportDraft {
    /// Check the draft, then complete it: the run's fields, and the display form of every cell and chart value.
    pub fn finish(self, run: RunInfo, ctx: &DisplayContext<'_>) -> Result<ReportResult, ResultError> {
        self.validate()?;
        let mut sections = self.sections;
        fill_sections(&mut sections, ctx)?;
        let result = ReportResult {
            version: RESULT_VERSION.to_owned(),
            report: run.report,
            title: self.title,
            headline: self.headline,
            now: Timestamp(run.now),
            options: run.options,
            sections,
            log: run.log,
            export: self.export,
        };
        super::validate::sections(&result.sections, Stage::Finished)?;
        Ok(result)
    }
}

fn fill_sections(sections: &mut [Section], ctx: &DisplayContext<'_>) -> Fill {
    sections
        .iter_mut()
        .enumerate()
        .try_for_each(|(i, s)| fill_blocks(&mut s.blocks, &format!("sections[{i}].blocks"), ctx))
}

fn fill_blocks(blocks: &mut [Block], path: &str, ctx: &DisplayContext<'_>) -> Fill {
    blocks.iter_mut().enumerate().try_for_each(|(i, b)| {
        let at = format!("{path}[{i}]");
        match b {
            Block::Table(t) => t.rows.iter_mut().enumerate().try_for_each(|(r, row)| {
                row.cells.iter_mut().enumerate().try_for_each(|(c, cell)| {
                    cell.as_mut()
                        .map_or(Ok(()), |cell| fill_cell(cell, &format!("{at}.rows[{r}].cells[{c}]"), ctx))
                })
            }),
            Block::Chart(c) => fill_chart(c, &at, ctx),
            Block::Group(g) => fill_blocks(&mut g.blocks, &format!("{at}.blocks"), ctx),
            Block::Text(_) | Block::Warning(_) | Block::OptionsSummary(_) => Ok(()),
        }
    })
}

fn fill_cell(cell: &mut Cell, path: &str, ctx: &DisplayContext<'_>) -> Fill {
    if let Some(m) = &cell.monetary {
        let (rounded, display) = ctx.monetary(m, &format!("{path}.monetary.commodity"))?;
        (cell.rounded, cell.display) = (Some(rounded), Some(display));
    } else if let Some(n) = &cell.number {
        let suffix = cell.suffix.as_deref().unwrap_or("");
        cell.display = Some(format!("{}{suffix}", display_number(n, cell.places, &ctx.locale.conventions())));
    } else if let (Some(p), Some(currency)) = (&cell.price, &cell.currency) {
        let currency = ctx.commodity(&currency.0, &format!("{path}.currency"))?;
        cell.display = Some(display_price(p, &currency, &ctx.locale.conventions()));
    } else if let Some(t) = cell.date {
        let day = ctx
            .zone
            .local_date(t.0)
            .ok_or_else(|| ResultError::new(format!("{path}.date"), "outside the supported range"))?;
        (cell.local_date, cell.display) = (Some(day.to_file_string()), Some(ctx.print_date(day)));
    } else if let Some(lines) = &mut cell.commodities {
        lines
            .iter_mut()
            .enumerate()
            .try_for_each(|(i, l)| fill_line(l, &format!("{path}.commodities[{i}]"), ctx))?;
    }
    Ok(())
}

fn fill_line(line: &mut CommodityLine, path: &str, ctx: &DisplayContext<'_>) -> Fill {
    line.display = Some(ctx.monetary(&line.monetary, &format!("{path}.monetary.commodity"))?.1);
    let converted = line
        .converted
        .as_ref()
        .map(|m| ctx.monetary(m, &format!("{path}.converted.commodity")));
    line.converted_display = converted.transpose()?.map(|(_, d)| d);
    Ok(())
}

fn fill_chart(chart: &mut Chart, path: &str, ctx: &DisplayContext<'_>) -> Fill {
    let show = value_formatter(&chart.y_axis, &format!("{path}.yAxis.commodity"), ctx)?;
    chart.series.iter_mut().for_each(|s| {
        s.displays = s
            .values
            .as_ref()
            .map(|values| values.iter().map(|v| v.as_ref().map(&show)).collect());
        s.points.iter_mut().flatten().for_each(|p| p.display = Some(show(&p.y)));
    });
    Ok(())
}

/// How a hover label shows a value of `axis`: the display form of section 3 for `currency`, two decimals and `%` for
/// `percent`, the exact decimal for `number`.
fn value_formatter(axis: &Axis, path: &str, ctx: &DisplayContext<'_>) -> Result<Show, ResultError> {
    let conv = ctx.locale.conventions();
    Ok(match (axis.format.unwrap_or_default(), &axis.commodity) {
        (AxisFormat::Currency, Some(c)) => {
            let format = ctx.commodity(&c.0, path)?;
            Box::new(move |v| display_monetary(v, &format, &conv).display)
        }
        (AxisFormat::Percent, _) => Box::new(move |v| format!("{}%", display_number(v, Some(2), &conv))),
        (AxisFormat::Number | AxisFormat::Currency, _) => Box::new(move |v| display_exact(v, &conv)),
    })
}
