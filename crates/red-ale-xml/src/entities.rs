//! Core entities in XML: commodities, prices, accounts with their lots, and transactions with their splits.
//!
//! Writers mirror `5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:57-112`, `gnc-pricedb-xml-v2.cpp`,
//! `gnc-account-xml-v2.cpp:74-160`, `gnc-lot-xml-v2.cpp` and `gnc-transaction-xml-v2.cpp`.

use red_ale_core::currencies::iso_currency;
use red_ale_core::{Account, AccountType, Commodity, CommodityRef, Guid, Lot, Price, ReconcileState, Split, Time64, Transaction};

use crate::XmlError;
use crate::dom::Element;
use crate::values::{
    Notices, commodity_ref_el, guid_el, int_el, malformed, numeric_el, parse_commodity_ref, parse_guid, parse_int, parse_numeric,
    parse_slots, parse_time, slots_el, text_el, time_el, unknown_children,
};

const VERSION: &str = "2.0.0";

/// Child elements of `gnc:commodity` that the mapping reads.
pub const COMMODITY_FIELDS: [&str; 9] = [
    "cmdty:space",
    "cmdty:id",
    "cmdty:name",
    "cmdty:xcode",
    "cmdty:fraction",
    "cmdty:get_quotes",
    "cmdty:quote_source",
    "cmdty:quote_tz",
    "cmdty:slots",
];

/// Read a `gnc:commodity`. A currency starts from the built-in ISO entry and takes the file's quote settings.
pub fn parse_commodity(el: &Element, notices: &mut Notices) -> Result<Commodity, XmlError> {
    let text = |name: &str| el.find(name).map(Element::text_content);
    let space = text("cmdty:space").ok_or_else(|| malformed(&el.name, "missing cmdty:space"))?;
    let id = text("cmdty:id").ok_or_else(|| malformed(&el.name, "missing cmdty:id"))?;
    let id = CommodityRef::new(space, id);
    let mut c = match iso_currency(&id.mnemonic).filter(|_| id.is_currency()) {
        Some(iso) => iso.to_commodity(),
        None => Commodity::new(id, None, 10_000),
    };
    if let Some(name) = text("cmdty:name") {
        c.fullname = Some(name);
    }
    if let Some(xcode) = text("cmdty:xcode") {
        c.cusip = Some(xcode);
    }
    if let Some(fraction) = el.find("cmdty:fraction") {
        c.fraction = parse_int(fraction)?;
    }
    c.quote_flag = el.find("cmdty:get_quotes").is_some();
    c.quote_source = text("cmdty:quote_source");
    c.quote_tz = text("cmdty:quote_tz");
    if let Some(slots) = el.find("cmdty:slots") {
        c.slots = parse_slots(slots, notices)?;
    }
    if c.fraction == 0 {
        return Err(malformed(&el.name, "fraction is zero"));
    }
    report_unknown(el, &COMMODITY_FIELDS, notices);
    Ok(c)
}

/// Whether GnuCash writes this commodity at all: currencies only with quotes on or slots.
/// `5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:68`.
pub fn commodity_is_written(c: &Commodity) -> bool {
    !c.is_currency() || c.quote_flag || !c.slots.is_empty()
}

/// `gnc_commodity_dom_tree_create`. Returns `None` for a currency GnuCash would not write.
pub fn commodity_el(c: &Commodity) -> Option<Element> {
    if !commodity_is_written(c) {
        return None;
    }
    let mut el = Element::new("gnc:commodity")
        .attr("version", VERSION)
        .child(text_el("cmdty:space", &c.id.namespace))
        .child(text_el("cmdty:id", &c.id.mnemonic));
    if !c.is_currency() {
        el = el
            .child_opt(c.fullname.as_deref().map(|n| text_el("cmdty:name", n)))
            .child_opt(c.cusip.as_deref().filter(|x| !x.is_empty()).map(|x| text_el("cmdty:xcode", x)))
            .child(int_el("cmdty:fraction", c.fraction));
    }
    if c.quote_flag {
        el.push(Element::new("cmdty:get_quotes"));
        let source = c.quote_source.as_deref().or(c.is_currency().then_some("currency"));
        el = el
            .child_opt(source.map(|s| text_el("cmdty:quote_source", s)))
            .child_opt(c.quote_tz.as_deref().map(|tz| text_el("cmdty:quote_tz", tz)));
    }
    Some(el.child_opt(slots_el("cmdty:slots", &c.slots)))
}

