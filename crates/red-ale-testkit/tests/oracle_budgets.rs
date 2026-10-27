//! Budgets against GnuCash 5.17 (`oracle/budgets.py`): GnuCash writes a book with budgets holding every kind of
//! amount and note, red-ale reads it, regenerates each budget exactly, and after an unrelated edit saves a book in which
//! GnuCash reads every budget as it read its own.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, dispatcher, oracle};
use serde_json::{Value, json};

fn script(args: &[&str]) -> String {
    oracle::check_version().unwrap();
    oracle::run_script(&oracle::scripts_dir().join("budgets.py"), args).unwrap_or_else(|e| panic!("oracle: {e}"))
}

/// Every budget as GnuCash reads it.
fn gnucash_budgets(book: &TempBook) -> Value {
    serde_json::from_str(&script(&["dump", &book.path_str()])).unwrap()
}

#[test]
fn budgets_red_ale_saved_read_back_in_gnucash_unchanged() {
    let book = TempBook::copy("synthetic/rich.gnucash");
    script(&["make", &book.path_str()]);
    let expected = gnucash_budgets(&book);
    let plan = &expected["00000000000000000000000000b0d6e7"];
    assert_eq!(plan["amounts"]["1feca2abf92347288c268ebf93753ec4"], json!({ "0": "1234/100", "1": "0/1" }));
    assert_eq!(plan["recurrence"]["weekendAdjust"], "forward");
    assert_eq!(plan["amounts"]["a8cf578f585b4708856fdf5b8928e552"], json!({ "11": "-5/3" }));
    assert_eq!(plan["notes"]["1feca2abf92347288c268ebf93753ec4"], json!({ "2": "Groceries & <more>" }));
    let existing = &expected["0123456789abcdef0123456789abcdef"]["notes"]["1feca2abf92347288c268ebf93753ec4"];
    assert_eq!(*existing, json!({ "0": "January food", "11": "December" }));

    let (read, residue) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    assert_eq!(read.budgets.len(), 2);
    assert_eq!(residue.originals_kept(), 0, "red-ale generates GnuCash's budgets as GnuCash wrote them");

    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    call(&mut d, "account.update", &json!({ "account": "Assets:Cash", "description": "petty" }));
    call(&mut d, "book.close", &json!({ "save": true }));
    let (text, _) = red_ale_xml::book::decode(&std::fs::read(&book.path).unwrap()).unwrap();
    assert!(text.contains("<act:description>petty</act:description>"), "the book was rewritten");
    assert!(text.contains("<slot:key>15</slot:key>"), "the amount past the last period is kept");
    assert_eq!(gnucash_budgets(&book), expected);
}

/// The book at `book`'s path copied into a fresh directory, so GnuCash's timestamped backup of one save never meets
/// another's.
fn copy_of(book: &TempBook) -> TempBook {
    let copy = TempBook::new_path();
    std::fs::copy(&book.path, &copy.path).unwrap();
    copy
}

/// GnuCash's view of the budget Book features and the default budget.
fn gnucash_features(book: &TempBook) -> Value {
    serde_json::from_str(&script(&["features", &book.path_str()])).unwrap()
}

fn stored(amount: &Value) -> String {
    format!("{}/{}", amount["num"].as_str().unwrap(), amount["denom"].as_str().unwrap())
}

/// A `budget.get` result's amounts and notes in the shape `budgets.py dump` prints them.
fn as_dumped(info: &Value) -> (Value, Value) {
    let mut amounts = serde_json::Map::new();
    info["amounts"].as_array().unwrap().iter().for_each(|a| {
        let cell = amounts.entry(a["account"].as_str().unwrap()).or_insert_with(|| json!({}));
        cell[a["period"].to_string()] = json!(stored(&a["amount"]));
    });
    let mut notes = serde_json::Map::new();
    info["notes"].as_array().unwrap().iter().for_each(|n| {
        let cell = notes.entry(n["account"].as_str().unwrap()).or_insert_with(|| json!({}));
        cell[n["period"].to_string()] = n["note"].clone();
    });
    (Value::Object(amounts), Value::Object(notes))
}

