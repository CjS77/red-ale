//! CSV account and price import through `import.preview` and `import.apply` (`design/api/import.md`, "CSV accounts",
//! "CSV prices").

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

/// 2024-07-01 12:00 UTC.
const NOW: i64 = 1_719_835_200;

struct Book {
    _dir: tempfile::TempDir,
    d: Dispatcher,
}

impl Book {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut d = Dispatcher::new(Settings::default());
        d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(NOW), 1 << 100))));
        d.call("book.create", json!({ "path": dir.path().join("book.gnucash").display().to_string() })).unwrap();
        Self { _dir: dir, d }
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.d.call(method, params.clone()).unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        self.d.call(method, params).unwrap_err()
    }

    fn accounts(&mut self, text: &str) -> Value {
        self.ok("import.preview", json!({ "format": "csvAccounts", "files": [{ "name": "accounts.csv", "text": text }] }))
    }

    fn prices(&mut self, text: &str, layout: Value, overwrite: bool) -> Value {
        self.ok("import.preview", json!({ "format": "csvPrices", "files": [{ "text": text }], "layout": layout, "overwrite": overwrite }))
    }

    fn tree(&mut self) -> String {
        self.ok("export.csvTree", json!({}))["csv"].as_str().unwrap().to_owned()
    }

    fn acme(&mut self) {
        self.ok("commodity.create", json!({ "namespace": "NASDAQ", "mnemonic": "ACME", "fullname": "Acme Corp", "fraction": 1000 }));
    }
}

/// The record of the account with this full name.
fn row<'a>(p: &'a Value, full_name: &str) -> &'a Value {
    p["accounts"].as_array().unwrap().iter().find(|r| r["fullName"] == full_name).unwrap()
}

fn actions(p: &Value, list: &str) -> Vec<String> {
    p[list].as_array().unwrap().iter().map(|r| r["action"].as_str().unwrap().to_owned()).collect()
}

/// A book with a tree that uses every field the export writes.
fn rich_tree(b: &mut Book) {
    b.acme();
    let accounts = [
        json!({ "name": "Assets", "type": "ASSET", "commodity": "USD", "placeholder": true, "code": "1000", "description": "What we own" }),
        json!({ "name": "Checking", "type": "BANK", "parent": "Assets", "commodity": "USD", "color": "#336699", "notes": "Main, \"daily\" account\nline two" }),
        json!({ "name": "Brokerage", "type": "STOCK", "parent": "Assets", "commodity": "NASDAQ::ACME", "hidden": true }),
        json!({ "name": "Euro", "type": "CASH", "parent": "Assets", "commodity": "EUR", "taxRelated": true }),
        json!({ "name": "Expenses", "type": "EXPENSE", "commodity": "USD" }),
        json!({ "name": "Food; Drink", "type": "EXPENSE", "parent": "Expenses", "commodity": "USD", "code": "5100" }),
        json!({ "name": "Equity", "type": "EQUITY", "commodity": "USD" }),
    ];
    accounts.into_iter().for_each(|a| {
        b.ok("account.create", a);
    });
}

#[test]
fn an_account_export_recreates_the_tree_in_a_new_book() {
    let mut old = Book::new();
    rich_tree(&mut old);
    let csv = old.tree();
    let mut new = Book::new();
    new.acme();
    let p = new.accounts(&csv);
    assert_eq!(p["format"], "csvAccounts");
    assert_eq!(actions(&p, "accounts"), ["add"; 7]);
    assert_eq!(p["ignoredLines"], json!([{ "file": 0, "line": 1, "reason": "headerRows" }]));
    assert_eq!(p["summary"]["add"], 7);
    assert_eq!(p["newAccounts"].as_array().unwrap().len(), 7);
    assert!(p["newAccounts"].as_array().unwrap().contains(&json!({ "name": "Assets:Brokerage", "type": "STOCK", "commodity": "NASDAQ::ACME" })));
    assert_eq!(p["transactions"], json!([]));
    let checking = row(&p, "Assets:Checking");
    assert_eq!(checking["account"], Value::Null);
    assert_eq!(checking["fields"]["notes"], "Main, \"daily\" account\nline two");
    assert_eq!(checking["fields"]["color"], "rgb(51,102,153)");
    let applied = new.ok("import.apply", json!({ "proposal": p["id"] }));
    assert_eq!(applied["accounts"]["created"].as_array().unwrap().len(), 7);
    let mut expected = csv.replace("CASH,Assets:Euro,Euro,,,,,EUR,CURRENCY,F,T,F", "CASH,Assets:Euro,Euro,,,,,EUR,CURRENCY,F,F,F");
    expected = expected.replace("\r\n", "\n");
    assert_eq!(new.tree().replace("\r\n", "\n"), expected, "the same tree, but for Tax Info, which GnuCash's import never sets");

    let again = new.accounts(&csv);
    assert_eq!(actions(&again, "accounts"), ["skip", "skip", "skip", "skip", "skip", "skip", "skip"]);
    assert_eq!(again["summary"]["add"], 0, "importing the export again changes nothing");
    let euro = row(&again, "Assets:Euro");
    assert_eq!(euro["warnings"][0]["code"], "not-updated");
    assert_eq!(euro["warnings"][0]["index"], euro["index"]);
    let tree = new.tree();
    let applied = new.ok("import.apply", json!({ "proposal": again["id"] }));
    assert_eq!((&applied["accounts"]["created"], &applied["accounts"]["updated"]), (&json!([]), &json!([])));
    assert_eq!(new.tree(), tree, "applying the second import changed nothing");
}

