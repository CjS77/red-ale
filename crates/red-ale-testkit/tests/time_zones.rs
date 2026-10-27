//! The local time zone through the dispatcher: "today", stored dates and day boundaries at both extremes. The clock
//! is [`TEST_NOW`], 2024-07-01 12:00:00 UTC: 2 July 02:00 in Kiritimati (UTC+14), 1 July 01:00 in Pago Pago (UTC−11).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::TimeZone;
use red_ale_testkit::{TEST_NOW, TempBook, call, dispatcher_in};
use serde_json::{Value, json};

fn book_in(hours: i32) -> (TempBook, red_ale_rpc::Dispatcher) {
    let book = TempBook::new_path();
    let mut d = dispatcher_in(TimeZone::fixed(hours * 3600).unwrap());
    call(&mut d, "book.create", &json!({ "path": book.path_str() }));
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Bank", "type": "BANK", "commodity": "USD" }),
    );
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Food", "type": "EXPENSE", "commodity": "USD" }),
    );
    (book, d)
}

fn post(d: &mut red_ale_rpc::Dispatcher, date: Option<&str>) -> Value {
    let mut params = json!({ "splits": [{ "account": "Bank", "value": "-1" }, { "account": "Food", "value": "1" }] });
    if let Some(date) = date {
        params["datePosted"] = json!(date);
    }
    let guid = call(d, "transaction.post", &params)["guid"].clone();
    call(d, "transaction.get", &json!({ "guid": guid }))
}

#[test]
fn today_is_the_local_date() {
    assert_eq!(TEST_NOW.to_file_string(), "2024-07-01 12:00:00 +0000");
    let (_k, mut kiritimati) = book_in(14);
    assert_eq!(post(&mut kiritimati, None)["datePosted"], "2024-07-02");
    assert_eq!(
        call(&mut kiritimati, "period.dates", &json!({ "start": "today", "end": "today" }))["startTime"],
        "2024-07-01T10:00:00Z"
    );
    let (_p, mut pago) = book_in(-11);
    assert_eq!(post(&mut pago, None)["datePosted"], "2024-07-01");
    assert_eq!(
        call(&mut pago, "period.dates", &json!({ "start": "today", "end": "today" }))["endTime"],
        "2024-07-02T10:59:59Z"
    );
}

#[test]
fn dates_are_stored_at_the_zones_neutral_time() {
    let (_k, mut kiritimati) = book_in(14);
    assert_eq!(post(&mut kiritimati, Some("2024-06-20"))["datePostedTime"], "2024-06-20T09:59:00Z");
    let price = call(
        &mut kiritimati,
        "price.add",
        &json!({ "commodity": "EUR", "currency": "USD", "value": "1.1", "date": "2024-06-20" }),
    );
    assert_eq!(
        (price["price"]["time"].as_str(), price["price"]["date"].as_str()),
        (Some("2024-06-20T09:59:00Z"), Some("2024-06-20"))
    );
    let (_p, mut pago) = book_in(-11);
    assert_eq!(post(&mut pago, Some("2024-06-20"))["datePostedTime"], "2024-06-20T11:59:00Z");
}