/// The first three period starts as GnuCash's dump gives them: local midnight, here UTC.
fn period_starts(info: &Value) -> Value {
    info["periods"].as_array().unwrap()[..3]
        .iter()
        .map(|p| red_ale_core::Date::parse(p["startDate"].as_str().unwrap()).unwrap().start_utc().0)
        .collect()
}

#[test]
fn a_budget_red_ale_created_filled_and_estimated_reads_back_in_gnucash() {
    let book = TempBook::copy("synthetic/rich.gnucash");
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    call(&mut d, "budget.scrubSigns", &json!({}));
    let monthly = json!({ "mult": 1, "periodType": "month", "start": "2024-01-31" });
    let plan = call(&mut d, "budget.create", &json!({ "name": "Plan", "description": "d", "numPeriods": 6, "recurrence": monthly }));
    let plan_guid = plan["guid"].as_str().unwrap().to_owned();
    let amounts = json!([
        { "account": "Expenses:Food", "period": 0, "amount": "12.34" },
        { "account": "Expenses:Food", "period": 1, "amount": "0.00" },
        { "account": "Income:Salary", "period": 0, "amount": "-5/3" },
        { "account": "Liabilities:Visa", "period": 5, "amount": "0" }
    ]);
    call(&mut d, "budget.setAmounts", &json!({ "budget": plan_guid, "amounts": amounts }));
    let notes = json!([
        { "account": "Expenses:Food", "period": 2, "note": "Groceries & <more>" },
        { "account": "Expenses:Food", "period": 3, "note": "" }
    ]);
    call(&mut d, "budget.setNotes", &json!({ "budget": plan_guid, "notes": notes }));
    let est = call(&mut d, "budget.create", &json!({ "name": "Est", "recurrence": monthly }))["guid"]
        .as_str()
        .unwrap()
        .to_owned();
    let params = json!({ "budget": est, "accounts": ["Assets", "Expenses:Food"], "startDate": "2024-01-15", "significantDigits": 3 });
    call(&mut d, "budget.estimate", &params);
    call(&mut d, "book.setOptions", &json!({ "defaultBudget": plan_guid }));
    let ours: Vec<Value> = [&plan_guid, &est]
        .iter()
        .map(|g| call(&mut d, "budget.get", &json!({ "budget": g })))
        .collect();
    call(&mut d, "book.close", &json!({ "save": true }));

    let gnucash = gnucash_budgets(&book);
    ours.iter().for_each(|info| {
        let theirs = &gnucash[info["guid"].as_str().unwrap()];
        let (amounts, notes) = as_dumped(info);
        assert_eq!(theirs["amounts"], amounts, "{}: amounts with their stored denominators", info["name"]);
        assert_eq!(theirs["notes"], notes, "{}", info["name"]);
        let fields = |b: &Value| (b["name"].clone(), b["description"].clone(), b["numPeriods"].clone());
        assert_eq!(fields(theirs), fields(info));
        assert_eq!(theirs["recurrence"]["periodStarts"], period_starts(info), "{}: period dates", info["name"]);
    });
    assert_eq!(
        gnucash[&plan_guid]["amounts"]["1feca2abf92347288c268ebf93753ec4"],
        json!({ "0": "1234/100", "1": "0/100" })
    );
    let features = gnucash_features(&book);
    assert_eq!(features, json!({ "naturalSigns": true, "extraColumns": false, "unknown": null, "default": plan_guid }));
}

