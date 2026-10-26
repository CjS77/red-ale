//! Yahoo's chart endpoint: `chart.result[0].meta` for securities and rates alike (a rate's symbol is
//! `{base}{quote}=X`). Unofficial and best-effort.
//!
//! Prices quoted in hundredths are turned into units exactly, as Finance::Quote's YahooJSON module does with
//! `GBp`/`GBX` (to `GBP`), `ZAc` (to `ZAR`) and `ILA` (to `ILS`) (`Finance::Quote 1.70:
//! lib/Finance/Quote/YahooJSON.pm:266-308`), and the date is the local date of `regularMarketTime`, as its
//! `localtime` gives it (`:310-314`).

use red_ale_core::ops::quotes::QuoteRecord;
use serde_json::Value;

use super::{AnswerError, Context, hundredths, text, unix_date};

const FORMAT: &str = "yahoo-chart";

/// Currencies Yahoo quotes in hundredths, with the currency they are hundredths of.
const HUNDREDTHS: [(&str, &str); 4] = [("GBp", "GBP"), ("GBX", "GBP"), ("ZAc", "ZAR"), ("ILA", "ILS")];

pub fn parse(v: &Value, cx: &Context<'_>) -> Result<QuoteRecord, AnswerError> {
    let chart = v.get("chart").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"chart\""))?;
    if let Some(error) = chart.get("error").filter(|e| !e.is_null()) {
        let message = text(error, "description").or_else(|| text(error, "code")).unwrap_or("Yahoo returned an error");
        return Ok(QuoteRecord::failed(message));
    }
    let meta = chart.pointer("/result/0/meta").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"chart.result[0].meta\""))?;
    let price = text(meta, "regularMarketPrice").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"regularMarketPrice\""))?;
    let currency = text(meta, "currency").or(cx.currency);
    let (price, currency) = match HUNDREDTHS.iter().find(|(small, _)| Some(*small) == currency) {
        Some((_, unit)) => (hundredths(price).ok_or_else(|| AnswerError::unexpected(FORMAT, "a price that is not a number"))?, Some(*unit)),
        None => (price.to_owned(), currency),
    };
    let date = text(meta, "regularMarketTime").and_then(|t| unix_date(t, cx.zone));
    Ok(QuoteRecord::last(price, currency.map(str::to_owned), date))
}

#[cfg(test)]
mod tests {
    use super::super::Format;
    use super::super::fixtures::{quote, rate, read, security};
    use super::*;

    #[test]
    fn a_chart() {
        let r = read(Format::YahooChart, "yahoo-chart.json", &security("AAPL"), None);
        assert_eq!(r, Ok(quote("183.38", Some("USD"), Some("05/03/2024"))));
        let r = read(Format::YahooChart, "yahoo-chart-rate.json", &rate("EUR", "USD"), None);
        assert_eq!(r, Ok(quote("1.0789", Some("USD"), Some("05/03/2024"))));
    }

    #[test]
    fn pence_become_pounds_exactly() {
        let r = read(Format::YahooChart, "yahoo-chart-pence.json", &security("VOD.L"), None);
        assert_eq!(r, Ok(quote("0.7218", Some("GBP"), Some("05/03/2024"))));
    }

    #[test]
    fn an_error_is_no_quote() {
        let r = read(Format::YahooChart, "yahoo-chart-error.json", &security("NOPE"), None);
        assert_eq!(r, Ok(QuoteRecord::failed("No data found, symbol may be delisted")));
    }
}
