//! The account and accounting-period methods through the dispatcher.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use red_ale_service::config::ConfigStore;
use serde_json::{Value, json};

struct Book {
    _dir: tempfile::TempDir,
    d: Dispatcher,
}

fn dispatcher(settings: Settings) -> Dispatcher {
    let mut d = Dispatcher::new(settings);
    // 2024-07-01T12:00:00Z
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(1_719_835_200), 0xa0 << 120))));
    d
}

fn open(fixture: &str) -> Book {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(format!("{}/../../fixtures/{fixture}", env!("CARGO_MANIFEST_DIR")), &path).unwrap();
    let mut d = dispatcher(Settings::default());
    d.call("book.open", json!({ "path": path.display().to_string() })).unwrap();
    Book { _dir: dir, d }
}

fn create() -> Book {
    let dir = tempfile::tempdir().unwrap();
    let mut d = dispatcher(Settings::default());
    d.call(
        "book.create",
        json!({ "path": dir.path().join("new.gnucash").display().to_string() }),
    )
    .unwrap();
    Book { _dir: dir, d }
}

impl Book {
    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.d
            .call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        match self.d.call(method, params.clone()) {
            Ok(v) => panic!("{method} {params} should fail, gave {v}"),
            Err(e) => e,
        }
    }

    fn add(&mut self, name: &str, ty: &str, parent: Option<&str>) -> String {
        let mut p = json!({ "name": name, "type": ty, "commodity": "USD" });
        if let Some(parent) = parent {
            p["parent"] = json!(parent);
        }
        self.ok("account.create", p)["guid"].as_str().unwrap().to_owned()
    }

    fn post(&mut self, debit: &str, credit: &str, amount: &str, date: &str) {
        self.d
            .context()
            .mutate_rpc(|book, env| {
                let mut t = red_ale_core::ops::commit::NewTransaction::new(
                    red_ale_core::CommodityRef::currency("USD"),
                    red_ale_core::Date::parse(date).unwrap(),
                );
                let a = red_ale_rpc::wire::AccountRef(debit.into()).resolve(book)?;
                let b = red_ale_rpc::wire::AccountRef(credit.into()).resolve(book)?;
                let n = red_ale_core::Numeric::parse_decimal(amount).unwrap();
                t.splits = vec![
                    red_ale_core::ops::commit::NewSplit::new(a, n),
                    red_ale_core::ops::commit::NewSplit::new(b, n.neg()),
                ];
                red_ale_core::ops::commit::add_transaction(book, env, t).map_err(RpcError::from)
            })
            .unwrap();
    }
}

fn amount(v: &Value) -> String {
    format!("{}/{}", v["num"].as_str().unwrap(), v["denom"].as_str().unwrap())
}

#[test]
fn types_are_listed_with_gnucash_names_and_rules() {
    let mut b = create();
    let types = b.ok("account.types", json!({}));
    let ar = types.as_array().unwrap().iter().find(|t| t["name"] == "RECEIVABLE").unwrap();
    assert_eq!(ar["label"], "A/Receivable");
    assert_eq!(ar["fundamental"], "ASSET");
    assert_eq!(ar["changesWithSplits"], json!(["RECEIVABLE"]));
    let currency = types.as_array().unwrap().iter().find(|t| t["name"] == "CURRENCY").unwrap();
    assert_eq!(currency["choosable"], false);
    let stock = types.as_array().unwrap().iter().find(|t| t["name"] == "STOCK").unwrap();
    assert_eq!(stock["commodity"], "security");
}

