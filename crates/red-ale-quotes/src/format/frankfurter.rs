//! Frankfurter: the ECB's reference rates, currencies only: `rates.<quote>` is the price of one `base`.

use red_ale_core::ops::quotes::QuoteRecord;
use serde_json::Value;

use super::{AnswerError, Ask, Context, text};

const FORMAT: &str = "frankfurter";

pub fn parse(v: &Value, cx: &Context<'_>) -> Result<QuoteRecord, AnswerError> {
    let Ask::Rate { base, quote } = cx.ask else { return Err(AnswerError::unexpected(FORMAT, "Frankfurter has only currency rates")) };
    let rates = v.get("rates").ok_or_else(|| AnswerError::unexpected(FORMAT, "no \"rates\""))?;
    Ok(text(rates, quote).map_or_else(|| QuoteRecord::failed(format!("no rate from {base} to {quote}")), |r| QuoteRecord::last(r, None, None)))
}

#[cfg(test)]
mod tests {
    use super::super::Format;
    use super::super::fixtures::{quote, rate, read, security};
    use super::*;

    #[test]
    fn a_rate() {
        let r = read(Format::Frankfurter, "frankfurter-latest.json", &rate("EUR", "USD"), None);
        assert_eq!(r, Ok(quote("1.0795", None, None)));
        let missing = read(Format::Frankfurter, "frankfurter-latest.json", &rate("EUR", "XAU"), None);
        assert_eq!(missing, Ok(QuoteRecord::failed("no rate from EUR to XAU")));
        assert!(read(Format::Frankfurter, "frankfurter-latest.json", &security("AAPL"), None).is_err());
        assert!(matches!(read(Format::Frankfurter, "frankfurter-not-found.json", &rate("EUR", "ZZZ"), None), Err(AnswerError::Unexpected { .. })));
    }
}
