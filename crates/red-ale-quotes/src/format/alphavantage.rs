//! Alpha Vantage: `GLOBAL_QUOTE` for securities, `CURRENCY_EXCHANGE_RATE` for rates.
//!
//! What Finance::Quote's AlphaVantage module does with an answer is translated from `Finance::Quote 1.70:
//! lib/Finance/Quote/AlphaVantage.pm` (GPL-2.0-or-later; this translation is GPL-3.0-or-later): the error keys
//! (`:248-260`), the empty `Global Quote` of an unknown symbol (`:262-275`), and the currency guessed from the
//! symbol's exchange suffix (`:49-125`, `:318-347`), since the answer has none.

use red_ale_core::ops::quotes::QuoteRecord;
use serde_json::Value;

use super::{AnswerError, Ask, Context, hundredths, iso_date, text};

const FORMAT: &str = "alphavantage";

/// The currency of each exchange suffix (`AlphaVantage.pm:49-125`). `MXP` is Finance::Quote's, though no longer an
/// ISO code; GnuCash then fails the quote with an unknown currency, as it would.
const SUFFIX_CURRENCIES: &[(&str, &str)] = &[
    (".US", "USD"), (".A", "USD"), (".B", "USD"), (".N", "USD"), (".O", "USD"), (".OB", "USD"), (".PK", "USD"),
    (".X", "USD"), (".BA", "ARS"), (".VI", "EUR"), (".AX", "AUD"), (".SA", "BRL"), (".BR", "EUR"), (".TO", "CAD"),
    (".TRV", "CAD"), (".V", "CAD"), (".TRT", "CAD"), (".SN", "CLP"), (".SS", "CNY"), (".SZ", "CNY"), (".CO", "DKK"),
    (".PA", "EUR"), (".BE", "EUR"), (".BM", "EUR"), (".D", "EUR"), (".F", "EUR"), (".FRK", "EUR"), (".H", "EUR"),
    (".HA", "EUR"), (".MU", "EUR"), (".DEX", "EUR"), (".ME", "RUB"), (".SG", "EUR"), (".DE", "EUR"), (".HK", "HKD"),
    (".BO", "INR"), (".CL", "INR"), (".NS", "INR"), (".JK", "IDR"), (".I", "EUR"), (".TA", "ILS"), (".MI", "EUR"),
    (".KS", "KRW"), (".KQ", "KRW"), (".KL", "MYR"), (".MX", "MXP"), (".NZ", "NZD"), (".AS", "EUR"), (".AMS", "EUR"),
    (".OL", "NOK"), (".LM", "PEN"), (".IN", "EUR"), (".SI", "SGD"), (".BC", "EUR"), (".BI", "EUR"), (".MF", "EUR"),
    (".MC", "EUR"), (".MA", "EUR"), (".VA", "EUR"), (".ST", "SEK"), (".STO", "SEK"), (".HE", "EUR"), (".S", "CHF"),
    (".TW", "TWD"), (".TWO", "TWD"), (".BK", "THB"), (".TH", "THB"), (".L", "GBP"), (".IL", "USD"), (".VX", "CHF"),
    (".SW", "CHF"),
];

/// The ticker sent for a mnemonic, and whether its price is to be divided by 100: a trailing `.X` asks for a
/// London price, quoted in pence, in pounds (`AlphaVantage.pm:197-203`).
pub fn ticker(mnemonic: &str) -> (&str, bool) {
    mnemonic.strip_suffix(".X").map_or((mnemonic, false), |t| (t, true))
}

pub fn parse(v: &Value, cx: &Context<'_>) -> Result<QuoteRecord, AnswerError> {
    if let Some(note) = ["Note", "Information"].into_iter().find_map(|k| text(v, k)) {
        return Err(AnswerError::RateLimited(note.to_owned()));
    }
    if let Some(message) = text(v, "Error Message") {
        return Ok(QuoteRecord::failed(message));
    }
    match cx.ask {
        Ask::Security { symbol } => security(v, symbol, cx.currency),
        Ask::Rate { .. } => rate(v),
    }
}

