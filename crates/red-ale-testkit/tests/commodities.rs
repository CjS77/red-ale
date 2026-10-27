//! The commodity table, security editor, ISO currencies and euro conversion through the dispatcher.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_rpc::codes;
use red_ale_testkit::{call, call_err, create, open};
use serde_json::json;

#[test]
fn the_table_lists_book_commodities_and_built_in_currencies() {
    let (_book, mut d) = open("synthetic/rich.gnucash");
    let namespaces = call(&mut d, "commodity.namespaces", &json!({}));
    let names: Vec<&str> = namespaces.as_array().unwrap().iter().map(|n| n["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["CURRENCY", "FUND", "NASDAQ", "template"]);
    assert_eq!(namespaces[0]["commodities"], 227);
    assert_eq!(namespaces[0]["reserved"], true);
    let all = call(&mut d, "commodity.list", &json!({}));
    assert_eq!(all.as_array().unwrap().len(), 227 + 3);
    let used = call(&mut d, "commodity.list", &json!({ "usedOnly": true }));
    let ids: Vec<&str> = used.as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"CURRENCY::USD") && ids.contains(&"NASDAQ::ACME") && !ids.contains(&"CURRENCY::JPY"));
    let currencies = call(&mut d, "commodity.list", &json!({ "namespace": "ISO4217" }));
    assert_eq!(currencies.as_array().unwrap().len(), 227);
}

#[test]
fn get_reports_quotes_and_usage() {
    let (_book, mut d) = open("synthetic/rich.gnucash");
    let acme = call(&mut d, "commodity.get", &json!({ "commodity": "NASDAQ::ACME" }));
    assert_eq!(acme["fullname"], "Acme Corp");
    assert_eq!(acme["cusip"], "US0000001");
    assert_eq!(acme["quoteSource"], "yahoo_json");
    assert_eq!(acme["quoteSourceType"], "single");
    assert_eq!(acme["quoteTz"], "America/New_York");
    assert_eq!(acme["usage"]["prices"], 2);
    assert!(acme["usage"]["accounts"].as_u64().unwrap() >= 1);
    let jpy = call(&mut d, "commodity.get", &json!({ "commodity": "JPY" }));
    assert_eq!(jpy["builtIn"], true);
    assert_eq!(jpy["defined"], false);
    assert_eq!(jpy["fraction"], 1);
    assert_eq!(jpy["quoteFlag"], false);
    assert_eq!(jpy["defaultSymbol"], "JP¥");
    let rub = call(&mut d, "commodity.get", &json!({ "commodity": "RUR" }));
    assert_eq!(rub["id"], "CURRENCY::RUB");
    assert_eq!(
        call(&mut d, "commodity.get", &json!({ "commodity": "CURRENCY::KRW" }))["fraction"],
        1
    );
    assert_eq!(
        call_err(&mut d, "commodity.get", &json!({ "commodity": "NASDAQ::NOPE" })).code,
        codes::NOT_FOUND
    );
}

#[test]
fn securities_are_created_with_the_editor_checks() {
    let (_book, mut d) = create();
    let c = call(
        &mut d,
        "commodity.create",
        &json!({ "namespace": "NYSE ", "mnemonic": "IBM", "fullname": "Intl Business Machines", "cusip": "US4592001014",
                 "quoteFlag": true, "quoteSource": "my_private_src", "quoteTz": "Mars/Olympus" }),
    );
    assert_eq!(c["id"], "NYSE::IBM");
    assert_eq!(c["fraction"], 10_000);
    assert_eq!(c["quoteSourceType"], "unknown");
    assert_eq!(c["quoteTz"], "Mars/Olympus");
    let dup = json!({ "namespace": "NYSE", "mnemonic": "IBM", "fullname": "x" });
    assert_eq!(call_err(&mut d, "commodity.create", &dup).code, codes::CONFLICT);
    let refused = [
        json!({ "namespace": "CURRENCY", "mnemonic": "XYZ", "fullname": "x" }),
        json!({ "namespace": "template", "mnemonic": "XYZ", "fullname": "x" }),
        json!({ "namespace": "NYSE", "mnemonic": "XYZ", "fullname": "" }),
        json!({ "namespace": "NYSE", "mnemonic": "XYZ", "fullname": "x", "fraction": 0 }),
        json!({ "namespace": "NYSE", "mnemonic": "XYZ", "fullname": "x", "quoteSource": "currency" }),
    ];
    refused
        .iter()
        .for_each(|p| assert_eq!(call_err(&mut d, "commodity.create", p).code, codes::INVALID, "{p}"));
    assert_eq!(
        call_err(
            &mut d,
            "commodity.create",
            &json!({ "namespace": "N", "mnemonic": "M", "fullname": "x", "fraction": 1.5 })
        )
        .code,
        codes::INVALID_PARAMS
    );
}

