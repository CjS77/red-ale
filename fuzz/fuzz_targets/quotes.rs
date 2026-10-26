//! Quote service answers: the JSON reader that keeps numbers as text, then every response format, for a security
//! and for a rate. The first byte picks the `json` format's pointers.

#![no_main]

use libfuzzer_sys::fuzz_target;
use red_ale_core::TimeZone;
use red_ale_quotes::format::{Ask, Context, Format, Pointers, parse};

const FORMATS: [Format; 6] = [Format::AlphaVantage, Format::TwelveData, Format::Finnhub, Format::Frankfurter, Format::YahooChart, Format::Json];

fuzz_target!(|data: &[u8]| {
    let Some((first, body)) = data.split_first() else { return };
    let _ = red_ale_quotes::json_text::parse(body);
    let pointers = match first % 3 {
        0 => Pointers::default(),
        1 => Pointers { price: Some("/price".into()), currency: Some("/ccy".into()), date: Some("/date".into()) },
        _ => Pointers { price: Some("/data/0/p".into()), currency: Some("".into()), date: Some("/t".into()) },
    };
    let zone = TimeZone::utc();
    let asks = [Ask::Security { symbol: "IBM".into() }, Ask::Rate { base: "EUR".into(), quote: "USD".into() }];
    let currency = (first & 4 != 0).then_some("USD");
    asks.iter().for_each(|ask| {
        let cx = Context { ask, currency, pointers: &pointers, zone: &zone };
        FORMATS.iter().for_each(|format| {
            let _ = parse(*format, body, &cx);
        });
    });
});
