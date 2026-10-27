//! The budget methods (`design/api/budgets.md`), on new books and on copies of `synthetic/rich.gnucash`, whose
//! "Budget 2024" predates natural signs. Today is 2024-07-01.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

const NOW: Time64 = Time64(1_719_835_200);
const LEGACY: &str = "0123456789abcdef0123456789abcdef";
const FOOD: &str = "1feca2abf92347288c268ebf93753ec4";

struct Book {
    dir: tempfile::TempDir,
    d: Dispatcher,
}

fn dispatcher() -> Dispatcher {
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(NOW, 0x00b0_0000_0000_0000_0000_0000_0000_0000))));
    d
}

fn path(dir: &tempfile::TempDir) -> String {
    dir.path().join("book.gnucash").display().to_string()
}

/// A copy of `rich.gnucash`.
fn rich() -> Book {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(format!("{}/../../fixtures/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR")), path(&dir)).unwrap();
    let mut d = dispatcher();
    d.call("book.open", json!({ "path": path(&dir) })).unwrap();
    Book { dir, d }
}

/// A new book with Expenses:Food and Income:Salary.
fn fresh() -> Book {
    let dir = tempfile::tempdir().unwrap();
    let mut b = Book { d: dispatcher(), dir };
    b.call("book.create", json!({ "path": path(&b.dir) }));
    b.call("account.create", json!({ "name": "Expenses", "type": "EXPENSE", "commodity": "USD" }));
    b.call("account.create", json!({ "name": "Food", "type": "EXPENSE", "commodity": "USD", "parent": "Expenses" }));
    b.call("account.create", json!({ "name": "Income", "type": "INCOME", "commodity": "USD" }));
    b.call("account.create", json!({ "name": "Salary", "type": "INCOME", "commodity": "USD", "parent": "Income" }));
    b
}

impl Book {
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.d
            .call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        self.d
            .call(method, params.clone())
            .err()
            .unwrap_or_else(|| panic!("{method} {params} should fail"))
    }

    fn reopen(&mut self, read_only: bool) {
        self.call("book.close", json!({ "save": true }));
        self.d = dispatcher();
        self.call("book.open", json!({ "path": path(&self.dir), "readOnly": read_only }));
    }

    fn create(&mut self, params: Value) -> String {
        self.call("budget.create", params)["guid"].as_str().unwrap().to_owned()
    }
}

fn amount(num: &str, denom: &str) -> Value {
    json!({ "num": num, "denom": denom })
}

