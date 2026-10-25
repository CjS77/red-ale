//! GnuCash 5.17 reads the prices red-ale writes, adds prices by the same same-day rules, and removes the same old
//! prices.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, create, open, oracle, oracle_check};
use serde_json::{Value, json};

#[test]
fn edited_prices_read_the_same_in_gnucash() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    let acme = |d: &mut red_ale_rpc::Dispatcher| call(d, "price.list", &json!({ "commodity": "NASDAQ::ACME", "currency": "USD" }));
    let newest = acme(&mut d)[0]["guid"].clone();
    call(&mut d, "price.update", &json!({ "price": newest, "value": "1234.5678", "type": "bid", "source": "user:price-editor" }));
    let oldest = acme(&mut d)[1]["guid"].clone();
    call(&mut d, "price.update", &json!({ "price": oldest, "date": "2024-06-10", "currency": "EUR" }));
    call(&mut d, "price.add", &json!({ "commodity": "FUND::ZETA", "currency": "USD", "value": "2.031", "date": "2024-06-08", "type": "nav" }));
    call(&mut d, "price.add", &json!({ "commodity": "USD", "currency": "EUR", "value": "91/100", "date": "2024-06-03" }));
    call(&mut d, "price.add", &json!({ "commodity": "GBP", "currency": "USD", "value": "1.27", "time": "2024-06-01T23:59:59Z" }));
    let zeta = call(&mut d, "price.list", &json!({ "commodity": "FUND::ZETA" }));
    call(&mut d, "price.delete", &json!({ "price": zeta[1]["guid"] }));
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}

/// Runs `gnc_pricedb_add_price` for each `COMMODITY,CURRENCY,EPOCH,SOURCE,NUM,DENOM` argument on an empty price
/// database and prints what each returned and the prices left.
const ADD: &str = r#"
import json, sys
from datetime import datetime, timezone
from gnucash import GncNumeric, GncPrice, Session, SessionOpenMode
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    b = s.book
    t, pdb = b.get_table(), b.get_price_db()
    look = lambda uid: t.lookup(*uid.split("::"))
    results = []
    for spec in sys.argv[2:]:
        c, cur, epoch, src, num, den = spec.split(",")
        p = GncPrice(b)
        p.begin_edit()
        p.set_commodity(look(c))
        p.set_currency(look(cur))
        p.set_time64(datetime.fromtimestamp(int(epoch), timezone.utc))
        p.set_source_string(src)
        p.set_value(GncNumeric(int(num), int(den)))
        p.commit_edit()
        results.append(bool(pdb.add_price(p)))
    left = []
    for ns in t.get_namespaces_list():
        for cm in ns.get_commodity_list():
            for p in pdb.get_prices(cm, None):
                v = p.get_value()
                left.append([p.get_commodity().get_unique_name(), p.get_currency().get_unique_name(),
                             int(p.get_time64().timestamp()), p.get_source_string(), f"{v.num()}/{v.denom()}"])
    print(json.dumps({"results": results, "left": sorted(left)}))
finally:
    s.end()
"#;

#[test]
fn adding_prices_follows_gnucash() {
    let (book, mut d) = create();
    let day = |d: i64, h: i64| 1_704_844_800 + d * 86_400 + h * 3600; // 2024-01-10 00:00 UTC
    let adds = [
        ("EUR", "USD", day(0, 10), "user:xfer-dialog", 110, 100),
        ("USD", "EUR", day(0, 12), "user:price-editor", 91, 100),
        ("EUR", "USD", day(0, 15), "Finance::Quote", 11, 10),
        ("EUR", "USD", day(1, 10), "Finance::Quote", 12, 10),
        ("EUR", "USD", day(1, 18), "Finance::Quote", 12, 10),
        ("USD", "EUR", day(1, 20), "user:price", 13, 10),
        ("GBP", "USD", day(2, 9), "user:price", 127, 100),
        ("GBP", "USD", day(2, 9) + 60, "user:price", 1270, 1000),
        ("GBP", "USD", day(3, 0), "user:split-register", 125, 100),
        ("GBP", "USD", day(3, 23), "temporary", 126, 100),
        ("GBP", "USD", day(4, 0) - 1, "invalid", 1, 1),
        ("GBP", "USD", day(4, 0), "invalid", 2, 1),
        ("GBP", "USD", day(4, 1), "user:stock-split", 3, 1),
        ("USD", "GBP", day(4, 2), "user:invoice-post", 4, 1),
        ("GBP", "USD", day(6, 12), "user:split-import", 5, 1),
        ("GBP", "USD", day(5, 12), "user:stock-transaction", 6, 1),
        ("GBP", "USD", day(5, 13), "user:stock-transaction", 6, 1),
    ];
    let ours: Vec<bool> = adds
        .iter()
        .map(|(c, cur, t, src, n, dn)| {
            let r = call(&mut d, "price.add", &json!({ "commodity": c, "currency": cur, "value": format!("{n}/{dn}"),
                "time": rfc3339(*t), "source": src }));
            r["outcome"] != "rejected"
        })
        .collect();
    let left: Vec<Value> = call(&mut d, "price.list", &json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let t = p["time"].as_str().unwrap();
            json!([p["commodity"], p["currency"], epoch(t), p["source"], format!("{}/{}", p["value"]["num"].as_str().unwrap(), p["value"]["denom"].as_str().unwrap())])
        })
        .collect();
    let mut left = left;
    left.sort_by_key(ToString::to_string);
    call(&mut d, "book.close", &json!({ "discard": true }));
    let specs: Vec<String> = adds.iter().map(|(c, cur, t, src, n, dn)| format!("CURRENCY::{c},CURRENCY::{cur},{t},{src},{n},{dn}")).collect();
    let path = book.path_str();
    let args: Vec<&str> = std::iter::once(path.as_str()).chain(specs.iter().map(String::as_str)).collect();
    let gnucash: Value = serde_json::from_str(&oracle::run_code(ADD, &args).unwrap()).unwrap();
    assert_eq!(gnucash["results"], json!(ours));
    let mut theirs = gnucash["left"].as_array().unwrap().clone();
    theirs.sort_by_key(ToString::to_string);
    assert_eq!(theirs, left);
}

