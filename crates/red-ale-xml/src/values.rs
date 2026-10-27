//! Codecs for the value encodings shared by every entity: GUIDs, numerics, timestamps, dates, commodity references,
//! booleans and slots. Writers follow `5.17:libgnucash/backend/xml/sixtp-dom-generators.cpp`; readers follow
//! `sixtp-dom-parsers.cpp`.

use red_ale_core::{CommodityRef, Date, Frame, Guid, Numeric, SlotValue, Time64};

use crate::XmlError;
use crate::dom::{Element, Node};

/// Things a reader noticed that make the book unsafe to edit, though it can still be read and saved unchanged.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Notices {
    /// Descriptions of content red-ale cannot represent.
    pub unrecognised: Vec<String>,
}

pub(crate) fn malformed(element: &str, message: impl Into<String>) -> XmlError {
    XmlError::Malformed {
        element: element.to_owned(),
        message: message.into(),
    }
}

/// `guid_to_dom_tree`: `<tag type="guid">hex</tag>`.
pub fn guid_el(tag: &str, guid: Guid) -> Element {
    Element::text(tag, guid.to_string()).attr("type", "guid")
}

/// `text_to_dom_tree`: empty text gives `<tag/>`.
pub fn text_el(tag: &str, text: &str) -> Element {
    Element::text(tag, text)
}

/// `text_to_dom_tree` only when `text` is not empty.
pub fn text_el_nonempty(tag: &str, text: &str) -> Option<Element> {
    (!text.is_empty()).then(|| Element::text(tag, text))
}

/// `int_to_dom_tree`.
pub fn int_el(tag: &str, value: i64) -> Element {
    Element::text(tag, value.to_string())
}

/// `int_to_dom_tree` of a boolean: `0` or `1`.
pub fn bool_int_el(tag: &str, value: bool) -> Element {
    Element::text(tag, if value { "1" } else { "0" })
}

/// `boolean_to_dom_tree`: `TRUE` or `FALSE`.
pub fn bool_word_el(tag: &str, value: bool) -> Element {
    Element::text(tag, if value { "TRUE" } else { "FALSE" })
}

/// `gnc_numeric_to_dom_tree`.
pub fn numeric_el(tag: &str, value: Numeric) -> Element {
    Element::text(tag, value.to_string())
}

/// `time64_to_dom_tree`: `<tag><ts:date>…</ts:date></tag>`.
pub fn time_el(tag: &str, t: Time64) -> Element {
    Element::new(tag).child(Element::text_child("ts:date", t.to_file_string()))
}

/// `gdate_to_dom_tree`: `<tag><gdate>YYYY-MM-DD</gdate></tag>`.
pub fn gdate_el(tag: &str, d: Date) -> Element {
    Element::new(tag).child(Element::text_child("gdate", d.to_file_string()))
}

/// `commodity_ref_to_dom_tree`.
pub fn commodity_ref_el(tag: &str, c: &CommodityRef) -> Element {
    Element::new(tag)
        .child(Element::text_child("cmdty:space", &c.namespace))
        .child(Element::text_child("cmdty:id", &c.mnemonic))
}

/// The text of an element, required.
pub fn text_of(el: &Element) -> String {
    el.text_content()
}

/// Parse a GUID element; GnuCash accepts `type="guid"` or `type="new"`.
pub fn parse_guid(el: &Element) -> Result<Guid, XmlError> {
    match el.get_attr("type") {
        Some("guid" | "new") => {}
        other => return Err(malformed(&el.name, format!("GUID element has type {other:?}"))),
    }
    Guid::parse(&el.text_content()).map_err(|e| malformed(&el.name, e.to_string()))
}

/// Parse an integer element, trimming ASCII whitespace.
pub fn parse_int(el: &Element) -> Result<i64, XmlError> {
    el.text_content()
        .trim()
        .parse()
        .map_err(|_| malformed(&el.name, format!("not an integer: {:?}", el.text_content())))
}

/// Parse a `0`/`1` integer boolean (any non-zero integer is true).
pub fn parse_bool_int(el: &Element) -> Result<bool, XmlError> {
    parse_int(el).map(|v| v != 0)
}

/// Parse a numeric element.
pub fn parse_numeric(el: &Element) -> Result<Numeric, XmlError> {
    Numeric::parse(&el.text_content()).map_err(|e| malformed(&el.name, e.to_string()))
}

/// Parse a `<tag><ts:date>…</ts:date></tag>` element.
pub fn parse_time(el: &Element) -> Result<Time64, XmlError> {
    let ts = el.find("ts:date").ok_or_else(|| malformed(&el.name, "missing ts:date"))?;
    Time64::parse_file_string(&ts.text_content()).map_err(|e| malformed(&el.name, e.to_string()))
}

/// Parse a `<tag><gdate>…</gdate></tag>` element.
pub fn parse_gdate(el: &Element) -> Result<Date, XmlError> {
    let g = el.find("gdate").ok_or_else(|| malformed(&el.name, "missing gdate"))?;
    Date::parse(&g.text_content()).map_err(|e| malformed(&el.name, e.to_string()))
}

/// Parse a commodity reference.
pub fn parse_commodity_ref(el: &Element) -> Result<CommodityRef, XmlError> {
    let space = el.find("cmdty:space").ok_or_else(|| malformed(&el.name, "missing cmdty:space"))?;
    let id = el.find("cmdty:id").ok_or_else(|| malformed(&el.name, "missing cmdty:id"))?;
    Ok(CommodityRef::new(space.text_content(), id.text_content()))
}

