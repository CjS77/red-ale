//! CSV transaction import through `import.preview` and `import.apply`, and the import map editor, on hand-built
//! books and GnuCash's own `sample1.csv` (`design/api/import.md`, "Tests").

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use base64::Engine;
use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

/// 2024-07-01 12:00 UTC.
const NOW: i64 = 1_719_835_200;

struct Book {
    dir: tempfile::TempDir,
    d: Dispatcher,
}

impl Book {
    /// A new book with a checking account and a few expense and income accounts, all in USD.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut d = Dispatcher::new(Settings::default());
        d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(NOW), 1 << 100))));
        d.call("book.create", json!({ "path": dir.path().join("book.gnucash").display().to_string() })).unwrap();
        let mut b = Self { dir, d };
        [("Assets", "ASSET", None), ("Checking", "BANK", Some("Assets")), ("Expenses", "EXPENSE", None), ("Groceries", "EXPENSE", Some("Expenses"))]
            .into_iter()
            .chain([("Rent", "EXPENSE", Some("Expenses")), ("Income", "INCOME", None), ("Salary", "INCOME", Some("Income"))])
            .for_each(|(name, ty, parent)| {
                let mut p = json!({ "name": name, "type": ty, "commodity": "USD" });
                if let Some(parent) = parent {
                    p["parent"] = json!(parent);
                }
                b.ok("account.create", p);
            });
        b
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        self.d.call(method, params).unwrap_err()
    }

    fn preview(&mut self, text: &str, layout: Value) -> Value {
        self.ok("import.preview", json!({ "format": "csvTransactions", "files": [{ "name": "statement.csv", "text": text }], "layout": layout }))
    }

    fn transactions(&mut self) -> Vec<Value> {
        self.ok("transaction.list", json!({}))["transactions"].as_array().unwrap().clone()
    }

    fn guid(&mut self, account: &str) -> String {
        self.ok("account.get", json!({ "account": account }))["guid"].as_str().unwrap().to_owned()
    }
}

/// A statement of the checking account: date, description, amount.
fn bank_layout() -> Value {
    json!({ "skipStartLines": 1, "columns": ["Date", "Description", "Amount"], "baseAccount": "Assets:Checking" })
}

const MARCH: &str = "Date,Description,Amount\n2024-03-01,Corner Grocer,-45.10\n2024-03-02,Rent March,-950.00\n2024-03-05,Salary ACME,2000.00\n";

fn actions(p: &Value) -> Vec<&str> {
    p["transactions"].as_array().unwrap().iter().map(|t| t["action"].as_str().unwrap()).collect()
}

fn usd(cents: &str) -> Value {
    json!({ "num": cents, "denom": "100" })
}

