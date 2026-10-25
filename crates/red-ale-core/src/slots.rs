//! Slots: the key-value data GnuCash attaches to entities (its KVP frames).

use std::collections::BTreeMap;

use crate::{Date, Guid, Numeric, Time64};

/// One Slot value. `5.17:libgnucash/engine/kvp-value.hpp`.
#[derive(Debug, Clone, PartialEq)]
pub enum SlotValue {
    /// A 64-bit integer.
    Integer(i64),
    /// A double, kept as the exact text it was read as. red-ale never computes with it.
    Double(String),
    /// An exact rational.
    Numeric(Numeric),
    /// A UTF-8 string.
    String(String),
    /// A GUID.
    Guid(Guid),
    /// An instant (`timespec` in the file).
    Timespec(Time64),
    /// A calendar date (`gdate` in the file).
    Date(Date),
    /// An ordered list of values.
    List(Vec<SlotValue>),
    /// Nested slots.
    Frame(Frame),
}

impl SlotValue {
    /// The string, if this is a string value.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// The integer, if this is an integer value.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// The numeric, if this is a numeric value.
    pub fn as_numeric(&self) -> Option<Numeric> {
        match self {
            Self::Numeric(n) => Some(*n),
            _ => None,
        }
    }

    /// The GUID, if this is a GUID value.
    pub fn as_guid(&self) -> Option<Guid> {
        match self {
            Self::Guid(g) => Some(*g),
            _ => None,
        }
    }

    /// The instant, if this is a timespec value.
    pub fn as_time(&self) -> Option<Time64> {
        match self {
            Self::Timespec(t) => Some(*t),
            _ => None,
        }
    }

    /// The date, if this is a date value.
    pub fn as_date(&self) -> Option<Date> {
        match self {
            Self::Date(d) => Some(*d),
            _ => None,
        }
    }

    /// The frame, if this is a frame value.
    pub fn as_frame(&self) -> Option<&Frame> {
        match self {
            Self::Frame(f) => Some(f),
            _ => None,
        }
    }
}

/// A set of Slots keyed by name. Kept sorted by key in byte order, which is the order GnuCash writes them in
/// (`KvpFrame` is a `std::map` ordered by `strcmp`, `5.17:libgnucash/engine/kvp-frame.hpp`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Frame(BTreeMap<String, SlotValue>);

impl Frame {
    /// An empty frame.
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the frame holds no slots.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The slots in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &SlotValue)> {
        self.0.iter()
    }

    /// The value directly under `key` (no path splitting).
    pub fn get_key(&self, key: &str) -> Option<&SlotValue> {
        self.0.get(key)
    }

    /// Insert directly under `key`, returning any previous value.
    pub fn insert_key(&mut self, key: impl Into<String>, value: SlotValue) -> Option<SlotValue> {
        self.0.insert(key.into(), value)
    }

    /// The value at a `/`-separated path such as `tax-US/code`.
    pub fn get(&self, path: &str) -> Option<&SlotValue> {
        let mut parts = path.split('/').filter(|p| !p.is_empty()).peekable();
        let mut frame = self;
        while let Some(part) = parts.next() {
            let value = frame.0.get(part)?;
            if parts.peek().is_none() {
                return Some(value);
            }
            frame = value.as_frame()?;
        }
        None
    }

    /// The string at a path.
    pub fn get_str(&self, path: &str) -> Option<&str> {
        self.get(path).and_then(SlotValue::as_str)
    }

    /// Set the value at a path, creating intermediate frames. A non-frame value in the way is replaced by a frame,
    /// as `KvpFrame::set_path` does.
    pub fn set(&mut self, path: &str, value: SlotValue) {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let Some((last, parents)) = parts.split_last() else { return };
        let mut frame = self;
        for part in parents {
            let entry = frame.0.entry((*part).to_owned()).or_insert_with(|| SlotValue::Frame(Frame::new()));
            if !matches!(entry, SlotValue::Frame(_)) {
                *entry = SlotValue::Frame(Frame::new());
            }
            let SlotValue::Frame(inner) = entry else { return };
            frame = inner;
        }
        frame.0.insert((*last).to_owned(), value);
    }

    /// Remove the value at a path. Frames left empty along the path are kept, as GnuCash keeps them.
    pub fn remove(&mut self, path: &str) -> Option<SlotValue> {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let (last, parents) = parts.split_last()?;
        let mut frame = self;
        for part in parents {
            frame = match frame.0.get_mut(*part)? {
                SlotValue::Frame(f) => f,
                _ => return None,
            };
        }
        frame.0.remove(*last)
    }

    /// Set a string at `path`, or remove it when `value` is `None` or empty, the way GnuCash's setters treat empty
    /// strings for optional fields.
    pub fn set_opt_str(&mut self, path: &str, value: Option<&str>) {
        match value {
            Some(v) if !v.is_empty() => self.set(path, SlotValue::String(v.to_owned())),
            _ => {
                self.remove(path);
            }
        }
    }
}

impl FromIterator<(String, SlotValue)> for Frame {
    fn from_iter<T: IntoIterator<Item = (String, SlotValue)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_create_and_find_nested_frames() {
        let mut f = Frame::new();
        f.set("tax-US/code", SlotValue::String("N123".into()));
        f.set("placeholder", SlotValue::String("true".into()));
        assert_eq!(f.get_str("tax-US/code"), Some("N123"));
        assert_eq!(f.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["placeholder", "tax-US"]);
        assert!(f.remove("tax-US/code").is_some());
        assert!(f.get("tax-US").is_some_and(|v| v.as_frame().is_some_and(Frame::is_empty)));
    }
}