/// Write a frame's slots as children of `tag`; `None` when the frame is empty (`qof_instance_slots_to_dom_tree`).
pub fn slots_el(tag: &str, frame: &Frame) -> Option<Element> {
    (!frame.is_empty()).then(|| {
        let mut el = Element::new(tag);
        add_slots(&mut el, frame);
        el
    })
}

fn add_slots(parent: &mut Element, frame: &Frame) {
    frame.iter().for_each(|(key, value)| {
        let mut slot = Element::new("slot");
        slot.push(Element::text_child("slot:key", key));
        slot.push(slot_value_el(value));
        parent.push(slot);
    });
}

/// `add_kvp_value_node`, `5.17:libgnucash/backend/xml/sixtp-dom-generators.cpp:206-311`.
fn slot_value_el(value: &SlotValue) -> Element {
    const TAG: &str = "slot:value";
    match value {
        SlotValue::Integer(i) => Element::text(TAG, i.to_string()).attr("type", "integer"),
        SlotValue::Double(text) => Element::text(TAG, text.as_str()).attr("type", "double"),
        SlotValue::Numeric(n) => Element::text(TAG, n.to_string()).attr("type", "numeric"),
        SlotValue::String(s) => Element::text_child(TAG, s.as_str()).attr("type", "string"),
        SlotValue::Guid(g) => Element::text(TAG, g.to_string()).attr("type", "guid"),
        SlotValue::Timespec(t) => time_el(TAG, *t).attr("type", "timespec"),
        SlotValue::Date(d) => gdate_el(TAG, *d).attr("type", "gdate"),
        SlotValue::List(items) => {
            let mut el = Element::new(TAG).attr("type", "list");
            items.iter().for_each(|v| el.push(slot_value_el(v)));
            el
        }
        SlotValue::Frame(f) => {
            let mut el = Element::new(TAG).attr("type", "frame");
            add_slots(&mut el, f);
            el
        }
    }
}

/// Read the `slot` children of a slots element into a frame. Slots of a type GnuCash does not know are reported in
/// `notices` (GnuCash drops them; red-ale keeps them through the entity's original element and opens read-only).
pub fn parse_slots(el: &Element, notices: &mut Notices) -> Result<Frame, XmlError> {
    let mut frame = Frame::new();
    for slot in el.elements().filter(|e| e.name == "slot") {
        let key = slot
            .find("slot:key")
            .map(Element::text_content)
            .ok_or_else(|| malformed("slot", "missing slot:key"))?;
        let Some(value_el) = slot.find("slot:value") else {
            notices.unrecognised.push(format!("slot {key:?} has no value"));
            continue;
        };
        match parse_slot_value(value_el, notices)? {
            Some(value) => {
                frame.insert_key(key, value);
            }
            None => notices
                .unrecognised
                .push(format!("slot {key:?} has unknown type {:?}", value_el.get_attr("type"))),
        }
    }
    Ok(frame)
}

fn parse_slot_value(el: &Element, notices: &mut Notices) -> Result<Option<SlotValue>, XmlError> {
    let Some(kind) = el.get_attr("type") else { return Ok(None) };
    Ok(Some(match kind {
        "integer" => SlotValue::Integer(parse_int(el)?),
        "double" => SlotValue::Double(el.text_content()),
        "numeric" => SlotValue::Numeric(parse_numeric(el)?),
        "string" => SlotValue::String(el.text_content()),
        "guid" => SlotValue::Guid(Guid::parse(&el.text_content()).map_err(|e| malformed("slot:value", e.to_string()))?),
        "timespec" => SlotValue::Timespec(parse_time(el)?),
        "gdate" => SlotValue::Date(parse_gdate(el)?),
        "list" => SlotValue::List(
            el.elements()
                .filter(|c| c.name == "slot:value")
                .map(|c| parse_slot_value(c, notices))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
        ),
        "frame" => SlotValue::Frame(parse_slots(el, notices)?),
        _ => return Ok(None),
    }))
}

/// Child elements of `el` whose names are not in `known`: what the mapping did not consume.
pub fn unknown_children(el: &Element, known: &[&str]) -> Vec<(Option<String>, Element)> {
    let mut previous: Option<String> = None;
    let mut out = Vec::new();
    for child in &el.children {
        let Node::Element(c) = child else { continue };
        if known.contains(&c.name.as_str()) {
            previous = Some(c.name.clone());
        } else {
            out.push((previous.clone(), c.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::parse_document;

    #[test]
    fn slots_round_trip_every_type() {
        let text = r#"<act:slots>
  <slot>
    <slot:key>a</slot:key>
    <slot:value type="string"></slot:value>
  </slot>
  <slot>
    <slot:key>d</slot:key>
    <slot:value type="gdate">
      <gdate>2024-01-15</gdate>
    </slot:value>
  </slot>
  <slot>
    <slot:key>f</slot:key>
    <slot:value type="frame">
      <slot>
        <slot:key>n</slot:key>
        <slot:value type="numeric">1/3</slot:value>
      </slot>
    </slot:value>
  </slot>
  <slot>
    <slot:key>l</slot:key>
    <slot:value type="list">
      <slot:value type="integer">1</slot:value>
      <slot:value type="double">2.5</slot:value>
    </slot:value>
  </slot>
  <slot>
    <slot:key>t</slot:key>
    <slot:value type="timespec">
      <ts:date>2024-01-15 10:59:00 +0000</ts:date>
    </slot:value>
  </slot>
  <slot>
    <slot:key>z</slot:key>
    <slot:value type="frame"/>
  </slot>
</act:slots>"#;
        let el = parse_document(text).unwrap();
        let mut notices = Notices::default();
        let frame = parse_slots(&el, &mut notices).unwrap();
        assert!(notices.unrecognised.is_empty());
        assert_eq!(slots_el("act:slots", &frame).unwrap().dump(0), text);
    }
}
