//! Tables of the report result as HTML (`design/reports.md` §8, "Tables"): an amount column with `levels` becomes
//! that many cell columns, GnuCash's staircase; a `commodities` cell becomes a small nested table; a column's
//! `labelLines` become stacked header rows; links become data attributes, never `href`s, and a linked document's link
//! becomes nothing.

use red_ale_locale::Conventions;

use super::look::Look;
use super::text::{escape, escape_lines};
use crate::number::{Rational, display_exact};
use crate::result::{Align, Anchor, Cell, CellContent, Column, ColumnRole, Link, Row, RowRole, Table};

/// The most staircase positions or spanned columns drawn, so a malformed result cannot blow up the page.
const MAX_WIDTH: usize = 64;

pub(super) fn table(t: &Table, look: &Look) -> String {
    let widths: Vec<usize> = t
        .columns
        .iter()
        .map(|c| usize::try_from(c.levels.unwrap_or(1)).unwrap_or(1).clamp(1, MAX_WIDTH))
        .collect();
    let total: usize = widths.iter().sum::<usize>().max(1);
    let caption = t.title.as_ref().map(|title| format!("<caption>{}</caption>", escape(title)));
    let labelled = |c: &Column| !c.label.is_empty() || c.label_lines.iter().flatten().any(|l| !l.is_empty());
    let head = t.columns.iter().any(labelled).then(|| head(&t.columns, &widths));
    let rows = t.rows.iter().scan(0_usize, |striped, row| {
        let alternate = matches!(row.role, RowRole::Data | RowRole::Account) && {
            *striped += 1;
            *striped % 2 == 0
        };
        Some(self::row(t, row, &widths, total, alternate, look))
    });
    let body = std::iter::once("<tbody>".to_owned()).chain(rows).chain(["</tbody>".to_owned()]);
    let open = std::iter::once("<table class=\"report-table\">".to_owned());
    let parts = open.chain(caption).chain(head).chain(body).chain(["</table>".to_owned()]);
    parts.chain(footnote_list(&t.footnotes)).collect::<Vec<_>>().join("\n")
}

/// The table's footnotes as an ordered list after it, as GnuCash's footnote collector lists them
/// (`5.17:gnucash/report/reports/standard/budget.scm:246-251`); nothing when there are none.
fn footnote_list(notes: &[String]) -> Option<String> {
    (!notes.is_empty()).then(|| {
        let items: Vec<String> = notes.iter().map(|n| format!("<li>{}</li>", escape_lines(n))).collect();
        format!("<ol class=\"footnotes\">{}</ol>", items.concat())
    })
}

/// A cell's footnote mark: a space and the note's number as a superscript, titled with the note
/// (`5.17:gnucash/report/reports/standard/budget.scm:253-259`).
fn footnote_mark(cell: &Cell, notes: &[String]) -> String {
    let note = |n: u32| usize::try_from(n).ok().and_then(|n| n.checked_sub(1)).and_then(|i| notes.get(i));
    cell.footnote
        .and_then(|n| note(n).map(|text| format!(" <sup class=\"footnote\" title=\"{}\">{n}</sup>", escape(text))))
        .unwrap_or_default()
}

fn align_of(c: &Column) -> Align {
    c.align
        .unwrap_or(if c.role == ColumnRole::Amount { Align::Right } else { Align::Left })
}

fn align_name(a: Align) -> &'static str {
    match a {
        Align::Left => "left",
        Align::Right => "right",
        Align::Center => "center",
    }
}

fn colspan(n: usize) -> String {
    if n > 1 { format!(" colspan=\"{n}\"") } else { String::new() }
}

/// The header: one row, or as many as the most `labelLines` of a column, GnuCash's multi-row column headers; a column
/// without lines shows its label in the first row.
fn head(columns: &[Column], widths: &[usize]) -> String {
    let rows = columns.iter().filter_map(|c| c.label_lines.as_ref().map(Vec::len)).max().unwrap_or(1);
    let line = |c: &'_ Column, k: usize| match &c.label_lines {
        Some(lines) => lines.get(k).map_or(String::new(), |l| escape_lines(l)),
        None if k == 0 => escape_lines(&c.label),
        None => String::new(),
    };
    let row = |k: usize| {
        let cells: Vec<String> = columns
            .iter()
            .zip(widths)
            .map(|(c, w)| format!("<th class=\"column-heading-{}\"{}>{}</th>", align_name(align_of(c)), colspan(*w), line(c, k)))
            .collect();
        format!("<tr>{}</tr>", cells.concat())
    };
    format!("<thead>\n{}\n</thead>", (0..rows).map(row).collect::<Vec<_>>().join("\n"))
}