/// Child elements of `price` that the mapping reads.
pub const PRICE_FIELDS: [&str; 7] = [
    "price:id",
    "price:commodity",
    "price:currency",
    "price:time",
    "price:source",
    "price:type",
    "price:value",
];

/// Read a `price` element of the price database.
pub fn parse_price(el: &Element, notices: &mut Notices) -> Result<Price, XmlError> {
    let req = |name: &str| el.find(name).ok_or_else(|| malformed(&el.name, format!("missing {name}")));
    report_unknown(el, &PRICE_FIELDS, notices);
    Ok(Price {
        guid: parse_guid(req("price:id")?)?,
        commodity: parse_commodity_ref(req("price:commodity")?)?,
        currency: parse_commodity_ref(req("price:currency")?)?,
        time: parse_time(req("price:time")?)?,
        source: el.find("price:source").map(Element::text_content),
        price_type: el.find("price:type").map(Element::text_content),
        value: parse_numeric(req("price:value")?)?,
    })
}

/// `gnc_price_to_dom_tree`.
pub fn price_el(p: &Price) -> Element {
    Element::new("price")
        .child(guid_el("price:id", p.guid))
        .child(commodity_ref_el("price:commodity", &p.commodity))
        .child(commodity_ref_el("price:currency", &p.currency))
        .child(time_el("price:time", p.time))
        .child_opt(p.source.as_deref().filter(|s| !s.is_empty()).map(|s| text_el("price:source", s)))
        .child_opt(p.price_type.as_deref().filter(|s| !s.is_empty()).map(|s| text_el("price:type", s)))
        .child(numeric_el("price:value", p.value))
}

/// Child elements of `gnc:account` that the mapping reads.
pub const ACCOUNT_FIELDS: [&str; 11] = [
    "act:name",
    "act:id",
    "act:type",
    "act:commodity",
    "act:commodity-scu",
    "act:non-standard-scu",
    "act:code",
    "act:description",
    "act:slots",
    "act:parent",
    "act:lots",
];

/// Read a `gnc:account` and the lots inside it.
pub fn parse_account(el: &Element, notices: &mut Notices) -> Result<(Account, Vec<Lot>), XmlError> {
    let req = |name: &str| el.find(name).ok_or_else(|| malformed(&el.name, format!("missing {name}")));
    report_unknown(el, &ACCOUNT_FIELDS, notices);
    let type_text = req("act:type")?.text_content();
    let account_type: AccountType = type_text
        .parse()
        .map_err(|_| malformed("act:type", format!("unknown account type {type_text:?}")))?;
    let guid = parse_guid(req("act:id")?)?;
    let account = Account {
        guid,
        name: req("act:name")?.text_content(),
        account_type,
        commodity: el.find("act:commodity").map(parse_commodity_ref).transpose()?,
        commodity_scu: el.find("act:commodity-scu").map(parse_int).transpose()?,
        non_standard_scu: el.find("act:non-standard-scu").is_some(),
        code: el.find("act:code").map(Element::text_content),
        description: el.find("act:description").map(Element::text_content),
        slots: el
            .find("act:slots")
            .map(|s| parse_slots(s, notices))
            .transpose()?
            .unwrap_or_default(),
        parent: el.find("act:parent").map(parse_guid).transpose()?,
    };
    let lots = el
        .find("act:lots")
        .map(|lots| lots.elements().map(|l| parse_lot(l, guid, notices)).collect::<Result<Vec<_>, _>>())
        .transpose()?
        .unwrap_or_default();
    Ok((account, lots))
}

