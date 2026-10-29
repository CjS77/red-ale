//! HTML and SVG from a report result and a style sheet (`design/reports.md` §8).
//!
//! [`html`] turns a finished [`ReportResult`] into one self-contained HTML5 document: inline CSS only, charts as
//! inline SVG, no script, no external resource and no `href`, every piece of text escaped, and the same bytes for the
//! same result, style sheet and context. The renderer reads only the result and the style sheet, never the Book, so a
//! result a client got as JSON renders the same way.
//!
//! - [`csv`]: a result's export rows as GnuCash's CSV export writes them.
//! - [`StyleSheetTemplate`], [`StyleSheet`] and [`StyleSheets`]: GnuCash 5.17's style sheets and their options, and
//!   lookup by name over the predefined ones and the Configuration's ([`StyleSheetEntry`]).

mod css;
mod csv;
mod look;
mod sheet;
mod svg;
mod table;
mod text;

use red_ale_core::TimeZone;
use red_ale_locale::Locale;

pub use csv::csv;
pub use sheet::{
    DEFAULT_STYLE_SHEET, PREDEFINED, StyleSheet, StyleSheetEntry, StyleSheetError, StyleSheetTemplate, StyleSheets, check_css, left_out,
};

use look::{Line, Look};
use text::{escape, escape_lines};

use crate::result::{
    Block, GridBlock, GridCell, ListBlock, LogEntry, LogLevel, OptionsSummary, ReportResult, Section, TextBlock, TextRole,
};
use crate::settings::DateFormat;

/// What the page needs besides the result and the style sheet: how to print the run's date, which header and footer
/// lines show. The run's `now` is the only time printed.
#[derive(Debug, Clone, Copy)]
pub struct RenderContext<'a> {
    /// The run's local time zone.
    pub zone: &'a TimeZone,
    /// How dates print (`reports.dateFormat`).
    pub date_format: DateFormat,
    /// The Locale the run's date and any figure without a display form follow.
    pub locale: Locale,
}

/// The report as one HTML document, through `sheet`; `None` renders with the base look and no style-sheet options,
/// as GnuCash renders a report whose style sheet no longer exists (`design/reports.md` §8).
pub fn html(result: &ReportResult, sheet: Option<&StyleSheet>, ctx: &RenderContext<'_>) -> String {
    let look = Look {
        numbers: ctx.locale.conventions(),
        ..Look::of(sheet)
    };
    let heading = result.headline.as_deref().unwrap_or(&result.title);
    let template = look.template.map_or("none", |t| match t {
        StyleSheetTemplate::Plain => "plain",
        StyleSheetTemplate::Css => "css",
        StyleSheetTemplate::Easy => "easy",
        StyleSheetTemplate::Fancy => "fancy",
        StyleSheetTemplate::Footer => "footer",
        StyleSheetTemplate::HeadOrTail => "head-or-tail",
    });
    let page = [
        "<!DOCTYPE html>".to_owned(),
        "<html lang=\"en\">".to_owned(),
        "<head>".to_owned(),
        "<meta charset=\"utf-8\">".to_owned(),
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">".to_owned(),
        format!("<title>{}</title>", escape(&result.title)),
        format!("<style>\n{}\n</style>", css::stylesheet(&look)),
        "</head>".to_owned(),
        format!("<body class=\"sheet-{template}\">"),
        "<div class=\"report\">".to_owned(),
        masthead(heading, &look.header, result, ctx),
        result
            .sections
            .iter()
            .enumerate()
            .map(|(i, s)| section(s, &i.to_string(), &look))
            .collect::<Vec<_>>()
            .join("\n"),
        colophon(&look.footer, result, ctx),
        "</div>".to_owned(),
        "</body>".to_owned(),
        "</html>".to_owned(),
    ];
    let mut out = page.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n");
    out.push('\n');
    out
}

/// The log entry the host adds when a report names a style sheet that does not exist and renders without one.
pub fn missing_style_sheet(name: &str) -> LogEntry {
    LogEntry {
        level: LogLevel::Debug,
        text: format!("style sheet {name:?} does not exist; rendered with no style sheet"),
        source: None,
        line: None,
    }
}