/// The role's name, as in the result's JSON.
fn role_name(r: RowRole) -> &'static str {
    match r {
        RowRole::Data => "data",
        RowRole::Heading => "heading",
        RowRole::Account => "account",
        RowRole::Subtotal => "subtotal",
        RowRole::Total => "total",
        RowRole::GrandTotal => "grandTotal",
        RowRole::Rule => "rule",
        RowRole::Spacer => "spacer",
    }
}

/// The GnuCash row style a role stands for, which the style sheet colours.
fn row_style(r: RowRole, alternate: bool) -> Option<&'static str> {
    match r {
        RowRole::Data | RowRole::Account if alternate => Some("alternate-row"),
        RowRole::Data | RowRole::Account => Some("normal-row"),
        RowRole::Heading | RowRole::Subtotal => Some("primary-subheading"),
        RowRole::Total => Some("secondary-subheading"),
        RowRole::GrandTotal => Some("grand-total"),
        RowRole::Rule | RowRole::Spacer => None,
    }
}

fn is_total(r: RowRole) -> bool {
    matches!(r, RowRole::Subtotal | RowRole::Total | RowRole::GrandTotal)
}

fn row(t: &Table, row: &Row, widths: &[usize], total: usize, alternate: bool, look: &Look) -> String {
    let style = row_style(row.role, alternate).map(|s| format!(" {s}")).unwrap_or_default();
    let account = row
        .account
        .filter(|_| look.links)
        .map(|a| format!(" data-account=\"{}\"", a.0))
        .unwrap_or_default();
    let open = format!("<tr class=\"role-{}{style}\"{account}>", role_name(row.role));
    let cells = match row.role {
        RowRole::Rule => format!("<td{}><hr></td>", colspan(total)),
        RowRole::Spacer => format!("<td{}></td>", colspan(total)),
        _ => cells(t, row, widths, look),
    };
    format!("{open}{cells}</tr>")
}

fn cells(t: &Table, row: &Row, widths: &[usize], look: &Look) -> String {
    let width = |col: usize| widths.get(col).copied().unwrap_or(1);
    row.cells
        .iter()
        .scan(0_usize, |next, cell| {
            let start = *next;
            let span = cell
                .as_ref()
                .map_or(1, |c| usize::try_from(c.columns()).unwrap_or(1).clamp(1, MAX_WIDTH));
            *next += span;
            Some((start, span, cell))
        })
        .map(|(start, span, cell)| {
            let Some(cell) = cell else {
                return "<td></td>".repeat(width(start));
            };
            let column = t.columns.get(start);
            let levels = width(start);
            if span == 1 && levels > 1 {
                let level = usize::try_from(row.level.unwrap_or(0)).unwrap_or(0).min(levels - 1);
                let before = levels - 1 - level;
                format!(
                    "{}{}{}",
                    "<td></td>".repeat(before),
                    td(cell, column, row, look, 1, &t.footnotes),
                    "<td></td>".repeat(level)
                )
            } else {
                td(cell, column, row, look, (start..start + span).map(width).sum(), &t.footnotes)
            }
        })
        .collect()
}

fn td(cell: &Cell, column: Option<&Column>, row: &Row, look: &Look, span: usize, notes: &[String]) -> String {
    let content = cell.content();
    let total = is_total(row.role);
    let label_column = column.is_some_and(|c| c.role == ColumnRole::Label);
    let text = matches!(content, Some(CellContent::Text(_) | CellContent::Anchors(_)) | None);
    let kind = match content {
        _ if text && total => "total-label-cell",
        _ if text => "text-cell",
        Some(CellContent::Date(_)) => "date-cell",
        _ if total => "total-number-cell",
        _ => "number-cell",
    };
    let negative = match content {
        Some(CellContent::Monetary(m)) => m.amount.is_negative(),
        Some(CellContent::Number(n) | CellContent::Price(n)) => n.is_negative(),
        _ => false,
    };
    let classes = [
        Some(kind),
        negative.then_some("neg"),
        (text && label_column).then_some("label-cell"),
        cell.rule.then_some("rule-above"),
        column.and_then(|c| c.align).map(|a| match a {
            Align::Left => "align-left",
            Align::Right => "align-right",
            Align::Center => "align-center",
        }),
    ];
    let class = classes.into_iter().flatten().collect::<Vec<_>>().join(" ");
    let depth = row
        .depth
        .filter(|d| *d > 0 && label_column)
        .map(|d| format!(" style=\"--depth:{d}\""))
        .unwrap_or_default();
    let data = data_attributes(cell, look);
    format!(
        "<td class=\"{class}\"{}{depth}{data}>{}{}</td>",
        colspan(span),
        linked(cell, look, content_html(cell, look)),
        footnote_mark(cell, notes)
    )
}