/// Seconds since the epoch as RFC 3339 UTC, without a date library in the test.
fn rfc3339(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    let date = red_ale_core::Date::from_ymd(1970, 1, 1).unwrap().add_days(days);
    format!("{date}T{:02}:{:02}:{:02}Z", secs / 3600, secs % 3600 / 60, secs % 60)
}

fn epoch(rfc3339: &str) -> i64 {
    let (date, time) = rfc3339.trim_end_matches('Z').split_once('T').unwrap();
    let d = red_ale_core::Date::parse(date).unwrap().start_utc().0;
    let parts: Vec<i64> = time.split(':').map(|x| x.parse().unwrap()).collect();
    d + parts[0] * 3600 + parts[1] * 60 + parts[2]
}

/// Runs `gnc_pricedb_remove_old_prices(pdb, [commodity], NULL, cutoff, flags, keep)`, or the dialog's two-step
/// "scaled" removal when keep is 5, and prints the prices of the commodity left.
const REMOVE_OLD: &str = r#"
import json, sys
from datetime import datetime, timezone
from gnucash import Session, SessionOpenMode
path, uid, cutoff, cutoff6, cutoff12, flags, keep = sys.argv[1:8]
s = Session("xml://" + path, SessionOpenMode.SESSION_READ_ONLY)
try:
    b = s.book
    cm = b.get_table().lookup(*uid.split("::"))
    pdb = b.get_price_db()
    at = lambda e: datetime.fromtimestamp(int(e), timezone.utc)
    if int(keep) == 5:
        pdb.remove_old_prices([cm], None, at(cutoff6), int(flags), 1)
        pdb.remove_old_prices([cm], None, at(cutoff12), int(flags), 2)
    else:
        pdb.remove_old_prices([cm], None, at(cutoff), int(flags), int(keep))
    left = sorted(f"{p.get_currency().get_unique_name()} {int(p.get_time64().timestamp())}" for p in pdb.get_prices(cm, None))
    print(json.dumps(left))
finally:
    s.end()
"#;

fn price_book() -> TempBook {
    let (book, mut d) = create();
    let sources = ["Finance::Quote", "user:price", "user:price-editor", "user:xfer-dialog", "Finance::Quote"];
    let start = red_ale_core::Date::from_ymd(2021, 1, 3).unwrap();
    (0..90i64).for_each(|i| {
        let date = start.add_days(i * 13 + i % 3);
        let currency = if i % 4 == 3 { "GBP" } else { "USD" };
        let source = sources[usize::try_from(i).unwrap() % sources.len()];
        call(&mut d, "price.add", &json!({ "commodity": "EUR", "currency": currency, "value": format!("{}/100", 100 + i), "date": date.to_string(), "source": source }));
    });
    call(&mut d, "price.add", &json!({ "commodity": "JPY", "currency": "USD", "value": "1/150", "date": "2021-02-01", "source": "Finance::Quote" }));
    call(&mut d, "book.close", &json!({ "save": true }));
    book
}

#[test]
fn remove_old_prices_matches_gnucash() {
    let base = price_book();
    let cutoff = red_ale_core::Date::from_ymd(2024, 1, 1).unwrap();
    let keeps = [("none", 0), ("lastOfWeek", 1), ("lastOfMonth", 2), ("lastOfQuarter", 3), ("lastOfPeriod", 4), ("scaled", 5)];
    let sources: [(&[&str], i32); 4] = [(&["fetched"], 1), (&["user"], 2), (&["app"], 4), (&["fetched", "user", "app"], 7)];
    let failures: Vec<String> = keeps
        .iter()
        .flat_map(|keep| sources.iter().map(move |src| (keep, src)))
        .filter_map(|((keep, keep_n), (src, flags))| {
            let book = TempBook::new_path();
            std::fs::copy(&base.path, &book.path).unwrap();
            let mut d = red_ale_testkit::dispatcher();
            call(&mut d, "book.open", &json!({ "path": book.path_str() }));
            call(&mut d, "price.removeOld", &json!({ "commodities": ["EUR"], "before": cutoff.to_string(), "keep": keep, "sources": src }));
            let mut ours: Vec<String> = call(&mut d, "price.list", &json!({ "commodity": "EUR" }))
                .as_array()
                .unwrap()
                .iter()
                .map(|p| format!("{} {}", p["currency"].as_str().unwrap(), epoch(p["time"].as_str().unwrap())))
                .collect();
            ours.sort();
            call(&mut d, "book.close", &json!({ "discard": true }));
            let six = cutoff.add_months(-6);
            let args = [
                base.path_str(),
                "CURRENCY::EUR".to_owned(),
                cutoff.neutral_time().0.to_string(),
                six.neutral_time().0.to_string(),
                six.add_months(-6).neutral_time().0.to_string(),
                flags.to_string(),
                keep_n.to_string(),
            ];
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let gnucash: Vec<String> = serde_json::from_str(&oracle::run_code(REMOVE_OLD, &args).unwrap()).unwrap();
            (gnucash != ours).then(|| format!("keep {keep}, sources {src:?}: GnuCash keeps {} prices, red-ale {}", gnucash.len(), ours.len()))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