fn masthead(heading: &str, lines: &[Line], result: &ReportResult, ctx: &RenderContext<'_>) -> String {
    let title = (!heading.is_empty()).then(|| format!("<h1 class=\"report-title\">{}</h1>", escape(heading)));
    let lines = lines.iter().map(|l| line(l, result, ctx));
    let body: Vec<String> = title.into_iter().chain(lines).collect();
    if body.is_empty() {
        String::new()
    } else {
        format!("<header class=\"masthead\">\n{}\n</header>", body.join("\n"))
    }
}

fn colophon(lines: &[Line], result: &ReportResult, ctx: &RenderContext<'_>) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let body: Vec<String> = lines.iter().map(|l| line(l, result, ctx)).collect();
    format!("<footer class=\"colophon\">\n{}\n</footer>", body.join("\n"))
}

/// A header or footer line, worded as GnuCash's style sheets word it.
fn line(l: &Line, result: &ReportResult, ctx: &RenderContext<'_>) -> String {
    match l {
        Line::PreparedBy(name) => format!("<p>Prepared by: <b>{}</b></p>", escape(name)),
        Line::PreparedFor(name) => format!("<p>Prepared for: <b>{}</b></p>", escape(name)),
        Line::Date { label, time } => format!("<p>{label}{}</p>", escape(&run_date(result, ctx, *time))),
        Line::Version => format!("<p>red-ale {}</p>", env!("CARGO_PKG_VERSION")),
        Line::Note(text) => format!("<p>{}</p>", escape_lines(text)),
    }
}

/// The run's `now` under `reports.dateFormat`, with the local time and offset when asked: GnuCash's
/// `qof-print-date` and `gnc-print-time64 … "%X %Z"` (`5.17:gnucash/report/stylesheets/head-or-tail.scm:398-407`),
/// with the numeric offset for the zone's name, which red-ale does not know.
fn run_date(result: &ReportResult, ctx: &RenderContext<'_>, time: bool) -> String {
    let t = result.now.0;
    let Some(local) = ctx.zone.to_local(t) else {
        return result.now.to_rfc3339().unwrap_or_default();
    };
    let date = ctx.date_format.print(local.date, &ctx.locale);
    if !time {
        return date;
    }
    let offset = ctx.zone.offset_at(t);
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.unsigned_abs() / 60;
    format!(
        "{date} {:02}:{:02}:{:02} {sign}{:02}{:02}",
        local.hour,
        local.minute,
        local.second,
        minutes / 60,
        minutes % 60
    )
}

/// A section; `path` is its position (`0`, or a grid cell's path and the child's section inside a view).
fn section(s: &Section, path: &str, look: &Look) -> String {
    let title = s.title.as_ref().map(|t| format!("<h2>{}</h2>", escape(t)));
    let blocks = s.blocks.iter().enumerate().map(|(i, b)| block(b, &format!("{path}-{i}"), look));
    let body: Vec<String> = title.into_iter().chain(blocks).collect();
    format!("<section>\n{}\n</section>", body.join("\n"))
}

/// A block; `path` is its position (`0-2`, `0-2-1` inside a group), which names a chart's `id`.
fn block(b: &Block, path: &str, look: &Look) -> String {
    match b {
        Block::Text(t) => text_block(t, look),
        Block::List(l) => list(l, look),
        Block::Warning(w) => {
            format!(
                "<div class=\"warning\" role=\"note\"><h3>{}</h3><p>{}</p></div>",
                escape(&w.title),
                escape_lines(&w.text)
            )
        }
        Block::Table(t) => format!("<div class=\"table-wrap\">\n{}\n</div>", table::table(t, look)),
        Block::Chart(c) => format!(
            "<figure class=\"chart\">\n{}\n</figure>",
            svg::chart(c, &format!("chart-{path}"), look.numbers)
        ),
        Block::Group(g) => {
            let inner: Vec<String> = g
                .blocks
                .iter()
                .enumerate()
                .map(|(i, b)| format!("<div>\n{}\n</div>", block(b, &format!("{path}-{i}"), look)))
                .collect();
            format!("<div class=\"group\">\n{}\n</div>", inner.join("\n"))
        }
        Block::Grid(g) => grid(g, path, look),
        Block::OptionsSummary(o) => options_summary(o),
    }
}