#[test]
fn updates_and_errors() {
    let mut b = Book::new();
    rich_tree(&mut b);
    let text = "BANK,Assets:Checking,Checking,1010,,rgb(1,2,3),,USD,CURRENCY,F,F,F\n\
                EXPENSE,Expenses:Travel,Travel,,,,,USD,CURRENCY,F,F,F\n\
                EXPENSE,Nowhere:Travel,Travel,,,,,USD,CURRENCY,F,F,F\n";
    let p = b.accounts(text);
    assert_eq!(p["errors"][0], json!({ "file": 0, "line": 1, "column": null, "message": "14 field(s), not the 12 of GnuCash's account export" }));
    let text = text.replace("rgb(1,2,3)", "\"rgb(1,2,3)\"");
    let p = b.accounts(&text);
    assert_eq!(actions(&p, "accounts"), ["update", "add"]);
    assert_eq!(p["accounts"][0]["changes"], json!([
        { "field": "code", "from": null, "to": "1010" },
        { "field": "color", "from": "rgb(51,102,153)", "to": "rgb(1,2,3)" },
    ]));
    assert_eq!(p["errors"], json!([{ "file": 0, "line": 3, "column": null, "message": "path to account Travel not found" }]));
    let e = b.err("import.apply", json!({ "proposal": p["id"] }));
    assert_eq!(e.code, codes::INVALID, "errors need skipErrors");
    let applied = b.ok("import.apply", json!({ "proposal": p["id"], "skipErrors": true, "changes": [{ "index": 1, "action": "skip" }] }));
    assert_eq!(applied["skipped"], json!([1]));
    assert_eq!(applied["accounts"]["updated"].as_array().unwrap().len(), 1);
    let checking = b.ok("account.get", json!({ "account": "Assets:Checking" }));
    assert_eq!((checking["code"].clone(), checking["color"].clone()), (json!("1010"), json!("rgb(1,2,3)")));
    let semi = b.ok("import.preview", json!({ "format": "csvAccounts", "separator": "semicolon", "headerRows": 0,
        "files": [{ "text": "EXPENSE;Expenses:Travel;Travel;;;;;USD;CURRENCY;F;F;F\n" }] }));
    assert_eq!(actions(&semi, "accounts"), ["add"]);
    let e = b.err("import.preview", json!({ "format": "csvAccounts", "files": [{ "text": "\n\n" }] }));
    assert_eq!(e.data.unwrap()["reason"], "columns");
}

fn all_columns() -> Value {
    json!({ "skipStartLines": 1, "columns": ["Date", "From Symbol", "From Namespace", "Currency To", "Amount"] })
}

