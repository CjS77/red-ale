//! red-ale's base stylesheet (`design/reports.md` §8, "The look"), with the style sheet's options filled in.
//!
//! Class names follow GnuCash's (`number-cell`, `total-label-cell`, `neg`, `alternate-row`, `primary-subheading`, …;
//! `5.17:gnucash/report/stylesheets/plain.scm:104-167`), so CSS written for GnuCash's CSS template styles red-ale's
//! pages too.

use super::look::Look;

/// A rule, or nothing when a colour is unset.
fn background(selector: &str, color: Option<&String>) -> Option<String> {
    color.map(|c| format!("{selector}{{background-color:{c}}}"))
}

/// The page's `<style>` text.
pub(super) fn stylesheet(look: &Look) -> String {
    let c = &look.colors;
    let f = &look.fonts;
    let neg = if look.negative_red { "color:#c0392b;" } else { "" };
    let cell_border = if look.border == "0px" {
        "none".to_owned()
    } else {
        "1px solid #d0d7de".to_owned()
    };
    let rules = [
        Some(":root{color-scheme:light}".to_owned()),
        Some(format!(
            "body{{margin:0;padding:24px 32px;background-color:{};color:{};line-height:1.35;{}}}",
            c.background.as_deref().unwrap_or("#ffffff"),
            c.text.as_deref().unwrap_or("#1f2328"),
            f.text_cell
        )),
        Some(".report{max-width:1200px;margin:0 auto}".to_owned()),
        Some(format!("h1.report-title{{margin:0 0 4px;{}}}", f.title)),
        Some(".masthead{margin:0 0 20px;color:#57606a}.masthead p,.colophon p{margin:2px 0}".to_owned()),
        Some(".colophon{margin:24px 0 0;padding-top:8px;border-top:1px solid #d0d7de;color:#57606a}".to_owned()),
        Some("section{margin:0 0 28px}section>h2{margin:0 0 10px;font-size:1.15em;font-weight:600}".to_owned()),
        Some("h3.text-heading{margin:16px 0 6px;font-size:1.05em;font-weight:600}p.note{color:#57606a;font-size:.92em}".to_owned()),
        Some(
            ".warning{margin:12px 0;padding:10px 14px;border-left:4px solid #d4a72c;background-color:#fff8c5}\
             .warning h3{margin:0 0 4px;font-size:1em}.warning p{margin:0}"
                .to_owned(),
        ),
        Some(".table-wrap{overflow-x:auto;margin:0 0 16px}".to_owned()),
        Some(format!(
            "table.report-table{{border-collapse:separate;border-spacing:{};border:{} solid #8c959f;{}}}",
            look.spacing,
            look.border,
            if look.border == "0px" { "border-style:none;" } else { "" }
        )),
        Some("table.report-table caption{text-align:left;font-weight:600;padding:0 0 6px}".to_owned()),
        Some(format!(
            "table.report-table td,table.report-table th{{padding:{};border:{cell_border};vertical-align:top}}",
            look.padding
        )),
        Some(format!(
            "th{{font-weight:600;border-bottom:1px solid #8c959f !important;{}}}",
            f.number_header
        )),
        Some(
            "th.column-heading-left{text-align:left}th.column-heading-center{text-align:center}\
             th.column-heading-right{text-align:right}"
                .to_owned(),
        ),
        Some(format!(
            "td.text-cell{{{}}}td.label-cell{{padding-left:calc({} + var(--depth,0) * 1.25em)}}",
            f.text_cell, look.padding
        )),
        Some(format!(
            "td.number-cell,td.total-number-cell{{text-align:right;white-space:nowrap;font-variant-numeric:tabular-nums}}\
             td.number-cell{{{}}}td.total-number-cell{{{}}}td.total-label-cell{{{}}}td.centered-label-cell{{text-align:center;{}}}",
            f.number_cell, f.total_number, f.total_label, f.centered_label
        )),
        Some(
            "td.date-cell{white-space:nowrap}td.align-left{text-align:left}td.align-right{text-align:right}\
             td.align-center{text-align:center}"
                .to_owned(),
        ),
        Some(format!(".neg{{{neg}}}")),
        Some(format!(
            ".link{{{}{}}}",
            c.link.as_deref().map(|l| format!("color:{l};")).unwrap_or_default(),
            f.account_link
        )),
        Some(
            "tr.role-heading td{font-weight:600}tr.role-spacer td{height:.8em}\
             tr.role-rule hr{border:0;border-top:1px solid #8c959f;margin:0}"
                .to_owned(),
        ),
        Some("tr.role-subtotal td.total-number-cell{border-top:1px solid #57606a}".to_owned()),
        Some("tr.role-total td.total-number-cell,tr.role-grandTotal td.total-number-cell{border-top:3px double #24292f}".to_owned()),
        Some("td.rule-above{border-top:1px solid #24292f !important}".to_owned()),
        Some(
            "table.commodities{border-collapse:collapse;margin-left:auto}\
             table.commodities td{padding:0 0 0 12px;border:none;text-align:right}"
                .to_owned(),
        ),
        background("tr.normal-row", c.cell.as_ref()),
        background("tr.alternate-row", c.alternate.as_ref()),
        background("tr.primary-subheading", c.subheading.as_ref()),
        background("tr.secondary-subheading", c.subsubheading.as_ref()),
        background("tr.grand-total", c.grand_total.as_ref()),
        Some(".group{display:grid;grid-template-columns:repeat(auto-fit,minmax(22rem,1fr));gap:24px;align-items:start}".to_owned()),
        Some(
            "figure.chart{margin:0 0 16px}svg.chart{display:block;max-width:100%;font-family:inherit}\
             svg.chart text{fill:#24292f}"
                .to_owned(),
        ),
        Some("table.options-summary td{padding:2px 12px 2px 0}".to_owned()),
        Some(
            "@media print{body{padding:0;background:none;color:#000}tr,td,th{background:none !important}\
             table,figure,.warning{break-inside:avoid}}"
                .to_owned(),
        ),
        look.css.clone(),
    ];
    rules.into_iter().flatten().collect::<Vec<_>>().join("\n")
}
