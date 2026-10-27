//! Built-in reference data: ISO 4217 currencies, euro legacy rates and quote sources.

use crate::iso4217_table::CURRENCIES;
use crate::{Commodity, CommodityRef, Numeric, NumericError, Round};

/// A built-in ISO currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsoCurrency {
    /// ISO code, such as `USD`.
    pub code: &'static str,
    /// Full name (English).
    pub fullname: &'static str,
    /// ISO numeric code, stored as the commodity's exchange code.
    pub exchange_code: &'static str,
    /// Smallest fraction (the SCU).
    pub fraction: i64,
    /// Local symbol, such as `$`; may be empty.
    pub symbol: &'static str,
}

/// Old ISO codes GnuCash maps to their replacements on lookup (`5.17:libgnucash/engine/gnc-commodity.cpp:131-145`).
const RENAMED: [(&str, &str); 6] = [
    ("RUR", "RUB"),
    ("PLZ", "PLN"),
    ("UAG", "UAH"),
    ("NIS", "ILS"),
    ("MXP", "MXN"),
    ("TRL", "TRY"),
];

/// The current code for a possibly renamed ISO code.
pub fn canonical_code(code: &str) -> &str {
    RENAMED.iter().find(|(old, _)| *old == code).map_or(code, |(_, new)| new)
}

/// The built-in currency with this code (after renaming), if any.
pub fn iso_currency(code: &str) -> Option<IsoCurrency> {
    let code = canonical_code(code);
    CURRENCIES
        .iter()
        .find(|c| c.0 == code)
        .map(|&(code, fullname, exchange_code, fraction, symbol)| IsoCurrency {
            code,
            fullname,
            exchange_code,
            fraction,
            symbol,
        })
}

/// Every built-in currency, in ISO code order.
pub fn iso_currencies() -> impl Iterator<Item = IsoCurrency> {
    CURRENCIES
        .iter()
        .map(|&(code, fullname, exchange_code, fraction, symbol)| IsoCurrency {
            code,
            fullname,
            exchange_code,
            fraction,
            symbol,
        })
}

impl IsoCurrency {
    /// The commodity GnuCash builds for this currency, with no quote settings.
    pub fn to_commodity(self) -> Commodity {
        let mut c = Commodity::new(CommodityRef::currency(self.code), Some(self.fullname.to_owned()), self.fraction);
        c.cusip = Some(self.exchange_code.to_owned());
        c
    }
}

/// Units of each euro legacy currency per euro (`5.17:libgnucash/engine/gnc-euro.cpp:32-55`).
const EURO_RATES: [(&str, i64, i64); 21] = [
    ("ATS", 137_603, 10_000),
    ("BEF", 403_399, 10_000),
    ("BGN", 195_583, 100_000),
    ("CYP", 585_274, 1_000_000),
    ("DEM", 195_583, 100_000),
    ("EEK", 156_466, 10_000),
    ("ESP", 166_386, 1_000),
    ("EUR", 1, 1),
    ("FIM", 594_573, 100_000),
    ("FRF", 655_957, 100_000),
    ("GRD", 340_750, 1_000),
    ("HRK", 753_450, 100_000),
    ("IEP", 787_564, 1_000_000),
    ("ITL", 193_627, 100),
    ("LUF", 403_399, 10_000),
    ("LVL", 702_804, 1_000_000),
    ("MTL", 429_300, 1_000_000),
    ("NLG", 220_371, 100_000),
    ("PTE", 200_482, 1_000),
    ("SIT", 239_640, 1_000),
    ("SKK", 301_260, 10_000),
];

/// The fixed rate of a euro legacy currency (units per euro), or `None` if it is not one.
/// `gnc_euro_currency_get_rate`, `5.17:libgnucash/engine/gnc-euro.cpp:110`.
pub fn euro_rate(currency: &CommodityRef) -> Option<Numeric> {
    if !currency.is_currency() {
        return None;
    }
    EURO_RATES
        .iter()
        .find(|(c, _, _)| *c == currency.mnemonic)
        .map(|&(_, n, d)| Numeric::new(n, d))
}

/// True for the euro and the currencies it replaced at a fixed rate.
pub fn is_euro_currency(currency: &CommodityRef) -> bool {
    euro_rate(currency).is_some()
}

/// The euro legacy currencies and the euro, with their rates.
pub fn euro_rates() -> impl Iterator<Item = (&'static str, Numeric)> {
    EURO_RATES.iter().map(|&(c, n, d)| (c, Numeric::new(n, d)))
}

