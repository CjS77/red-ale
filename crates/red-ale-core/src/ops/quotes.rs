//! Online quotes, as far as the Book sees them: which commodities a Quote fetch covers, and how a fetched quote
//! becomes a Price (`design/api/quotes.md`).
//!
//! The price building is translated from `GncQuotesImpl::parse_one_quote` and its helpers
//! (`5.17:libgnucash/app-utils/gnc-quotes.cpp:546-760`), Copyright (C) 2021 Geert Janssens, Copyright (C) 2022 John
//! Ralls. GPL-2.0-or-later; this translation is GPL-3.0-or-later. It sees only [`QuoteRecord`]s: the fields
//! Finance::Quote hands GnuCash, whatever service a quote came from.

use std::sync::LazyLock;

use regex::Regex;

use crate::commodity::TEMPLATE_NAMESPACE;
use crate::numeric::text;
use crate::ops::commodities::{CURRENCY_QUOTE_SOURCE, effective_quote, lookup, table, usage_table};
use crate::ops::prices::FQ_SOURCE;
use crate::{Book, CommodityRef, Date, DomainError, DomainResult, Guid, Price, Time64, TimeZone};

/// One commodity's answer, with the fields `parse_quote_json` reads (`gnc-quotes.cpp:575-597`). Every value is text,
/// as Finance::Quote's JSON reaches GnuCash through a `property_tree`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuoteRecord {
    /// The quote succeeded.
    pub success: bool,
    /// Why it failed, when it did.
    pub errormsg: Option<String>,
    /// The last trade price.
    pub last: Option<String>,
    /// The net asset value, for funds.
    pub nav: Option<String>,
    /// A price of no stated kind.
    pub price: Option<String>,
    /// The value is the rate the other way round, to be inverted.
    pub inverted: bool,
    /// The quote's date, month/day/year.
    pub date: Option<String>,
    /// The currency of the value, an ISO 4217 code in any case.
    pub currency: Option<String>,
}

impl QuoteRecord {
    /// A successful record with a `last` value.
    pub fn last(value: impl Into<String>, currency: Option<String>, date: Option<String>) -> Self {
        Self {
            success: true,
            last: Some(value.into()),
            currency,
            date,
            ..Self::default()
        }
    }

    /// A failed record with Finance::Quote's error message.
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            success: false,
            errormsg: Some(message.into()),
            ..Self::default()
        }
    }
}

/// Why no Price was built: `GncQuoteError` (`5.17:libgnucash/app-utils/gnc-quotes.hpp:39-49`), less
/// `UNKNOWN_PRICE_TYPE`, which is never raised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuoteFailure {
    /// No answer for the commodity (`NO_RESULT`).
    NoResult,
    /// The answer says the quote failed, with its message (`QUOTE_FAILED`).
    QuoteFailed(Option<String>),
    /// The answer has no currency (`NO_CURRENCY`).
    NoCurrency,
    /// The answer's currency is not an ISO currency of the Book's table (`UNKNOWN_CURRENCY`).
    UnknownCurrency(String),
    /// The answer has no `last`, `nav` or `price` (`NO_PRICE`).
    NoPrice,
    /// The value is not a number GnuCash can read exactly (`PRICE_PARSE_FAILURE`).
    PriceParseFailure(String),
}

impl QuoteFailure {
    /// The stable name `price.fetchStatus` reports.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoResult => "no-result",
            Self::QuoteFailed(_) => "quote-failed",
            Self::NoCurrency => "no-currency",
            Self::UnknownCurrency(_) => "unknown-currency",
            Self::NoPrice => "no-price",
            Self::PriceParseFailure(_) => "price-parse-failure",
        }
    }

    /// The detail, where there is one.
    pub fn message(&self) -> Option<String> {
        match self {
            Self::QuoteFailed(message) => message.clone(),
            Self::UnknownCurrency(code) => Some(format!("the quote's currency {code:?} is not an ISO currency")),
            Self::PriceParseFailure(value) => Some(format!("the quote's value {value:?} is not a number")),
            Self::NoResult | Self::NoCurrency | Self::NoPrice => None,
        }
    }
}

/// A commodity a Quote fetch covers, with the quote source it is fetched by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quotable {
    /// The commodity.
    pub commodity: CommodityRef,
    /// Its quote source's internal name; `currency` for every currency, as GnuCash asks for currencies under that
    /// name whatever their own source (`comm_vec_to_json_string`, `gnc-quotes.cpp:455-466`).
    pub source: String,
}

/// The commodities GnuCash never fetches: the default currency, and any currency with mnemonic `XXX`
/// (`gnc-quotes.cpp:459-463`, `:688-690`).
pub fn never_fetched(id: &CommodityRef, default_currency: &CommodityRef) -> bool {
    id == default_currency || id.mnemonic == "XXX"
}