#[test]
fn hierarchy_create_move_list_and_tree() {
    let mut b = create();
    b.add("Assets", "ASSET", None);
    b.add("Current", "ASSET", Some("Assets"));
    b.add("Checking", "BANK", Some("Assets:Current"));
    b.add("Liabilities", "LIABILITY", None);
    let card = b.ok(
        "account.create",
        json!({ "name": "Card", "type": "CREDIT", "parent": "Liabilities" }),
    );
    assert_eq!(card["commodity"], "CURRENCY::USD", "the parent's commodity by default");
    assert_eq!(card["fullName"], "Liabilities:Card");
    assert_eq!(
        b.err(
            "account.create",
            json!({ "name": "Card", "type": "CREDIT", "parent": "Liabilities" })
        )
        .code,
        codes::CONFLICT
    );
    assert_eq!(
        b.err("account.create", json!({ "name": "A:B", "type": "ASSET", "commodity": "USD" }))
            .code,
        codes::INVALID
    );
    assert_eq!(
        b.err("account.create", json!({ "name": "Food", "type": "EXPENSE", "parent": "Assets" }))
            .code,
        codes::INVALID
    );
    assert_eq!(
        b.err("account.create", json!({ "name": "X", "type": "WIDGET", "commodity": "USD" }))
            .code,
        codes::INVALID_PARAMS
    );
    assert_eq!(
        b.err(
            "account.create",
            json!({ "name": "X", "type": "BANK", "commodity": "USD", "bogus": 1 })
        )
        .code,
        codes::INVALID_PARAMS
    );
    let moved = b.ok("account.move", json!({ "account": "Assets:Current", "parent": "Liabilities" }));
    assert_eq!(moved["fullName"], "Liabilities:Current");
    let names: Vec<String> = b.ok("account.list", json!({}))["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["fullName"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names,
        [
            "Assets",
            "Liabilities",
            "Liabilities:Card",
            "Liabilities:Current",
            "Liabilities:Current:Checking"
        ]
    );
    let tree = b.ok("account.tree", json!({ "under": "Liabilities" }));
    let kids: Vec<&str> = tree["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(kids, ["Card", "Current"]);
    assert_eq!(
        b.err("account.move", json!({ "account": "Liabilities", "parent": "Liabilities:Current" }))
            .code,
        codes::INVALID
    );
    assert_eq!(b.err("account.get", json!({ "account": "Nowhere" })).code, codes::NOT_FOUND);
}

#[test]
fn settings_flags_codes_and_tax_fields() {
    let mut b = open("synthetic/rich.gnucash");
    let food = b.ok(
        "account.update",
        json!({
            "account": "Expenses:Food", "code": "5100", "description": "Groceries", "notes": "weekly", "placeholder": true,
            "hidden": true, "taxRelated": true, "taxUsCode": "N123", "taxUsPayerNameSource": "current", "taxUsCopyNumber": 2,
        }),
    );
    assert_eq!(
        (food["code"].as_str(), food["placeholder"].as_bool(), food["hidden"].as_bool()),
        (Some("5100"), Some(true), Some(true))
    );
    assert_eq!(
        food["taxUs"],
        json!({ "code": "N123", "payerNameSource": "current", "copyNumber": 2 })
    );
    let cleared = b.ok(
        "account.update",
        json!({ "account": "Expenses:Food", "notes": "", "taxUsCopyNumber": 0, "hidden": false }),
    );
    assert_eq!(
        (
            cleared["notes"].is_null(),
            cleared["taxUs"]["copyNumber"].as_i64(),
            cleared["hidden"].as_bool()
        ),
        (true, Some(1), Some(false))
    );
    let scu = b.ok("account.update", json!({ "account": "Expenses:Food", "scu": 1000 }));
    assert_eq!((scu["scu"].as_i64(), scu["nonStandardScu"].as_bool()), (Some(1000), Some(true)));
    let back = b.ok("account.update", json!({ "account": "Expenses:Food", "scu": "commodity" }));
    assert_eq!((back["scu"].as_i64(), back["nonStandardScu"].as_bool()), (Some(100), Some(false)));
    assert_eq!(
        b.err("account.update", json!({ "account": "Expenses:Food", "scu": 300 })).code,
        codes::INVALID
    );
}

#[test]
fn type_change_retypes_children_on_request() {
    let mut b = create();
    b.add("Stuff", "ASSET", None);
    b.add("Box", "BANK", Some("Stuff"));
    assert_eq!(
        b.err("account.update", json!({ "account": "Stuff", "type": "EXPENSE" })).code,
        codes::CONFLICT
    );
    b.ok(
        "account.update",
        json!({ "account": "Stuff", "type": "EXPENSE", "retypeChildren": true }),
    );
    assert_eq!(b.ok("account.get", json!({ "account": "Stuff:Box" }))["type"], "EXPENSE");
    b.add("Customers", "RECEIVABLE", None);
    b.add("Sales", "INCOME", None);
    b.post("Customers", "Sales", "10", "2024-03-01");
    assert_eq!(
        b.err("account.update", json!({ "account": "Customers", "type": "ASSET" })).code,
        codes::CONFLICT,
        "A/R with splits keeps its type"
    );
    assert_eq!(
        b.err("account.update", json!({ "account": "Sales", "commodity": "EUR" })).code,
        codes::CONFLICT,
        "commodity fixed by splits"
    );
    b.ok("account.update", json!({ "account": "Sales", "type": "EXPENSE" }));
}

#[test]
fn renumber_sub_accounts() {
    let mut b = create();
    b.add("Expenses", "EXPENSE", None);
    ["Rent", "Food", "Car"].iter().for_each(|n| {
        b.add(n, "EXPENSE", Some("Expenses"));
    });
    b.ok("account.update", json!({ "account": "Expenses", "code": "5000" }));
    let codes: Vec<(String, String)> = b
        .ok("account.renumber", json!({ "account": "Expenses", "interval": 5, "digits": 2 }))
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["fullName"].as_str().unwrap().to_owned(), c["code"].as_str().unwrap().to_owned()))
        .collect();
    assert_eq!(
        codes,
        [
            ("Expenses:Car".into(), "5000-05".into()),
            ("Expenses:Food".into(), "5000-10".into()),
            ("Expenses:Rent".into(), "5000-15".into())
        ]
    );
    assert_eq!(
        b.err("account.renumber", json!({ "account": "Expenses:Car" })).code,
        codes::INVALID,
        "no sub-accounts"
    );
}

