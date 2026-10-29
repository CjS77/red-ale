//! The options summary a report appends when "Add options summary" is `always`: every option whose value differs from
//! its default, as `gnc:html-render-options-changed` lists them (`5.17:gnucash/report/html-utilities.scm:298-345`).

use super::db::{OPTIONS_SUMMARY, OptionDb};
use super::model::{ChoiceKey, DateValue, OptionKind, OptionValue, relative_name};
use crate::number::Rational;
use crate::result::{DisplayContext, OptionsSummaryRow, PlotSize};

impl OptionDb {
    /// The changed options in order, leaving out internal options and the summary option itself, each value as
    /// GnuCash's `disp` shows it.
    pub fn summary(&self, ctx: &DisplayContext<'_>) -> Vec<OptionsSummaryRow> {
        self.options()
            .into_iter()
            .filter(|o| o.is_changed() && o.name() != OPTIONS_SUMMARY && o.kind != OptionKind::Internal)
            .map(|o| OptionsSummaryRow {
                section: o.section().to_owned(),
                name: o.name().to_owned(),
                value: disp(o.value(), ctx),
            })
            .collect()
    }
}

/// GnuCash's `disp`: booleans as Enabled or Disabled, lists joined with `, `, a date as `absolute . <date>` or
/// `relative . <name>`, a commodity by mnemonic, an account by full name, a budget by name, a report placement as
/// its list of `id, wide, high, Disabled`, anything else as Scheme's `display` shows it.
fn disp(value: &OptionValue, ctx: &DisplayContext<'_>) -> String {
    let account = |g: &red_ale_core::Guid| ctx.book.full_name(*g).unwrap_or_else(|| g.to_string());
    let list = |items: Vec<String>| if items.is_empty() { "null".to_owned() } else { items.join(", ") };
    match value {
        OptionValue::Bool(b) => if *b { "Enabled" } else { "Disabled" }.to_owned(),
        OptionValue::Text(s) => s.clone(),
        OptionValue::Key(k) => key(k),
        OptionValue::Keys(keys) => list(keys.iter().map(key).collect()),
        OptionValue::Number(n) => scheme_double(n),
        OptionValue::PlotSize(PlotSize::Percent(n)) => format!("percent . {n}"),
        OptionValue::PlotSize(PlotSize::Pixels(n)) => format!("pixels . {n}"),
        OptionValue::Date(DateValue::Absolute(t)) => {
            let day = ctx.zone.local_date(*t).map_or_else(|| t.0.to_string(), |d| ctx.print_date(d));
            format!("absolute . {day}")
        }
        OptionValue::Date(DateValue::Relative(r)) => format!("relative . {}", relative_name(*r)),
        OptionValue::Accounts(guids) => list(guids.iter().map(account).collect()),
        OptionValue::Account(g) => g.as_ref().map_or_else(|| "null".to_owned(), account),
        OptionValue::Commodity(c) => c.mnemonic.clone(),
        // `gnc-budget-get-name`; no budget is #f, which `disp` shows as a boolean.
        OptionValue::Budget(None) => "Disabled".to_owned(),
        OptionValue::Budget(Some(g)) => ctx.book.budget(*g).map_or_else(|| g.to_string(), |b| b.name.clone()),
        // `disp` has no case for an invoice and shows a pointer that differs at every run
        // (`5.17:gnucash/report/html-utilities.scm:302-323`); red-ale shows the GUID.
        OptionValue::Invoice(g) => g.map_or_else(|| "null".to_owned(), |g| g.to_string()),
        // Each child is `(id wide high #f)` to Scheme (`5.17:bindings/guile/gnc-optiondb.i:304-321`), a list `disp`
        // joins, the report's GUID in place of the instance id.
        OptionValue::Placements(children) => list(
            children
                .iter()
                .map(|p| format!("{}, {}, {}, Disabled", p.report, p.colspan, p.rowspan))
                .collect(),
        ),
    }
}

fn key(k: &ChoiceKey) -> String {
    k.to_string()
}

/// How Scheme displays the double GnuCash holds a number range in: `7.0`, `1.5`.
fn scheme_double(n: &Rational) -> String {
    match n.to_exact_decimal() {
        Some(text) if n.is_integer() => format!("{text}.0"),
        Some(text) => text,
        None => n.to_decimal(15),
    }
}
