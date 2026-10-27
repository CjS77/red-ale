//! Scheduled transactions and template accounts in XML.
//!
//! The writer mirrors `gnc_schedXaction_dom_tree_create` (`5.17:libgnucash/backend/xml/gnc-schedxaction-xml-v2.cpp:80-197`);
//! the reader follows the same file's handlers (`:207-638`) and its template-transaction reader (`:771-822`). Template
//! transactions are ordinary `gnc:transaction`s, read and written by [`crate::entities`].

use red_ale_core::commodity::TEMPLATE_NAMESPACE;
use red_ale_core::{Account, CommodityRef, DeferredInstance, Guid, Occurrences, ScheduledTransaction};

use crate::XmlError;
use crate::budgets::{parse_recurrence, recurrence_el};
use crate::dom::Element;
use crate::entities::{parse_account, report_unknown};
use crate::values::{Notices, gdate_el, guid_el, int_el, malformed, parse_gdate, parse_guid, parse_int, parse_slots, slots_el};

/// Child elements of `gnc:schedxaction` that the mapping reads.
pub const SCHEDULED_FIELDS: [&str; 18] = [
    "sx:id",
    "sx:name",
    "sx:enabled",
    "sx:autoCreate",
    "sx:autoCreateNotify",
    "sx:advanceCreateDays",
    "sx:advanceRemindDays",
    "sx:instanceCount",
    "sx:start",
    "sx:last",
    "sx:num-occur",
    "sx:rem-occur",
    "sx:end",
    "sx:templ-acct",
    "sx:freqspec",
    "sx:schedule",
    "sx:deferredInstance",
    "sx:slots",
];

const DEFERRED_FIELDS: [&str; 3] = ["sx:last", "sx:rem-occur", "sx:instanceCount"];

fn required<'e>(el: &'e Element, name: &str) -> Result<&'e Element, XmlError> {
    el.find(name).ok_or_else(|| malformed(&el.name, format!("missing {name}")))
}

/// An integer GnuCash keeps in a `gint`.
fn parse_i32(el: &Element) -> Result<i32, XmlError> {
    i32::try_from(parse_int(el)?).map_err(|_| malformed(&el.name, format!("out of range: {:?}", el.text_content())))
}

fn optional_i32(el: &Element, name: &str) -> Result<Option<i32>, XmlError> {
    el.find(name).map(parse_i32).transpose()
}

/// `y` is true and anything else false, as the handlers compare (`gnc-schedxaction-xml-v2.cpp:229-258`).
fn parse_yn(el: &Element, name: &str) -> Result<bool, XmlError> {
    required(el, name).map(|e| e.text_content() == "y")
}

fn yn_el(tag: &str, value: bool) -> Element {
    Element::text_child(tag, if value { "y" } else { "n" })
}

/// Read a `gnc:schedxaction`. A legacy `sx:freqspec` is reported, so the book opens read-only: GnuCash 5.17 turns it
/// into recurrences and never writes it (`design/notes/xml-format-core.md` §8.5). The element then keeps its original
/// text, since what is generated has no `sx:freqspec`.
pub fn parse_scheduled(el: &Element, notices: &mut Notices) -> Result<ScheduledTransaction, XmlError> {
    report_unknown(el, &SCHEDULED_FIELDS, notices);
    let name = required(el, "sx:name")?.text_content();
    if el.find("sx:freqspec").is_some() {
        notices.unrecognised.push(format!(
            "scheduled transaction {name:?} has a legacy frequency spec (sx:freqspec), which GnuCash 5.17 rewrites as a schedule"
        ));
    }
    Ok(ScheduledTransaction {
        guid: parse_guid(required(el, "sx:id")?)?,
        enabled: el.find("sx:enabled").is_none_or(|e| e.text_content() == "y"),
        auto_create: parse_yn(el, "sx:autoCreate")?,
        auto_create_notify: parse_yn(el, "sx:autoCreateNotify")?,
        advance_create_days: parse_i32(required(el, "sx:advanceCreateDays")?)?,
        advance_remind_days: parse_i32(required(el, "sx:advanceRemindDays")?)?,
        instance_count: optional_i32(el, "sx:instanceCount")?.unwrap_or(0),
        start: parse_gdate(required(el, "sx:start")?)?,
        last: el.find("sx:last").map(parse_gdate).transpose()?,
        end: el.find("sx:end").map(parse_gdate).transpose()?,
        occurrences: parse_occurrences(el)?,
        template_account: el.find("sx:templ-acct").map(parse_guid).transpose()?,
        schedule: parse_schedule(el, notices)?,
        deferred: el
            .elements()
            .filter(|c| c.name == "sx:deferredInstance")
            .map(|d| parse_deferred(d, notices))
            .collect::<Result<_, _>>()?,
        slots: el.find("sx:slots").map(|s| parse_slots(s, notices)).transpose()?.unwrap_or_default(),
        name,
    })
}