fn security(v: &Value, symbol: &str, setting: Option<&str>) -> Result<QuoteRecord, AnswerError> {
    let quote = v.get("Global Quote").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"Global Quote\""))?;
    if text(quote, "01. symbol").is_none_or(str::is_empty) {
        return Ok(QuoteRecord::failed(format!("Symbol {symbol} not found.")));
    }
    let price = text(quote, "05. price").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"05. price\""))?;
    let (currency, scaled) = currency(symbol, setting);
    let price = if scaled { hundredths(price).ok_or_else(|| AnswerError::unexpected(FORMAT, "a price that is not a number"))? } else { price.to_owned() };
    let date = text(quote, "07. latest trading day").and_then(iso_date);
    Ok(QuoteRecord::last(price, currency.map(str::to_owned), date))
}

/// The currency `AlphaVantage.pm:318-347` gives a symbol: its exchange suffix's, with `.X` and `.IL` (USD quoted in
/// cents) prices divided by 100; `USD` with no suffix; none with a suffix it doesn't know. The service's `currency`
/// setting stands in for the last two.
fn currency<'a>(symbol: &str, setting: Option<&'a str>) -> (Option<&'a str>, bool) {
    let (ticker, pounds) = ticker(symbol);
    let Some(dot) = ticker.find('.') else { return (Some(setting.unwrap_or("USD")), false) };
    let suffix = ticker[dot..].to_ascii_uppercase();
    match SUFFIX_CURRENCIES.iter().find(|(s, _)| *s == suffix) {
        Some((_, code)) => (Some(code), pounds || (suffix == ".IL" && *code == "USD")),
        None => (setting, false),
    }
}

fn rate(v: &Value) -> Result<QuoteRecord, AnswerError> {
    let rate = v.get("Realtime Currency Exchange Rate").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"Realtime Currency Exchange Rate\""))?;
    let value = text(rate, "5. Exchange Rate").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"5. Exchange Rate\""))?;
    Ok(QuoteRecord::last(value, None, None))
}

#[cfg(test)]
mod tests {
    use super::super::Format;
    use super::super::fixtures::{quote, rate, read, security};
    use super::*;

    #[test]
    fn a_global_quote() {
        let r = read(Format::AlphaVantage, "alphavantage-global-quote.json", &security("IBM"), None);
        assert_eq!(r, Ok(quote("187.2700", Some("USD"), Some("05/03/2024"))));
    }

    #[test]
    fn the_suffix_names_the_currency() {
        let r = read(Format::AlphaVantage, "alphavantage-global-quote-london.json", &security("GBPG.L"), None).unwrap();
        assert_eq!((r.last.as_deref(), r.currency.as_deref()), (Some("1234.5000"), Some("GBP")));
        let pence = read(Format::AlphaVantage, "alphavantage-global-quote-london.json", &security("GBPG.L.X"), None).unwrap();
        assert_eq!((pence.last.as_deref(), pence.currency.as_deref()), (Some("12.345000"), Some("GBP")));
        assert_eq!(currency("ABC.ZZ", None), (None, false));
        assert_eq!(currency("ABC.ZZ", Some("EUR")), (Some("EUR"), false));
        assert_eq!(currency("ABC", Some("EUR")), (Some("EUR"), false));
        assert_eq!(currency("TEVA.il", None), (Some("USD"), true));
    }

    #[test]
    fn an_unknown_symbol_is_no_quote() {
        let r = read(Format::AlphaVantage, "alphavantage-empty.json", &security("NOPE"), None);
        assert_eq!(r, Ok(QuoteRecord::failed("Symbol NOPE not found.")));
        let e = read(Format::AlphaVantage, "alphavantage-error.json", &security("IBM"), None).unwrap();
        assert!(!e.success && e.errormsg.unwrap().starts_with("Invalid API call"));
    }

    #[test]
    fn notes_are_the_rate_limit() {
        let r = read(Format::AlphaVantage, "alphavantage-rate-limit.json", &security("IBM"), None);
        assert!(matches!(r, Err(AnswerError::RateLimited(m)) if m.contains("25 requests per day")));
    }

    #[test]
    fn an_exchange_rate() {
        let r = read(Format::AlphaVantage, "alphavantage-exchange-rate.json", &rate("EUR", "USD"), None);
        assert_eq!(r, Ok(quote("1.07950000", None, None)));
    }
}
