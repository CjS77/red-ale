//! The generic `json` format: the service's JSON Pointers say where the price, its currency and its date are, so a
//! service red-ale doesn't know can be added from the Configuration alone.

use red_ale_core::ops::quotes::QuoteRecord;
use serde_json::Value;

use super::{AnswerError, Context, iso_date, unix_date};

const FORMAT: &str = "json";

pub fn parse(v: &Value, cx: &Context<'_>) -> Result<QuoteRecord, AnswerError> {
    let pointer = cx.pointers.price.as_deref().ok_or_else(|| AnswerError::unexpected(FORMAT, "the service has no price pointer"))?;
    let at = |p: Option<&str>| p.and_then(|p| v.pointer(p)).and_then(Value::as_str);
    let Some(price) = at(Some(pointer)) else { return Ok(QuoteRecord::failed(format!("nothing at {pointer}"))) };
    let currency = match &cx.pointers.currency {
        Some(p) => at(Some(p)),
        None => cx.currency,
    };
    let date = at(cx.pointers.date.as_deref()).and_then(|d| date(d, cx));
    Ok(QuoteRecord::last(price, currency.map(str::to_owned), date))
}

/// `YYYY-MM-DD`, RFC 3339 (its date as written) or Unix seconds (their local date).
fn date(text: &str, cx: &Context<'_>) -> Option<String> {
    if text.bytes().all(|b| b.is_ascii_digit()) { unix_date(text, cx.zone) } else { iso_date(text) }
}

#[cfg(test)]
mod tests {
    use red_ale_core::TimeZone;

    use super::super::fixtures::{fixture, quote, security};
    use super::super::{Format, Pointers};
    use super::*;

    fn read(name: &str, pointers: &Pointers, currency: Option<&str>) -> Result<QuoteRecord, AnswerError> {
        let ask = security("ACME");
        let zone = TimeZone::utc();
        super::super::parse(Format::Json, &fixture(name), &Context { ask: &ask, currency, pointers, zone: &zone })
    }

    fn pointers(price: &str, currency: Option<&str>, date: Option<&str>) -> Pointers {
        Pointers { price: Some(price.into()), currency: currency.map(Into::into), date: date.map(Into::into) }
    }

    #[test]
    fn values_where_the_pointers_say() {
        let p = pointers("/data/price", Some("/data/ccy"), Some("/data/asOf"));
        assert_eq!(read("json-proxy.json", &p, None), Ok(quote("42.1050", Some("EUR"), Some("05/03/2024"))));
        let p = pointers("/data/price", None, Some("/data/epoch"));
        assert_eq!(read("json-proxy.json", &p, Some("CHF")), Ok(quote("42.1050", Some("CHF"), Some("05/03/2024"))));
        let p = pointers("/data/price", None, Some("/data/stamp"));
        assert_eq!(read("json-proxy.json", &p, None), Ok(quote("42.1050", None, Some("05/03/2024"))));
    }

    #[test]
    fn a_price_pointer_that_finds_nothing_is_no_quote() {
        let p = pointers("/data/last", None, None);
        assert_eq!(read("json-proxy.json", &p, None), Ok(QuoteRecord::failed("nothing at /data/last")));
    }
}
