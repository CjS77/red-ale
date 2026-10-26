//! Finnhub: `/quote` for securities only. The answer has no currency: the service's `currency` setting, else `USD`,
//! since the free tier is mostly US markets. A price or time of zero is an unknown symbol, as Finance::Quote's
//! Finnhub module reads it (`Finance::Quote 1.70: lib/Finance/Quote/Finnhub.pm:168-191`).

use red_ale_core::numeric::text::parse as number;
use red_ale_core::ops::quotes::QuoteRecord;
use serde_json::Value;

use super::{AnswerError, Ask, Context, text, unix_date};

const FORMAT: &str = "finnhub";

pub fn parse(v: &Value, cx: &Context<'_>) -> Result<QuoteRecord, AnswerError> {
    let Ask::Security { symbol } = cx.ask else { return Err(AnswerError::unexpected(FORMAT, "Finnhub has no currency rates")) };
    let price = text(v, "c").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"c\""))?;
    let time = text(v, "t").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"t\""))?;
    let zero = |t: &str| number(t).is_none_or(red_ale_core::Numeric::is_zero);
    if zero(price) || zero(time) {
        return Ok(QuoteRecord::failed(format!("Symbol {symbol} not found")));
    }
    Ok(QuoteRecord::last(price, Some(cx.currency.unwrap_or("USD").to_owned()), unix_date(time, cx.zone)))
}

#[cfg(test)]
mod tests {
    use super::super::Format;
    use super::super::fixtures::{quote, rate, read, security};
    use super::*;

    #[test]
    fn a_quote_in_the_configured_currency() {
        let r = read(Format::Finnhub, "finnhub-quote.json", &security("AAPL"), None);
        assert_eq!(r, Ok(quote("183.38", Some("USD"), Some("05/03/2024"))));
        let r = read(Format::Finnhub, "finnhub-quote.json", &security("AAPL"), Some("CAD")).unwrap();
        assert_eq!(r.currency.as_deref(), Some("CAD"));
    }

    #[test]
    fn zeros_are_an_unknown_symbol() {
        let r = read(Format::Finnhub, "finnhub-unknown.json", &security("NOPE"), None);
        assert_eq!(r, Ok(QuoteRecord::failed("Symbol NOPE not found")));
        assert!(read(Format::Finnhub, "finnhub-quote.json", &rate("EUR", "USD"), None).is_err());
    }
}
