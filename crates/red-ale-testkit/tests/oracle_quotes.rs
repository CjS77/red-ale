//! GnuCash 5.17 reads the prices a Quote fetch stores as prices Finance::Quote fetched, and ranks them so.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::quote_server::{Answer, QuoteServer};
use red_ale_testkit::{call, create, oracle};
use serde_json::{Value, json};

/// Prints each price of the commodities named, with its source as GnuCash ranks it (`gnc_price_get_source`), then
/// tries two more prices on the first one's day: one from the transfer dialog, which a fetched quote outranks, and
/// one from the price editor, which outranks a fetched quote (`gnc_pricedb_add_price`).
const READ: &str = r#"
import json, sys
from datetime import datetime, timezone
from gnucash import GncNumeric, GncPrice, Session, SessionOpenMode
from gnucash import gnucash_core_c as c
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    b = s.book
    t, pdb = b.get_table(), b.get_price_db()
    look = lambda uid: t.lookup(*uid.split("::"))
    prices = []
    for uid in sys.argv[2:]:
        for p in pdb.get_prices(look(uid), None):
            v = p.get_value()
            prices.append({"commodity": uid, "currency": p.get_currency().get_unique_name(),
                           "time": int(p.get_time64().timestamp()), "fetched": p.get_source() == c.PRICE_SOURCE_FQ,
                           "source": p.get_source_string(), "type": p.get_typestr(), "value": f"{v.num()}/{v.denom()}"})
    def add(src, epoch):
        p = GncPrice(b)
        p.begin_edit()
        p.set_commodity(look(sys.argv[2]))
        p.set_currency(look("CURRENCY::USD"))
        p.set_time64(datetime.fromtimestamp(epoch + 3600, timezone.utc))
        p.set_source_string(src)
        p.set_value(GncNumeric(1, 1))
        p.commit_edit()
        return bool(pdb.add_price(p))
    first = prices[0]["time"]
    print(json.dumps({"prices": prices, "xferAdded": add("user:xfer-dialog", first), "editorAdded": add("user:price-editor", first)}))
finally:
    s.end()
"#;

#[test]
fn gnucash_ranks_fetched_prices_as_finance_quote_quotes() {
    let server = QuoteServer::start(vec![
        ("/av/quote", Answer::recorded("alphavantage-global-quote.json")),
        ("/av/fx", Answer::recorded("alphavantage-exchange-rate.json")),
    ]);
    let (book, mut d) = create();
    let url = server.url();
    call(&mut d, "config.set", &json!({ "changes": {
        "quotes.network": true,
        "quotes.services.local-av": {
            "format": "alphavantage",
            "securityUrl": format!("{url}/av/quote?symbol={{symbol}}&apikey={{key}}"),
            "currencyUrl": format!("{url}/av/fx?from={{base}}&to={{quote}}&apikey={{key}}"),
            "key": "demo",
        },
        "quotes.sources.alphavantage": ["local-av"],
        "quotes.sources.currency": ["local-av"],
    } }));
    call(&mut d, "commodity.create", &json!({ "namespace": "NYSE", "mnemonic": "IBM", "fullname": "IBM", "fraction": 10000,
        "quoteFlag": true, "quoteSource": "alphavantage" }));
    call(&mut d, "commodity.update", &json!({ "commodity": "EUR", "quoteFlag": true }));
    call(&mut d, "price.fetchQuotes", &json!({}));
    d.wait_for_quotes();
    assert_eq!(call(&mut d, "price.fetchStatus", &json!({}))["stored"], 2);
    let ours: Vec<Value> = ["NYSE::IBM", "CURRENCY::EUR"]
        .iter()
        .flat_map(|c| call(&mut d, "price.list", &json!({ "commodity": c })).as_array().unwrap().clone())
        .map(|p| json!([p["commodity"], p["source"], p["type"], format!("{}/{}", p["value"]["num"].as_str().unwrap(), p["value"]["denom"].as_str().unwrap())]))
        .collect();
    call(&mut d, "book.close", &json!({ "save": true }));
    let path = book.path_str();
    let gnucash: Value = serde_json::from_str(&oracle::run_code(READ, &[&path, "NYSE::IBM", "CURRENCY::EUR"]).unwrap()).unwrap();
    let theirs: Vec<Value> =
        gnucash["prices"].as_array().unwrap().iter().map(|p| json!([p["commodity"], p["source"], p["type"], p["value"]])).collect();
    assert_eq!(theirs, ours);
    assert_eq!(ours[0], json!(["NYSE::IBM", "Finance::Quote", "last", "1872700/10000"]));
    assert!(gnucash["prices"].as_array().unwrap().iter().all(|p| p["fetched"] == true));
    // 2024-05-03 at 10:59 UTC, the quote's date at its neutral time.
    assert_eq!(gnucash["prices"][0]["time"], 1_714_733_940);
    assert_eq!((gnucash["xferAdded"].clone(), gnucash["editorAdded"].clone()), (json!(false), json!(true)));
}