/// `sx:num-occur` sets both counts and `sx:rem-occur` then sets the remaining count (`gnc-schedxaction-xml-v2.cpp:549-604`).
/// A remaining count with no total is kept only in the element's original text.
fn parse_occurrences(el: &Element) -> Result<Option<Occurrences>, XmlError> {
    let Some(total) = optional_i32(el, "sx:num-occur")? else {
        return Ok(None);
    };
    let remaining = optional_i32(el, "sx:rem-occur")?.unwrap_or(total);
    Ok(Some(Occurrences { total, remaining }))
}

fn parse_schedule(el: &Element, notices: &mut Notices) -> Result<Vec<red_ale_core::Recurrence>, XmlError> {
    let Some(schedule) = el.find("sx:schedule") else {
        return Ok(Vec::new());
    };
    report_unknown(schedule, &["gnc:recurrence"], notices);
    schedule
        .elements()
        .filter(|r| r.name == "gnc:recurrence")
        .map(|r| parse_recurrence(r, notices))
        .collect()
}

/// GnuCash requires all three children (`gnc-schedxaction-xml-v2.cpp:509-515`) although it writes no `sx:last` for
/// an invalid date; red-ale reads one without it.
fn parse_deferred(el: &Element, notices: &mut Notices) -> Result<DeferredInstance, XmlError> {
    report_unknown(el, &DEFERRED_FIELDS, notices);
    Ok(DeferredInstance {
        last: el.find("sx:last").map(parse_gdate).transpose()?,
        remaining: parse_i32(required(el, "sx:rem-occur")?)?,
        instance_count: parse_i32(required(el, "sx:instanceCount")?)?,
    })
}

/// `gnc_schedXaction_dom_tree_create`: an occurrence count wins over an end date.
pub fn scheduled_el(sx: &ScheduledTransaction) -> Element {
    let limit = match (sx.occurrences.filter(|o| o.total != 0), sx.end) {
        (Some(o), _) => vec![
            int_el("sx:num-occur", i64::from(o.total)),
            int_el("sx:rem-occur", i64::from(o.remaining)),
        ],
        (None, Some(end)) => vec![gdate_el("sx:end", end)],
        (None, None) => Vec::new(),
    };
    let mut schedule = Element::new("sx:schedule");
    sx.schedule.iter().for_each(|r| schedule.push(recurrence_el("gnc:recurrence", r)));
    let mut el = Element::new("gnc:schedxaction")
        .attr("version", "2.0.0")
        .child(guid_el("sx:id", sx.guid))
        .child(Element::text_child("sx:name", &sx.name))
        .child(yn_el("sx:enabled", sx.enabled))
        .child(yn_el("sx:autoCreate", sx.auto_create))
        .child(yn_el("sx:autoCreateNotify", sx.auto_create_notify))
        .child(int_el("sx:advanceCreateDays", i64::from(sx.advance_create_days)))
        .child(int_el("sx:advanceRemindDays", i64::from(sx.advance_remind_days)))
        .child(int_el("sx:instanceCount", i64::from(sx.instance_count)))
        .child(gdate_el("sx:start", sx.start))
        .child_opt(sx.last.map(|d| gdate_el("sx:last", d)));
    limit.into_iter().for_each(|c| el.push(c));
    el = el.child_opt(sx.template_account.map(|g| guid_el("sx:templ-acct", g))).child(schedule);
    sx.deferred.iter().for_each(|d| el.push(deferred_el(d)));
    el.child_opt(slots_el("sx:slots", &sx.slots))
}