#[test]
fn prices_are_added_replaced_or_kept_as_gnucash_does() {
    let mut b = Book::new();
    b.acme();
    let old = b.ok("price.add", json!({ "commodity": "NASDAQ::ACME", "currency": "USD", "date": "2024-03-01", "value": "10", "source": "Finance::Quote" }));
    let text = "Date,Symbol,Namespace,Currency,Price\n2024-03-01,ACME,NASDAQ,USD,10.5\n2024-03-04,ACME,NASDAQ,USD,$10.123456789\n";
    let p = b.prices(text, all_columns(), false);
    assert_eq!(p["format"], "csvPrices");
    assert_eq!(actions(&p, "prices"), ["skip", "add"]);
    assert_eq!(p["prices"][0]["existing"], json!({ "price": old["price"]["guid"] }));
    let second = &p["prices"][1];
    assert_eq!(second["value"], json!({ "num": "10123457", "denom": "1000000" }), "USD's fraction × 10000, half up");
    assert_eq!(second["amount"], json!({ "num": "10123456789", "denom": "1000000000" }));
    assert_eq!(second["time"], "2024-03-04T10:59:00Z");
    assert_eq!(p["summary"]["add"], 1);
    assert_eq!(p["summary"]["skip"], 1);
    let applied = b.ok("import.apply", json!({ "proposal": p["id"], "changes": [{ "index": 0, "action": "update" }] }));
    assert_eq!(applied["prices"]["added"].as_array().unwrap().len(), 1);
    assert_eq!(applied["prices"]["replaced"].as_array().unwrap().len(), 1);
    let list = b.ok("price.list", json!({ "commodity": "NASDAQ::ACME" }));
    let found: Vec<(Value, Value, Value)> = list.as_array().unwrap().iter().map(|p| (p["date"].clone(), p["source"].clone(), p["type"].clone())).collect();
    assert_eq!(found, [(json!("2024-03-04"), json!("user:price"), json!("last")), (json!("2024-03-01"), json!("user:price"), json!("last"))]);

    let p = b.prices(text, all_columns(), true);
    assert_eq!(actions(&p, "prices"), ["update", "update"], "over-writing replaces the prices just imported");
    let p = b.prices(text, all_columns(), false);
    assert_eq!(actions(&p, "prices"), ["skip", "skip"], "a second import of the same file adds nothing");
    let applied = b.ok("import.apply", json!({ "proposal": p["id"] }));
    assert_eq!((&applied["prices"]["added"], &applied["prices"]["replaced"]), (&json!([]), &json!([])));
    assert_eq!(b.ok("price.list", json!({ "commodity": "NASDAQ::ACME" })), list, "applying it changed no price");
}

#[test]
fn price_layouts_come_from_the_configuration_and_errors_name_their_column() {
    let mut b = Book::new();
    b.acme();
    let layout = json!({ "fromCommodity": "NASDAQ::ACME", "toCurrency": "CURRENCY::EUR", "columns": ["Date", "Amount"],
                         "dateFormat": "d-m-y", "currencyFormat": "comma", "separators": ";" });
    b.ok("config.set", json!({ "changes": { "import.csvPriceLayouts.Acme in euros": layout } }));
    let p = b.prices("01/03/2024;12,5\n02/03/2024;\n31/02/2024;13\n", json!("Acme in euros"), false);
    assert_eq!(actions(&p, "prices"), ["add"]);
    assert_eq!(p["prices"][0]["currency"], "CURRENCY::EUR");
    assert_eq!(p["errors"], json!([
        { "file": 0, "line": 2, "column": 2, "message": "Amount: Column value can not be empty." },
        { "file": 0, "line": 3, "column": 1, "message": "Date: Day of month is not valid for year" },
    ]));
    let e = b.err("import.preview", json!({ "format": "csvPrices", "files": [{ "text": "x" }], "layout": "Nope" }));
    assert_eq!(e.code, codes::INVALID);
    let e = b.err("import.preview", json!({ "format": "csvPrices", "files": [{ "text": "x,y" }], "layout": { "columns": ["Date"] } }));
    assert_eq!(e.data.unwrap()["reason"], "columns");
    let e = b.err("import.preview", json!({ "format": "csvPrices", "files": [{ "text": "x,y" }], "layout": { "fromCommodity": "NYSE::NOPE", "columns": ["Date", "Amount"] } }));
    assert_eq!(e.code, codes::NOT_FOUND);
    let e = b.err("config.set", json!({ "changes": { "import.csvPriceLayouts.Bad": { "columns": ["Price"] } } }));
    assert_eq!(e.code, codes::INVALID_PARAMS, "{e}");
}

#[test]
fn price_dates_in_the_locale_format() {
    let mut b = Book::new();
    b.acme();
    b.ok("config.set", json!({ "changes": { "locale": "fr_FR" } }));
    let layout = json!({ "fromCommodity": "NASDAQ::ACME", "toCurrency": "EUR", "columns": ["Date", "Amount"], "dateFormat": "locale",
                         "currencyFormat": "locale", "separators": ";" });
    let p = b.prices("4 mars 2024;12,5\n5 mars 24;1\u{202f}012,25\n", layout, false);
    let found: Vec<(Value, Value)> = p["prices"].as_array().unwrap().iter().map(|p| (p["date"].clone(), p["amount"].clone())).collect();
    assert_eq!(found, [
        (json!("2024-03-04"), json!({ "num": "125", "denom": "10" })),
        (json!("2024-03-05"), json!({ "num": "101225", "denom": "100" })),
    ]);
}
