//! The price database, price editor and remove-old-prices through the dispatcher.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_rpc::codes;
use red_ale_testkit::{call, call_err, create, open};
use serde_json::{Value, json};

fn add(d: &mut red_ale_rpc::Dispatcher, params: &Value) -> Value {
    call(d, "price.add", params)
}

#[test]
fn prices_are_listed_in_gnucash_order() {
    let (_book, mut d) = open("synthetic/rich.gnucash");
    let all = call(&mut d, "price.list", &json!({}));
    let commodities: Vec<&str> = all.as_array().unwrap().iter().map(|p| p["commodity"].as_str().unwrap()).collect();
    assert_eq!(commodities, ["CURRENCY::EUR", "FUND::ZETA", "NASDAQ::ACME", "NASDAQ::ACME"]);
    assert_eq!(all[2]["date"], "2024-06-05", "newest first");
    assert_eq!(all[2]["value"], json!({ "num": "12000", "denom": "10" }), "kept exactly as stored");
    let acme = call(
        &mut d,
        "price.list",
        &json!({ "commodity": "NASDAQ::ACME", "currency": "USD", "to": "2024-06-04" }),
    );
    assert_eq!(acme.as_array().unwrap().len(), 1);
    assert_eq!(acme[0]["source"], "user:price");
    let reverse = call(
        &mut d,
        "price.list",
        &json!({ "commodity": "USD", "currency": "EUR", "bothDirections": true }),
    );
    assert_eq!(reverse.as_array().unwrap().len(), 1);
    assert_eq!(
        call(&mut d, "price.list", &json!({ "commodity": "USD", "currency": "EUR" })),
        json!([])
    );
    assert_eq!(
        call_err(&mut d, "price.list", &json!({ "currency": "EUR" })).code,
        codes::INVALID_PARAMS
    );
    let one = call(&mut d, "price.get", &json!({ "price": all[0]["guid"] }));
    assert_eq!(one["type"], "transaction");
}

#[test]
fn adding_follows_same_day_precedence() {
    let (_book, mut d) = create();
    let first = add(
        &mut d,
        &json!({ "commodity": "EUR", "currency": "USD", "value": "1.10", "date": "2024-01-10", "source": "user:xfer-dialog" }),
    );
    assert_eq!(first["outcome"], "added");
    assert_eq!(first["price"]["time"], "2024-01-10T10:59:00Z");
    assert_eq!(first["price"]["source"], "user:xfer-dialog");
    let editor = add(
        &mut d,
        &json!({ "commodity": "USD", "currency": "EUR", "value": "0.91", "date": "2024-01-10", "type": "last" }),
    );
    assert_eq!(editor["outcome"], "replaced");
    assert_eq!(editor["replaced"], first["price"]["guid"]);
    assert_eq!(editor["price"]["source"], "user:price-editor");
    let fq = add(
        &mut d,
        &json!({ "commodity": "EUR", "currency": "USD", "value": "1.1", "time": "2024-01-10T18:00:00+02:00", "source": "Finance::Quote" }),
    );
    assert_eq!(fq["outcome"], "rejected");
    assert_eq!(fq["existing"], editor["price"]["guid"]);
    let refused = [
        json!({ "commodity": "EUR", "currency": "USD", "value": "1", "date": "2024-01-11", "source": "my-script" }),
        json!({ "commodity": "EUR", "currency": "EUR", "value": "1", "date": "2024-01-11" }),
    ];
    refused
        .iter()
        .for_each(|p| assert_eq!(call_err(&mut d, "price.add", p).code, codes::INVALID, "{p}"));
    assert_eq!(
        call_err(
            &mut d,
            "price.add",
            &json!({ "commodity": "X::Y", "currency": "USD", "value": "1", "date": "2024-01-11" })
        )
        .code,
        codes::NOT_FOUND
    );
    let both = json!({ "commodity": "EUR", "currency": "USD", "value": "1", "date": "2024-01-11", "time": "2024-01-11T00:00:00Z" });
    assert_eq!(call_err(&mut d, "price.add", &both).code, codes::INVALID_PARAMS);
    assert_eq!(
        call_err(
            &mut d,
            "price.add",
            &json!({ "commodity": "EUR", "currency": "USD", "value": 1, "date": "2024-01-11" })
        )
        .code,
        codes::INVALID_PARAMS
    );
}

#[test]
fn lookups_look_both_ways() {
    let (_book, mut d) = create();
    add(
        &mut d,
        &json!({ "commodity": "EUR", "currency": "USD", "value": "1.10", "date": "2024-01-01" }),
    );
    add(
        &mut d,
        &json!({ "commodity": "USD", "currency": "EUR", "value": "0.8", "date": "2024-01-05" }),
    );
    let latest = call(&mut d, "price.latest", &json!({ "commodity": "EUR", "currency": "USD" }));
    assert_eq!(latest["reversed"], true);
    assert_eq!(latest["rate"], json!({ "num": "10", "denom": "8" }));
    let before = call(
        &mut d,
        "price.nearest",
        &json!({ "commodity": "EUR", "currency": "USD", "date": "2024-01-04", "mode": "before" }),
    );
    assert_eq!(before["reversed"], false);
    assert_eq!(before["price"]["date"], "2024-01-01");
    let near = call(
        &mut d,
        "price.nearest",
        &json!({ "commodity": "EUR", "currency": "USD", "date": "2024-01-04" }),
    );
    assert_eq!(near["price"]["date"], "2024-01-05");
    let none = call(
        &mut d,
        "price.nearest",
        &json!({ "commodity": "EUR", "currency": "USD", "date": "2024-01-04", "mode": "sameDay" }),
    );
    assert_eq!(none, Value::Null);
    assert_eq!(
        call(&mut d, "price.latest", &json!({ "commodity": "GBP", "currency": "USD" })),
        Value::Null
    );
}

