//! Response formats: one parser per Quote service red-ale knows, and the generic `json` format, each turning an
//! answer into a [`QuoteRecord`] (`design/api/quotes.md`, "Response formats").
//!
//! The parsers are an anti-corruption layer: no provider's field names get past them. Each is a pure function of
//! the answer's bytes and what was asked. A parser says three things:
//!
//! - a quote: a record with `success` and a value, its currency and date where the answer has them;
//! - the service's own "no quote": a failed record with the service's message, as Finance::Quote reports it;
//! - [`AnswerError`]: not the format's JSON, or a rate limit. The fetch then tries the next service.

use std::str::FromStr;

use red_ale_core::ops::quotes::{QuoteRecord, mdy};
use red_ale_core::{Date, Time64, TimeZone};
use serde_json::Value;

mod alphavantage;
mod finnhub;
mod frankfurter;
mod pointers;
#[cfg(test)]
mod proptests;
mod twelvedata;
mod yahoo;

/// Which parser reads a service's answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Alpha Vantage's `GLOBAL_QUOTE` and `CURRENCY_EXCHANGE_RATE`.
    AlphaVantage,
    /// Twelve Data's `/quote` and `/exchange_rate`.
    TwelveData,
    /// Finnhub's `/quote`.
    Finnhub,
    /// Frankfurter's ECB rates.
    Frankfurter,
    /// Yahoo's chart endpoint.
    YahooChart,
    /// Any JSON, read through the service's JSON Pointers.
    Json,
}

impl FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "alphavantage" => Ok(Self::AlphaVantage),
            "twelvedata" => Ok(Self::TwelveData),
            "finnhub" => Ok(Self::Finnhub),
            "frankfurter" => Ok(Self::Frankfurter),
            "yahoo-chart" => Ok(Self::YahooChart),
            "json" => Ok(Self::Json),
            other => Err(format!("unknown quote response format {other:?}")),
        }
    }
}

/// What a request asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// A security's price, by the commodity's mnemonic, as GnuCash sends it (`gnc-quotes.cpp:457`, `:468`).
    Security {
        /// The mnemonic.
        symbol: String,
    },
    /// The price of one `base` in `quote`: two ISO currency codes.
    Rate {
        /// The currency priced.
        base: String,
        /// The currency it is priced in.
        quote: String,
    },
}

/// The `json` format's JSON Pointers.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Pointers {
    /// Where the price is (required for the format).
    #[serde(default)]
    pub price: Option<String>,
    /// Where its currency is.
    #[serde(default)]
    pub currency: Option<String>,
    /// Where its date is.
    #[serde(default)]
    pub date: Option<String>,
}

/// What a parser needs besides the answer.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// What was asked.
    pub ask: &'a Ask,
    /// The service's `currency` setting, an ISO code: the currency of a price when the answer has none.
    pub currency: Option<&'a str>,
    /// The `json` format's pointers.
    pub pointers: &'a Pointers,
    /// The local time zone: a Unix time becomes its local date, as Finance::Quote's `localtime` makes it.
    pub zone: &'a TimeZone,
}

/// An answer that is no quote and no "no quote" either.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AnswerError {
    /// Not JSON at all.
    #[error("the answer is not JSON: {0}")]
    NotJson(String),
    /// JSON, but not the format's.
    #[error("the answer is not in the {format} format: {detail}")]
    Unexpected {
        /// The format expected.
        format: &'static str,
        /// What was missing or wrong.
        detail: String,
    },
    /// The service refused for its rate limit.
    #[error("rate limit: {0}")]
    RateLimited(String),
}

impl AnswerError {
    fn unexpected(format: &'static str, detail: impl Into<String>) -> Self {
        Self::Unexpected { format, detail: detail.into() }
    }
}

/// Read an answer in `format`.
pub fn parse(format: Format, body: &[u8], cx: &Context<'_>) -> Result<QuoteRecord, AnswerError> {
    let value = crate::json_text::parse(body).map_err(|e| AnswerError::NotJson(e.to_string()))?;
    match format {
        Format::AlphaVantage => alphavantage::parse(&value, cx),
        Format::TwelveData => twelvedata::parse(&value, cx),
        Format::Finnhub => finnhub::parse(&value, cx),
        Format::Frankfurter => frankfurter::parse(&value, cx),
        Format::YahooChart => yahoo::parse(&value, cx),
        Format::Json => pointers::parse(&value, cx),
    }
}