fn deferred_el(d: &DeferredInstance) -> Element {
    Element::new("sx:deferredInstance")
        .child_opt(d.last.map(|l| gdate_el("sx:last", l)))
        .child(int_el("sx:rem-occur", i64::from(d.remaining)))
        .child(int_el("sx:instanceCount", i64::from(d.instance_count)))
}

/// Read one `gnc:account` of `gnc:template-transactions`, as `tt_act_handler` and `add_template_transaction_local` do
/// (`gnc-schedxaction-xml-v2.cpp:771-822`, `5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:303-343`): an account with no
/// commodity gets `template:template`, a parentless ROOT becomes the template root, and a parentless account of any
/// other type goes under the template root read so far. `root` is that template root.
///
/// What GnuCash would lose or move out of sight is reported, so the book opens read-only: lots in a template account,
/// a second template root (GnuCash drops the first one's tree from the file), and a parentless account read before any
/// template root (GnuCash puts it under a template root of its own, with a new GUID).
pub fn parse_template_account(el: &Element, root: &mut Option<Guid>, notices: &mut Notices) -> Result<Account, XmlError> {
    let (mut account, lots) = parse_account(el, notices)?;
    let guid = account.guid;
    if !lots.is_empty() {
        notices.unrecognised.push(format!("template account {guid} holds lots"));
    }
    if account.commodity.is_none() {
        // `xaccAccountSetCommodity` also sets the SCU to the commodity's fraction (`5.17:libgnucash/engine/Account.cpp:2683-2684`).
        account.commodity = Some(CommodityRef::new(TEMPLATE_NAMESPACE, "template"));
        account.commodity_scu = Some(1);
        account.non_standard_scu = false;
    }
    match (account.parent, account.account_type) {
        (Some(_), _) => {}
        (None, red_ale_core::AccountType::Root) => {
            if root.replace(guid).is_some() {
                notices.unrecognised.push(format!("a second template root {guid}"));
            }
        }
        (None, _) => match *root {
            Some(r) => account.parent = Some(r),
            None => notices.unrecognised.push(format!("template account {guid} has no parent and no template root")),
        },
    }
    Ok(account)
}

#[cfg(test)]
mod tests {
    use red_ale_core::{Date, Numeric, PeriodType, SlotValue};

    use super::*;
    use crate::dom::parse_document;
    use crate::entities::{account_el, parse_transaction, transaction_el};