/// Every commodity with quotes on and a quote source, in id order (namespace, then mnemonic), as
/// `gnc_quotes_get_quotable_commodities` collects them (`gnc-quotes.cpp:978-1057`), less those never fetched. With
/// `namespaces`, only namespaces it matches. Whether a source is supported is the Configuration's business, so
/// commodities with any source are listed.
pub fn quotable(book: &Book, default_currency: &CommodityRef, namespaces: Option<&Regex>) -> Vec<Quotable> {
    let usage = usage_table(book);
    table(book)
        .iter()
        .filter(|c| c.id.namespace != TEMPLATE_NAMESPACE && namespaces.is_none_or(|re| re.is_match(&c.id.namespace)))
        .filter(|c| !never_fetched(&c.id, default_currency))
        .filter_map(|c| {
            let quote = effective_quote(c, usage.get(&c.id).map_or(0, |u| u.accounts));
            quote.flag.then(|| quote.source.map(|source| quotable_as(&c.id, source))).flatten()
        })
        .collect()
}

/// A commodity named in a fetch, whatever its quote flag: `None` when it is never fetched; `invalid` when it has
/// no quote source; `not-found` when it does not exist.
pub fn named_quotable(book: &Book, id: &CommodityRef, default_currency: &CommodityRef) -> DomainResult<Option<Quotable>> {
    let commodity = lookup(book, id).ok_or_else(|| DomainError::not_found("commodity", id))?;
    if never_fetched(&commodity.id, default_currency) {
        return Ok(None);
    }
    let users = usage_table(book).get(&commodity.id).map_or(0, |u| u.accounts);
    let source = effective_quote(&commodity, users).source;
    let source = source.ok_or_else(|| DomainError::invalid(format!("commodity {} has no quote source", commodity.id)))?;
    Ok(Some(quotable_as(&commodity.id, source)))
}

fn quotable_as(id: &CommodityRef, source: String) -> Quotable {
    let source = if id.is_currency() {
        CURRENCY_QUOTE_SOURCE.to_owned()
    } else {
        source
    };
    Quotable {
        commodity: id.clone(),
        source,
    }
}

/// Build the Price for `commodity` from its answer, as `parse_one_quote` does (`gnc-quotes.cpp:680-754`): `record`
/// is `None` when no answer came. A quote with no usable date is stamped `now`.
pub fn build_price(
    book: &Book,
    commodity: &CommodityRef,
    record: Option<&QuoteRecord>,
    now: Time64,
    zone: &TimeZone,
    guid: Guid,
) -> Result<Price, QuoteFailure> {
    let record = record.ok_or(QuoteFailure::NoResult)?;
    if !record.success {
        return Err(QuoteFailure::QuoteFailed(record.errormsg.clone()));
    }
    let (price_type, text) = price_and_type(record).ok_or(QuoteFailure::NoPrice)?;
    let value = text::parse(text).ok_or_else(|| QuoteFailure::PriceParseFailure(text.to_owned()))?;
    // GncNumeric::inv (`5.17:libgnucash/engine/gnc-numeric.cpp:422-430`): swapped, unreduced, zero kept.
    let value = if record.inverted { value.invert().unwrap_or(value) } else { value };
    let currency = currency(book, record)?;
    let time = price_time(record, now, zone);
    Ok(Price {
        guid,
        commodity: commodity.clone(),
        currency,
        time,
        source: Some(FQ_SOURCE.to_owned()),
        price_type: Some(price_type.to_owned()),
        value,
    })
}

/// `get_price_and_type` (`gnc-quotes.cpp:553-573`): `last`, else `nav`, else `price` with type `unknown`, as the
/// old Guile code named it.
fn price_and_type(record: &QuoteRecord) -> Option<(&'static str, &str)> {
    [("last", &record.last), ("nav", &record.nav), ("unknown", &record.price)]
        .into_iter()
        .find_map(|(kind, value)| value.as_deref().map(|v| (kind, v)))
}

/// `get_currency` (`gnc-quotes.cpp:652-678`): the answer's currency, upper-cased, among the ISO currencies.
fn currency(book: &Book, record: &QuoteRecord) -> Result<CommodityRef, QuoteFailure> {
    let code = record.currency.as_deref().ok_or(QuoteFailure::NoCurrency)?;
    let id = CommodityRef::currency(code.to_uppercase());
    lookup(book, &id)
        .map(|c| c.id.clone())
        .ok_or_else(|| QuoteFailure::UnknownCurrency(code.to_owned()))
}

/// `calc_price_time` (`gnc-quotes.cpp:599-630`): the date at its day-neutral time, else now. Finance::Quote's
/// `time` is made up, so it is never read.
fn price_time(record: &QuoteRecord, now: Time64, zone: &TimeZone) -> Time64 {
    record
        .date
        .as_deref()
        .filter(|d| !d.is_empty())
        .and_then(parse_mdy)
        .map_or(now, |d| zone.neutral_time(d))
}

