//! CSV account and price imports red-ale applied, checked against GnuCash 5.17: GnuCash opens the book with no
//! warnings and sees the same data (`oracle_check`), and ranks the imported prices by the source GnuCash's own price
//! import gives them, `user:price` (`design/api/import.md`, "CSV accounts", "CSV prices").

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{call, create, oracle, oracle_check};
use serde_json::{Value, json};

fn gnucash(script: &str, args: &[&str]) -> Value {
    oracle::check_version().unwrap();
    let out = oracle::run_script(&oracle::scripts_dir().join(script), args).unwrap_or_else(|e| panic!("{e}"));
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"))
}

fn acme(d: &mut red_ale_rpc::Dispatcher) {
    call(
        d,
        "commodity.create",
        &json!({ "namespace": "NASDAQ", "mnemonic": "ACME", "fullname": "Acme Corp", "fraction": 1000 }),
    );
}

fn accounts(d: &mut red_ale_rpc::Dispatcher, csv: &str) -> Value {
    let p = call(d, "import.preview", &json!({ "format": "csvAccounts", "files": [{ "text": csv }] }));
    assert_eq!(p["errors"], json!([]), "{p}");
    call(d, "import.apply", &json!({ "proposal": p["id"] }))
}

#[test]
fn gnucash_reads_an_account_tree_red_ale_imported() {
    let (_source, mut old) = create();
    acme(&mut old);
    [
        json!({ "name": "Assets", "type": "ASSET", "commodity": "USD", "placeholder": true, "code": "1000", "description": "What we own" }),
        json!({ "name": "Checking", "type": "BANK", "parent": "Assets", "commodity": "USD", "color": "#336699", "notes": "Main, \"daily\"\nline two" }),
        json!({ "name": "Brokerage", "type": "STOCK", "parent": "Assets", "commodity": "NASDAQ::ACME", "hidden": true }),
        json!({ "name": "Euro", "type": "CASH", "parent": "Assets", "commodity": "EUR" }),
        json!({ "name": "Expenses", "type": "EXPENSE", "commodity": "USD" }),
        json!({ "name": "Food; Drink", "type": "EXPENSE", "parent": "Expenses", "commodity": "USD", "code": "5100" }),
    ]
    .into_iter()
    .for_each(|a| {
        call(&mut old, "account.create", &a);
    });
    let csv = call(&mut old, "export.csvTree", &json!({}))["csv"].as_str().unwrap().to_owned();

    let (book, mut d) = create();
    acme(&mut d);
    let applied = accounts(&mut d, &csv);
    assert_eq!(applied["accounts"]["created"].as_array().unwrap().len(), 6);
    let update = "EXPENSE,Expenses:Food; Drink,Food; Drink,5200,Eating,\"rgb(10,20,30)\",Weekly,USD,CURRENCY,F,F,F\n";
    let applied = accounts(&mut d, update);
    assert_eq!(applied["accounts"]["updated"].as_array().unwrap().len(), 1, "{applied}");
    let food = call(&mut d, "account.get", &json!({ "account": "Expenses:Food; Drink" }));
    let fields = (&food["code"], &food["description"], &food["color"], &food["notes"]);
    assert_eq!(
        fields,
        (&json!("5200"), &json!("Eating"), &json!("rgb(10,20,30)"), &json!("Weekly")),
        "{food}"
    );

    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    assert_eq!(
        gnucash("opened.py", &[&book.path_str()]),
        json!({ "unknownFeatures": null, "logged": [] })
    );
}

/// Every price with its source string, whether GnuCash ranks it as `PRICE_SOURCE_USER_PRICE`, its type and value.
const SOURCES: &str = r#"
import json, sys
from gnucash import Session, SessionOpenMode, gnucash_core_c as c
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    t, pdb = s.book.get_table(), s.book.get_price_db()
    out = []
    for ns in t.get_namespaces_list():
        for cm in ns.get_commodity_list():
            for p in pdb.get_prices(cm, None):
                v = p.get_value()
                out.append([p.get_commodity().get_unique_name(), p.get_currency().get_unique_name(),
                            p.get_time64().strftime("%Y-%m-%dT%H:%M:%S"), p.get_source_string(),
                            c.gnc_price_get_source(p.instance) == c.PRICE_SOURCE_USER_PRICE, p.get_typestr(),
                            f"{v.num()}/{v.denom()}"])
    print(json.dumps(sorted(out)))
finally:
    s.end()
"#;

#[test]
fn gnucash_ranks_imported_prices_as_its_own_price_import() {
    let (book, mut d) = create();
    acme(&mut d);
    call(
        &mut d,
        "price.add",
        &json!({ "commodity": "NASDAQ::ACME", "currency": "USD", "date": "2024-03-01", "value": "10", "source": "Finance::Quote" }),
    );
    call(
        &mut d,
        "price.add",
        &json!({ "commodity": "USD", "currency": "EUR", "date": "2024-03-04", "value": "0.9", "source": "Finance::Quote" }),
    );
    let layout = json!({ "skipStartLines": 1, "columns": ["Date", "From Symbol", "From Namespace", "Currency To", "Amount"] });
    let text = "Date,Symbol,Namespace,Currency,Price\n2024-03-01,ACME,NASDAQ,USD,10.5\n2024-03-05,ACME,NASDAQ,USD,$10.123456789\n\
                2024-03-04,EUR,CURRENCY,USD,1.0857\n2024-03-06,EUR,CURRENCY,USD,1.09\n";
    let p = call(
        &mut d,
        "import.preview",
        &json!({ "format": "csvPrices", "files": [{ "text": text }], "layout": layout, "overwrite": true }),
    );
    let actions: Vec<&str> = p["prices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        ["update", "add", "update", "add"],
        "both same-day prices, the second the inverse pair, are replaced"
    );
    let applied = call(&mut d, "import.apply", &json!({ "proposal": p["id"] }));
    assert_eq!(applied["prices"]["replaced"].as_array().unwrap().len(), 2);

    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    assert_eq!(
        gnucash("opened.py", &[&book.path_str()]),
        json!({ "unknownFeatures": null, "logged": [] })
    );
    oracle::check_version().unwrap();
    let out = oracle::run_code(SOURCES, &[&book.path_str()]).unwrap_or_else(|e| panic!("{e}"));
    let prices: Value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"));
    assert_eq!(
        prices,
        json!([
            [
                "CURRENCY::EUR",
                "CURRENCY::USD",
                "2024-03-04T10:59:00",
                "user:price",
                true,
                "last",
                "1085700/1000000"
            ],
            [
                "CURRENCY::EUR",
                "CURRENCY::USD",
                "2024-03-06T10:59:00",
                "user:price",
                true,
                "last",
                "1090000/1000000"
            ],
            [
                "NASDAQ::ACME",
                "CURRENCY::USD",
                "2024-03-01T10:59:00",
                "user:price",
                true,
                "last",
                "10500000/1000000"
            ],
            [
                "NASDAQ::ACME",
                "CURRENCY::USD",
                "2024-03-05T10:59:00",
                "user:price",
                true,
                "last",
                "10123457/1000000"
            ],
        ]),
        "values at the currency's fraction × 10000, as GnuCash's price import stores them"
    );
}
