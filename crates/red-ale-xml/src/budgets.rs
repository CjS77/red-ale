//! Budgets and their Recurrences in XML.
//!
//! Writers mirror `5.17:libgnucash/backend/xml/gnc-budget-xml-v2.cpp:55-86` and
//! `5.17:libgnucash/backend/xml/gnc-recurrence-xml-v2.cpp:125-154`; readers follow the same files' handlers
//! (`gnc-budget-xml-v2.cpp:91-151`, `gnc-recurrence-xml-v2.cpp:51-122`).

use red_ale_core::budget::{DEFAULT_NAME, DEFAULT_NUM_PERIODS};
use red_ale_core::{Budget, Recurrence, WeekendAdjust};

use crate::XmlError;
use crate::dom::Element;
use crate::entities::report_unknown;
use crate::values::{Notices, gdate_el, guid_el, int_el, malformed, parse_gdate, parse_guid, parse_int, parse_slots, slots_el, text_el};

/// Child elements of `gnc:budget` that the mapping reads.
pub const BUDGET_FIELDS: [&str; 6] = [
    "bgt:id",
    "bgt:name",
    "bgt:description",
    "bgt:num-periods",
    "bgt:recurrence",
    "bgt:slots",
];

/// Child elements of a recurrence that the mapping reads.
pub const RECURRENCE_FIELDS: [&str; 4] = [
    "recurrence:mult",
    "recurrence:period_type",
    "recurrence:start",
    "recurrence:weekend_adj",
];

fn required<'e>(el: &'e Element, name: &str) -> Result<&'e Element, XmlError> {
    el.find(name).ok_or_else(|| malformed(&el.name, format!("missing {name}")))
}

/// An unsigned integer element that must fit `T`, as `dom_tree_to_guint` and `dom_tree_to_guint16` read them.
fn parse_unsigned<T: TryFrom<i64>>(el: &Element) -> Result<T, XmlError> {
    T::try_from(parse_int(el)?).map_err(|_| malformed(&el.name, format!("out of range: {:?}", el.text_content())))
}

/// Read a `gnc:budget`. A missing name or description, or a count of 0, leaves GnuCash's default
/// (`5.17:libgnucash/engine/gnc-budget.cpp:110-114`, `:472`).
pub fn parse_budget(el: &Element, notices: &mut Notices) -> Result<Budget, XmlError> {
    report_unknown(el, &BUDGET_FIELDS, notices);
    let num_periods: u32 = parse_unsigned(required(el, "bgt:num-periods")?)?;
    Ok(Budget {
        guid: parse_guid(required(el, "bgt:id")?)?,
        name: el.find("bgt:name").map_or_else(|| DEFAULT_NAME.to_owned(), Element::text_content),
        description: el.find("bgt:description").map(Element::text_content).unwrap_or_default(),
        num_periods: if num_periods == 0 { DEFAULT_NUM_PERIODS } else { num_periods },
        recurrence: parse_recurrence(required(el, "bgt:recurrence")?, notices)?,
        slots: el
            .find("bgt:slots")
            .map(|s| parse_slots(s, notices))
            .transpose()?
            .unwrap_or_default(),
    })
}

/// `gnc_budget_dom_tree_create`.
pub fn budget_el(b: &Budget) -> Element {
    Element::new("gnc:budget")
        .attr("version", "2.0.0")
        .child(guid_el("bgt:id", b.guid))
        .child(text_el("bgt:name", &b.name))
        .child(text_el("bgt:description", &b.description))
        .child(int_el("bgt:num-periods", i64::from(b.num_periods)))
        .child(recurrence_el("bgt:recurrence", &b.recurrence))
        .child_opt(slots_el("bgt:slots", &b.slots))
}

/// Read a recurrence (`dom_tree_to_recurrence`): a missing weekend adjustment is none.
pub fn parse_recurrence(el: &Element, notices: &mut Notices) -> Result<Recurrence, XmlError> {
    report_unknown(el, &RECURRENCE_FIELDS, notices);
    let bad = |e: red_ale_core::RecurrenceError| malformed(&el.name, e.to_string());
    let weekend_adjust = el
        .find("recurrence:weekend_adj")
        .map(|w| w.text_content().parse())
        .transpose()
        .map_err(bad)?
        .unwrap_or_default();
    Recurrence::new(
        parse_unsigned(required(el, "recurrence:mult")?)?,
        required(el, "recurrence:period_type")?.text_content().parse().map_err(bad)?,
        parse_gdate(required(el, "recurrence:start")?)?,
        weekend_adjust,
    )
    .map_err(bad)
}

/// `recurrence_to_dom_tree`: the weekend adjustment is left out when it is none, so that GnuCash 2.2 can read the file.
pub fn recurrence_el(tag: &str, r: &Recurrence) -> Element {
    let weekend = (r.weekend_adjust() != WeekendAdjust::None).then(|| text_el("recurrence:weekend_adj", r.weekend_adjust().as_str()));
    Element::new(tag)
        .attr("version", "1.0.0")
        .child(int_el("recurrence:mult", i64::from(r.mult())))
        .child(text_el("recurrence:period_type", r.period_type().as_str()))
        .child(gdate_el("recurrence:start", r.start()))
        .child_opt(weekend)
}

#[cfg(test)]
mod tests {
    use red_ale_core::PeriodType;