fn parse_lot(el: &Element, account: Guid, notices: &mut Notices) -> Result<Lot, XmlError> {
    if el.name != "gnc:lot" {
        return Err(malformed(&el.name, "unexpected element in act:lots"));
    }
    report_unknown(el, &["lot:id", "lot:slots"], notices);
    let id = el.find("lot:id").ok_or_else(|| malformed("gnc:lot", "missing lot:id"))?;
    Ok(Lot {
        guid: parse_guid(id)?,
        account,
        slots: el
            .find("lot:slots")
            .map(|s| parse_slots(s, notices))
            .transpose()?
            .unwrap_or_default(),
    })
}

/// `gnc_lot_dom_tree_create`.
pub fn lot_el(lot: &Lot) -> Element {
    Element::new("gnc:lot")
        .attr("version", VERSION)
        .child(guid_el("lot:id", lot.guid))
        .child_opt(slots_el("lot:slots", &lot.slots))
}

/// `gnc_account_dom_tree_create`. `lots` must be this account's lots; they are written sorted by GUID.
pub fn account_el(a: &Account, lots: &[&Lot]) -> Element {
    let mut el = Element::new("gnc:account")
        .attr("version", VERSION)
        .child(text_el("act:name", &a.name))
        .child(guid_el("act:id", a.guid))
        .child(text_el("act:type", a.account_type.as_str()));
    if let Some(c) = &a.commodity {
        el = el
            .child(commodity_ref_el("act:commodity", c))
            .child(int_el("act:commodity-scu", a.commodity_scu.unwrap_or(0)));
        if a.non_standard_scu {
            el.push(Element::new("act:non-standard-scu"));
        }
    }
    el = el
        .child_opt(a.code.as_deref().filter(|s| !s.is_empty()).map(|s| text_el("act:code", s)))
        .child_opt(
            a.description
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| text_el("act:description", s)),
        )
        .child_opt(slots_el("act:slots", &a.slots))
        .child_opt(a.parent.map(|p| guid_el("act:parent", p)));
    if !lots.is_empty() {
        let mut sorted: Vec<&&Lot> = lots.iter().collect();
        sorted.sort_by_key(|l| l.guid);
        let mut lots_el = Element::new("act:lots");
        sorted.into_iter().for_each(|l| lots_el.push(lot_el(l)));
        el.push(lots_el);
    }
    el
}

/// Child elements of `gnc:transaction` that the mapping reads.
pub const TRANSACTION_FIELDS: [&str; 8] = [
    "trn:id",
    "trn:currency",
    "trn:num",
    "trn:date-posted",
    "trn:date-entered",
    "trn:description",
    "trn:slots",
    "trn:splits",
];

const SPLIT_FIELDS: [&str; 10] = [
    "split:id",
    "split:memo",
    "split:action",
    "split:reconciled-state",
    "split:reconcile-date",
    "split:value",
    "split:quantity",
    "split:account",
    "split:lot",
    "split:slots",
];

/// Read a `gnc:transaction` with its splits.
pub fn parse_transaction(el: &Element, notices: &mut Notices) -> Result<Transaction, XmlError> {
    let req = |name: &str| el.find(name).ok_or_else(|| malformed(&el.name, format!("missing {name}")));
    report_unknown(el, &TRANSACTION_FIELDS, notices);
    let splits = el
        .find("trn:splits")
        .map(|s| s.elements().map(|sp| parse_split(sp, notices)).collect::<Result<Vec<_>, _>>())
        .transpose()?
        .unwrap_or_default();
    Ok(Transaction {
        guid: parse_guid(req("trn:id")?)?,
        currency: parse_commodity_ref(req("trn:currency")?)?,
        num: el.find("trn:num").map(Element::text_content),
        date_posted: el.find("trn:date-posted").map(parse_time).transpose()?.unwrap_or_default(),
        date_entered: el.find("trn:date-entered").map(parse_time).transpose()?.unwrap_or_default(),
        description: el.find("trn:description").map(Element::text_content),
        slots: el
            .find("trn:slots")
            .map(|s| parse_slots(s, notices))
            .transpose()?
            .unwrap_or_default(),
        splits,
    })
}