fn data_attributes(cell: &Cell, look: &Look) -> String {
    if !look.links {
        return String::new();
    }
    let account = cell.account.map(|a| format!(" data-account=\"{}\"", a.0));
    account.into_iter().chain(link_data(cell.link.as_ref(), look)).collect()
}

/// ` data-link` with the link's JSON, while links are on; nothing for a linked document, which the result only
/// describes (`design/reports.md` §8).
pub(super) fn link_data(link: Option<&Link>, look: &Look) -> Option<String> {
    link.filter(|l| look.links && l.is_live())
        .and_then(|l| serde_json::to_string(l).ok())
        .map(|json| format!(" data-link=\"{}\"", escape(&json)))
}

/// Content that names an account or links somewhere, marked so the link font and colour apply, as GnuCash's `<a>`
/// would be; with links off, plain text, as GnuCash drops the `<a>` tag.
fn linked(cell: &Cell, look: &Look, html: String) -> String {
    if look.links && (cell.account.is_some() || cell.link.as_ref().is_some_and(Link::is_live)) {
        format!("<span class=\"link\">{html}</span>")
    } else {
        html
    }
}

/// An `anchors` cell's text, one anchor per line, as the Transaction Report's invoice cell
/// (`5.17:gnucash/report/trep-engine.scm:1036-1049`).
fn anchors_html(anchors: &[Anchor], look: &Look) -> String {
    let one = |a: &Anchor| match link_data(Some(&a.link), look) {
        Some(data) => format!("<span class=\"link\"{data}>{}</span>", escape_lines(&a.text)),
        None => escape_lines(&a.text),
    };
    anchors.iter().map(one).collect::<Vec<_>>().join("<br>")
}

fn content_html(cell: &Cell, look: &Look) -> String {
    let numbers: &Conventions = &look.numbers;
    let shown = |display: &Option<String>, exact: &dyn Fn() -> String| escape(&display.clone().unwrap_or_else(exact)).into_owned();
    let display_exact = |v: &Rational| display_exact(v, numbers);
    match cell.content() {
        Some(CellContent::Text(text)) => escape_lines(text),
        Some(CellContent::Anchors(anchors)) => anchors_html(anchors, look),
        Some(CellContent::Monetary(m)) => shown(&cell.display, &|| display_exact(&m.amount)),
        Some(CellContent::Number(n) | CellContent::Price(n)) => shown(&cell.display, &|| display_exact(n)),
        Some(CellContent::Date(t)) => shown(&cell.display, &|| {
            cell.local_date.clone().unwrap_or_else(|| t.to_rfc3339().unwrap_or_default())
        }),
        Some(CellContent::Commodities(lines)) => {
            let rows: Vec<String> = lines
                .iter()
                .map(|l| {
                    let neg = |negative: bool| if negative { " class=\"neg\"" } else { "" };
                    let native = shown(&l.display, &|| display_exact(&l.monetary.amount));
                    let converted = l.converted.as_ref().map(|c| {
                        format!(
                            "<td{}>{}</td>",
                            neg(c.amount.is_negative()),
                            shown(&l.converted_display, &|| display_exact(&c.amount))
                        )
                    });
                    format!(
                        "<tr><td{}>{native}</td>{}</tr>",
                        neg(l.monetary.amount.is_negative()),
                        converted.unwrap_or_default()
                    )
                })
                .collect();
            format!("<table class=\"commodities\">{}</table>", rows.concat())
        }
        None => String::new(),
    }
}