/// `GncDate(text, "m-d-y")`'s regular expression (`5.17:libgnucash/engine/gnc-datetime.cpp:122-135`, `:728-754`):
/// month, day and year separated by any of `-/.'` and space, or `MMDDCCYY`. A year below 69 is 20xx, one below 100
/// is 19xx. Boost's `from_us_string`, which GnuCash tries first, reads the same digits; month names, which only it
/// reads, never reach here, since every date red-ale hands on is numeric.
pub fn parse_mdy(text: &str) -> Option<Date> {
    static SEPARATED: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new("([0-9]+)[-/.' ]+([0-9]+)[-/.' ]+([0-9]+)").ok());
    static COMPACT: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new("([0-9]{2})([0-9]{2})([0-9]{4})").ok());
    let caps = [&SEPARATED, &COMPACT]
        .into_iter()
        .find_map(|re| re.as_ref().and_then(|re| re.captures(text)))?;
    let number = |i: usize| caps.get(i).and_then(|m| m.as_str().parse::<u32>().ok());
    let year = i32::try_from(number(3)?).ok()?;
    let year = match year {
        y if y < 69 => y + 2000,
        y if y < 100 => y + 1900,
        y => y,
    };
    Date::from_ymd(year, number(1)?, number(2)?)
}

/// A date as the month/day/year text a [`QuoteRecord`] carries.
pub fn mdy(date: Date) -> String {
    format!("{:02}/{:02}/{:04}", date.month(), date.day(), date.year())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::commodities::{NewSecurity, create_security};
    use crate::{FixedEnv, Numeric};

    fn book() -> Book {
        let mut env = FixedEnv::new(Time64(0), 1);
        let mut book = Book::new_empty(&mut env);
        let security = |ns: &str, m: &str, source: Option<&str>, flag: bool| NewSecurity {
            namespace: ns.into(),
            mnemonic: m.into(),
            fullname: m.into(),
            fraction: 10000,
            quote_flag: flag,
            quote_source: source.map(Into::into),
            ..NewSecurity::default()
        };
        [
            security("NYSE", "HPE", Some("alphavantage"), true),
            security("NASDAQ", "AAPL", Some("alphavantage"), true),
            security("NASDAQ", "IDLE", Some("alphavantage"), false),
            security("FUND", "ZETA", None, true),
        ]
        .iter()
        .for_each(|s| drop(create_security(&mut book, s).unwrap()));
        book
    }

    fn usd() -> CommodityRef {
        CommodityRef::currency("USD")
    }

    fn hpe() -> CommodityRef {
        CommodityRef::new("NYSE", "HPE")
    }

    /// 2022-09-01 12:00 UTC.
    const NOW: Time64 = Time64(1_662_033_600);

    fn build(record: &QuoteRecord) -> Result<Price, QuoteFailure> {
        build_price(&book(), &hpe(), Some(record), NOW, &TimeZone::utc(), Guid::from_bytes([7; 16]))
    }

    #[test]
    fn a_dated_quote_is_stored_at_the_neutral_time() {
        // gtest-gnc-quotes.cpp:250-283, fetch_one_commodity.
        let p = build(&QuoteRecord::last("13.37", Some("USD".into()), Some("09/01/2022".into()))).unwrap();
        assert_eq!(p.currency, usd());
        assert_eq!(p.time, Date::from_ymd(2022, 9, 1).unwrap().neutral_time());
        assert_eq!(p.source.as_deref(), Some("Finance::Quote"));
        assert_eq!(p.price_type.as_deref(), Some("last"));
        assert!(p.value.same_repr(Numeric::new(1337, 100)));
    }

    #[test]
    fn values_keep_their_places_and_invert_exactly() {
        // gtest-gnc-quotes.cpp:285-313, fetch_one_currency: 1.0004 is 10004/10000, stamped now.
        let p = build(&QuoteRecord::last("1.0004", Some("usd".into()), None)).unwrap();
        assert!(p.value.same_repr(Numeric::new(10004, 10000)));
        assert_eq!(p.time, NOW);
        let inverted = QuoteRecord {
            inverted: true,
            ..QuoteRecord::last("1.0832", Some("USD".into()), None)
        };
        assert!(build(&inverted).unwrap().value.same_repr(Numeric::new(10000, 10832)));
    }

    #[test]
    fn types_follow_the_field_found() {
        let nav = QuoteRecord {
            success: true,
            nav: Some("2.031".into()),
            currency: Some("USD".into()),
            ..QuoteRecord::default()
        };
        assert_eq!(build(&nav).unwrap().price_type.as_deref(), Some("nav"));
        let price = QuoteRecord {
            success: true,
            price: Some("2".into()),
            currency: Some("USD".into()),
            ..QuoteRecord::default()
        };
        assert_eq!(build(&price).unwrap().price_type.as_deref(), Some("unknown"));
    }

    #[test]
    fn each_failure() {
        // gtest-gnc-quotes.cpp:315-375.
        let b = book();
        assert_eq!(
            build_price(&b, &hpe(), None, NOW, &TimeZone::utc(), Guid::from_bytes([1; 16])),
            Err(QuoteFailure::NoResult)
        );
        assert_eq!(
            build(&QuoteRecord::failed("no listing")),
            Err(QuoteFailure::QuoteFailed(Some("no listing".into())))
        );
        assert_eq!(build(&QuoteRecord::last("13.37", None, None)), Err(QuoteFailure::NoCurrency));
        assert_eq!(
            build(&QuoteRecord::last("13.37", Some("BTC".into()), None)),
            Err(QuoteFailure::UnknownCurrency("BTC".into()))
        );
        let empty = QuoteRecord {
            success: true,
            currency: Some("USD".into()),
            ..QuoteRecord::default()
        };
        assert_eq!(build(&empty), Err(QuoteFailure::NoPrice));
        assert_eq!(
            build(&QuoteRecord::last("lots", Some("USD".into()), None)),
            Err(QuoteFailure::PriceParseFailure("lots".into()))
        );
        assert_eq!(
            build(&QuoteRecord::last("99999999999999999999", Some("USD".into()), None))
                .unwrap_err()
                .code(),
            "price-parse-failure"
        );
    }

    #[test]
    fn a_bad_date_means_now() {
        let p = build(&QuoteRecord::last("1", Some("USD".into()), Some("13/45/2022".into()))).unwrap();
        assert_eq!(p.time, NOW);
    }

    #[test]
    fn dates_read_month_first() {
        assert_eq!(parse_mdy("09/01/2022"), Date::from_ymd(2022, 9, 1));
        assert_eq!(parse_mdy("9-1-22"), Date::from_ymd(2022, 9, 1));
        assert_eq!(parse_mdy("12.31.99"), Date::from_ymd(1999, 12, 31));
        assert_eq!(parse_mdy("09012022"), Date::from_ymd(2022, 9, 1));
        assert_eq!(parse_mdy("2022"), None);
        assert_eq!(mdy(Date::from_ymd(2024, 5, 3).unwrap()), "05/03/2024");
    }

    #[test]
    fn quotable_commodities_in_id_order() {
        let mut b = book();
        let mut eur = b.commodity(&CommodityRef::currency("EUR")).cloned().unwrap_or_else(|| {
            crate::currencies::iso_currency("EUR")
                .map(crate::currencies::IsoCurrency::to_commodity)
                .unwrap()
        });
        eur.quote_flag = true;
        b.commodities.insert(eur.id.clone(), std::sync::Arc::new(eur));
        let ids = |q: Vec<Quotable>| q.into_iter().map(|q| format!("{} {}", q.commodity, q.source)).collect::<Vec<_>>();
        assert_eq!(
            ids(quotable(&b, &usd(), None)),
            ["CURRENCY::EUR currency", "NASDAQ::AAPL alphavantage", "NYSE::HPE alphavantage"]
        );
        let ny = regex::RegexBuilder::new("^ny").case_insensitive(true).build().unwrap();
        assert_eq!(ids(quotable(&b, &usd(), Some(&ny))), ["NYSE::HPE alphavantage"]);
        assert_eq!(ids(quotable(&b, &CommodityRef::currency("EUR"), None)).len(), 2);
    }

    #[test]
    fn named_commodities_need_a_source() {
        let b = book();
        assert_eq!(
            named_quotable(&b, &CommodityRef::new("NASDAQ", "AAPL"), &usd())
                .unwrap()
                .unwrap()
                .source,
            "alphavantage"
        );
        // A security keeps its source only with quotes on, as GnuCash writes it.
        assert!(matches!(
            named_quotable(&b, &CommodityRef::new("NASDAQ", "IDLE"), &usd()),
            Err(DomainError::Invalid(_))
        ));
        assert_eq!(
            named_quotable(&b, &CommodityRef::currency("GBP"), &usd()).unwrap().unwrap().source,
            "currency"
        );
        assert_eq!(named_quotable(&b, &usd(), &usd()).unwrap(), None);
        assert!(matches!(
            named_quotable(&b, &CommodityRef::new("FUND", "ZETA"), &usd()),
            Err(DomainError::Invalid(_))
        ));
        assert!(matches!(
            named_quotable(&b, &CommodityRef::new("FUND", "NONE"), &usd()),
            Err(DomainError::NotFound { .. })
        ));
    }
}