    use super::*;
    use crate::dom::parse_document;

    fn recurrence_text(mult: u16, period: &str, weekend: Option<&str>) -> String {
        let weekend = weekend.map(|w| format!("\n    <recurrence:weekend_adj>{w}</recurrence:weekend_adj>")).unwrap_or_default();
        format!(
            "  <bgt:recurrence version=\"1.0.0\">
    <recurrence:mult>{mult}</recurrence:mult>
    <recurrence:period_type>{period}</recurrence:period_type>
    <recurrence:start>
      <gdate>2024-01-31</gdate>
    </recurrence:start>{weekend}
  </bgt:recurrence>"
        )
    }

    fn budget_text(recurrence: &str) -> String {
        format!(
            r#"<gnc:budget version="2.0.0">
  <bgt:id type="guid">0123456789abcdef0123456789abcdef</bgt:id>
  <bgt:name>Plan &amp; more</bgt:name>
  <bgt:description/>
  <bgt:num-periods>12</bgt:num-periods>
{recurrence}
  <bgt:slots>
    <slot>
      <slot:key>1feca2abf92347288c268ebf93753ec4</slot:key>
      <slot:value type="frame">
        <slot>
          <slot:key>0</slot:key>
          <slot:value type="numeric">1234/100</slot:value>
        </slot>
        <slot>
          <slot:key>1</slot:key>
          <slot:value type="numeric">0/1</slot:value>
        </slot>
        <slot>
          <slot:key>15</slot:key>
          <slot:value type="numeric">100/1</slot:value>
        </slot>
      </slot:value>
    </slot>
    <slot>
      <slot:key>notes</slot:key>
      <slot:value type="frame">
        <slot>
          <slot:key>1feca2abf92347288c268ebf93753ec4</slot:key>
          <slot:value type="frame">
            <slot>
              <slot:key>0</slot:key>
              <slot:value type="string">January food</slot:value>
            </slot>
            <slot>
              <slot:key>1</slot:key>
              <slot:value type="string"></slot:value>
            </slot>
          </slot:value>
        </slot>
      </slot:value>
    </slot>
    <slot>
      <slot:key>someone-else</slot:key>
      <slot:value type="integer">7</slot:value>
    </slot>
  </bgt:slots>
</gnc:budget>"#
        )
    }

    fn read(text: &str) -> (Budget, Notices) {
        let mut notices = Notices::default();
        let budget = parse_budget(&parse_document(text).unwrap(), &mut notices).unwrap();
        (budget, notices)
    }

    #[test]
    fn budgets_with_every_period_type_regenerate_byte_for_byte() {
        let cases = PeriodType::ALL.iter().zip([None, Some("back"), Some("forward")].iter().cycle());
        cases.for_each(|(period, weekend)| {
            let mult = if *period == PeriodType::Once { 0 } else { 3 };
            let text = budget_text(&recurrence_text(mult, period.as_str(), *weekend));
            let (budget, notices) = read(&text);
            assert!(notices.unrecognised.is_empty(), "{notices:?}");
            assert_eq!(budget.recurrence.period_type(), *period);
            assert_eq!(budget_el(&budget).dump(0), text, "{period}");
        });
    }

    #[test]
    fn amounts_keep_what_was_written() {
        let (budget, _) = read(&budget_text(&recurrence_text(1, "month", None)));
        let food = red_ale_core::Guid::parse("1feca2abf92347288c268ebf93753ec4").unwrap();
        assert_eq!(budget.name, "Plan & more");
        assert!(budget.amount(food, 0).is_some_and(|n| n.same_repr(red_ale_core::Numeric::new(1234, 100))));
        assert!(budget.amount(food, 1).is_some_and(|n| n.same_repr(red_ale_core::Numeric::new(0, 1))));
        assert_eq!(budget.amount(food, 2), None);
        assert_eq!(budget.amount(food, 15), None, "kept, but past the last period");
        assert_eq!(budget.note(food, 0), Some("January food"));
    }

    #[test]
    fn unknown_children_are_reported_and_a_none_weekend_adjustment_is_not_regenerated() {
        let explicit_none = budget_text(&recurrence_text(1, "month", Some("none")));
        let (budget, notices) = read(&explicit_none);
        assert!(notices.unrecognised.is_empty());
        assert_ne!(budget_el(&budget).dump(0), explicit_none, "GnuCash leaves out none");
        let extra = explicit_none.replacen("  <bgt:slots>", "  <bgt:colour>red</bgt:colour>\n  <bgt:slots>", 1);
        assert_eq!(read(&extra).1.unrecognised, ["unknown element bgt:colour in gnc:budget"]);
    }

    #[test]
    fn malformed_recurrences_are_refused() {
        let refused = |r: &str| {
            parse_budget(&parse_document(&budget_text(r)).unwrap(), &mut Notices::default())
                .unwrap_err()
                .to_string()
        };
        assert!(refused(&recurrence_text(1, "fortnight", None)).contains("unknown recurrence period type"));
        assert!(refused(&recurrence_text(0, "month", None)).contains("never repeats"));
        assert!(refused(&recurrence_text(1, "month", Some("sideways"))).contains("unknown weekend adjustment"));
        assert!(refused(&recurrence_text(1, "month", None).replace("<recurrence:mult>1", "<recurrence:mult>65536")).contains("out of range"));
    }
}