/// Convert an amount in a legacy currency to euros, rounded half away from zero to cents
/// (`gnc_convert_to_euro`, `5.17:libgnucash/engine/gnc-euro.cpp:80`).
pub fn convert_to_euro(currency: &CommodityRef, value: Numeric) -> Result<Numeric, NumericError> {
    match euro_rate(currency) {
        Some(rate) => value.div(rate, 100, crate::How::round(Round::HalfUp)),
        None => Ok(Numeric::new(0, 1)),
    }
}

/// Convert euros to a legacy currency with that currency's fraction (`gnc_convert_from_euro`, `gnc-euro.cpp:96`).
pub fn convert_from_euro(currency: &CommodityRef, value: Numeric) -> Result<Numeric, NumericError> {
    let fraction = iso_currency(&currency.mnemonic).map_or(100, |c| c.fraction);
    match euro_rate(currency) {
        Some(rate) => value.mul(rate, fraction, crate::How::round(Round::HalfUp)),
        None => Ok(Numeric::new(0, 1)),
    }
}

/// The kind of a quote source (`QuoteSourceType`, `5.17:libgnucash/engine/gnc-commodity.h:128-143`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteSourceType {
    /// One source for one market.
    Single,
    /// A group of sources.
    Multi,
    /// Not built in; kept as read.
    Unknown,
    /// The currency source.
    Currency,
}

const SINGLE_SOURCES: [&str; 42] = [
    "alphavantage",
    "amfiindia",
    "asegr",
    "asx",
    "borsa_italiana",
    "bseindia",
    "bvb",
    "cse",
    "comdirect",
    "consorsbank",
    "deka",
    "ftfunds",
    "finanzpartner",
    "finnhub",
    "fondsweb",
    "goldmoney",
    "googleweb",
    "marketwatch",
    "morningstarjp",
    "fool",
    "nzx",
    "nseindia",
    "onvista",
    "bourso",
    "sinvestor",
    "za",
    "six",
    "stockdata",
    "stooq",
    "swissfunddata",
    "tesouro_direto",
    "tsx",
    "tradegate",
    "treasurydirect",
    "twelvedata",
    "unionfunds",
    "usbonds",
    "tsp",
    "xetra",
    "yahoo_json",
    "yahooweb",
    "financeapi",
];

const MULTI_SOURCES: [&str; 7] = ["canada", "europe", "india", "nasdaq", "nyse", "ukfunds", "usa"];

/// The type of a quote source by internal name; names GnuCash does not know are `Unknown` and are kept as read.
/// `5.17:libgnucash/engine/gnc-commodity.cpp:180-247`.
pub fn quote_source_type(name: &str) -> QuoteSourceType {
    if name == "currency" {
        QuoteSourceType::Currency
    } else if SINGLE_SOURCES.contains(&name) {
        QuoteSourceType::Single
    } else if MULTI_SOURCES.contains(&name) {
        QuoteSourceType::Multi
    } else {
        QuoteSourceType::Unknown
    }
}

/// All built-in quote sources with their types, in GnuCash's menu order.
pub fn quote_sources() -> impl Iterator<Item = (&'static str, QuoteSourceType)> {
    std::iter::once(("currency", QuoteSourceType::Currency))
        .chain(SINGLE_SOURCES.iter().map(|s| (*s, QuoteSourceType::Single)))
        .chain(MULTI_SOURCES.iter().map(|s| (*s, QuoteSourceType::Multi)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_gnucash_517() {
        assert_eq!(iso_currencies().count(), 227);
        assert_eq!(iso_currency("KRW").unwrap().fraction, 1);
        assert_eq!(iso_currency("RUR").unwrap().code, "RUB");
        assert_eq!(iso_currency("USD").unwrap().exchange_code, "840");
    }

    #[test]
    fn euro_conversions_match_the_oracle() {
        let dem = CommodityRef::currency("DEM");
        assert!(
            convert_to_euro(&dem, Numeric::new(10_000, 100))
                .unwrap()
                .same_repr(Numeric::new(5113, 100))
        );
        let itl = CommodityRef::currency("ITL");
        assert!(
            convert_from_euro(&itl, Numeric::new(100, 100))
                .unwrap()
                .same_repr(Numeric::new(1936, 1))
        );
        assert!(
            convert_to_euro(&dem, Numeric::new(-9779, 1_000_000))
                .unwrap()
                .same_repr(Numeric::new(0, 100))
        );
        assert!(euro_rate(&CommodityRef::currency("USD")).is_none());
    }
}