#[test]
fn create_get_and_reopen_keep_amounts_notes_and_denominators() {
    let mut b = fresh();
    let created = b.call(
        "budget.create",
        json!({ "name": "2025", "numPeriods": 3, "recurrence": { "mult": 1, "periodType": "month", "start": "2024-01-31" } }),
    );
    let g = created["guid"].as_str().unwrap().to_owned();
    assert_eq!(created["description"], "");
    assert_eq!(created["isDefault"], true);
    assert_eq!(
        created["recurrence"],
        json!({ "mult": 1, "periodType": "month", "start": "2024-01-31", "weekendAdjust": "none" })
    );
    assert_eq!(
        created["periods"],
        json!([
            { "index": 0, "startDate": "2024-01-31", "endDate": "2024-02-28" },
            { "index": 1, "startDate": "2024-02-29", "endDate": "2024-03-30" },
            { "index": 2, "startDate": "2024-03-31", "endDate": "2024-04-29" }
        ])
    );
    let set = b.call(
        "budget.setAmounts",
        json!({ "budget": g, "amounts": [
            { "account": "Expenses:Food", "period": 0, "amount": "12.34" },
            { "account": "Expenses:Food", "period": 1, "amount": "0" },
            { "account": "Income:Salary", "period": 0, "amount": { "num": "-5", "denom": "3" } },
            { "account": "Income:Salary", "period": 2, "amount": null }
        ] }),
    );
    assert_eq!((set["set"].clone(), set["unset"].clone()), (json!(3), json!(0)));
    let notes = b.call(
        "budget.setNotes",
        json!({ "budget": g, "notes": [{ "account": "Expenses:Food", "period": 2, "note": "Groceries & <more>" }] }),
    );
    assert_eq!((notes["set"].clone(), notes["deleted"].clone()), (json!(1), json!(0)));
    let list = b.call("budget.list", json!({}));
    assert_eq!(list["naturalSigns"], true);
    assert_eq!(list["needsSignScrub"], false);
    assert_eq!(list["defaultFrom"], "first");
    assert!(list["budgets"][0].get("amounts").is_none());
    b.reopen(true);
    let got = b.call("budget.get", json!({ "budget": g }));
    let cells: Vec<(Value, Value, Value)> = got["amounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["accountName"].clone(), a["period"].clone(), a["amount"].clone()))
        .collect();
    assert_eq!(
        cells,
        [
            (json!("Expenses:Food"), json!(0), amount("1234", "100")),
            (json!("Expenses:Food"), json!(1), amount("0", "1")),
            (json!("Income:Salary"), json!(0), amount("-5", "3")),
        ]
    );
    assert_eq!(got["notes"][0]["note"], "Groceries & <more>");
    let only = b.call("budget.get", json!({ "budget": g, "accounts": ["Income:Salary"] }));
    assert_eq!(only["amounts"].as_array().unwrap().len(), 1);
    assert_eq!(only["notes"], json!([]));
    assert_eq!(b.err("budget.setAmounts", json!({ "budget": g, "amounts": [] })).code, codes::READ_ONLY);
}

#[test]
fn update_hides_amounts_and_copy_and_delete_follow_gnucash() {
    let mut b = fresh();
    let g = b.create(json!({ "name": "Plan" }));
    let cells = json!([
        { "account": "Expenses:Food", "period": 11, "amount": "5" },
        { "account": "Expenses:Food", "period": 0, "amount": "7" }
    ]);
    b.call("budget.setAmounts", json!({ "budget": g, "amounts": cells }));
    b.call("budget.setNotes", json!({ "budget": g, "notes": [{ "account": "Expenses:Food", "period": 0, "note": "n" }] }));
    let lowered = b.call("budget.update", json!({ "budget": g, "numPeriods": 6, "description": "half" }));
    assert_eq!((lowered["numPeriods"].clone(), lowered["description"].clone()), (json!(6), json!("half")));
    assert_eq!(lowered["hidden"], json!({ "amounts": 1, "notes": 0 }));
    assert_eq!(lowered["amounts"].as_array().unwrap().len(), 1);
    let copy = b.call("budget.copy", json!({ "budget": g }));
    assert_eq!(copy["name"], "Copy of Plan");
    assert_eq!(copy["description"], "half");
    assert_eq!(copy["notes"], json!([]), "notes are not copied");
    assert_eq!(copy["hidden"], json!({ "amounts": 0, "notes": 0 }), "nor hidden amounts");
    let raised = b.call("budget.update", json!({ "budget": g, "numPeriods": 12 }));
    assert_eq!(raised["amounts"].as_array().unwrap().len(), 2, "hidden amounts come back");

    let copy_guid = copy["guid"].as_str().unwrap();
    let options = b.call("book.setOptions", json!({ "defaultBudget": copy_guid }));
    assert_eq!(options["defaultBudget"], copy_guid);
    let list = b.call("budget.list", json!({}));
    assert_eq!((list["default"].as_str(), list["defaultFrom"].as_str()), (Some(copy_guid), Some("option")));
    assert_eq!(b.call("budget.delete", json!({ "budget": copy_guid })), json!({ "deleted": true, "featureUnset": false }));
    assert_eq!(b.call("book.options", json!({}))["defaultBudget"], copy_guid, "left stale");
    assert_eq!(b.call("budget.list", json!({}))["defaultFrom"], "first");
    assert_eq!(b.call("budget.delete", json!({ "budget": g })), json!({ "deleted": true, "featureUnset": true }));
    assert_eq!(b.call("budget.list", json!({}))["naturalSigns"], false);
    let gone = b.err("book.setOptions", json!({ "defaultBudget": g }));
    assert_eq!((gone.code, gone.data), (codes::NOT_FOUND, Some(json!({ "kind": "budget", "id": g }))));
}

#[test]
fn refusals_are_the_documented_codes() {
    let mut b = fresh();
    let g = b.create(json!({}));
    let invalid = [
        ("budget.create", json!({ "numPeriods": 61 })),
        ("budget.create", json!({ "recurrence": { "mult": 0, "periodType": "once", "start": "2024-01-01" } })),
        ("budget.create", json!({ "recurrence": { "mult": 1, "periodType": "month", "start": "2024-01-01", "weekendAdjust": "back" } })),
        ("budget.update", json!({ "budget": g, "numPeriods": 0 })),
        ("budget.setAmounts", json!({ "budget": g, "amounts": [{ "account": "Expenses:Food", "period": 12, "amount": "1" }] })),
        (
            "budget.setAmounts",
            json!({ "budget": g, "amounts": [
                { "account": "Expenses:Food", "period": 1, "amount": "1" },
                { "account": "Expenses:Food", "period": 1, "amount": null }
            ] }),
        ),
        ("budget.estimate", json!({ "budget": g, "accounts": [], "startDate": "2024-01-01" })),
        ("budget.estimate", json!({ "budget": g, "accounts": ["Expenses:Food"], "startDate": "2024-01-01", "significantDigits": 13 })),
    ];
    invalid
        .into_iter()
        .for_each(|(m, p)| assert_eq!(b.err(m, p.clone()).code, codes::INVALID, "{m} {p}"));
    let not_found = b.err("budget.get", json!({ "budget": "00000000000000000000000000000001" }));
    assert_eq!(not_found.data.unwrap()["kind"], "budget");
    let account = b.err("budget.setNotes", json!({ "budget": g, "notes": [{ "account": "Nope", "period": 0, "note": "x" }] }));
    assert_eq!((account.code, account.data.unwrap()["kind"].clone()), (codes::NOT_FOUND, json!("account")));
    let number = b.err("budget.setAmounts", json!({ "budget": g, "amounts": [{ "account": "Expenses:Food", "period": 0, "amount": 5 }] }));
    assert_eq!(number.code, codes::INVALID_PARAMS, "amounts are never JSON numbers");
    assert_eq!(b.err("budget.estimate", json!({ "budget": g, "accounts": ["Expenses:Food"] })).code, codes::INVALID_PARAMS);
    let got = b.call("budget.get", json!({ "budget": g }));
    assert_eq!(got["amounts"], json!([]), "nothing applied");
}

#[test]
fn a_legacy_book_needs_the_scrub_before_edits() {
    let mut b = rich();
    let list = b.call("budget.list", json!({}));
    assert_eq!((list["needsSignScrub"].clone(), list["naturalSigns"].clone()), (json!(true), json!(false)));
    assert_eq!((list["default"].as_str(), list["defaultFrom"].as_str()), (Some(LEGACY), Some("first")));
    let edits = [
        ("budget.create", json!({})),
        ("budget.update", json!({ "budget": LEGACY, "name": "x" })),
        ("budget.copy", json!({ "budget": LEGACY })),
        ("budget.setAmounts", json!({ "budget": LEGACY, "amounts": [] })),
        ("budget.setNotes", json!({ "budget": LEGACY, "notes": [] })),
        ("budget.estimate", json!({ "budget": LEGACY, "accounts": [FOOD], "startDate": "2024-01-01" })),
    ];
    edits.into_iter().for_each(|(m, p)| {
        let e = b.err(m, p);
        assert_eq!((e.code, e.data), (codes::CONFLICT, Some(json!({ "reason": "natural-signs" }))), "{m}");
    });
    let got = b.call("budget.get", json!({ "budget": LEGACY }));
    assert_eq!(got["notes"][0]["note"], "January food");
    let scrub = b.call("budget.scrubSigns", json!({}));
    assert_eq!(
        scrub,
        json!({ "featureSet": true, "featureUnset": false, "budgets": [{ "budget": LEGACY, "policy": "creditAccounts", "flipped": 0 }] })
    );
    assert_eq!(b.call("budget.scrubSigns", json!({}))["budgets"], json!([]), "a second run does nothing");
    let estimate = b.call(
        "budget.estimate",
        json!({ "budget": LEGACY, "accounts": ["Expenses:Food"], "startDate": "2024-01-01" }),
    );
    let figures: Vec<Value> = estimate["amounts"].as_array().unwrap().iter().map(|a| a["amount"].clone()).collect();
    assert_eq!(figures.len(), 12);
    assert_eq!(figures[5], amount("12", "1"), "June's 12.34 to one significant digit");
    assert_eq!(figures[0], amount("0", "1"), "a zero change is a set zero");
}