    const SX: &str = r#"<gnc:schedxaction version="2.0.0">
  <sx:id type="guid">52f96837087e9670c933f9421b0ac626</sx:id>
  <sx:name>Rent &amp; rates</sx:name>
  <sx:enabled>n</sx:enabled>
  <sx:autoCreate>y</sx:autoCreate>
  <sx:autoCreateNotify>y</sx:autoCreateNotify>
  <sx:advanceCreateDays>3</sx:advanceCreateDays>
  <sx:advanceRemindDays>-2</sx:advanceRemindDays>
  <sx:instanceCount>7</sx:instanceCount>
  <sx:start>
    <gdate>2024-01-31</gdate>
  </sx:start>
  <sx:last>
    <gdate>2024-05-31</gdate>
  </sx:last>
LIMIT
  <sx:templ-acct type="guid">22f2905cbcfa0b910c0dbc348cfc1161</sx:templ-acct>
  <sx:schedule>
    <gnc:recurrence version="1.0.0">
      <recurrence:mult>1</recurrence:mult>
      <recurrence:period_type>end of month</recurrence:period_type>
      <recurrence:start>
        <gdate>2024-01-31</gdate>
      </recurrence:start>
      <recurrence:weekend_adj>back</recurrence:weekend_adj>
    </gnc:recurrence>
    <gnc:recurrence version="1.0.0">
      <recurrence:mult>2</recurrence:mult>
      <recurrence:period_type>week</recurrence:period_type>
      <recurrence:start>
        <gdate>2024-01-03</gdate>
      </recurrence:start>
    </gnc:recurrence>
  </sx:schedule>
  <sx:deferredInstance>
    <sx:last>
      <gdate>2024-03-31</gdate>
    </sx:last>
    <sx:rem-occur>9</sx:rem-occur>
    <sx:instanceCount>4</sx:instanceCount>
  </sx:deferredInstance>
  <sx:deferredInstance>
    <sx:rem-occur>8</sx:rem-occur>
    <sx:instanceCount>5</sx:instanceCount>
  </sx:deferredInstance>
  <sx:slots>
    <slot>
      <slot:key>note</slot:key>
      <slot:value type="string">kept</slot:value>
    </slot>
  </sx:slots>
</gnc:schedxaction>"#;

    const COUNT: &str = "  <sx:num-occur>12</sx:num-occur>\n  <sx:rem-occur>7</sx:rem-occur>";
    const END: &str = "  <sx:end>\n    <gdate>2025-01-31</gdate>\n  </sx:end>";

    fn read(text: &str) -> (ScheduledTransaction, Notices) {
        let mut notices = Notices::default();
        let sx = parse_scheduled(&parse_document(text).unwrap(), &mut notices).unwrap();
        (sx, notices)
    }

    fn regenerates(text: &str) -> ScheduledTransaction {
        let (sx, notices) = read(text);
        assert!(notices.unrecognised.is_empty(), "{notices:?}");
        assert_eq!(scheduled_el(&sx).dump(0), text);
        sx
    }

    #[test]
    fn every_field_regenerates_byte_for_byte() {
        let sx = regenerates(&SX.replace("LIMIT", COUNT));
        assert_eq!(sx.name, "Rent & rates");
        assert!(!sx.enabled && sx.auto_create && sx.auto_create_notify);
        assert_eq!((sx.advance_create_days, sx.advance_remind_days, sx.instance_count), (3, -2, 7));
        assert_eq!(sx.occurrences, Some(Occurrences { total: 12, remaining: 7 }));
        assert_eq!(sx.end, None);
        let periods: Vec<PeriodType> = sx.schedule.iter().map(red_ale_core::Recurrence::period_type).collect();
        assert_eq!(periods, [PeriodType::EndOfMonth, PeriodType::Week]);
        let deferred = DeferredInstance {
            last: Date::from_ymd(2024, 3, 31),
            remaining: 9,
            instance_count: 4,
        };
        assert_eq!(sx.deferred, [deferred, DeferredInstance { last: None, remaining: 8, instance_count: 5 }]);
        assert_eq!(sx.slots.get_key("note"), Some(&SlotValue::String("kept".into())));
        let sx = regenerates(&SX.replace("LIMIT", END));
        assert_eq!((sx.end, sx.occurrences), (Date::from_ymd(2025, 1, 31), None));
    }