#[test]
fn securities_are_edited_and_renamed_everywhere() {
    let (_book, mut d) = open("synthetic/rich.gnucash");
    let edited = call(
        &mut d,
        "commodity.update",
        &json!({ "commodity": "NASDAQ::ACME", "mnemonic": "ACM", "fraction": 100, "cusip": null, "quoteTz": null, "userSymbol": "A$" }),
    );
    assert_eq!(edited["id"], "NASDAQ::ACM");
    assert_eq!(edited["cusip"], serde_json::Value::Null);
    assert_eq!(edited["quoteTz"], serde_json::Value::Null);
    assert_eq!(edited["quoteSource"], "yahoo_json");
    assert_eq!(edited["userSymbol"], "A$");
    assert_eq!(edited["usage"]["prices"], 2);
    assert_eq!(
        call_err(&mut d, "commodity.get", &json!({ "commodity": "NASDAQ::ACME" })).code,
        codes::NOT_FOUND
    );
    let taken = json!({ "commodity": "NASDAQ::ACM", "namespace": "FUND", "mnemonic": "ZETA" });
    assert_eq!(call_err(&mut d, "commodity.update", &taken).code, codes::CONFLICT);
}

#[test]
fn currencies_take_only_quote_settings() {
    let (_book, mut d) = open("synthetic/rich.gnucash");
    let usd = call(&mut d, "commodity.update", &json!({ "commodity": "USD", "quoteFlag": false }));
    assert_eq!(usd["quoteFlag"], false);
    let gbp = call(
        &mut d,
        "commodity.update",
        &json!({ "commodity": "GBP", "quoteFlag": true, "userSymbol": "£" }),
    );
    assert_eq!(gbp["quoteFlag"], true);
    assert_eq!(gbp["defined"], true);
    assert_eq!(gbp["userSymbol"], serde_json::Value::Null, "the built-in symbol is not stored");
    let refused = json!({ "commodity": "USD", "fraction": 1000 });
    assert_eq!(call_err(&mut d, "commodity.update", &refused).code, codes::INVALID);
    assert_eq!(
        call_err(
            &mut d,
            "commodity.update",
            &json!({ "commodity": "template::template", "quoteFlag": true })
        )
        .code,
        codes::INVALID
    );
}

#[test]
fn iso_currencies_and_euro_conversion() {
    let (_book, mut d) = create();
    let all = call(&mut d, "currency.list", &json!({}));
    assert_eq!(all.as_array().unwrap().len(), 227);
    let bgn = all.as_array().unwrap().iter().find(|c| c["code"] == "BGN").unwrap();
    assert_eq!(bgn["euroRate"], json!({ "num": "195583", "denom": "100000" }));
    let krw = all.as_array().unwrap().iter().find(|c| c["code"] == "KRW").unwrap();
    assert_eq!(krw["fraction"], 1);
    let to = call(
        &mut d,
        "currency.euroConvert",
        &json!({ "amount": "100.00", "from": "DEM", "to": "EUR" }),
    );
    assert_eq!(to["amount"], json!({ "num": "5113", "denom": "100" }));
    let from = call(
        &mut d,
        "currency.euroConvert",
        &json!({ "amount": "1.00", "from": "EUR", "to": "ITL" }),
    );
    assert_eq!(from["amount"], json!({ "num": "1936", "denom": "1" }));
    assert_eq!(from["rate"], json!({ "num": "193627", "denom": "100" }));
    assert_eq!(
        call_err(
            &mut d,
            "currency.euroConvert",
            &json!({ "amount": "1", "from": "USD", "to": "EUR" })
        )
        .code,
        codes::INVALID
    );
}
