//! Online quotes end to end (`design/api/quotes.md`): `price.fetchQuotes` against Quote services served from
//! 127.0.0.1 with recorded answers. No test reaches the network.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use red_ale_rpc::{Dispatcher, codes};
use red_ale_testkit::quote_server::{Answer, QuoteServer};
use red_ale_testkit::{TEST_NOW, TempBook, call, call_err, create};
use serde_json::{Value, json};

/// A security priced by the local `json` service: 13.37 USD on 2022-09-01.
const HPE: &str = r#"{"data": {"price": 13.37, "ccy": "USD", "stamp": "2022-09-01"}}"#;

/// The Configuration of the tests: the network on, a `json` service for securities answering the `alphavantage`
/// source, and a `frankfurter` service for currencies, both on `server`. `more` adds or overrides settings.
fn configure(d: &mut Dispatcher, server: &QuoteServer, more: &Value) {
    let url = server.url();
    let mut changes = json!({
        "quotes.network": true,
        "quotes.timeoutSeconds": 5,
        "quotes.services.local": {
            "format": "json",
            "securityUrl": format!("{url}/quote/{{symbol}}"),
            "pointers": { "price": "/data/price", "currency": "/data/ccy", "date": "/data/stamp" },
        },
        "quotes.services.fx": { "format": "frankfurter", "currencyUrl": format!("{url}/fx/{{base}}/{{quote}}") },
        "quotes.sources.alphavantage": ["local"],
        "quotes.sources.currency": ["fx"],
    });
    more.as_object().unwrap().iter().for_each(|(k, v)| changes[k] = v.clone());
    call(d, "config.set", &json!({ "changes": changes }));
}

fn security(d: &mut Dispatcher, namespace: &str, mnemonic: &str, source: &str) {
    call(
        d,
        "commodity.create",
        &json!({ "namespace": namespace, "mnemonic": mnemonic, "fullname": mnemonic, "fraction": 10000,
        "quoteFlag": true, "quoteSource": source }),
    );
}

/// Start a fetch, wait for it, and return its final status.
fn fetch(d: &mut Dispatcher, params: &Value) -> Value {
    let started = call(d, "price.fetchQuotes", params);
    d.wait_for_quotes();
    call(d, "price.fetchStatus", &json!({ "fetch": started["fetch"] }))
}

fn entry<'a>(status: &'a Value, commodity: &str) -> &'a Value {
    status["commodities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["commodity"] == commodity)
        .unwrap_or_else(|| panic!("{commodity} in {status}"))
}

#[test]
fn prices_are_built_as_gnucash_builds_them() {
    let server = QuoteServer::start(vec![
        ("/quote/HPE", Answer::ok(HPE)),
        ("/fx/EUR/USD", Answer::recorded("frankfurter-latest.json")),
    ]);
    let (_book, mut d) = create();
    configure(&mut d, &server, &json!({}));
    security(&mut d, "NYSE", "HPE", "alphavantage");
    call(&mut d, "commodity.update", &json!({ "commodity": "EUR", "quoteFlag": true }));
    let started = call(&mut d, "price.fetchQuotes", &json!({}));
    assert_eq!(started["commodities"], json!(["CURRENCY::EUR", "NYSE::HPE"]));
    d.wait_for_quotes();
    let status = call(&mut d, "price.fetchStatus", &json!({}));
    assert_eq!(
        (status["state"].as_str(), status["stored"].as_u64(), status["failed"].as_u64()),
        (Some("finished"), Some(2), Some(0))
    );
    let hpe = entry(&status, "NYSE::HPE");
    assert_eq!(
        (hpe["state"].as_str(), hpe["outcome"].as_str(), hpe["service"].as_str()),
        (Some("stored"), Some("added"), Some("local"))
    );
    // gtest-gnc-quotes.cpp:250-283: the date at 10:59 UTC, the value exact, source Finance::Quote, type last.
    assert_eq!(hpe["price"]["value"], json!({ "num": "1337", "denom": "100" }));
    assert_eq!(hpe["price"]["time"], "2022-09-01T10:59:00Z");
    assert_eq!(
        (hpe["price"]["source"].as_str(), hpe["price"]["type"].as_str()),
        (Some("Finance::Quote"), Some("last"))
    );
    // A currency rate is undated, so stamped now, in the default currency (USD under the C locale).
    let eur = &entry(&status, "CURRENCY::EUR")["price"];
    assert_eq!(
        (eur["value"].clone(), eur["currency"].as_str()),
        (json!({ "num": "10795", "denom": "10000" }), Some("CURRENCY::USD"))
    );
    assert_eq!(eur["time"], json!(red_ale_rpc::wire::Timestamp(TEST_NOW)));
    let stored = call(&mut d, "price.list", &json!({ "commodity": "NYSE::HPE" }));
    assert_eq!(stored.as_array().unwrap().len(), 1);
    assert_eq!(server.hits().len(), 2);
}