#[test]
fn the_price_editor_edits_and_deletes() {
    let (_book, mut d) = create();
    let a = add(
        &mut d,
        &json!({ "commodity": "EUR", "currency": "USD", "value": "1.10", "date": "2024-01-01", "type": "last" }),
    )["price"]["guid"]
        .clone();
    let b = add(
        &mut d,
        &json!({ "commodity": "EUR", "currency": "USD", "value": "1.20", "date": "2024-01-02", "source": "Finance::Quote" }),
    )["price"]["guid"]
        .clone();
    let edited = call(
        &mut d,
        "price.update",
        &json!({ "price": a, "value": "1.105", "type": null, "source": "user:price" }),
    );
    assert_eq!(edited["price"]["value"], json!({ "num": "1105", "denom": "1000" }));
    assert_eq!(edited["price"]["type"], Value::Null);
    assert_eq!(edited["replaced"], Value::Null);
    let onto_worse = call(
        &mut d,
        "price.update",
        &json!({ "price": b, "date": "2024-01-01", "source": "user:price-editor" }),
    );
    assert_eq!(onto_worse["replaced"], a);
    assert_eq!(call_err(&mut d, "price.get", &json!({ "price": a })).code, codes::NOT_FOUND);
    let c = add(
        &mut d,
        &json!({ "commodity": "EUR", "currency": "USD", "value": "1.3", "date": "2024-01-03", "source": "Finance::Quote" }),
    )["price"]["guid"]
        .clone();
    let onto_better = json!({ "price": c, "date": "2024-01-01" });
    assert_eq!(call_err(&mut d, "price.update", &onto_better).code, codes::CONFLICT);
    assert_eq!(call(&mut d, "price.delete", &json!({ "price": c }))["guid"], c);
    assert_eq!(call_err(&mut d, "price.delete", &json!({ "price": c })).code, codes::NOT_FOUND);
}

#[test]
fn remove_old_prices_keeps_the_last_of_each_period() {
    let (_book, mut d) = create();
    [
        "2021-01-15",
        "2022-01-10",
        "2022-01-20",
        "2023-01-05",
        "2023-06-01",
        "2023-06-02",
        "2024-06-01",
    ]
    .iter()
    .for_each(|day| {
        add(
            &mut d,
            &json!({ "commodity": "EUR", "currency": "USD", "value": "1", "date": day, "source": "Finance::Quote" }),
        );
    });
    add(
        &mut d,
        &json!({ "commodity": "EUR", "currency": "USD", "value": "1", "date": "2020-01-01" }),
    );
    let removed = call(
        &mut d,
        "price.removeOld",
        &json!({ "commodities": ["EUR"], "before": "2024-01-01", "keep": "lastOfMonth" }),
    );
    assert_eq!(removed["removed"].as_array().unwrap().len(), 4);
    let left: Vec<String> = call(&mut d, "price.list", &json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["date"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        left,
        ["2024-06-01", "2023-06-02", "2023-01-05", "2020-01-01"],
        "the editor's price is not a fetched one"
    );
    let all = call(
        &mut d,
        "price.removeOld",
        &json!({ "commodities": ["EUR"], "before": "2024-06-01", "keep": "none", "sources": ["fetched", "app"] }),
    );
    assert_eq!(all["removed"].as_array().unwrap().len(), 3);
    assert_eq!(
        call_err(&mut d, "price.removeOld", &json!({ "commodities": [], "before": "2024-01-01" })).code,
        codes::INVALID_PARAMS
    );
}

#[test]
fn deleting_a_commodity_takes_its_prices_only_when_asked() {
    let (_book, mut d) = open("synthetic/rich.gnucash");
    assert_eq!(
        call_err(
            &mut d,
            "commodity.delete",
            &json!({ "commodity": "NASDAQ::ACME", "deletePrices": true })
        )
        .code,
        codes::CONFLICT
    );
    assert_eq!(
        call_err(&mut d, "commodity.delete", &json!({ "commodity": "USD" })).code,
        codes::INVALID
    );
    call(
        &mut d,
        "commodity.create",
        &json!({ "namespace": "X", "mnemonic": "Y", "fullname": "y" }),
    );
    call(
        &mut d,
        "price.add",
        &json!({ "commodity": "X::Y", "currency": "USD", "value": "2", "date": "2024-01-01" }),
    );
    assert_eq!(
        call_err(&mut d, "commodity.delete", &json!({ "commodity": "X::Y" })).code,
        codes::CONFLICT
    );
    let done = call(&mut d, "commodity.delete", &json!({ "commodity": "X::Y", "deletePrices": true }));
    assert_eq!(done["deletedPrices"].as_array().unwrap().len(), 1);
    assert_eq!(
        call_err(&mut d, "commodity.get", &json!({ "commodity": "X::Y" })).code,
        codes::NOT_FOUND
    );
}