    #[test]
    fn optional_parts_regenerate_byte_for_byte() {
        let schedule_start = SX.find("  <sx:schedule>").unwrap();
        let schedule_end = SX.find("  <sx:deferredInstance>").unwrap();
        let bare = format!("{}  <sx:schedule/>\n{}", &SX[..schedule_start], &SX[schedule_end..]);
        let bare = bare.replace("LIMIT\n", "").replace("  <sx:last>\n    <gdate>2024-05-31</gdate>\n  </sx:last>\n", "");
        let deferred = bare.find("  <sx:deferredInstance>").unwrap();
        let bare = format!("{}</gnc:schedxaction>", &bare[..deferred]).replace("<sx:name>Rent &amp; rates", "<sx:name>");
        let sx = regenerates(&bare);
        assert!(sx.schedule.is_empty() && sx.deferred.is_empty() && sx.slots.is_empty() && sx.last.is_none());
        assert_eq!(sx.name, "");
    }

    #[test]
    fn a_book_with_both_an_end_and_a_count_keeps_both() {
        let (sx, _) = read(&SX.replace("LIMIT", &format!("{COUNT}\n{END}")));
        assert_eq!(sx.end, Date::from_ymd(2025, 1, 31));
        assert_eq!(sx.occurrences, Some(Occurrences { total: 12, remaining: 7 }));
        assert_eq!(scheduled_el(&sx).dump(0), SX.replace("LIMIT", COUNT), "GnuCash writes only the count");
    }

    #[test]
    fn a_legacy_frequency_spec_is_reported_and_unknown_children_too() {
        let freqspec = SX
            .replace("LIMIT", END)
            .replace("version=\"2.0.0\"", "version=\"1.0.0\"")
            .replacen("  <sx:schedule>", "  <sx:freqspec>\n    <gnc:freqspec version=\"1.0.0\"/>\n  </sx:freqspec>\n  <sx:schedule>", 1);
        let (sx, notices) = read(&freqspec);
        assert_eq!(notices.unrecognised.len(), 1);
        assert!(notices.unrecognised[0].contains("legacy frequency spec"), "{notices:?}");
        assert_ne!(scheduled_el(&sx).dump(0), freqspec);
        let extra = SX.replace("LIMIT", END).replacen("  <sx:slots>", "  <sx:colour>red</sx:colour>\n  <sx:slots>", 1);
        assert_eq!(read(&extra).1.unrecognised, ["unknown element sx:colour in gnc:schedxaction"]);
    }

    #[test]
    fn out_of_range_counts_are_refused() {
        let text = SX.replace("LIMIT", END).replace("<sx:instanceCount>7", "<sx:instanceCount>2147483648");
        let error = parse_scheduled(&parse_document(&text).unwrap(), &mut Notices::default()).unwrap_err();
        assert!(error.to_string().contains("out of range"), "{error}");
    }

    const FORMULA_TRANSACTION: &str = r#"<gnc:transaction version="2.0.0">
  <trn:id type="guid">68bf5e7740392eb15a079ee99f18a357</trn:id>
  <trn:currency>
    <cmdty:space>CURRENCY</cmdty:space>
    <cmdty:id>EUR</cmdty:id>
  </trn:currency>
  <trn:date-posted>
    <ts:date>2024-08-01 10:59:00 +0000</ts:date>
  </trn:date-posted>
  <trn:date-entered>
    <ts:date>2024-08-01 08:40:17 +0000</ts:date>
  </trn:date-entered>
  <trn:description>Loan</trn:description>
  <trn:splits>
    <trn:split>
      <split:id type="guid">676a518eb1cf05ca552434533288a5cd</split:id>
      <split:memo>interest</split:memo>
      <split:reconciled-state>n</split:reconciled-state>
      <split:value>0/100</split:value>
      <split:quantity>0/1</split:quantity>
      <split:account type="guid">22f2905cbcfa0b910c0dbc348cfc1161</split:account>
      <split:slots>
        <slot>
          <slot:key>sched-xaction</slot:key>
          <slot:value type="frame">
            <slot>
              <slot:key>account</slot:key>
              <slot:value type="guid">1feca2abf92347288c268ebf93753ec4</slot:value>
            </slot>
            <slot>
              <slot:key>credit-formula</slot:key>
              <slot:value type="string"></slot:value>
            </slot>
            <slot>
              <slot:key>credit-numeric</slot:key>
              <slot:value type="numeric">0/1</slot:value>
            </slot>
            <slot>
              <slot:key>debit-formula</slot:key>
              <slot:value type="string">ipmt( rate / 12 : i : 360 : -principal : 0 : 0 )</slot:value>
            </slot>
            <slot>
              <slot:key>debit-numeric</slot:key>
              <slot:value type="numeric">0/1</slot:value>
            </slot>
            <slot>
              <slot:key>shares</slot:key>
              <slot:value type="string">(x + y)/42</slot:value>
            </slot>
          </slot:value>
        </slot>
      </split:slots>
    </trn:split>
  </trn:splits>
</gnc:transaction>"#;