#[test]
fn delete_with_move() {
    let mut b = create();
    b.add("Bank", "BANK", None);
    b.add("Old", "BANK", None);
    b.add("Old Sub", "BANK", Some("Old"));
    b.add("Food", "EXPENSE", None);
    b.post("Food", "Old", "12.50", "2024-01-05");
    b.post("Food", "Old:Old Sub", "1.25", "2024-01-06");
    assert_eq!(
        b.err("account.delete", json!({ "account": "Old" })).code,
        codes::INVALID,
        "choices are required"
    );
    let bad = json!({ "account": "Old", "transactions": { "moveTo": "Food" }, "subaccount": "delete", "subaccountTransactions": "delete" });
    assert_eq!(
        b.err("account.delete", bad).code,
        codes::INVALID,
        "the target must be a BANK account"
    );
    let r = b.ok(
        "account.delete",
        json!({ "account": "Old", "transactions": { "moveTo": "Bank" }, "subaccount": { "moveTo": "Bank" } }),
    );
    assert_eq!(r["deletedAccounts"].as_array().unwrap().len(), 1);
    assert_eq!(b.ok("account.get", json!({ "account": "Bank:Old Sub" }))["hasSplits"], true);
    let bal = b.ok("account.balance", json!({ "account": "Bank", "includeSubaccounts": true }));
    assert_eq!(amount(&bal["balances"]["balance"]), "-1375/100");
    assert!(b.ok("book.info", json!({}))["dirty"].as_bool().unwrap());
}

#[test]
fn balances_as_of_and_reconciled() {
    let mut b = create();
    b.add("Bank", "BANK", None);
    b.add("Salary", "INCOME", None);
    b.post("Bank", "Salary", "100", "2024-01-31");
    b.post("Bank", "Salary", "50.25", "2024-02-29");
    let now = b.ok("account.balance", json!({ "account": "Bank" }));
    assert_eq!(amount(&now["balances"]["balance"]), "15025/100");
    assert_eq!(amount(&now["balances"]["cleared"]), "0/1");
    let jan = b.ok("account.balance", json!({ "account": "Bank", "asOf": "2024-01-31" }));
    assert_eq!(amount(&jan["balances"]["balance"]), "10000/100");
    let before = b.ok("account.balance", json!({ "account": "Bank", "asOf": "2023-12-31" }));
    assert_eq!(amount(&before["balances"]["balance"]), "0/1");
}

#[test]
fn accounting_period_in_memory_and_in_a_file() {
    let mut b = create();
    let got = b.ok("period.get", json!({}));
    assert_eq!(
        (got["start"].as_str(), got["end"].as_str(), got["configFile"].is_null()),
        (Some("year"), Some("year"), true)
    );
    let dates = b.ok("period.dates", json!({}));
    let want = json!({
        "today": "2024-07-01",
        "start": "2024-01-01",
        "end": "2024-12-31",
        "startTime": "2024-01-01T00:00:00Z",
        "endTime": "2024-12-31T23:59:59Z"
    });
    assert_eq!(dates, want, "local 00:00:00 and 23:59:59, in UTC here");
    assert_eq!(
        b.err("period.set", json!({ "start": "fiscal-year" })).code,
        codes::INVALID,
        "no fiscal year end"
    );
    b.ok(
        "period.set",
        json!({ "start": "fiscal-year", "end": "fiscal-year", "fiscalYearEnd": "03-31" }),
    );
    assert_eq!(b.ok("book.info", json!({}))["dirty"], false, "the period is not book data");
    let dates = b.ok("period.dates", json!({ "date": "2024-03-31" }));
    assert_eq!(
        (dates["start"].as_str(), dates["end"].as_str()),
        (Some("2023-04-01"), Some("2024-03-31"))
    );
    b.ok("book.close", json!({}));
    assert_eq!(
        b.ok("period.get", json!({}))["fiscalYearEnd"],
        "03-31",
        "kept for the server's lifetime"
    );
    assert_eq!(b.err("period.set", json!({ "start": "someday" })).code, codes::INVALID_PARAMS);

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("red-ale.json");
    let settings = Settings {
        config: ConfigStore::at(&file),
        ..Settings::default()
    };
    let mut d = dispatcher(settings.clone());
    d.call("period.set", json!({ "start": "previous-month", "end": "2024-12-31" }))
        .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("previous-month"), "{text}");
    let mut again = dispatcher(Settings {
        config: ConfigStore::at(&file),
        ..Settings::default()
    });
    let got = again.call("period.get", json!({})).unwrap();
    assert_eq!(
        (got["start"].as_str(), got["end"].as_str()),
        (Some("previous-month"), Some("2024-12-31"))
    );
}