#[test]
fn a_statement_is_proposed_applied_and_learned_then_imports_again_as_nothing_new() {
    let mut b = Book::new();
    let p = b.preview(MARCH, bank_layout());
    assert_eq!(p["format"], "csvTransactions");
    assert_eq!(actions(&p), ["add", "add", "add"]);
    assert_eq!(p["summary"]["add"], 3);
    assert_eq!(p["summary"]["imbalanced"], 3, "no destination is known yet, so each would be balanced into Imbalance-USD");
    assert_eq!(p["newAccounts"], json!([{ "name": "Imbalance-USD", "type": "BANK", "commodity": "CURRENCY::USD" }]));
    assert_eq!(p["ignoredLines"], json!([{ "file": 0, "line": 1, "reason": "skipStartLines" }]));
    let t0 = &p["transactions"][0];
    assert_eq!(t0["source"], json!({ "file": 0, "lines": [2, 2] }));
    assert_eq!(t0["datePosted"], "2024-03-01");
    assert_eq!(t0["splits"][0]["accountName"], "Assets:Checking");
    assert_eq!(t0["splits"][0]["value"], usd("-4510"));
    assert_eq!(t0["other"], json!({ "account": null, "accountName": null, "from": null, "value": usd("4510"), "amount": { "num": "0", "denom": "1" } }));
    assert_eq!(t0["imbalance"]["value"], usd("4510"));
    assert_eq!(t0["imbalance"]["account"], Value::Null);
    assert_eq!(t0["warnings"][0]["code"], "imbalance");

    let changes = json!([{ "index": 0, "account": "Expenses:Groceries" }, { "index": 1, "account": "Expenses:Rent" }]);
    let applied = b.ok("import.apply", json!({ "proposal": p["id"], "changes": changes }));
    assert_eq!(applied["added"].as_array().unwrap().len(), 3);
    let imbalance = applied["imbalanceSplits"].as_array().unwrap();
    assert_eq!(imbalance.len(), 1, "only Salary had no destination");
    assert_eq!(imbalance[0]["index"], 2);
    assert_eq!(imbalance[0]["value"], usd("-200000"));
    assert!(applied["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "imbalance" && w["index"] == 2));
    assert_eq!(applied["accounts"]["created"].as_array().unwrap().len(), 1);
    let bayes = applied["learned"]["bayes"].as_array().unwrap();
    assert_eq!(bayes.len(), 2, "the two destinations chosen are learned");
    assert_eq!(bayes[0]["tokens"], json!(["Friday", "Grocer", "Corner"]), "GnuCash's token order: each new one in front");
    assert_eq!(b.transactions().len(), 3);
    let features = b.ok("book.info", json!({}))["features"].clone();
    let names: Vec<&str> = features.as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Account GUID based bayesian with flat KVP"], "the only feature, as GnuCash sets it");

    let again = b.preview(MARCH, bank_layout());
    assert_eq!(actions(&again), ["clear", "clear", "clear"], "the matcher finds every transaction");
    assert_eq!(again["summary"]["add"], 0);
    assert_eq!(again["transactions"][0]["matches"][0]["score"], 8, "amount 3, same day 3, description 2");
    let cleared = b.ok("import.apply", json!({ "proposal": again["id"] }));
    assert_eq!(cleared["cleared"].as_array().unwrap().len(), 3);
    assert_eq!(b.transactions().len(), 3, "nothing added");
}

#[test]
fn a_later_statement_finds_its_destinations_in_the_learned_map() {
    let mut b = Book::new();
    let p = b.preview(MARCH, bank_layout());
    let changes = json!([{ "index": 0, "account": "Expenses:Groceries" }, { "index": 1, "account": "Expenses:Rent" }, { "index": 2, "account": "Income:Salary" }]);
    b.ok("import.apply", json!({ "proposal": p["id"], "changes": changes }));
    // Saturday's token was learned with the rent, so the April rent is on a Saturday too: on a Tuesday, the
    // Salary's Tuesday would tie with Rent at certainty, and GnuCash takes the account first seen in token order.
    let april = "Date,Description,Amount\n2024-04-01,Corner Grocer,-51.20\n2024-04-06,Rent April,-950.00\n2024-04-08,Bookshop,-12.00\n";
    let p = b.preview(april, bank_layout());
    let other = |i: usize| (p["transactions"][i]["other"]["accountName"].clone(), p["transactions"][i]["other"]["from"].clone());
    assert_eq!(other(0), (json!("Expenses:Groceries"), json!("bayes")));
    assert_eq!(other(1), (json!("Expenses:Rent"), json!("bayes")));
    assert_eq!(other(2), (Value::Null, Value::Null), "nothing was learned about a bookshop");
    assert_eq!(p["summary"]["imbalanced"], 1);
    let applied = b.ok("import.apply", json!({ "proposal": p["id"] }));
    assert!(applied["learned"]["bayes"].as_array().unwrap().is_empty(), "an account the map found is not learned again");
    assert_eq!(applied["imbalanceSplits"].as_array().unwrap().len(), 1);
}

#[test]
fn every_action_and_the_callers_changes() {
    let mut b = Book::new();
    let existing = |b: &mut Book, date: &str, desc: &str, cents: &str| {
        b.ok(
            "transaction.post",
            json!({ "description": desc, "datePosted": date, "splits": [
                { "account": "Assets:Checking", "value": usd(cents) },
                { "account": "Expenses:Groceries", "value": usd(cents.trim_start_matches('-')) },
            ]}),
        )["guid"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let exact = existing(&mut b, "2024-05-10", "Corner Grocer", "-2000");
    let fee = existing(&mut b, "2024-05-12", "Cash machine", "-10000");
    existing(&mut b, "2024-05-20", "Market", "-3300");
    let file = "Date,Description,Amount\n2024-05-10,Corner Grocer,-20.00\n2024-05-12,Cash machine,-101.50\n2024-05-22,Fish stall,-33.00\n2024-05-25,Florist,-8.00\n";
    let p = b.preview(file, bank_layout());
    assert_eq!(actions(&p), ["clear", "update", "skip", "add"]);
    let t = &p["transactions"];
    assert_eq!(t[0]["matches"][0]["transaction"], exact.as_str());
    assert_eq!(t[0]["matches"][0]["updateProposed"], false);
    assert_eq!(t[1]["matches"][0]["transaction"], fee.as_str());
    assert_eq!(t[1]["matches"][0]["updateProposed"], true, "1.50 apart: within the ATM fee threshold");
    assert_eq!(t[2]["matches"][0]["score"], 5, "same amount, 2 days apart, different description");
    assert_eq!(t[1]["imbalance"], Value::Null, "update moves the other split with it");

    assert_eq!(b.err("import.apply", json!({ "proposal": p["id"], "changes": [{ "index": 3, "action": "clear" }] })).code, codes::INVALID);
    assert_eq!(b.err("import.apply", json!({ "proposal": p["id"], "changes": [{ "index": 9, "action": "skip" }] })).code, codes::INVALID);
    let bogus = "0123456789abcdef0123456789abcdef";
    assert_eq!(b.err("import.apply", json!({ "proposal": p["id"], "changes": [{ "index": 0, "match": bogus }] })).code, codes::INVALID);
    b.ok("account.update", json!({ "account": "Expenses", "placeholder": true }));
    let p = b.preview(file, bank_layout());
    assert_eq!(b.err("import.apply", json!({ "proposal": p["id"], "changes": [{ "index": 3, "account": "Expenses" }] })).code, codes::INVALID);

    let changes = json!([{ "index": 2, "action": "add", "account": "Expenses:Groceries" }, { "index": 3, "action": "skip" }]);
    let applied = b.ok("import.apply", json!({ "proposal": p["id"], "changes": changes }));
    let index = |list: &str| applied[list].as_array().unwrap().iter().map(|x| x["index"].as_u64().unwrap()).collect::<Vec<_>>();
    assert_eq!((index("cleared"), index("updated"), index("added")), (vec![0], vec![1], vec![2]));
    assert_eq!(applied["skipped"], json!([3]));
    let updated = b.ok("transaction.get", json!({ "guid": fee }));
    let checking = b.guid("Assets:Checking");
    let split = updated["splits"].as_array().unwrap().iter().find(|s| s["account"] == checking.as_str()).unwrap().clone();
    assert_eq!(split["value"], usd("-10150"));
    assert_eq!(split["reconcileState"], "c");
    assert_eq!(b.transactions().len(), 4);
}

#[test]
fn a_stale_proposal_is_a_conflict_and_a_discarded_one_is_gone() {
    let mut b = Book::new();
    let p = b.preview(MARCH, bank_layout());
    let q = b.preview(MARCH, bank_layout());
    assert_ne!(p["id"], q["id"]);
    let revision = p["bookRevision"].as_u64().unwrap();
    b.ok("account.create", json!({ "name": "Cash", "type": "CASH", "commodity": "USD", "parent": "Assets" }));
    let e = b.err("import.apply", json!({ "proposal": p["id"] }));
    assert_eq!(e.code, codes::CONFLICT);
    assert_eq!(e.data.unwrap(), json!({ "reason": "book-revision", "proposalRevision": revision, "bookRevision": revision + 1 }));
    assert_eq!(b.ok("import.discard", json!({ "proposal": q["id"] })), json!({ "discarded": true }));
    let e = b.err("import.apply", json!({ "proposal": q["id"] }));
    assert_eq!((e.code, e.data.unwrap()["kind"].clone()), (codes::NOT_FOUND, json!("importProposal")));
    assert_eq!(b.err("import.discard", json!({ "proposal": q["id"] })).code, codes::NOT_FOUND);

    let fresh = b.preview(MARCH, bank_layout());
    b.ok("import.apply", json!({ "proposal": fresh["id"] }));
    assert_eq!(b.err("import.apply", json!({ "proposal": fresh["id"] })).code, codes::NOT_FOUND, "an applied proposal is no longer held");
}

#[test]
fn seventeen_previews_push_out_the_first() {
    let mut b = Book::new();
    let first = b.preview(MARCH, bank_layout());
    (0..16).for_each(|_| {
        b.preview(MARCH, bank_layout());
    });
    assert_eq!(b.err("import.apply", json!({ "proposal": first["id"] })).code, codes::NOT_FOUND);
}

#[test]
fn apply_is_one_mutation_all_or_nothing() {
    let mut b = Book::new();
    b.ok("commodity.create", json!({ "namespace": "NASDAQ", "mnemonic": "ACME", "fullname": "Acme", "fraction": 1 }));
    b.ok("account.create", json!({ "name": "Acme", "type": "STOCK", "commodity": "NASDAQ::ACME", "parent": "Assets" }));
    let before = b.ok("book.info", json!({}));
    let p = b.preview(MARCH, bank_layout());
    // Index 0 is applied last (GnuCash's rows are in reverse order of arrival): its amount rounds to nothing at the
    // stock's fraction of 1, after the other two were added.
    let changes = json!([
        { "index": 0, "account": "Assets:Acme", "amount": "0.1" },
        { "index": 1, "account": "Expenses:Rent" },
        { "index": 2, "account": "Income:Salary" },
    ]);
    let e = b.err("import.apply", json!({ "proposal": p["id"], "changes": changes }));
    assert_eq!(e.code, codes::INVALID, "{e}");
    assert!(b.transactions().is_empty(), "nothing was added");
    assert_eq!(b.ok("book.info", json!({}))["features"], before["features"], "nothing was learned");
    assert!(b.ok("importMap.list", json!({}))["entries"].as_array().unwrap().is_empty());
    let retry = json!([{ "index": 0, "account": "Expenses:Groceries" }]);
    assert_eq!(b.ok("import.apply", json!({ "proposal": p["id"], "changes": retry }))["added"].as_array().unwrap().len(), 3, "the proposal stays held");
}

#[test]
fn read_only_by_date_is_flagged_and_still_applied() {
    let mut b = Book::new();
    b.ok("book.setOptions", json!({ "autoReadOnlyDays": 120 }));
    let p = b.preview(MARCH, bank_layout());
    assert_eq!(p["thresholdDate"], "2024-03-03");
    let flags: Vec<bool> = p["transactions"].as_array().unwrap().iter().map(|t| t["readOnlyByDate"].as_bool().unwrap()).collect();
    assert_eq!(flags, [true, true, false]);
    assert_eq!(p["summary"]["readOnlyByDate"], 2);
    assert!(p["transactions"][0]["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "read-only-by-date"));
    let applied = b.ok("import.apply", json!({ "proposal": p["id"] }));
    assert_eq!(applied["added"].as_array().unwrap().len(), 3);
    let warned: Vec<u64> = applied["warnings"].as_array().unwrap().iter().filter(|w| w["code"] == "read-only-by-date").map(|w| w["index"].as_u64().unwrap()).collect();
    assert_eq!(warned, [0, 1]);
}

#[test]
fn error_rows_need_skip_errors_and_account_names_are_matched_and_learned() {
    let mut b = Book::new();
    let layout = json!({ "skipStartLines": 1, "columns": ["Date", "Description", "Account", "Amount"] });
    let file = "Date,Description,Account,Amount\n2024-03-01,Grocer,Assets:Checking,-5.00\n2024-03-02,Pay,Current,10.00\n2024-03-33,Bad,Assets:Checking,1.00\n";
    let p = b.ok("import.preview", json!({ "format": "csvTransactions", "files": [{ "text": file }], "layout": layout.clone() }));
    assert_eq!(p["transactions"].as_array().unwrap().len(), 1);
    let errors: Vec<(u64, Value)> = p["errors"].as_array().unwrap().iter().map(|e| (e["line"].as_u64().unwrap(), e["column"].clone())).collect();
    assert_eq!(errors, [(3, json!(3)), (4, json!(1))]);
    assert_eq!(p["errors"][0]["message"], "Account: Account value can't be mapped back to an account.");
    let names = &p["csv"]["accountNames"];
    assert_eq!(names[1], json!({ "name": "Current", "account": null, "accountName": null, "from": null }));
    assert_eq!(names[0]["from"], "fullName");
    assert_eq!(b.err("import.apply", json!({ "proposal": p["id"] })).code, codes::INVALID);

    let mapped = json!({ "format": "csvTransactions", "files": [{ "text": file }], "layout": layout.clone(), "accountNames": { "Current": "Assets:Checking" } });
    let p = b.ok("import.preview", mapped);
    assert_eq!(p["transactions"].as_array().unwrap().len(), 2);
    assert_eq!(p["csv"]["accountNames"][1]["from"], "param");
    let applied = b.ok("import.apply", json!({ "proposal": p["id"], "skipErrors": true }));
    let checking = b.guid("Assets:Checking");
    assert_eq!(applied["learned"]["csvAccountMap"], json!([{ "name": "Current", "account": checking }]));
    let p = b.ok("import.preview", json!({ "format": "csvTransactions", "files": [{ "text": file }], "layout": layout }));
    assert_eq!(p["csv"]["accountNames"][1]["from"], "map", "the name is now in the csv-account-map");
    let entries = b.ok("importMap.list", json!({ "kind": "csvAccountMap" }))["entries"].clone();
    assert_eq!(entries, json!([{ "account": { "guid": checking, "fullName": "Assets:Checking" }, "kind": "csvAccountMap", "key": "Current",
        "target": { "guid": checking, "fullName": "Assets:Checking" }, "count": null, "valid": true }]));
}

#[test]
fn files_in_other_encodings_and_unreadable_files() {
    let mut b = Book::new();
    let latin1: Vec<u8> = b"Date,Description,Amount\n2024-03-01,Caf\xe9 Ol\xe9,-4.50\n".to_vec();
    let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    let preview = |b: &mut Book, file: Value| b.d.call("import.preview", json!({ "format": "csvTransactions", "files": [file], "layout": bank_layout() }));
    let p = preview(&mut b, json!({ "base64": encode(&latin1), "encoding": "iso-8859-1" })).unwrap();
    assert_eq!(p["transactions"][0]["description"], "Café Olé");
    let utf16: Vec<u8> = [0xff, 0xfe].into_iter().chain("Date,Description,Amount\n2024-03-01,Thé,-2.00\n".encode_utf16().flat_map(u16::to_le_bytes)).collect();
    let p = preview(&mut b, json!({ "base64": encode(&utf16), "encoding": "utf-16" })).unwrap();
    assert_eq!(p["transactions"][0]["description"], "Thé");

    let e = preview(&mut b, json!({ "base64": encode(&latin1) })).unwrap_err();
    assert_eq!(e.code, codes::IMPORT_FILE);
    assert_eq!(e.data.unwrap(), json!({ "file": 0, "line": 2, "reason": "encoding", "message": "bytes not valid in UTF-8 at line 2; is the file in another encoding?" }));
    assert_eq!(preview(&mut b, json!({ "text": "a", "base64": "YQ==" })).unwrap_err().code, codes::INVALID_PARAMS);
    assert_eq!(preview(&mut b, json!({ "text": "a", "encoding": "utf-8" })).unwrap_err().code, codes::INVALID_PARAMS);
    let e = b.err("import.preview", json!({ "format": "csvTransactions", "files": [{ "text": MARCH }], "layout": { "columns": ["Date", "Amount"], "baseAccount": "Assets:Checking" } }));
    assert_eq!((e.code, e.data.unwrap()["reason"].clone()), (codes::IMPORT_FILE, json!("columns")));
    assert_eq!(b.err("import.preview", json!({ "format": "csvTransactions", "files": [{ "text": MARCH }], "layout": "No Such Layout" })).code, codes::INVALID);
    let e = b.err("import.preview", json!({ "format": "csvTransactions", "files": [{ "text": MARCH }], "layout": { "columns": ["Date"], "baseAccount": "Nowhere" } }));
    assert_eq!(e.code, codes::NOT_FOUND);
}

#[test]
fn layouts_by_name_from_the_configuration() {
    let mut b = Book::new();
    b.ok("config.set", json!({ "changes": { "import.csvLayouts.MyBank": bank_layout() } }));
    let p = b.preview(MARCH, json!("MyBank"));
    assert_eq!(p["transactions"].as_array().unwrap().len(), 3);
    let e = b.err("config.set", json!({ "changes": { "import.csvLayouts.Bad": { "dateFormat": "y/m/d" } } }));
    assert_eq!(e.code, codes::INVALID_PARAMS);
    let e = b.err("config.set", json!({ "changes": { "import.csvLayouts.Bad": { "columns": ["Bogus"] } } }));
    assert_eq!(e.code, codes::INVALID_PARAMS);
}

/// `5.17:gnucash/import-export/csv-imp/test/sample1.csv`, as the CSV importer's tests read it.
#[test]
fn gnucash_sample1() {
    let mut b = Book::new();
    b.ok("account.create", json!({ "name": "Miscellaneous", "type": "EXPENSE", "commodity": "USD" }));
    let text = std::fs::read_to_string(format!("{}/../../fixtures/csv/sample1.csv", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let layout = json!({ "skipStartLines": 1, "dateFormat": "m-d-y", "currencyFormat": "period",
        "columns": ["Date", "Number", "Description", "Notes", "Account", "Deposit", "Withdrawal", "None"] });
    let p = b.ok("import.preview", json!({ "format": "csvTransactions", "files": [{ "name": "sample1.csv", "text": text }], "layout": layout }));
    let t = &p["transactions"][0];
    assert_eq!((t["datePosted"].clone(), t["num"].clone(), t["description"].clone()), (json!("2015-05-01"), json!("45"), json!("Acme Inc.")));
    assert_eq!(t["splits"][0]["accountName"], "Miscellaneous");
    assert_eq!(t["splits"][0]["amount"], usd("-110000"));
}

/// Multi-split lines in GnuCash's own export layout: lines with the same Transaction ID and date are one
/// transaction, which balances, so nothing is added to Imbalance.
#[test]
fn the_gnucash_export_layout_groups_lines() {
    let mut b = Book::new();
    let head = "Date,Transaction ID,Number,Description,Notes,Commodity/Currency,Void Reason,Action,Memo,Full Account Name,Account Name,Amount With Sym,Amount Num.,Value With Sym,Value Num.,Reconcile,Reconcile Date,Rate/Price\n";
    let lines = [
        "2024-03-01,aa01,,Corner Grocer,,CURRENCY::USD,,,,Assets:Checking,Checking,$-45.10,-45.10,$-45.10,-45.10,n,,1.00",
        ",,,,,,,,food,Expenses:Groceries,Groceries,$45.10,45.10,$45.10,45.10,n,,1.00",
        "2024-03-02,aa02,7,Rent,,CURRENCY::USD,,,,Assets:Checking,Checking,$-950.00,-950.00,$-950.00,-950.00,c,,1.00",
        "2024-03-02,aa02,7,Rent,,CURRENCY::USD,,,,Expenses:Rent,Rent,$950.00,950.00,$950.00,950.00,n,,1.00",
    ];
    let text = format!("{head}{}\n", lines.join("\n"));
    let p = b.preview(&text, json!("GnuCash Export Format"));
    assert_eq!(p["errors"], json!([]));
    let t = p["transactions"].as_array().unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(t[0]["splits"].as_array().unwrap().len(), 2);
    assert_eq!(t[0]["splits"][1]["memo"], "food");
    assert_eq!(t[1]["splits"][0]["reconcileState"], "c");
    assert_eq!(p["summary"]["imbalanced"], 0);
    assert!(t.iter().all(|t| t["other"].is_null()));
    b.ok("import.apply", json!({ "proposal": p["id"] }));
    assert_eq!(b.transactions().len(), 2);
}

#[test]
fn the_import_map_editor_lists_and_deletes() {
    let mut b = Book::new();
    let p = b.preview(MARCH, bank_layout());
    let changes = json!([{ "index": 0, "account": "Expenses:Groceries" }, { "index": 1, "account": "Expenses:Rent" }]);
    b.ok("import.apply", json!({ "proposal": p["id"], "changes": changes }));
    let entries = b.ok("importMap.list", json!({ "account": "Assets:Checking", "kind": "bayes" }))["entries"].clone();
    let keys: Vec<(&str, &str)> =
        entries.as_array().unwrap().iter().map(|e| (e["key"].as_str().unwrap(), e["target"]["fullName"].as_str().unwrap())).collect();
    assert_eq!(keys, [("Corner", "Expenses:Groceries"), ("Friday", "Expenses:Groceries"), ("Grocer", "Expenses:Groceries"), ("March", "Expenses:Rent"), ("Rent", "Expenses:Rent"), ("Saturday", "Expenses:Rent")]);
    assert!(entries.as_array().unwrap().iter().all(|e| e["count"] == 1 && e["valid"] == true));

    let deleted = b.ok("importMap.delete", json!({ "entries": [{ "account": "Assets:Checking", "kind": "bayes", "key": "Friday" }] }));
    assert_eq!(deleted["deleted"].as_array().unwrap().len(), 1);
    let e = b.err("importMap.delete", json!({ "entries": [{ "account": "Assets:Checking", "kind": "bayes", "key": "Friday" }] }));
    assert_eq!(e.code, codes::NOT_FOUND);
    assert_eq!(b.err("importMap.delete", json!({})).code, codes::INVALID);
    assert_eq!(b.err("importMap.delete", json!({ "entries": [], "invalid": true })).code, codes::INVALID);

    b.ok("account.delete", json!({ "account": "Expenses:Rent", "transactions": "delete" }));
    let invalid = b.ok("importMap.list", json!({ "invalidOnly": true }))["entries"].clone();
    assert_eq!(invalid.as_array().unwrap().len(), 3, "the entries for the deleted Rent account");
    assert!(invalid.as_array().unwrap().iter().all(|e| e["target"].is_null() && e["valid"] == false));
    assert_eq!(b.ok("importMap.delete", json!({ "invalid": true }))["deleted"].as_array().unwrap().len(), 3);
    let left = b.ok("importMap.delete", json!({ "entries": [{ "account": "Assets:Checking", "kind": "bayes" }] }))["deleted"].clone();
    assert_eq!(left.as_array().unwrap().len(), 2);
    assert!(b.ok("importMap.list", json!({}))["entries"].as_array().unwrap().is_empty());
}

#[test]
fn a_read_only_book_previews_but_does_not_apply() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash").display().to_string();
    {
        let mut b = Book::new();
        b.ok("book.close", json!({ "save": true }));
        std::fs::copy(b.dir.path().join("book.gnucash"), &path).unwrap();
    }
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(NOW), 1 << 100))));
    d.call("book.open", json!({ "path": path, "readOnly": true })).unwrap();
    let p = d.call("import.preview", json!({ "format": "csvTransactions", "files": [{ "text": MARCH }], "layout": bank_layout() })).unwrap();
    assert_eq!(d.call("import.apply", json!({ "proposal": p["id"] })).unwrap_err().code, codes::READ_ONLY);
}

/// What a transaction is, for comparing two books: date, number, description and each split's account, value and
/// memo, in account order.
fn shape(b: &mut Book) -> Vec<Value> {
    let names: Vec<(String, String)> = ["Assets:Checking", "Expenses:Groceries", "Expenses:Rent", "Income:Salary"].iter().map(|n| (b.guid(n), (*n).to_owned())).collect();
    let mut out: Vec<Value> = b
        .transactions()
        .iter()
        .map(|t| {
            let full = b.ok("transaction.get", json!({ "guid": t["guid"] }));
            let mut splits: Vec<Value> = full["splits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| {
                    let account = names.iter().find(|(g, _)| s["account"] == g.as_str()).map(|(_, n)| n.clone());
                    json!({ "account": account, "value": s["value"], "memo": s["memo"] })
                })
                .collect();
            splits.sort_by_key(|s| s.to_string());
            json!({ "date": full["datePosted"], "num": full["num"], "description": full["description"], "splits": splits })
        })
        .collect();
    out.sort_by_key(|t| t.to_string());
    out
}

/// A file exported as CSV imports back to the same transactions (`design/milestone3.md`, "Done when"), read with
/// GnuCash's export layout and the C locale's export date format.
#[test]
fn a_csv_export_imports_back_to_the_same_transactions() {
    let mut a = Book::new();
    let post = |b: &mut Book, date: &str, num: &str, desc: &str, splits: Value| {
        b.ok("transaction.post", json!({ "datePosted": date, "num": num, "description": desc, "splits": splits }));
    };
    post(&mut a, "2024-03-01", "", "Corner, Grocer", json!([{ "account": "Assets:Checking", "value": "-45.10" }, { "account": "Expenses:Groceries", "value": "45.10", "memo": "fruit \"fresh\"" }]));
    post(&mut a, "2024-03-02", "7", "Rent", json!([{ "account": "Assets:Checking", "value": "-950.00" }, { "account": "Expenses:Rent", "value": "950.00" }]));
    post(&mut a, "2024-03-05", "", "Salary and shop", json!([
        { "account": "Assets:Checking", "value": "1980.00" }, { "account": "Income:Salary", "value": "-2000.00" }, { "account": "Expenses:Groceries", "value": "20.00" }]));
    let csv = a.ok("export.csvTransactions", json!({ "accounts": ["Assets:Checking"] }))["csv"].as_str().unwrap().to_owned();
    let mut layout = serde_json::to_value(red_ale_import::csv::TxLayout::gnucash_export()).unwrap();
    layout["dateFormat"] = json!("m-d-y");
    let mut b = Book::new();
    let p = b.preview(&csv, layout);
    assert_eq!(p["errors"], json!([]), "{csv}");
    assert_eq!(p["summary"]["imbalanced"], 0);
    b.ok("import.apply", json!({ "proposal": p["id"] }));
    let expected = shape(&mut a);
    let text = serde_json::to_string(&expected).unwrap();
    assert!(text.contains("fruit \\\"fresh\\\"") && text.contains("\"num\":\"7\"") && text.contains("2024-03-05"), "{text}");
    assert_eq!(shape(&mut b), expected);
}

/// Single-line mode with a Transfer Account builds both splits (`gnc-imp-props-tx.cpp:801-845`); accounts in two
/// currencies need a price, value or transfer amount column (`gnc-import-tx.cpp:542-556`).
#[test]
fn transfer_accounts_and_the_multi_currency_checks() {
    let mut b = Book::new();
    let layout = json!({ "skipStartLines": 1, "columns": ["Date", "Description", "Account", "Amount", "Transfer Account", "Memo", "Transfer Memo"] });
    let file = "h\n2024-03-01,Shop,Assets:Checking,-12.00,Expenses:Groceries,card,basket\n";
    let p = b.preview(file, layout.clone());
    let t = &p["transactions"][0];
    assert_eq!(t["splits"].as_array().unwrap().len(), 2);
    assert_eq!((t["splits"][1]["accountName"].clone(), t["splits"][1]["value"].clone()), (json!("Expenses:Groceries"), usd("1200")));
    assert_eq!((t["splits"][0]["memo"].clone(), t["splits"][1]["memo"].clone()), (json!("card"), json!("basket")));
    assert!(t["other"].is_null() && t["imbalance"].is_null());

    b.ok("account.create", json!({ "name": "Euro", "type": "BANK", "commodity": "EUR", "parent": "Assets" }));
    // GnuCash counts only the Account column's currencies, so this is not multi-currency to it: the transfer split
    // has no amount, goes to the matcher, and is added at a rate of 1 with a warning.
    let euro = "h\n2024-03-01,Shop,Assets:Euro,-12.00,Expenses:Groceries,,\n";
    let p = b.preview(euro, layout);
    assert_eq!(p["transactions"][0]["other"]["from"], "file");
    assert_eq!(p["transactions"][0]["warnings"][0]["code"], "missing-rate");
    let multi = json!({ "skipStartLines": 1, "multiSplit": true, "columns": ["Date", "Description", "Account", "Amount"] });
    let file = "h\n2024-03-01,Shop,Assets:Euro,-12.00\n,,Expenses:Groceries,13.00\n";
    let e = b.err("import.preview", json!({ "format": "csvTransactions", "files": [{ "text": file }], "layout": multi }));
    assert_eq!((e.code, e.data.unwrap()["reason"].clone()), (codes::IMPORT_FILE, json!("columns")));
    let layout = json!({ "skipStartLines": 1, "columns": ["Date", "Description", "Account", "Amount", "Transfer Account", "Transfer Amount"] });
    let p = b.preview("h\n2024-03-01,Shop,Assets:Euro,-10.00,Expenses:Groceries,11.00\n", layout);
    let t = &p["transactions"][0];
    assert_eq!(t["currency"], "CURRENCY::EUR");
    assert_eq!((t["splits"][1]["value"].clone(), t["splits"][1]["amount"].clone()), (usd("1000"), usd("1100")));
}
