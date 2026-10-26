//! No answer makes a parser panic (`design/milestone3.md`, "No crashes"): arbitrary bytes, arbitrary JSON built
//! from the keys the services use, and recorded answers with bytes changed, read in every format, for a security
//! and for a rate, with arbitrary JSON Pointers.

use proptest::prelude::*;
use serde_json::{Map, Value};

use super::fixtures::fixture;
use super::*;

const FIXTURES: [&str; 19] = [
    "alphavantage-empty.json",
    "alphavantage-error.json",
    "alphavantage-exchange-rate.json",
    "alphavantage-global-quote.json",
    "alphavantage-global-quote-london.json",
    "alphavantage-rate-limit.json",
    "finnhub-quote.json",
    "finnhub-unknown.json",
    "frankfurter-latest.json",
    "frankfurter-not-found.json",
    "json-proxy.json",
    "twelvedata-exchange-rate.json",
    "twelvedata-not-found.json",
    "twelvedata-quote.json",
    "twelvedata-rate-limit.json",
    "yahoo-chart-error.json",
    "yahoo-chart.json",
    "yahoo-chart-pence.json",
    "yahoo-chart-rate.json",
];

/// Keys the services' answers use, so generated JSON reaches past each parser's first lookup.
const KEYS: [&str; 24] = [
    "Global Quote",
    "05. price",
    "07. latest trading day",
    "Realtime Currency Exchange Rate",
    "5. Exchange Rate",
    "6. Last Refreshed",
    "Note",
    "Information",
    "close",
    "rate",
    "currency",
    "datetime",
    "timestamp",
    "status",
    "code",
    "c",
    "t",
    "rates",
    "date",
    "chart",
    "result",
    "meta",
    "regularMarketPrice",
    "regularMarketTime",
];

const FORMATS: [Format; 6] = [Format::AlphaVantage, Format::TwelveData, Format::Finnhub, Format::Frankfurter, Format::YahooChart, Format::Json];

fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        "-?[0-9]{0,12}(\\.[0-9]{0,8})?(e-?[0-9]{1,3})?".prop_map(Value::String),
        "\\PC{0,20}".prop_map(Value::String),
        any::<i64>().prop_map(Value::from),
    ];
    leaf.prop_recursive(4, 48, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::vec((prop::sample::select(KEYS.to_vec()), inner), 0..6)
                .prop_map(|entries| Value::Object(entries.into_iter().map(|(k, v)| (k.to_owned(), v)).collect::<Map<_, _>>())),
        ]
    })
}

fn pointers() -> impl Strategy<Value = Pointers> {
    let pointer = prop::option::of(prop_oneof!["(/[a-z0-9~]{0,6}){0,4}", "\\PC{0,12}"]);
    (pointer.clone(), pointer.clone(), pointer).prop_map(|(price, currency, date)| Pointers { price, currency, date })
}

/// Read `body` in every format, as a security and as a rate.
fn read_everywhere(body: &[u8], pointers: &Pointers, currency: Option<&str>) {
    let zone = TimeZone::utc();
    let asks = [Ask::Security { symbol: "IBM".into() }, Ask::Rate { base: "EUR".into(), quote: "USD".into() }];
    asks.iter().for_each(|ask| {
        let cx = Context { ask, currency, pointers, zone: &zone };
        FORMATS.iter().for_each(|f| {
            let _ = parse(*f, body, &cx);
        });
    });
}

/// A recorded answer with `edits` applied: each replaces, inserts or deletes one byte at a position.
fn mutated(name: &str, edits: &[(usize, u8, u8)]) -> Vec<u8> {
    edits.iter().fold(fixture(name), |mut bytes, (at, op, byte)| {
        let at = at % (bytes.len() + 1);
        match op % 3 {
            0 if at < bytes.len() => bytes[at] = *byte,
            1 => bytes.insert(at, *byte),
            _ if at < bytes.len() => {
                bytes.remove(at);
            }
            _ => {}
        }
        bytes
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_bytes_never_panic(body in prop::collection::vec(any::<u8>(), 0..300), p in pointers()) {
        read_everywhere(&body, &p, None);
        let _ = crate::json_text::parse(&body);
    }

    #[test]
    fn arbitrary_json_never_panics(value in json_value(), p in pointers(), currency in prop::option::of("[A-Z]{0,4}")) {
        read_everywhere(value.to_string().as_bytes(), &p, currency.as_deref());
    }

    #[test]
    fn changed_recordings_never_panic(
        name in prop::sample::select(FIXTURES.to_vec()),
        edits in prop::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 1..8),
        p in pointers(),
    ) {
        read_everywhere(&mutated(name, &edits), &p, Some("USD"));
    }
}
