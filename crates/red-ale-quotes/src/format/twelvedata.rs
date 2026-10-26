//! Twelve Data: `/quote` for securities, `/exchange_rate` for rates. An error answer is
//! `{"status": "error", "code", "message"}`, as Finance::Quote's TwelveData module reads it
//! (`Finance::Quote 1.70: lib/Finance/Quote/TwelveData.pm:119-123`); code 429 is the rate limit.

use red_ale_core::ops::quotes::QuoteRecord;
use serde_json::Value;

use super::{AnswerError, Ask, Context, iso_date, text};

const FORMAT: &str = "twelvedata";

pub fn parse(v: &Value, cx: &Context<'_>) -> Result<QuoteRecord, AnswerError> {
    if text(v, "status") == Some("error") {
        let message = text(v, "message").unwrap_or("Twelve Data returned an error").to_owned();
        return if text(v, "code") == Some("429") { Err(AnswerError::RateLimited(message)) } else { Ok(QuoteRecord::failed(message)) };
    }
    match cx.ask {
        Ask::Security { .. } => {
            let close = text(v, "close").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"close\""))?;
            let currency = text(v, "currency").or(cx.currency).map(str::to_owned);
            Ok(QuoteRecord::last(close, currency, text(v, "datetime").and_then(iso_date)))
        }
        Ask::Rate { .. } => {
            let rate = text(v, "rate").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"rate\""))?;
            Ok(QuoteRecord::last(rate, None, None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Format;
    use super::super::fixtures::{quote, rate, read, security};
    use super::*;

    #[test]
    fn a_quote() {
        let r = read(Format::TwelveData, "twelvedata-quote.json", &security("AAPL"), None);
        assert_eq!(r, Ok(quote("183.38000", Some("USD"), Some("05/03/2024"))));
    }

    #[test]
    fn an_exchange_rate() {
        // The rate is a JSON number: its digits are kept.
        let r = read(Format::TwelveData, "twelvedata-exchange-rate.json", &rate("EUR", "USD"), None);
        assert_eq!(r, Ok(quote("1.07950", None, None)));
    }

    #[test]
    fn errors_and_the_rate_limit() {
        let r = read(Format::TwelveData, "twelvedata-not-found.json", &security("NOPE"), None).unwrap();
        assert!(!r.success && r.errormsg.unwrap().contains("not found"));
        let r = read(Format::TwelveData, "twelvedata-rate-limit.json", &security("AAPL"), None);
        assert!(matches!(r, Err(AnswerError::RateLimited(m)) if m.contains("API credits")));
    }
}