#[test]
fn find_and_balances_use_local_day_boundaries() {
    let (_k, mut d) = book_in(14);
    post(&mut d, Some("2024-06-20"));
    let hits = |d: &mut red_ale_rpc::Dispatcher, op: &str, date: &str| {
        let q = json!({ "query": { "datePosted": { "op": op, "date": date } } });
        call(d, "transaction.find", &q)["matches"].as_array().unwrap().len()
    };
    assert_eq!(hits(&mut d, "on", "2024-06-20"), 2, "both splits, on the local day");
    assert_eq!(
        hits(&mut d, "on", "2024-06-19"),
        0,
        "09:59 UTC is not on the UTC day before in Kiritimati"
    );
    assert_eq!(hits(&mut d, "after", "2024-06-19"), 2);
    assert_eq!(hits(&mut d, "onOrBefore", "2024-06-19"), 0);
    let balance = |d: &mut red_ale_rpc::Dispatcher, day: &str| {
        call(d, "account.balance", &json!({ "account": "Bank", "asOf": day }))["balances"]["balance"]["num"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(balance(&mut d, "2024-06-19"), "0");
    assert_eq!(balance(&mut d, "2024-06-20"), "-100");
}

#[test]
fn same_day_prices_are_local_days() {
    // 2024-06-19 23:30 and 2024-06-20 01:00 UTC: different UTC days, one day (20 June) in Kiritimati.
    let add = |d: &mut red_ale_rpc::Dispatcher, time: &str, value: &str| {
        call(
            d,
            "price.add",
            &json!({ "commodity": "EUR", "currency": "USD", "value": value, "time": time }),
        )["outcome"]
            .clone()
    };
    let (_k, mut kiritimati) = book_in(14);
    add(&mut kiritimati, "2024-06-19T23:30:00Z", "1.1");
    assert_eq!(add(&mut kiritimati, "2024-06-20T01:00:00Z", "1.2"), "replaced");
    let (_u, mut utc) = book_in(0);
    add(&mut utc, "2024-06-19T23:30:00Z", "1.1");
    assert_eq!(add(&mut utc, "2024-06-20T01:00:00Z", "1.2"), "added");
}

#[test]
fn reconcile_reads_statement_dates_in_the_local_zone() {
    let (_k, mut d) = book_in(14);
    let view = call(
        &mut d,
        "reconcile.start",
        &json!({ "account": "Bank", "statementDate": "2024-06-30" }),
    );
    assert_eq!(view["statementTime"], "2024-06-30T09:59:59Z", "23:59:59 in Kiritimati");
    let fixed = call(
        &mut d,
        "reconcile.start",
        &json!({ "account": "Bank", "statementDate": "2024-06-30", "utcOffsetMinutes": 0 }),
    );
    assert_eq!(
        fixed["statementTime"], "2024-06-30T23:59:59Z",
        "a given offset overrides the local zone"
    );
}

/// A book with income posted on 29 June at its neutral time and at 2024-06-30 12:00:00 UTC, an instant GnuCash 2.x
/// and imports leave: still 30 June in UTC, already 1 July 02:00 in Kiritimati.
fn income_at_noon_utc() -> TempBook {
    let book = TempBook::new_path();
    let mut d = dispatcher_in(TimeZone::utc());
    call(&mut d, "book.create", &json!({ "path": book.path_str() }));
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Bank", "type": "BANK", "commodity": "USD" }),
    );
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Salary", "type": "INCOME", "commodity": "USD" }),
    );
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Retained", "type": "EQUITY", "commodity": "USD" }),
    );
    ["2024-06-29", "2024-06-30"].iter().for_each(|day| {
        let splits = json!([{ "account": "Bank", "value": "50" }, { "account": "Salary", "value": "-50" }]);
        call(&mut d, "transaction.post", &json!({ "datePosted": day, "splits": splits }));
    });
    call(&mut d, "book.save", &json!({ "compressed": false }));
    call(&mut d, "book.close", &json!({}));
    let xml = std::fs::read_to_string(&book.path).unwrap();
    assert_eq!(xml.matches("2024-06-30 10:59:00 +0000").count(), 1, "one posted time to move");
    std::fs::write(&book.path, xml.replace("2024-06-30 10:59:00 +0000", "2024-06-30 12:00:00 +0000")).unwrap();
    book
}

fn open_in(book: &TempBook, hours: i32) -> red_ale_rpc::Dispatcher {
    let mut d = dispatcher_in(TimeZone::fixed(hours * 3600).unwrap());
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    d
}

#[test]
fn close_book_closes_balances_at_the_local_day_end() {
    let close = |hours: i32| {
        let book = income_at_noon_utc();
        let mut d = open_in(&book, hours);
        let params = json!({ "date": "2024-06-30", "incomeTotalAccount": "Retained", "expenseTotalAccount": "Retained" });
        let closing = call(&mut d, "book.closeBooks", &params)["transactions"][0].clone();
        let txn = call(&mut d, "transaction.get", &json!({ "guid": closing["guid"] }));
        (
            closing["total"]["num"].as_str().unwrap().to_owned(),
            txn["datePostedTime"].as_str().unwrap().to_owned(),
        )
    };
    assert_eq!(
        close(0),
        ("-10000".to_owned(), "2024-06-30T10:59:00Z".to_owned()),
        "UTC: both incomes by 23:59:59 UTC"
    );
    assert_eq!(
        close(14),
        ("-5000".to_owned(), "2024-06-30T09:59:00Z".to_owned()),
        "Kiritimati: 30 June ends at 09:59:59 UTC"
    );
}

#[test]
fn balance_limits_take_the_balance_at_the_local_day_end() {
    let balance = |hours: i32| {
        let book = income_at_noon_utc();
        let mut d = open_in(&book, hours);
        let checked = call(
            &mut d,
            "account.checkBalanceLimit",
            &json!({ "account": "Salary", "asOf": "2024-06-30" }),
        );
        checked["balance"]["num"].as_str().unwrap().to_owned()
    };
    assert_eq!(balance(0), "-10000");
    assert_eq!(balance(14), "-5000");
}

#[test]
fn the_read_only_threshold_counts_from_the_local_today() {
    // TEST_NOW is 2 July in Kiritimati and 1 July in Pago Pago.
    let check = |hours: i32| {
        let (_book, mut d) = book_in(hours);
        let guid = post(&mut d, Some("2024-06-30"))["guid"].clone();
        let options = call(&mut d, "book.setOptions", &json!({ "autoReadOnlyDays": 1 }));
        let refused = d.call("transaction.update", json!({ "guid": guid, "description": "x" })).is_err();
        (options["readOnlyThreshold"].as_str().unwrap().to_owned(), refused)
    };
    assert_eq!(check(14), ("2024-07-01".to_owned(), true));
    assert_eq!(check(-11), ("2024-06-30".to_owned(), false));
}
