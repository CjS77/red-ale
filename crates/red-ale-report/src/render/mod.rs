//! HTML and SVG from a report result and a style sheet (`design/reports.md` §8).
//!
//! [`html`] turns a finished [`ReportResult`] into one self-contained HTML5 document: inline CSS only, charts as
//! inline SVG, no script, no external resource and no `href`, every piece of text escaped, and the same bytes for the
//! same result, style sheet and context. The renderer reads only the result and the style sheet, never the Book, so a
//! result a client got as JSON renders the same way.
//!
//! - [`StyleSheetTemplate`], [`StyleSheet`] and [`StyleSheets`]: GnuCash 5.17's style sheets and their options, and
//!   lookup by name over the predefined ones and the Configuration's ([`StyleSheetEntry`]).

mod css;
mod look;
mod sheet;
mod svg;
mod table;
mod text;

use red_ale_core::TimeZone;

pub use sheet::{
    DEFAULT_STYLE_SHEET, PREDEFINED, StyleSheet, StyleSheetEntry, StyleSheetError, StyleSheetTemplate, StyleSheets, check_css,
};

use look::{Line, Look};
use text::{escape, escape_lines};

use crate::result::{Block, LogEntry, LogLevel, OptionsSummary, ReportResult, Section, TextBlock, TextRole};
use crate::settings::DateFormat;

/// What the page needs besides the result and the style sheet: how to print the run's date, which header and footer
/// lines show. The run's `now` is the only time printed.
#[derive(Debug, Clone, Copy)]
pub struct RenderContext<'a> {
    /// The run's local time zone.
    pub zone: &'a TimeZone,
    /// How dates print (`reports.dateFormat`).
    pub date_format: DateFormat,
}

/// The report as one HTML document, through `sheet`; `None` renders with the base look and no style-sheet options,
/// as GnuCash renders a report whose style sheet no longer exists (`design/reports.md` §8).
pub fn html(result: &ReportResult, sheet: Option<&StyleSheet>, ctx: &RenderContext<'_>) -> String {
    let look = Look::of(sheet);
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
        result.sections.iter().enumerate().map(|(i, s)| section(s, i, &look)).collect::<Vec<_>>().join("\n"),
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
    if body.is_empty() { String::new() } else { format!("<header class=\"masthead\">\n{}\n</header>", body.join("\n")) }
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
    let date = ctx.date_format.print(local.date);
    if !time {
        return date;
    }
    let offset = ctx.zone.offset_at(t);
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.unsigned_abs() / 60;
    format!("{date} {:02}:{:02}:{:02} {sign}{:02}{:02}", local.hour, local.minute, local.second, minutes / 60, minutes % 60)
}

fn section(s: &Section, index: usize, look: &Look) -> String {
    let title = s.title.as_ref().map(|t| format!("<h2>{}</h2>", escape(t)));
    let blocks = s.blocks.iter().enumerate().map(|(i, b)| block(b, &format!("{index}-{i}"), look));
    let body: Vec<String> = title.into_iter().chain(blocks).collect();
    format!("<section>\n{}\n</section>", body.join("\n"))
}

/// A block; `path` is its position (`0-2`, `0-2-1` inside a group), which names a chart's `id`.
fn block(b: &Block, path: &str, look: &Look) -> String {
    match b {
        Block::Text(t) => text_block(t),
        Block::Warning(w) => {
            format!("<div class=\"warning\" role=\"note\"><h3>{}</h3><p>{}</p></div>", escape(&w.title), escape_lines(&w.text))
        }
        Block::Table(t) => format!("<div class=\"table-wrap\">\n{}\n</div>", table::table(t, look)),
        Block::Chart(c) => format!("<figure class=\"chart\">\n{}\n</figure>", svg::chart(c, &format!("chart-{path}"))),
        Block::Group(g) => {
            let inner: Vec<String> = g.blocks.iter().enumerate().map(|(i, b)| format!("<div>\n{}\n</div>", block(b, &format!("{path}-{i}"), look))).collect();
            format!("<div class=\"group\">\n{}\n</div>", inner.join("\n"))
        }
        Block::OptionsSummary(o) => options_summary(o),
    }
}

fn text_block(t: &TextBlock) -> String {
    match t.role {
        TextRole::Heading => format!("<h3 class=\"text-heading\">{}</h3>", escape_lines(&t.text)),
        TextRole::Paragraph => format!("<p>{}</p>", escape_lines(&t.text)),
        TextRole::Note => format!("<p class=\"note\">{}</p>", escape_lines(&t.text)),
    }
}

/// `gnc:html-render-options-changed` (`5.17:gnucash/report/html-utilities.scm:298-348`): `Section / Name: value`.
fn options_summary(o: &OptionsSummary) -> String {
    let rows: Vec<String> = o
        .rows
        .iter()
        .map(|r| format!("<tr><td><b>{} / {}</b></td><td>{}</td></tr>", escape(&r.section), escape(&r.name), escape_lines(&r.value)))
        .collect();
    format!("<table class=\"options-summary\">\n{}\n</table>", rows.join("\n"))
}

#[cfg(test)]
mod tests;