#[test]
fn same_day_rules_apply_to_fetched_prices() {
    let server = QuoteServer::start(vec![("/quote/HPE", Answer::ok(HPE)), ("/quote/ACME", Answer::ok(HPE))]);
    let (_book, mut d) = create();
    configure(&mut d, &server, &json!({}));
    security(&mut d, "NYSE", "HPE", "alphavantage");
    security(&mut d, "NYSE", "ACME", "alphavantage");
    let editor = call(
        &mut d,
        "price.add",
        &json!({ "commodity": "NYSE::ACME", "currency": "USD", "value": "12", "date": "2022-09-01" }),
    );
    let first = fetch(&mut d, &json!({}));
    let acme = entry(&first, "NYSE::ACME");
    // The price editor's price outranks a fetched one (gnc-pricedb.cpp's source ranks).
    assert_eq!(
        (acme["outcome"].as_str(), acme["existing"]["guid"].clone()),
        (Some("rejected"), editor["price"]["guid"].clone())
    );
    assert_eq!(acme["price"], Value::Null);
    // A fetched price replaces a same-day one of equal rank (gnc_pricedb_add_price).
    let second = fetch(&mut d, &json!({ "commodities": ["NYSE::HPE"] }));
    let hpe = entry(&second, "NYSE::HPE");
    assert_eq!(hpe["outcome"], "replaced");
    assert_eq!(hpe["existing"]["guid"], entry(&first, "NYSE::HPE")["price"]["guid"]);
}

