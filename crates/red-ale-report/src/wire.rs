//! Wire forms of core identifiers in the report result and option values: GUIDs, commodities and instants.
//!
//! They match the forms of the JSON-RPC API (`design/api.md`, "Types"). Their schemas are inlined rather than named,
//! so they never clash with the API's own definitions when `api.schema` lists the report result.

use std::borrow::Cow;
use std::fmt;

use chrono::{DateTime, SecondsFormat, Utc};
use red_ale_core::{CommodityRef, Guid, Time64};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// A GUID: 32 lowercase hexadecimal digits; read in either case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(pub Guid);

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = Cow::<str>::deserialize(d)?;
        Guid::parse(text.trim()).map(Id).map_err(de::Error::custom)
    }
}

impl JsonSchema for Id {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "Guid".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "pattern": "^[0-9a-fA-F]{32}$", "description": "A GUID: 32 hexadecimal digits." })
    }
}

impl From<Guid> for Id {
    fn from(g: Guid) -> Self {
        Self(g)
    }
}

/// A commodity as `NAMESPACE::MNEMONIC` (`CURRENCY::USD`); a bare code such as `USD` names a currency.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommodityId(pub CommodityRef);

/// Parse `NAMESPACE::MNEMONIC`, or a bare currency code.
pub fn parse_commodity(s: &str) -> Result<CommodityRef, String> {
    let s = s.trim();
    match s.split_once("::") {
        Some((ns, m)) if !ns.is_empty() && !m.is_empty() => Ok(CommodityRef::new(ns, m)),
        Some(_) => Err(format!("not a commodity: {s:?}")),
        None if !s.is_empty() => Ok(CommodityRef::currency(s)),
        None => Err("empty commodity".to_owned()),
    }
}

impl Serialize for CommodityId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for CommodityId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = Cow::<str>::deserialize(d)?;
        parse_commodity(&text).map(CommodityId).map_err(de::Error::custom)
    }
}

impl JsonSchema for CommodityId {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "CommodityId".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "A commodity as NAMESPACE::MNEMONIC (CURRENCY::USD); a bare code such as USD names a currency."
        })
    }
}

impl From<CommodityRef> for CommodityId {
    fn from(c: CommodityRef) -> Self {
        Self(c)
    }
}

/// Serde for a [`CommodityRef`] field in its [`CommodityId`] form.
pub(crate) mod commodity_ref {
    use super::{CommodityId, CommodityRef, Deserialize, Deserializer, Serialize, Serializer};

    pub(crate) fn serialize<S: Serializer>(c: &CommodityRef, s: S) -> Result<S::Ok, S::Error> {
        CommodityId(c.clone()).serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<CommodityRef, D::Error> {
        CommodityId::deserialize(d).map(|c| c.0)
    }
}

/// An instant: RFC 3339 in UTC with a `Z` suffix (`2024-01-15T10:59:00Z`). Read from RFC 3339 with any offset, or
/// from an integer count of seconds since the epoch, the Time64 a script holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub Time64);

impl Timestamp {
    /// The RFC 3339 text, or `None` for an instant outside chrono's range.
    pub fn to_rfc3339(self) -> Option<String> {
        DateTime::<Utc>::from_timestamp(self.0.0, 0).map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true))
    }

    /// Parse RFC 3339 text.
    pub fn parse(s: &str) -> Result<Self, String> {
        DateTime::parse_from_rfc3339(s.trim())
            .map(|dt| Self(Time64(dt.timestamp())))
            .map_err(|e| format!("not an RFC 3339 timestamp: {s:?} ({e})"))
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.to_rfc3339() {
            Some(text) => s.serialize_str(&text),
            None => Err(serde::ser::Error::custom(format!("instant out of range: {}", self.0.0))),
        }
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Timestamp;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an RFC 3339 timestamp or whole seconds since the epoch")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Timestamp, E> {
                Timestamp::parse(v).map_err(E::custom)
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Timestamp, E> {
                Ok(Timestamp(Time64(v)))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Timestamp, E> {
                i64::try_from(v)
                    .map(|s| Timestamp(Time64(s)))
                    .map_err(|_| E::custom(format!("instant out of range: {v}")))
            }
        }
        d.deserialize_any(V)
    }
}

impl JsonSchema for Timestamp {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "Timestamp".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "format": "date-time", "description": "An instant, RFC 3339; written in UTC with a Z suffix." })
    }
}

impl From<Time64> for Timestamp {
    fn from(t: Time64) -> Self {
        Self(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identifiers_read_and_write_the_api_forms() {
        let c: CommodityId = serde_json::from_value(json!("USD")).unwrap();
        assert_eq!(serde_json::to_value(&c).unwrap(), json!("CURRENCY::USD"));
        assert!(serde_json::from_value::<CommodityId>(json!("::X")).is_err());
        let t: Timestamp = serde_json::from_value(json!("2024-01-15T12:59:00+02:00")).unwrap();
        assert_eq!(serde_json::to_value(t).unwrap(), json!("2024-01-15T10:59:00Z"));
        let s: Timestamp = serde_json::from_value(json!(1_705_316_340)).unwrap();
        assert_eq!(s, t);
        let g: Id = serde_json::from_value(json!("4A1C0000000000000000000000000000")).unwrap();
        assert_eq!(serde_json::to_value(g).unwrap(), json!("4a1c0000000000000000000000000000"));
    }
}
