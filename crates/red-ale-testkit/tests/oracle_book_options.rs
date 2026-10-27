//! The num field source and the auto read-only threshold against GnuCash 5.17 (`oracle/book_options.py`): GnuCash
//! sets the options as red-ale does, reads what red-ale wrote with no unknown feature, gives every split the Number
//! red-ale reports (`gnc_get_num_action`), and finds read-only by date (`xaccTransIsReadonlyByPostedDate`) exactly the
//! transactions red-ale refuses to change.
//!
//! GnuCash computes the threshold from the real clock, so red-ale's clock here is noon UTC today: the same calendar
//! day in GnuCash's `TZ=UTC` and in any zone from UTC−11 to UTC+11. The extreme-zone test runs both on the real clock
//! in the same zone instead.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use red_ale_core::{Date, FixedEnv, Time64, TimeZone};
use red_ale_rpc::Dispatcher;
use red_ale_service::Settings;
use red_ale_testkit::{TempBook, call, oracle, oracle_check};
use serde_json::{Value, json};

const RICH: &str = "synthetic/rich.gnucash";
const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";

fn script(args: &[&str]) -> String {
    script_in(args, "UTC")
}

fn script_in(args: &[&str], tz: &str) -> String {
    oracle::check_version().unwrap();
    oracle::run_script_in(&oracle::scripts_dir().join("book_options.py"), args, Some(tz)).unwrap_or_else(|e| panic!("oracle: {e}"))
}

fn real_now() -> Time64 {
    Time64(i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()).unwrap())
}

/// Noon UTC on today's UTC date.
fn noon_today() -> Time64 {
    let secs = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()).unwrap();
    Time64(secs - secs.rem_euclid(86_400) + 43_200)
}

fn today_dispatcher() -> Dispatcher {
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| {
        Box::new(FixedEnv::new(noon_today(), 0x00ed_0000_0000_0000_0000_0000_0000_0000))
    }));
    d
}

fn open_today(book: &TempBook) -> Dispatcher {
    let mut d = today_dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    d
}

fn days_ago(n: i64) -> String {
    noon_today().utc_date().unwrap().add_days(-n).to_string()
}

/// Every transaction red-ale refuses to change because of the threshold.
fn refused_by_date(d: &mut Dispatcher, txns: &[Value]) -> BTreeMap<String, bool> {
    txns.iter()
        .map(|t| {
            let guid = t["guid"].as_str().unwrap().to_owned();
            let refused = match d.call("transaction.update", json!({ "guid": guid })) {
                Ok(_) => false,
                Err(e) => e.data.as_ref().is_some_and(|data| data["reason"] == "auto-read-only"),
            };
            (guid, refused)
        })
        .collect()
}