#[test]
fn the_sign_scrub_flips_what_gnucashs_own_scrub_flips() {
    let source = TempBook::copy("synthetic/rich.gnucash");
    script(&["legacy", &source.path_str()]);
    let (theirs, ours) = (copy_of(&source), copy_of(&source));
    assert_eq!(script(&["scrub", &theirs.path_str()]).trim(), "true");
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": ours.path_str() }));
    assert_eq!(call(&mut d, "budget.list", &json!({}))["needsSignScrub"], true);
    let report = call(&mut d, "budget.scrubSigns", &json!({}));
    call(&mut d, "book.close", &json!({ "save": true }));
    let policies: std::collections::BTreeMap<String, String> = report["budgets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["budget"].as_str().unwrap().to_owned(), b["policy"].as_str().unwrap().to_owned()))
        .collect();
    let gnucash = gnucash_budgets(&theirs);
    let by_name = |name: &str| {
        let (guid, _) = gnucash.as_object().unwrap().iter().find(|(_, b)| b["name"] == name).unwrap();
        policies[guid].clone()
    };
    assert_eq!(
        ["Budget 2024", "IncExp", "None", "Credit"].map(by_name),
        ["creditAccounts", "incomeExpense", "none", "creditAccounts"]
    );
    assert_eq!(gnucash_budgets(&ours), gnucash, "every amount as GnuCash's scrub left it");
    [&theirs, &ours].iter().for_each(|b| {
        let f = gnucash_features(b);
        assert_eq!((&f["naturalSigns"], &f["unknown"]), (&json!(true), &Value::Null));
    });
}

#[test]
fn actuals_are_gnucashs() {
    let book = TempBook::copy("synthetic/rich.gnucash");
    let budget = "0123456789abcdef0123456789abcdef";
    let periods = 14;
    let gnucash: Value = serde_json::from_str(&script(&["actuals", &book.path_str(), budget, &periods.to_string()])).unwrap();
    let (read, _) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    let b = read.budget(red_ale_core::Guid::parse(budget).unwrap()).unwrap();
    let zone = red_ale_core::TimeZone::utc();
    let ours: serde_json::Map<String, Value> = read
        .descendants(read.root)
        .iter()
        .map(|a| {
            let figures: Vec<String> = (0..periods)
                .map(|p| {
                    let n = red_ale_core::ops::budgets::actual(&read, b, a.guid, p, &zone).unwrap();
                    format!("{}/{}", n.num(), n.denom())
                })
                .collect();
            (a.guid.to_string(), json!(figures))
        })
        .collect();
    assert_eq!(Value::Object(ours), gnucash, "every account's actual in every period, past the count too");
}

#[test]
fn the_estimate_is_the_dialogs_engine_calls() {
    let book = TempBook::copy("synthetic/rich.gnucash");
    let budget = "0123456789abcdef0123456789abcdef";
    let accounts = ["Assets", "Assets:Brokerage", "Expenses:Food", "Income"];
    [("1", "0"), ("3", "0"), ("2", "1")].iter().for_each(|(digits, average)| {
        let work = copy_of(&book);
        let mut d = dispatcher();
        call(&mut d, "book.open", &json!({ "path": work.path_str() }));
        call(&mut d, "budget.scrubSigns", &json!({}));
        let params = json!({
            "budget": budget, "accounts": accounts, "startDate": "2024-01-15",
            "significantDigits": digits.parse::<u8>().unwrap(), "useAverage": *average == "1"
        });
        let result = call(&mut d, "budget.estimate", &params);
        let mut ours = serde_json::Map::new();
        result["amounts"].as_array().unwrap().iter().for_each(|a| {
            let list = ours.entry(a["account"].as_str().unwrap()).or_insert_with(|| json!([]));
            list.as_array_mut().unwrap().push(json!(stored(&a["amount"])));
        });
        let guids: Vec<String> = ours.keys().cloned().collect();
        let path = book.path_str();
        let mut args = vec!["estimate", path.as_str(), budget, "2024-01-15", digits, average];
        args.extend(guids.iter().map(String::as_str));
        let gnucash: Value = serde_json::from_str(&script(&args)).unwrap();
        assert_eq!(Value::Object(ours), gnucash, "{digits} digits, average {average}");
    });
}

#[test]
fn the_feature_comes_with_the_first_budget_and_goes_with_the_last() {
    let book = TempBook::new_path();
    let mut d = dispatcher();
    call(&mut d, "book.create", &json!({ "path": book.path_str() }));
    let g = call(&mut d, "budget.create", &json!({}))["guid"].as_str().unwrap().to_owned();
    call(&mut d, "book.close", &json!({ "save": true }));
    let f = gnucash_features(&book);
    assert_eq!(f, json!({ "naturalSigns": true, "extraColumns": false, "unknown": null, "default": g }));
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    assert_eq!(call(&mut d, "budget.delete", &json!({ "budget": g }))["featureUnset"], true);
    call(&mut d, "book.close", &json!({ "save": true }));
    let f = gnucash_features(&book);
    assert_eq!(f, json!({ "naturalSigns": false, "extraColumns": false, "unknown": null, "default": null }));
}
