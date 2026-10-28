//! Wire types shared by every method: exact amounts, identifiers, dates and references.
//!
//! These translate between the published language and core types; they hold no domain rules.

use std::borrow::Cow;
use std::fmt;

use chrono::{DateTime, SecondsFormat, Utc};
use red_ale_core::{
    Book, CommodityRef, Date, DeferredInstance, Guid, Numeric, PeriodType, Recurrence, ScheduledTransaction, Time64, WeekendAdjust,
};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};

use crate::error::RpcError;

/// An exact amount. Written as `{ "num": "1234", "denom": "100" }`; read from that form or from a string, either a
/// decimal such as `"12.34"` (denominator from the digits) or a rational such as `"1234/100"`. JSON numbers are
/// refused, so no amount ever passes through a float.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Amount(pub Numeric);

impl From<Numeric> for Amount {
    fn from(n: Numeric) -> Self {
        Self(n)
    }
}

impl Serialize for Amount {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire {
            num: String,
            denom: String,
        }
        Wire {
            num: self.0.num().to_string(),
            denom: self.0.denom().to_string(),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Amount;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(r#"an amount: "12.34", "1234/100" or {"num": "1234", "denom": "100"}"#)
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Amount, E> {
                let n = Numeric::parse_any(v).map_err(|_| E::custom(format!("not an amount: {v:?}")))?;
                check_denom(n).map(Amount).map_err(E::custom)
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Amount, A::Error> {
                let (mut num, mut denom) = (None, None);
                while let Some(key) = map.next_key::<String>()? {
                    let slot = match key.as_str() {
                        "num" => &mut num,
                        "denom" => &mut denom,
                        other => return Err(de::Error::unknown_field(other, &["num", "denom"])),
                    };
                    let text: String = map.next_value()?;
                    *slot = Some(
                        text.trim()
                            .parse::<i64>()
                            .map_err(|_| de::Error::custom(format!("{key} is not a 64-bit integer: {text:?}")))?,
                    );
                }
                let num = num.ok_or_else(|| de::Error::missing_field("num"))?;
                let denom = denom.ok_or_else(|| de::Error::missing_field("denom"))?;
                check_denom(Numeric::new(num, denom)).map(Amount).map_err(de::Error::custom)
            }

            fn visit_f64<E: de::Error>(self, _: f64) -> Result<Amount, E> {
                Err(E::custom("amounts must be strings or {num, denom}, never JSON numbers"))
            }

            fn visit_i64<E: de::Error>(self, _: i64) -> Result<Amount, E> {
                Err(E::custom("amounts must be strings or {num, denom}, never JSON numbers"))
            }

            fn visit_u64<E: de::Error>(self, _: u64) -> Result<Amount, E> {
                Err(E::custom("amounts must be strings or {num, denom}, never JSON numbers"))
            }
        }
        d.deserialize_any(V)
    }
}

fn check_denom(n: Numeric) -> Result<Numeric, String> {
    if n.denom() > 0 {
        Ok(n)
    } else {
        Err(format!("denominator must be positive, got {}", n.denom()))
    }
}

impl JsonSchema for Amount {
    fn schema_name() -> Cow<'static, str> {
        "Amount".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "An exact amount. Read from a decimal string (\"12.34\"), a rational string (\"1234/100\") or {num, denom} with string members; written as {num, denom}. Never a JSON number.",
            "oneOf": [
                { "type": "string", "pattern": "^\\s*[-+]?(\\d+(\\.\\d*)?|\\.\\d+)\\s*$|^\\s*-?\\d+\\s*/\\s*\\d+\\s*$" },
                {
                    "type": "object",
                    "properties": {
                        "num": { "type": "string", "pattern": "^-?\\d+$" },
                        "denom": { "type": "string", "pattern": "^\\d+$" }
                    },
                    "required": ["num", "denom"],
                    "additionalProperties": false
                }
            ]
        })
    }
}

macro_rules! string_newtype {
    ($name:ident, $inner:ty, $schema_name:literal, $schema:tt, $to:expr, $from:expr) => {
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                let to: fn(&$inner) -> String = $to;
                s.serialize_str(&to(&self.0))
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = String::deserialize(d)?;
                let from: fn(&str) -> Result<$inner, String> = $from;
                from(&text).map($name).map_err(de::Error::custom)
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                $schema_name.into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!($schema)
            }
        }
    };
}

/// A GUID: 32 lowercase hexadecimal digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(pub Guid);

string_newtype!(
    Id,
    Guid,
    "Guid",
    { "type": "string", "pattern": "^[0-9a-fA-F]{32}$", "description": "A GUID: 32 hexadecimal digits." },
    |g| g.to_string(),
    |s| Guid::parse(s).map_err(|e| e.to_string())
);

impl From<Guid> for Id {
    fn from(g: Guid) -> Self {
        Self(g)
    }
}

/// A calendar date, `YYYY-MM-DD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Day(pub Date);