    #[test]
    fn template_transactions_with_formulas_regenerate_byte_for_byte() {
        let mut notices = Notices::default();
        let t = parse_transaction(&parse_document(FORMULA_TRANSACTION).unwrap(), &mut notices).unwrap();
        assert!(notices.unrecognised.is_empty());
        assert_eq!(transaction_el(&t).dump(0), FORMULA_TRANSACTION);
        let split = red_ale_core::TemplateSplit::of(&t.splits[0]).unwrap();
        assert_eq!(split.debit_formula(), Some("ipmt( rate / 12 : i : 360 : -principal : 0 : 0 )"));
        assert_eq!((split.credit_formula(), split.debit_numeric()), (Some(""), Some(Numeric::new(0, 1))));
        assert_eq!(split.shares(), Some("(x + y)/42"));
        assert!(t.splits[0].value.same_repr(Numeric::new(0, 100)) && t.splits[0].amount.same_repr(Numeric::new(0, 1)));
    }

    fn template_account(name: &str, account_type: &str, parent: Option<&str>, commodity: bool) -> String {
        let commodity = if commodity {
            concat!(
                "  <act:commodity>\n    <cmdty:space>template</cmdty:space>\n    <cmdty:id>template</cmdty:id>\n",
                "  </act:commodity>\n  <act:commodity-scu>1</act:commodity-scu>\n"
            )
        } else {
            ""
        };
        let parent = parent.map(|p| format!("  <act:parent type=\"guid\">{p}</act:parent>\n")).unwrap_or_default();
        let guid = format!("{name:0>32}");
        let head = format!("<gnc:account version=\"2.0.0\">\n  <act:name>{name}</act:name>\n  <act:id type=\"guid\">{guid}</act:id>\n");
        format!("{head}  <act:type>{account_type}</act:type>\n{commodity}{parent}</gnc:account>")
    }

    #[test]
    fn template_accounts_follow_gnucash_s_reader() {
        let mut notices = Notices::default();
        let mut root = None;
        let mut read =
            |text: &str, root: &mut Option<Guid>| parse_template_account(&parse_document(text).unwrap(), root, &mut notices).unwrap();
        let orphan = read(&template_account("a1", "BANK", None, true), &mut root);
        assert_eq!(orphan.parent, None);
        let r = read(&template_account("b2", "ROOT", None, true), &mut root);
        assert_eq!(root, Some(r.guid));
        let bare = template_account("c3", "BANK", None, false);
        let child = read(&bare, &mut root);
        assert_eq!(child.parent, Some(r.guid), "a parentless account goes under the template root");
        assert_eq!(child.commodity, Some(CommodityRef::new("template", "template")));
        assert_eq!(account_el(&child, &[]).dump(0), template_account("c3", "BANK", Some(&r.guid.to_string()), true));
        read(&template_account("d4", "ROOT", None, true), &mut root);
        assert_eq!(notices.unrecognised.len(), 2, "{notices:?}");
        assert!(notices.unrecognised[0].contains("no parent and no template root"));
        assert!(notices.unrecognised[1].contains("a second template root"));
    }
}