/// A grid as a full-width table of its rows (`5.17:gnucash/report/reports/standard/view-column.scm:99-155`), each
/// cell top-aligned with its spans, a column span of 0 written as 1 as browsers read it. A cell holds its child's
/// title as a heading, its sections (positions below the cell's, so chart ids stay unique) and its "Single Report"
/// link, or the warning that took the child's place.
fn grid(g: &GridBlock, path: &str, look: &Look) -> String {
    let rows = g.rows.iter().enumerate().map(|(r, row)| {
        let cells: Vec<String> = row
            .cells
            .iter()
            .enumerate()
            .map(|(c, cell)| grid_cell(cell, &format!("{path}-{r}-{c}"), look))
            .collect();
        format!("<tr>\n{}\n</tr>", cells.join("\n"))
    });
    let open = "<table class=\"grid\" style=\"width: 100%\">".to_owned();
    let lines: Vec<String> = std::iter::once(open).chain(rows).chain(["</table>".to_owned()]).collect();
    lines.join("\n")
}

fn grid_cell(cell: &GridCell, path: &str, look: &Look) -> String {
    let body = match (&cell.result, &cell.warning) {
        (Some(child), _) => {
            let heading = child.headline.as_deref().unwrap_or(&child.title);
            let title = (!heading.is_empty()).then(|| format!("<h3>{}</h3>", escape(heading)));
            let sections = child.sections.iter().enumerate().map(|(i, s)| section(s, &format!("{path}-{i}"), look));
            title.into_iter().chain(sections).collect::<Vec<_>>().join("\n")
        }
        (None, Some(w)) => block(&Block::Warning(w.clone()), path, look),
        (None, None) => String::new(),
    };
    let link = table::link_data(cell.link.as_ref(), look)
        .map(|data| format!("\n<p class=\"note\"><span class=\"link\"{data}>Single Report</span></p>"))
        .unwrap_or_default();
    format!(
        "<td rowspan=\"{}\" colspan=\"{}\" style=\"vertical-align: top\">\n{body}{link}\n</td>",
        cell.rowspan,
        cell.colspan.max(1)
    )
}

/// A text block; a link, while links are on, is a data attribute on its element and marks the text as a link.
fn text_block(t: &TextBlock, look: &Look) -> String {
    let data = table::link_data(t.link.as_ref(), look);
    let text = match &data {
        Some(_) => format!("<span class=\"link\">{}</span>", escape_lines(&t.text)),
        None => escape_lines(&t.text),
    };
    let data = data.unwrap_or_default();
    match t.role {
        TextRole::Heading => format!("<h3 class=\"text-heading\"{data}>{text}</h3>"),
        TextRole::Paragraph => format!("<p{data}>{text}</p>"),
        TextRole::Note => format!("<p class=\"note\"{data}>{text}</p>"),
    }
}

/// A list block as `<ul>`, as `gnc:html-markup-ul` writes one (`5.17:gnucash/report/html-text.scm:210-215`); an
/// item naming an account carries it as `data-account`, as table cells do.
fn list(l: &ListBlock, look: &Look) -> String {
    let item = |text: &str, account: Option<String>| match account {
        Some(a) => format!("<li data-account=\"{a}\"><span class=\"link\">{}</span></li>", escape_lines(text)),
        None => format!("<li>{}</li>", escape_lines(text)),
    };
    let items = l
        .items
        .iter()
        .map(|i| item(&i.text, i.account.filter(|_| look.links).map(|a| a.0.to_string())));
    let parts = std::iter::once("<ul class=\"list\">".to_owned()).chain(items).chain(["</ul>".to_owned()]);
    parts.collect::<Vec<_>>().join("\n")
}

/// `gnc:html-render-options-changed` (`5.17:gnucash/report/html-utilities.scm:298-348`): `Section / Name: value`,
/// or `Name: value` for a row with no section, a view's `Embedded Report` rows (`:339-343`).
fn options_summary(o: &OptionsSummary) -> String {
    let label = |section: &str, name: &str| {
        if section.is_empty() {
            escape(name).into_owned()
        } else {
            format!("{} / {}", escape(section), escape(name))
        }
    };
    let rows: Vec<String> = o
        .rows
        .iter()
        .map(|r| format!("<tr><td><b>{}</b></td><td>{}</td></tr>", label(&r.section, &r.name), escape_lines(&r.value)))
        .collect();
    format!("<table class=\"options-summary\">\n{}\n</table>", rows.join("\n"))
}