#[test]
fn services_are_tried_in_order_and_rates_both_ways_round() {
    let server = QuoteServer::start(vec![
        ("/broken/HPE", Answer::status(500, "{\"error\": \"down\"}")),
        ("/quote/HPE", Answer::ok(HPE)),
        ("/fx/EUR/USD", Answer::status(404, "{\"message\":\"not found\"}")),
        (
            "/fx/USD/EUR",
            Answer::ok(r#"{"amount":1.0,"base":"USD","date":"2024-05-03","rates":{"EUR":0.9264}}"#),
        ),
    ]);
    let (_book, mut d) = create();
    configure(
        &mut d,
        &server,
        &json!({
            "quotes.services.broken": { "format": "json", "securityUrl": format!("{}/broken/{{symbol}}", server.url()), "pointers": { "price": "/p" } },
            "quotes.sources.alphavantage": ["broken", "local"],
        }),
    );
    security(&mut d, "NYSE", "HPE", "alphavantage");
    let status = fetch(&mut d, &json!({ "commodities": ["NYSE::HPE", "EUR"] }));
    assert_eq!(entry(&status, "NYSE::HPE")["service"], "local");
    // finance-quote-wrapper.in:151-180: no EUR->USD rate, so USD->EUR inverted exactly.
    let eur = &entry(&status, "CURRENCY::EUR")["price"];
    assert_eq!(
        (eur["value"].clone(), eur["currency"].as_str()),
        (json!({ "num": "10000", "denom": "9264" }), Some("CURRENCY::USD"))
    );
    let targets: Vec<String> = server.hits().into_iter().map(|h| h.target).collect();
    assert_eq!(targets.iter().filter(|t| t.starts_with("/fx/")).count(), 2);
}

#[test]
fn each_failure_has_its_reason() {
    let slow = Answer::ok(HPE).after(Duration::from_secs(3));
    let server = QuoteServer::start(vec![
        ("/quote/SLOW", slow),
        ("/quote/DOWN", Answer::status(503, "<html>Service Unavailable</html>")),
        ("/quote/GONE", Answer::ok(r#"{"data": {}}"#)),
        ("/quote/COIN", Answer::ok(r#"{"data": {"price": "1", "ccy": "BTC"}}"#)),
        ("/quote/JUNK", Answer::ok(r#"{"data": {"price": "lots", "ccy": "USD"}}"#)),
    ]);
    let (_book, mut d) = create();
    configure(&mut d, &server, &json!({ "quotes.timeoutSeconds": 1 }));
    ["SLOW", "DOWN", "GONE", "COIN", "JUNK"]
        .iter()
        .for_each(|m| security(&mut d, "NYSE", m, "alphavantage"));
    security(&mut d, "FUND", "TIAA", "tiaacref");
    let status = fetch(&mut d, &json!({}));
    let reason = |c: &str| entry(&status, c)["reason"].as_str().unwrap().to_owned();
    assert_eq!(reason("NYSE::SLOW"), "timeout");
    assert_eq!(reason("NYSE::DOWN"), "service-error");
    assert!(entry(&status, "NYSE::DOWN")["message"].as_str().unwrap().contains("HTTP 503"));
    assert_eq!(reason("NYSE::GONE"), "quote-failed");
    assert_eq!(reason("NYSE::COIN"), "unknown-currency");
    assert_eq!(reason("NYSE::JUNK"), "price-parse-failure");
    // GnuCash leaves a commodity whose source it can't fetch out silently; red-ale says why.
    assert_eq!(reason("FUND::TIAA"), "unsupported-source");
    assert_eq!((status["stored"].as_u64(), status["failed"].as_u64()), (Some(0), Some(6)));
}

#[test]
fn with_the_network_off_nothing_connects() {
    let server = QuoteServer::start(vec![("/quote/HPE", Answer::ok(HPE))]);
    let (_book, mut d) = create();
    configure(&mut d, &server, &json!({ "quotes.network": false }));
    security(&mut d, "NYSE", "HPE", "alphavantage");
    let e = call_err(&mut d, "price.fetchQuotes", &json!({}));
    assert_eq!(e.code, codes::NETWORK_DISABLED);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!((server.connections(), server.hits().len()), (0, 0));
    assert_eq!(
        call_err(&mut d, "price.fetchStatus", &json!({})).data.unwrap()["kind"],
        "quoteFetch"
    );
}

#[test]
fn a_slow_fetch_holds_up_nothing_and_closing_the_book_cancels_it() {
    let server = QuoteServer::start(vec![("/quote/HPE", Answer::ok(HPE).after(Duration::from_secs(3)))]);
    let book = TempBook::new_path();
    let mut d = red_ale_testkit::dispatcher();
    call(&mut d, "book.create", &json!({ "path": book.path_str() }));
    configure(&mut d, &server, &json!({}));
    security(&mut d, "NYSE", "HPE", "alphavantage");
    call(&mut d, "book.save", &json!({}));
    let begun = Instant::now();
    let started = call(&mut d, "price.fetchQuotes", &json!({}));
    assert!(begun.elapsed() < Duration::from_secs(1), "price.fetchQuotes returns at once");
    call(&mut d, "account.list", &json!({}));
    let status = call(&mut d, "price.fetchStatus", &json!({}));
    assert_eq!(
        (status["state"].as_str(), entry(&status, "NYSE::HPE")["state"].as_str()),
        (Some("running"), Some("pending"))
    );
    assert_eq!(status["finished"], Value::Null);
    let e = call_err(&mut d, "price.fetchQuotes", &json!({}));
    assert_eq!(e.code, codes::CONFLICT);
    assert!(begun.elapsed() < Duration::from_secs(2), "nothing waited for the service");
    call(&mut d, "book.close", &json!({ "discard": true }));
    std::thread::sleep(Duration::from_secs(4));
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    assert_eq!(call(&mut d, "price.list", &json!({})), json!([]));
    assert_eq!(
        call_err(&mut d, "price.fetchStatus", &json!({ "fetch": started["fetch"] })).code,
        codes::NOT_FOUND
    );
}

#[test]
fn a_commodity_deleted_during_the_fetch_gets_no_price() {
    let server = QuoteServer::start(vec![("/quote/HPE", Answer::ok(HPE).after(Duration::from_millis(800)))]);
    let (_book, mut d) = create();
    configure(&mut d, &server, &json!({}));
    security(&mut d, "NYSE", "HPE", "alphavantage");
    call(&mut d, "price.fetchQuotes", &json!({}));
    call(&mut d, "commodity.delete", &json!({ "commodity": "NYSE::HPE" }));
    d.wait_for_quotes();
    let status = call(&mut d, "price.fetchStatus", &json!({}));
    assert_eq!(entry(&status, "NYSE::HPE")["reason"], "not-found");
    assert_eq!(call(&mut d, "price.list", &json!({})), json!([]));
}

#[test]
fn each_service_keeps_its_interval_and_services_run_in_parallel() {
    let server = QuoteServer::start(vec![
        ("/quote/AAA", Answer::ok(HPE)),
        ("/quote/BBB", Answer::ok(HPE)),
        ("/other/CCC", Answer::ok(HPE)),
    ]);
    let (_book, mut d) = create();
    configure(
        &mut d,
        &server,
        &json!({
            "quotes.services.local": { "format": "json", "securityUrl": format!("{}/quote/{{symbol}}", server.url()), "minIntervalSeconds": 1,
                "pointers": { "price": "/data/price", "currency": "/data/ccy" } },
            "quotes.services.other": { "format": "json", "securityUrl": format!("{}/other/{{symbol}}", server.url()),
                "pointers": { "price": "/data/price", "currency": "/data/ccy" } },
            "quotes.sources.twelvedata": ["other"],
        }),
    );
    security(&mut d, "NYSE", "AAA", "alphavantage");
    security(&mut d, "NYSE", "BBB", "alphavantage");
    security(&mut d, "NYSE", "CCC", "twelvedata");
    let status = fetch(&mut d, &json!({}));
    assert_eq!(status["stored"], 3);
    let hits = server.hits();
    let at = |t: &str| hits.iter().find(|h| h.target == t).unwrap().at;
    // Requests to one service follow commodity order, a second apart; another service doesn't wait for them.
    assert!(at("/quote/BBB").duration_since(at("/quote/AAA")) >= Duration::from_millis(990));
    assert!(at("/other/CCC").duration_since(at("/quote/AAA")) < Duration::from_millis(500));
}

#[test]
fn keys_never_leave_their_service() {
    const KEY: &str = "k3y-SECRET";
    let server = QuoteServer::start(vec![(
        "/keyed/HPE",
        Answer::status(401, format!("{{\"error\": \"bad key {KEY}\"}}")),
    )]);
    let book = TempBook::new_path();
    let mut d = red_ale_testkit::dispatcher();
    call(&mut d, "book.create", &json!({ "path": book.path_str() }));
    configure(
        &mut d,
        &server,
        &json!({
            "quotes.services.keyed": { "format": "json", "securityUrl": format!("{}/keyed/{{symbol}}?apikey={{key}}", server.url()),
                "key": KEY, "pointers": { "price": "/p" } },
            "quotes.sources.alphavantage": ["keyed"],
        }),
    );
    security(&mut d, "NYSE", "HPE", "alphavantage");
    let status = fetch(&mut d, &json!({}));
    let message = entry(&status, "NYSE::HPE")["message"].as_str().unwrap().to_owned();
    assert!(message.contains("apikey=***") && message.contains("bad key ***"), "{message}");
    // The key went to its own service, and nowhere else.
    assert_eq!(server.hits()[0].target, format!("/keyed/HPE?apikey={KEY}"));
    let shown = [
        status.to_string(),
        call(&mut d, "config.get", &json!({})).to_string(),
        call(&mut d, "api.schema", &json!({})).to_string(),
    ];
    shown.iter().for_each(|text| assert!(!text.contains(KEY)));
    call(&mut d, "book.close", &json!({ "save": true }));
    std::fs::read_dir(book.dir.path()).unwrap().flatten().for_each(|f| {
        let bytes = std::fs::read(f.path()).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains(KEY), "{}", f.path().display());
    });
}

/// A key set with `config.set` while a book is open, then used by a fetch that stores its price, then a transaction
/// posted: the key reaches neither the `.log` journal, the book, its backups nor any result. The journal holds
/// transactions only, as GnuCash's does (`design/notes/storage-and-safety.md` §8), so the posting is what opens it.
#[test]
fn a_keyed_fetch_journals_its_price_and_not_its_key() {
    const KEY: &str = "k3y-JOURNAL";
    let server = QuoteServer::start(vec![("/keyed/HPE", Answer::ok(r#"{"p": "13.37", "c": "USD", "d": "2022-09-01"}"#))]);
    let book = TempBook::new_path();
    let mut d = red_ale_testkit::dispatcher();
    call(&mut d, "book.create", &json!({ "path": book.path_str() }));
    security(&mut d, "NYSE", "HPE", "alphavantage");
    configure(
        &mut d,
        &server,
        &json!({
            "quotes.services.keyed": { "format": "json", "securityUrl": format!("{}/keyed/{{symbol}}?apikey={{key}}", server.url()),
                "pointers": { "price": "/p", "currency": "/c", "date": "/d" } },
            "quotes.sources.alphavantage": ["keyed"],
        }),
    );
    let set = call(&mut d, "config.set", &json!({ "changes": { "quotes.services.keyed.key": KEY } }));
    let status = fetch(&mut d, &json!({}));
    assert_eq!(status["stored"], 1, "{status}");
    assert_eq!(server.hits()[0].target, format!("/keyed/HPE?apikey={KEY}"));
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Cash", "type": "CASH", "commodity": "USD" }),
    );
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Food", "type": "EXPENSE", "commodity": "USD" }),
    );
    call(
        &mut d,
        "transaction.post",
        &json!({ "description": "Keyed fetch day", "datePosted": "2022-09-01",
        "splits": [{ "account": "Cash", "value": "-5" }, { "account": "Food", "value": "5" }] }),
    );
    let files = || {
        std::fs::read_dir(book.dir.path())
            .unwrap()
            .flatten()
            .map(|f| f.path())
            .collect::<Vec<_>>()
    };
    let journal = files()
        .into_iter()
        .find(|p| p.extension().is_some_and(|e| e == "log"))
        .unwrap_or_else(|| panic!("a journal in {:?}", files()));
    let journaled = std::fs::read_to_string(&journal).unwrap();
    assert!(journaled.contains("Keyed fetch day"), "{journaled}");
    assert!(!journaled.contains(KEY));
    let shown = [
        set.to_string(),
        status.to_string(),
        call(&mut d, "config.get", &json!({})).to_string(),
        call(&mut d, "price.list", &json!({})).to_string(),
    ];
    shown.iter().for_each(|text| assert!(!text.contains(KEY), "{text}"));
    call(&mut d, "book.close", &json!({ "save": true }));
    files().iter().for_each(|f| {
        assert!(
            !String::from_utf8_lossy(&std::fs::read(f).unwrap()).contains(KEY),
            "{}",
            f.display()
        );
    });
}

#[test]
fn what_a_fetch_covers() {
    let server = QuoteServer::start(vec![]);
    let (_book, mut d) = create();
    configure(&mut d, &server, &json!({}));
    security(&mut d, "NYSE", "HPE", "alphavantage");
    security(&mut d, "NASDAQ", "AAPL", "alphavantage");
    let by_namespace = call(&mut d, "price.fetchQuotes", &json!({ "namespace": "^ny" }));
    assert_eq!(by_namespace["commodities"], json!(["NYSE::HPE"]));
    d.wait_for_quotes();
    let e = call_err(&mut d, "price.fetchQuotes", &json!({ "namespace": "(" }));
    assert_eq!(e.code, codes::INVALID);
    assert_eq!(
        call_err(&mut d, "price.fetchQuotes", &json!({ "namespace": "^FUND" })).code,
        codes::INVALID
    );
    assert_eq!(
        call_err(&mut d, "price.fetchQuotes", &json!({ "commodities": ["NYSE::NONE"] })).code,
        codes::NOT_FOUND
    );
    assert_eq!(
        call_err(&mut d, "price.fetchQuotes", &json!({ "commodities": ["USD"] })).code,
        codes::INVALID,
        "the default currency only"
    );
    assert_eq!(
        call_err(&mut d, "price.fetchQuotes", &json!({ "commodities": ["USD"], "namespace": "x" })).code,
        codes::INVALID_PARAMS
    );
}

#[test]
fn a_read_only_book_cannot_fetch() {
    let server = QuoteServer::start(vec![]);
    let book = TempBook::copy("synthetic/rich.gnucash");
    let mut d = red_ale_testkit::dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str(), "readOnly": true }));
    configure(&mut d, &server, &json!({}));
    assert_eq!(call_err(&mut d, "price.fetchQuotes", &json!({})).code, codes::READ_ONLY);
    let mut none = red_ale_testkit::dispatcher();
    assert_eq!(call_err(&mut none, "price.fetchQuotes", &json!({})).code, codes::NO_BOOK);
}

/// The servers store a fetch's prices as soon as its requests are done, without waiting for another call.
#[test]
fn the_json_rpc_server_stores_prices_when_the_requests_are_done() {
    use std::sync::{Arc, Mutex};
    let server = QuoteServer::start(vec![("/quote/HPE", Answer::ok(HPE).after(Duration::from_millis(300)))]);
    let (_book, mut d) = create();
    configure(&mut d, &server, &json!({}));
    security(&mut d, "NYSE", "HPE", "alphavantage");
    let dispatcher = Arc::new(Mutex::new(d));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (addr, handle) = runtime
        .block_on(red_ale_rpc::server::serve(
            Arc::clone(&dispatcher),
            "127.0.0.1:0".parse().unwrap(),
            (1 << 20, 1 << 20),
        ))
        .unwrap();
    let started = post(
        addr,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "price.fetchQuotes", "params": {} }),
    );
    assert!(started["result"]["fetch"].is_string(), "{started}");
    let info = post(addr, &json!({ "jsonrpc": "2.0", "id": 2, "method": "book.info", "params": {} }));
    assert!(info["result"].is_object());
    let prices = || dispatcher.lock().unwrap().context().book().unwrap().prices.len();
    let waited = Instant::now();
    while prices() == 0 && waited.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(prices(), 1, "stored with no further call");
    handle.stop().unwrap();
    runtime.block_on(handle.stopped());
}

/// One JSON-RPC request over HTTP/1.1.
fn post(addr: std::net::SocketAddr, body: &Value) -> Value {
    use std::io::{Read, Write};
    let body = body.to_string();
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    let head = format!(
        "POST / HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    serde_json::from_str(answer.split_once("\r\n\r\n").unwrap().1).unwrap()
}
