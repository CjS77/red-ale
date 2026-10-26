//! Tables of the report result as HTML (`design/reports.md` §8, "Tables"): an amount column with `levels` becomes
//! that many cell columns, GnuCash's staircase; a `commodities` cell becomes a small nested table; links become data
//! attributes, never `href`s.

use red_ale_locale::Conventions;

use super::look::Look;
use super::text::{escape, escape_lines};
use crate::number::{Rational, display_exact};
use crate::result::{Align, Cell, CellContent, Column, ColumnRole, Row, RowRole, Table};

/// The most staircase positions or spanned columns drawn, so a malformed result cannot blow up the page.
const MAX_WIDTH: usize = 64;

pub(super) fn table(t: &Table, look: &Look) -> String {
    let widths: Vec<usize> = t.columns.iter().map(|c| usize::try_from(c.levels.unwrap_or(1)).unwrap_or(1).clamp(1, MAX_WIDTH)).collect();
    let total: usize = widths.iter().sum::<usize>().max(1);
    let caption = t.title.as_ref().map(|title| format!("<caption>{}</caption>", escape(title)));
    let head = t.columns.iter().any(|c| !c.label.is_empty()).then(|| head(&t.columns, &widths));
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
    parts.collect::<Vec<_>>().join("\n")
}

fn align_of(c: &Column) -> Align {
    c.align.unwrap_or(if c.role == ColumnRole::Amount { Align::Right } else { Align::Left })
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

fn head(columns: &[Column], widths: &[usize]) -> String {
    let cells: Vec<String> = columns
        .iter()
        .zip(widths)
        .map(|(c, w)| format!("<th class=\"column-heading-{}\"{}>{}</th>", align_name(align_of(c)), colspan(*w), escape_lines(&c.label)))
        .collect();
    format!("<thead>\n<tr>{}</tr>\n</thead>", cells.concat())
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
    let account = row.account.filter(|_| look.links).map(|a| format!(" data-account=\"{}\"", a.0)).unwrap_or_default();
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
            let span = cell.as_ref().map_or(1, |c| usize::try_from(c.columns()).unwrap_or(1).clamp(1, MAX_WIDTH));
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
                format!("{}{}{}", "<td></td>".repeat(before), td(cell, column, row, look, 1), "<td></td>".repeat(level))
            } else {
                td(cell, column, row, look, (start..start + span).map(width).sum())
            }
        })
        .collect()
}

fn td(cell: &Cell, column: Option<&Column>, row: &Row, look: &Look, span: usize) -> String {
    let content = cell.content();
    let total = is_total(row.role);
    let label_column = column.is_some_and(|c| c.role == ColumnRole::Label);
    let text = matches!(content, Some(CellContent::Text(_)) | None);
    let kind = match content {
        Some(CellContent::Text(_)) | None if total => "total-label-cell",
        Some(CellContent::Text(_)) | None => "text-cell",
        Some(CellContent::Date(_)) => "date-cell",
        Some(_) if total => "total-number-cell",
        Some(_) => "number-cell",
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
    let depth = row.depth.filter(|d| *d > 0 && label_column).map(|d| format!(" style=\"--depth:{d}\"")).unwrap_or_default();
    let data = data_attributes(cell, look);
    format!("<td class=\"{class}\"{}{depth}{data}>{}</td>", colspan(span), linked(cell, look, content_html(cell, &look.numbers)))
}

fn data_attributes(cell: &Cell, look: &Look) -> String {
    if !look.links {
        return String::new();
    }
    let account = cell.account.map(|a| format!(" data-account=\"{}\"", a.0));
    let link = cell.link.as_ref().and_then(|l| serde_json::to_string(l).ok()).map(|json| format!(" data-link=\"{}\"", escape(&json)));
    account.into_iter().chain(link).collect()
}

/// Content that names an account or links somewhere, marked so the link font and colour apply, as GnuCash's `<a>`
/// would be; with links off, plain text, as GnuCash drops the `<a>` tag.
fn linked(cell: &Cell, look: &Look, html: String) -> String {
    if look.links && (cell.account.is_some() || cell.link.is_some()) { format!("<span class=\"link\">{html}</span>") } else { html }
}

fn content_html(cell: &Cell, numbers: &Conventions) -> String {
    let shown = |display: &Option<String>, exact: &dyn Fn() -> String| escape(&display.clone().unwrap_or_else(exact)).into_owned();
    let display_exact = |v: &Rational| display_exact(v, numbers);
    match cell.content() {
        Some(CellContent::Text(text)) => escape_lines(text),
        Some(CellContent::Monetary(m)) => shown(&cell.display, &|| display_exact(&m.amount)),
        Some(CellContent::Number(n) | CellContent::Price(n)) => shown(&cell.display, &|| display_exact(n)),
        Some(CellContent::Date(t)) => {
            shown(&cell.display, &|| cell.local_date.clone().unwrap_or_else(|| t.to_rfc3339().unwrap_or_default()))
        }
        Some(CellContent::Commodities(lines)) => {
            let rows: Vec<String> = lines
                .iter()
                .map(|l| {
                    let neg = |negative: bool| if negative { " class=\"neg\"" } else { "" };
                    let native = shown(&l.display, &|| display_exact(&l.monetary.amount));
                    let converted = l.converted.as_ref().map(|c| {
                        format!("<td{}>{}</td>", neg(c.amount.is_negative()), shown(&l.converted_display, &|| display_exact(&c.amount)))
                    });
                    format!("<tr><td{}>{native}</td>{}</tr>", neg(l.monetary.amount.is_negative()), converted.unwrap_or_default())
                })
                .collect();
            format!("<table class=\"commodities\">{}</table>", rows.concat())
        }
        None => String::new(),
    }
}