string_newtype!(
    Day,
    Date,
    "Date",
    { "type": "string", "format": "date", "description": "A calendar date, YYYY-MM-DD." },
    |d| d.to_file_string(),
    |s| Date::parse(s).map_err(|e| e.to_string())
);

impl From<Date> for Day {
    fn from(d: Date) -> Self {
        Self(d)
    }
}

/// An instant, RFC 3339 in UTC (`2024-01-15T10:59:00Z`). Read with any offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub Time64);

string_newtype!(
    Timestamp,
    Time64,
    "Timestamp",
    { "type": "string", "format": "date-time", "description": "An instant, RFC 3339; written in UTC with a Z suffix." },
    |t| DateTime::<Utc>::from_timestamp(t.0, 0).map_or_else(String::new, |dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true)),
    |s| DateTime::parse_from_rfc3339(s.trim()).map(|dt| Time64(dt.timestamp())).map_err(|e| format!("not an RFC 3339 timestamp: {s:?} ({e})"))
);

impl From<Time64> for Timestamp {
    fn from(t: Time64) -> Self {
        Self(t)
    }
}

/// A commodity, by GnuCash's unique name `NAMESPACE::MNEMONIC` (`CURRENCY::USD`, `NASDAQ::AAPL`). A bare code such as
/// `USD` is read as a currency.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommodityId(pub CommodityRef);

string_newtype!(
    CommodityId,
    CommodityRef,
    "CommodityId",
    {
        "type": "string",
        "description": "A commodity as NAMESPACE::MNEMONIC (CURRENCY::USD, NASDAQ::AAPL); a bare code such as USD names a currency."
    },
    |c| c.to_string(),
    parse_commodity
);

fn parse_commodity(s: &str) -> Result<CommodityRef, String> {
    let s = s.trim();
    match s.split_once("::") {
        Some((ns, m)) if !ns.is_empty() && !m.is_empty() => Ok(CommodityRef::new(ns, m)),
        Some(_) => Err(format!("not a commodity: {s:?}")),
        None if !s.is_empty() => Ok(CommodityRef::currency(s)),
        None => Err("empty commodity".to_owned()),
    }
}

impl From<CommodityRef> for CommodityId {
    fn from(c: CommodityRef) -> Self {
        Self(c)
    }
}

/// An account, by GUID or by full name (`Assets:Current Assets:Checking`). A string that is a GUID of an existing
/// account names that account; anything else is looked up as a full name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountRef(pub String);

impl AccountRef {
    /// The account's GUID in `book`.
    pub fn resolve(&self, book: &Book) -> Result<Guid, RpcError> {
        let text = self.0.trim();
        if let Ok(guid) = Guid::parse(text)
            && book.account(guid).is_some()
        {
            return Ok(guid);
        }
        book.account_by_full_name(text)
            .map(|a| a.guid)
            .ok_or_else(|| RpcError::not_found("account", text))
    }
}

impl JsonSchema for AccountRef {
    fn schema_name() -> Cow<'static, str> {
        "AccountRef".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "An account, by GUID or by full name with ':' between levels (Assets:Current Assets:Checking)."
        })
    }
}

/// A Recurrence's step, as the wire names it (`design/api.md`, "Wire types").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PeriodTypeWire {
    /// Once, on the start date.
    Once,
    /// Every *n* days.
    Day,
    /// Every *n* weeks.
    Week,
    /// Every *n* months on the start's day.
    Month,
    /// Every *n* months on the last day.
    EndOfMonth,
    /// Every *n* months on the start's weekday in the same week of the month.
    NthWeekday,
    /// Every *n* months on the last of the start's weekday.
    LastWeekday,
    /// Every *n* years.
    Year,
}

const PERIOD_TYPES: [(PeriodTypeWire, PeriodType); 8] = [
    (PeriodTypeWire::Once, PeriodType::Once),
    (PeriodTypeWire::Day, PeriodType::Day),
    (PeriodTypeWire::Week, PeriodType::Week),
    (PeriodTypeWire::Month, PeriodType::Month),
    (PeriodTypeWire::EndOfMonth, PeriodType::EndOfMonth),
    (PeriodTypeWire::NthWeekday, PeriodType::NthWeekday),
    (PeriodTypeWire::LastWeekday, PeriodType::LastWeekday),
    (PeriodTypeWire::Year, PeriodType::Year),
];

/// Where an occurrence on a weekend moves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum WeekendAdjustWire {
    /// It stays on the weekend.
    #[default]
    None,
    /// To the Friday before.
    Back,
    /// To the Monday after.
    Forward,
}

const WEEKEND_ADJUSTS: [(WeekendAdjustWire, WeekendAdjust); 3] = [
    (WeekendAdjustWire::None, WeekendAdjust::None),
    (WeekendAdjustWire::Back, WeekendAdjust::Back),
    (WeekendAdjustWire::Forward, WeekendAdjust::Forward),
];

fn wire_of<W: Copy, C: PartialEq>(table: &[(W, C)], c: C, fallback: W) -> W {
    table.iter().find(|(_, x)| *x == c).map_or(fallback, |(w, _)| *w)
}

