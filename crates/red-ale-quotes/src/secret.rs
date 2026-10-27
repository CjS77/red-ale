//! Keys and the URLs that carry them. A key is used only in the request to its own service: it is never shown by
//! `Debug`, and every URL or message that leaves this crate has it replaced by `***` (`design/api/quotes.md`, "Keys
//! and the network").

use std::fmt;

/// What a key reads as wherever it would be shown.
pub const MASK: &str = "***";

/// A Quote service's key.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// A key; `None` when empty, since an empty key is no key.
    pub fn new(key: impl Into<String>) -> Option<Self> {
        let key = key.into();
        (!key.is_empty()).then_some(Self(key))
    }

    /// `text` with every occurrence of the key, as written or percent-encoded, replaced by [`MASK`].
    pub fn redact(&self, text: &str) -> String {
        let encoded = encode(&self.0);
        let once = text.replace(&self.0, MASK);
        if encoded == self.0 { once } else { once.replace(&encoded, MASK) }
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(MASK)
    }
}

/// A request's URL, which may hold a key, with the form it is shown in.
#[derive(Clone)]
pub struct KeyedUrl {
    url: String,
    shown: String,
    key: Option<Secret>,
}

impl fmt::Debug for KeyedUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.shown)
    }
}

impl KeyedUrl {
    /// The URL with its key as `***`.
    pub fn shown(&self) -> &str {
        &self.shown
    }

    /// The URL to request, key and all; only the HTTP client reads it.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// `text` with this URL's key, if any, replaced by `***`.
    pub fn redact(&self, text: &str) -> String {
        self.key.as_ref().map_or_else(|| text.to_owned(), |k| k.redact(text))
    }
}

/// Fill a URL template: each `{name}` with its value, percent-encoded, and `{key}` with the key (`***` in the shown
/// form). The Configuration refuses any other placeholder, so one left over stays as written.
pub fn fill(template: &str, values: &[(&str, &str)], key: Option<&Secret>) -> KeyedUrl {
    let (url, shown) = pieces(template).fold((String::new(), String::new()), |(mut url, mut shown), piece| {
        match piece {
            Piece::Text(text) => {
                url.push_str(text);
                shown.push_str(text);
            }
            Piece::Placeholder("key") => {
                url.push_str(&key.map(|k| encode(&k.0)).unwrap_or_default());
                shown.push_str(MASK);
            }
            Piece::Placeholder(name) => {
                let value = values
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map_or_else(|| format!("{{{name}}}"), |(_, v)| encode(v));
                url.push_str(&value);
                shown.push_str(&value);
            }
        }
        (url, shown)
    });
    KeyedUrl {
        url,
        shown,
        key: key.cloned(),
    }
}

/// True when the template sends a key.
pub fn wants_key(template: &str) -> bool {
    pieces(template).any(|p| p == Piece::Placeholder("key"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece<'a> {
    Text(&'a str),
    Placeholder(&'a str),
}

/// A template's text and `{placeholders}`, in order.
fn pieces(template: &str) -> impl Iterator<Item = Piece<'_>> {
    let mut rest = template;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let placeholder = rest
            .strip_prefix('{')
            .and_then(|r| r.split_once('}'))
            .filter(|(name, _)| !name.contains('{'));
        if let Some((name, after)) = placeholder {
            rest = after;
            return Some(Piece::Placeholder(name));
        }
        let end = rest[1..].find('{').map_or(rest.len(), |i| i + 1);
        let (text, after) = rest.split_at(end);
        rest = after;
        Some(Piece::Text(text))
    })
}

/// Percent-encode everything but RFC 3986's unreserved characters.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => char::from(b).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_fill_and_hide_the_key() {
        let key = Secret::new("s3cr+t/K").unwrap();
        let t = "https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={symbol}&apikey={key}";
        let u = fill(t, &[("symbol", "BRK.B")], Some(&key));
        assert_eq!(
            u.url(),
            "https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol=BRK.B&apikey=s3cr%2Bt%2FK"
        );
        assert_eq!(
            u.shown(),
            "https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol=BRK.B&apikey=***"
        );
        assert_eq!(format!("{u:?}"), u.shown());
        assert_eq!(format!("{key:?}"), "***");
        assert_eq!(u.redact("failed: apikey=s3cr%2Bt%2FK or s3cr+t/K"), "failed: apikey=*** or ***");
        assert!(wants_key(t) && !wants_key("http://127.0.0.1:8200/quote/{symbol}"));
    }

    #[test]
    fn values_are_percent_encoded() {
        let u = fill(
            "https://h/v8/finance/chart/{base}{quote}=X?d={date}",
            &[("base", "EUR"), ("quote", "USD"), ("date", "2024-05-03")],
            None,
        );
        assert_eq!(u.url(), "https://h/v8/finance/chart/EURUSD=X?d=2024-05-03");
        let u = fill("https://h/q/{symbol}", &[("symbol", "A B&C/é")], None);
        assert_eq!(u.url(), "https://h/q/A%20B%26C%2F%C3%A9");
        assert_eq!(fill("https://h/{other}", &[], None).url(), "https://h/{other}");
        assert_eq!(Secret::new(""), None);
    }
}