fn parse_split(el: &Element, notices: &mut Notices) -> Result<Split, XmlError> {
    if el.name != "trn:split" {
        return Err(malformed(&el.name, "unexpected element in trn:splits"));
    }
    report_unknown(el, &SPLIT_FIELDS, notices);
    let req = |name: &str| el.find(name).ok_or_else(|| malformed(&el.name, format!("missing {name}")));
    let state_text = el.find("split:reconciled-state").map_or_else(|| "n".into(), Element::text_content);
    let reconcile_state = state_text
        .chars()
        .next()
        .and_then(ReconcileState::from_char)
        .ok_or_else(|| malformed("split:reconciled-state", format!("bad state {state_text:?}")))?;
    Ok(Split {
        guid: parse_guid(req("split:id")?)?,
        memo: el.find("split:memo").map(Element::text_content),
        action: el.find("split:action").map(Element::text_content),
        reconcile_state,
        reconcile_date: el.find("split:reconcile-date").map(parse_time).transpose()?,
        value: parse_numeric(req("split:value")?)?,
        amount: parse_numeric(req("split:quantity")?)?,
        // GnuCash writes the null GUID for a split with no account and finds no account for it on load
        // (5.17:libgnucash/backend/xml/gnc-transaction-xml-v2.cpp:114-117, :274-292).
        account: el.find("split:account").map(parse_guid).transpose()?.filter(|g| !g.is_null()),
        lot: el.find("split:lot").map(parse_guid).transpose()?,
        slots: el
            .find("split:slots")
            .map(|s| parse_slots(s, notices))
            .transpose()?
            .unwrap_or_default(),
    })
}

fn text_child_nonempty(tag: &str, value: Option<&str>) -> Option<Element> {
    value.filter(|v| !v.is_empty()).map(|v| Element::text_child(tag, v))
}

fn split_el(s: &Split) -> Element {
    Element::new("trn:split")
        .child(guid_el("split:id", s.guid))
        .child_opt(text_child_nonempty("split:memo", s.memo.as_deref()))
        .child_opt(text_child_nonempty("split:action", s.action.as_deref()))
        .child(Element::text_child(
            "split:reconciled-state",
            s.reconcile_state.as_char().to_string(),
        ))
        .child_opt(
            s.reconcile_date
                .filter(|t| *t != Time64(0))
                .map(|t| time_el("split:reconcile-date", t)),
        )
        .child(numeric_el("split:value", s.value))
        .child(numeric_el("split:quantity", s.amount))
        .child(guid_el("split:account", s.account.unwrap_or(Guid::NULL)))
        .child_opt(s.lot.map(|l| guid_el("split:lot", l)))
        .child_opt(slots_el("split:slots", &s.slots))
}

/// `gnc_transaction_dom_tree_create`.
pub fn transaction_el(t: &Transaction) -> Element {
    let mut splits = Element::new("trn:splits");
    t.splits.iter().for_each(|s| splits.push(split_el(s)));
    Element::new("gnc:transaction")
        .attr("version", VERSION)
        .child(guid_el("trn:id", t.guid))
        .child(commodity_ref_el("trn:currency", &t.currency))
        .child_opt(text_child_nonempty("trn:num", t.num.as_deref()))
        .child(time_el("trn:date-posted", t.date_posted))
        .child(time_el("trn:date-entered", t.date_entered))
        .child_opt(t.description.as_deref().map(|d| Element::text_child("trn:description", d)))
        .child_opt(slots_el("trn:slots", &t.slots))
        .child(splits)
}

/// Report each child element of `el` that is not in `known` as unrecognised content.
pub(crate) fn report_unknown(el: &Element, known: &[&str], notices: &mut Notices) {
    unknown_children(el, known)
        .iter()
        .for_each(|(_, c)| notices.unrecognised.push(format!("unknown element {} in {}", c.name, el.name)));
}