fn core_of<W: PartialEq, C: Copy>(table: &[(W, C)], w: W, fallback: C) -> C {
    table.iter().find(|(x, _)| *x == w).map_or(fallback, |(_, c)| *c)
}

/// A Recurrence: every `mult` steps of `periodType` from `start`, moved off weekends by `weekendAdjust`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[schemars(rename = "Recurrence")]
pub struct RecurrenceWire {
    /// How many steps apart occurrences are; at least 1 unless `once`.
    pub mult: u16,
    /// The step.
    pub period_type: PeriodTypeWire,
    /// The first occurrence.
    pub start: Day,
    /// Where occurrences on a weekend move to; `none` when absent.
    #[serde(default)]
    pub weekend_adjust: WeekendAdjustWire,
}

impl RecurrenceWire {
    /// The core Recurrence, as given; a zero multiplier on a repeating type is `invalid`.
    pub fn to_core(self) -> Result<Recurrence, RpcError> {
        Recurrence::new(
            self.mult,
            core_of(&PERIOD_TYPES, self.period_type, PeriodType::Once),
            self.start.0,
            core_of(&WEEKEND_ADJUSTS, self.weekend_adjust, WeekendAdjust::None),
        )
        .map_err(|e| RpcError::invalid(e.to_string()))
    }
}

impl From<Recurrence> for RecurrenceWire {
    fn from(r: Recurrence) -> Self {
        Self {
            mult: r.mult(),
            period_type: wire_of(&PERIOD_TYPES, r.period_type(), PeriodTypeWire::Once),
            start: Day(r.start()),
            weekend_adjust: wire_of(&WEEKEND_ADJUSTS, r.weekend_adjust(), WeekendAdjustWire::None),
        }
    }
}

/// A scheduled transaction's end (`design/api/scheduled-transactions.md`, "The model on the wire"): `{ never: true }`,
/// `{ date }`, `{ occurrences, remaining }`, or, when a file holds both, `{ date, occurrences, remaining }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename = "End")]
pub struct EndWire {
    /// True when there is no end.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub never: Option<bool>,
    /// No occurrence after this date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<Day>,
    /// At most this many occurrences in all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurrences: Option<i32>,
    /// This many still to come.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<i32>,
}

impl EndWire {
    /// The end of `sx` as stored.
    pub fn of(sx: &ScheduledTransaction) -> Self {
        let never = (sx.end.is_none() && sx.occurrences.is_none()).then_some(true);
        Self {
            never,
            date: sx.end.map(Day),
            occurrences: sx.occurrences.map(|o| o.total),
            remaining: sx.occurrences.map(|o| o.remaining),
        }
    }
}

/// A Deferred instance: the temporal state saved before a postponed instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename = "DeferredInstance")]
pub struct DeferredInstanceWire {
    /// The last occurrence before the postponed one, or null.
    pub last_occurrence: Option<Day>,
    /// Occurrences remaining then.
    pub remaining_occurrences: i32,
    /// The Instance count then.
    pub instance_count: i32,
}

impl From<&DeferredInstance> for DeferredInstanceWire {
    fn from(d: &DeferredInstance) -> Self {
        Self {
            last_occurrence: d.last.map(Day),
            remaining_occurrences: d.remaining,
            instance_count: d.instance_count,
        }
    }
}

/// Params for methods that take none.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

/// A result with nothing to report.
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct Done {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn amounts_are_exact_and_never_numbers() {
        let a: Amount = serde_json::from_value(json!("12.30")).unwrap();
        assert!(a.0.same_repr(Numeric::new(1230, 100)));
        let b: Amount = serde_json::from_value(json!({ "num": "-5", "denom": "3" })).unwrap();
        assert!(b.0.same_repr(Numeric::new(-5, 3)));
        assert_eq!(serde_json::to_value(b).unwrap(), json!({ "num": "-5", "denom": "3" }));
        assert!(serde_json::from_value::<Amount>(json!(12.3)).is_err());
        assert!(serde_json::from_value::<Amount>(json!(12)).is_err());
        assert!(serde_json::from_value::<Amount>(json!("1/0")).is_err());
        assert!(serde_json::from_value::<Amount>(json!({ "num": "1", "denom": "2", "x": "1" })).is_err());
    }

    #[test]
    fn commodities_and_times() {
        let c: CommodityId = serde_json::from_value(json!("USD")).unwrap();
        assert_eq!(c.0, CommodityRef::currency("USD"));
        assert_eq!(serde_json::to_value(&c).unwrap(), json!("CURRENCY::USD"));
        let s: CommodityId = serde_json::from_value(json!("NASDAQ::AAPL")).unwrap();
        assert_eq!(s.0, CommodityRef::new("NASDAQ", "AAPL"));
        let t: Timestamp = serde_json::from_value(json!("2024-01-15T12:59:00+02:00")).unwrap();
        assert_eq!(serde_json::to_value(t).unwrap(), json!("2024-01-15T10:59:00Z"));
    }
}