/// The symbol a request for `mnemonic` sends: Alpha Vantage's `.X` asks for a London price in pounds rather than
/// pence, so the suffix is not sent (`Finance::Quote 1.70: lib/Finance/Quote/AlphaVantage.pm:197-203`).
pub fn request_symbol(format: Format, mnemonic: &str) -> &str {
    match format {
        Format::AlphaVantage => alphavantage::ticker(mnemonic).0,
        _ => mnemonic,
    }
}

/// A field's text: a string, or a number as written (see [`crate::json_text`]).
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// The mnemonic of a `currency` setting, which may be a commodity id such as `CURRENCY::USD`.
pub fn currency_code(setting: &str) -> &str {
    setting.rsplit_once("::").map_or(setting, |(_, code)| code)
}

/// An ISO date (`YYYY-MM-DD`, or the date of an RFC 3339 time) as a record's month/day/year.
fn iso_date(text: &str) -> Option<String> {
    text.get(..10).and_then(|d| Date::parse(d).ok()).map(mdy)
}

/// A Unix time in seconds as its local date, month/day/year.
fn unix_date(text: &str, zone: &TimeZone) -> Option<String> {
    let seconds: i64 = text.parse().ok()?;
    zone.local_date(Time64(seconds)).map(mdy)
}

/// A price in hundredths of a currency (pence, cents, agorot) in units, exactly: `1234.5` pence is `12.345` pounds.
/// Finance::Quote scales such prices by 0.01 (`scale_field`, `Finance::Quote 1.70: lib/Finance/Quote.pm:848-859`).
fn hundredths(text: &str) -> Option<String> {
    let n = red_ale_core::numeric::text::parse(text)?;
    Some(red_ale_core::Numeric::new(n.num(), n.denom().checked_mul(100)?).to_decimal_string())
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! Recorded answers. The services' documentation can't be reached from tests, so these were written from the
    //! services' published examples and field lists as `design/api/quotes.md` describes them (docs-only, October
    //! 2026), not captured from live services.

    use super::*;

    pub fn fixture(name: &str) -> Vec<u8> {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    pub fn read(format: Format, name: &str, ask: &Ask, currency: Option<&str>) -> Result<QuoteRecord, AnswerError> {
        let pointers = Pointers::default();
        let zone = TimeZone::utc();
        parse(format, &fixture(name), &Context { ask, currency, pointers: &pointers, zone: &zone })
    }

    pub fn security(symbol: &str) -> Ask {
        Ask::Security { symbol: symbol.into() }
    }

    pub fn rate(base: &str, quote: &str) -> Ask {
        Ask::Rate { base: base.into(), quote: quote.into() }
    }

    pub fn quote(last: &str, currency: Option<&str>, date: Option<&str>) -> QuoteRecord {
        QuoteRecord::last(last, currency.map(Into::into), date.map(Into::into))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hundredths_are_exact() {
        assert_eq!(hundredths("1234.5").as_deref(), Some("12.345"));
        assert_eq!(hundredths("5").as_deref(), Some("0.05"));
        assert_eq!(hundredths("-7.1").as_deref(), Some("-0.071"));
        assert_eq!(hundredths("x"), None);
    }

    #[test]
    fn dates_become_month_day_year() {
        assert_eq!(iso_date("2024-05-03").as_deref(), Some("05/03/2024"));
        assert_eq!(iso_date("2024-05-03 15:59:00").as_deref(), Some("05/03/2024"));
        assert_eq!(iso_date("2024-05-03T23:30:00-04:00").as_deref(), Some("05/03/2024"));
        assert_eq!(iso_date("May 3"), None);
        assert_eq!(unix_date("1714766401", &TimeZone::utc()).as_deref(), Some("05/03/2024"));
        assert_eq!(unix_date("1714766401", &TimeZone::fixed(10 * 3600).unwrap()).as_deref(), Some("05/04/2024"));
    }

    #[test]
    fn formats_by_name() {
        assert_eq!("yahoo-chart".parse(), Ok(Format::YahooChart));
        assert!("csv".parse::<Format>().is_err());
        assert_eq!(currency_code("CURRENCY::GBP"), "GBP");
        assert_eq!(currency_code("EUR"), "EUR");
        assert_eq!(request_symbol(Format::AlphaVantage, "GBPG.L.X"), "GBPG.L");
        assert_eq!(request_symbol(Format::TwelveData, "GBPG.L.X"), "GBPG.L.X");
    }
}