/// Each transaction's splits' Numbers as red-ale reports them.
fn numbers(txns: &[Value]) -> BTreeMap<String, BTreeMap<String, Value>> {
    txns.iter()
        .map(|t| {
            let splits = t["splits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| (s["guid"].as_str().unwrap().to_owned(), s["number"].clone()))
                .collect();
            (t["guid"].as_str().unwrap().to_owned(), splits)
        })
        .collect()
}

#[test]
fn gnucash_agrees_with_red_ales_numbers_and_threshold() {
    let book = TempBook::copy(RICH);
    let mut d = open_today(&book);
    (8..=12).for_each(|k| {
        call(
            &mut d,
            "transaction.post",
            &json!({
                "datePosted": days_ago(k), "num": format!("N{k}"), "description": format!("{k} days ago"),
                "splits": [
                    { "account": CHECKING, "value": "-1.00", "action": format!("A{k}") },
                    { "account": "Expenses:Food", "value": "1.00" }
                ]
            }),
        );
    });
    let options = call(
        &mut d,
        "book.setOptions",
        &json!({ "splitActionForNum": true, "autoReadOnlyDays": 10 }),
    );
    assert_eq!(options["readOnlyThreshold"], days_ago(10));
    let txns = call(&mut d, "transaction.list", &json!({}))["transactions"]
        .as_array()
        .unwrap()
        .clone();
    let refused = refused_by_date(&mut d, &txns);
    assert!(
        !refused[txns.iter().find(|t| t["description"] == "10 days ago").unwrap()["guid"]
            .as_str()
            .unwrap()]
    );
    assert!(
        refused[txns.iter().find(|t| t["description"] == "11 days ago").unwrap()["guid"]
            .as_str()
            .unwrap()]
    );
    let ours = numbers(&txns);

    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let gnucash: Value = serde_json::from_str(&script(&["inspect", &book.path_str()])).unwrap();
    assert_eq!(gnucash["splitActionForNum"], true);
    assert_eq!(gnucash["autoReadonlyDays"], 10);
    assert_eq!(gnucash["threshold"], options["readOnlyThreshold"]);
    assert_eq!(
        gnucash["unknownFeatures"],
        Value::Null,
        "GnuCash would warn about the book's features"
    );
    let theirs = gnucash["transactions"].as_object().unwrap();
    assert_eq!(theirs.len(), refused.len());
    refused
        .iter()
        .for_each(|(g, r)| assert_eq!(theirs[g]["readOnlyByDate"], *r, "read-only by date: {g}"));
    ours.iter().for_each(|(g, splits)| {
        splits
            .iter()
            .for_each(|(s, n)| assert_eq!(&theirs[g]["numbers"][s], n, "the Number of split {s}"));
    });
}

#[test]
fn red_ale_sets_the_options_as_gnucash_does() {
    let (gnucash, ours) = (TempBook::copy(RICH), TempBook::copy(RICH));
    script(&["set", &gnucash.path_str(), "true", "30"]);
    let mut d = open_today(&ours);
    call(
        &mut d,
        "book.setOptions",
        &json!({ "splitActionForNum": true, "autoReadOnlyDays": 30 }),
    );
    call(&mut d, "book.close", &json!({ "save": true }));
    let slots = |b: &TempBook| -> Value { serde_json::from_str(&script(&["slots", &b.path_str()])).unwrap() };
    let expected = slots(&gnucash);
    assert_eq!(
        expected["options"]["Accounts"]["Use Split Action Field for Number"],
        json!(["string", "t"])
    );
    assert_eq!(slots(&ours), expected);
}

#[test]
fn a_threshold_of_zero_makes_nothing_read_only_in_either() {
    let book = TempBook::copy(RICH);
    let mut d = open_today(&book);
    call(&mut d, "book.setOptions", &json!({ "autoReadOnlyDays": 0 }));
    let txns = call(&mut d, "transaction.list", &json!({}))["transactions"]
        .as_array()
        .unwrap()
        .clone();
    assert!(refused_by_date(&mut d, &txns).values().all(|r| !r));
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    let gnucash: Value = serde_json::from_str(&script(&["inspect", &book.path_str()])).unwrap();
    assert!(
        gnucash["transactions"]
            .as_object()
            .unwrap()
            .values()
            .all(|t| t["readOnlyByDate"] == false)
    );
    assert_eq!(gnucash["threshold"], Value::Null);
}

/// GnuCash's threshold is today in its local zone (`qof_book_get_autoreadonly_gdate`, `5.17:libgnucash/engine/qofbook.cpp:1006-1019`)
/// minus the days; red-ale's is the same in the same zone, at the zones a day apart from each other and from UTC.
#[test]
fn the_threshold_counts_from_the_local_today_at_both_extremes() {
    let thresholds: Vec<Date> = ["Pacific/Pago_Pago", "Pacific/Kiritimati"]
        .iter()
        .map(|tz| {
            let zone = TimeZone::from_tzif(tz, &std::fs::read(format!("/usr/share/zoneinfo/{tz}")).unwrap()).unwrap();
            let book = TempBook::copy(RICH);
            let mut d = Dispatcher::new(Settings::default());
            let env = move || FixedEnv::new(real_now(), 0x00ed_0000_0000_0000_0000_0000_0000_0000).with_zone(zone.clone());
            d.set_env_factory(Box::new(move || Box::new(env())));
            call(&mut d, "book.open", &json!({ "path": book.path_str() }));
            let options = call(&mut d, "book.setOptions", &json!({ "autoReadOnlyDays": 10 }));
            let txns = call(&mut d, "transaction.list", &json!({}))["transactions"]
                .as_array()
                .unwrap()
                .clone();
            let refused = refused_by_date(&mut d, &txns);
            call(&mut d, "book.close", &json!({ "save": true }));
            let gnucash: Value = serde_json::from_str(&script_in(&["inspect", &book.path_str()], tz)).unwrap();
            assert_eq!(gnucash["threshold"], options["readOnlyThreshold"], "{tz}");
            let theirs = gnucash["transactions"].as_object().unwrap();
            refused
                .iter()
                .for_each(|(g, r)| assert_eq!(theirs[g]["readOnlyByDate"], *r, "{tz}: read-only by date: {g}"));
            Date::parse(options["readOnlyThreshold"].as_str().unwrap()).unwrap()
        })
        .collect();
    assert!(
        thresholds[0] < thresholds[1],
        "Kiritimati's today is always after Pago Pago's: {thresholds:?}"
    );
}
