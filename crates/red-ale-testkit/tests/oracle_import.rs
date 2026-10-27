//! A CSV import red-ale applied, checked against GnuCash 5.17: GnuCash opens the book with no warnings and sees the
//! same data (`oracle_check`), its own `gnc_account_imap_find_account_bayes` finds the accounts the import learned,
//! and the book declares only the feature GnuCash would have set (`design/api/import.md`, "Tests").

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::Guid;
use red_ale_import::import_map::bayes::find_account_bayes;
use red_ale_testkit::{TempBook, call, create, oracle, oracle_check};
use serde_json::{Value, json};

fn gnucash(script: &str, args: &[&str]) -> Value {
    oracle::check_version().unwrap();
    let out = oracle::run_script(&oracle::scripts_dir().join(script), args).unwrap_or_else(|e| panic!("{e}"));
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"))
}

fn preview(d: &mut red_ale_rpc::Dispatcher, text: &str, layout: &Value, names: &Value) -> Value {
    call(
        d,
        "import.preview",
        &json!({ "format": "csvTransactions", "files": [{ "text": text }], "layout": layout, "accountNames": names }),
    )
}

#[test]
fn gnucash_reads_an_imported_book_and_finds_what_it_learned() {
    let (book, mut d) = create();
    [
        ("Checking", "BANK"),
        ("Groceries", "EXPENSE"),
        ("Rent", "EXPENSE"),
        ("Salary", "INCOME"),
    ]
    .iter()
    .for_each(|(name, ty)| {
        call(&mut d, "account.create", &json!({ "name": name, "type": ty, "commodity": "USD" }));
    });
    call(
        &mut d,
        "transaction.post",
        &json!({ "description": "Cash machine", "datePosted": "2024-03-04", "splits": [
        { "account": "Checking", "value": "-100.00" }, { "account": "Groceries", "value": "100.00" }] }),
    );
    let layout = json!({ "skipStartLines": 1, "columns": ["Date", "Description", "Account", "Amount", "Void Reason"] });
    let march = "Date,Description,Account,Amount,Void\n2024-03-01,Corner Grocer,Bank,-45.10,\n2024-03-02,Rent March,Bank,-950.00,\n\
                 2024-03-04,Cash machine,Bank,-101.50,\n2024-03-05,Salary ACME,Bank,2000.00,\n2024-03-06,Mistake,Bank,-1.00,typo\n";
    let p = preview(&mut d, march, &layout, &json!({ "Bank": "Checking" }));
    assert_eq!(p["transactions"][2]["action"], "update");
    let changes =
        json!([{ "index": 0, "account": "Groceries" }, { "index": 1, "account": "Rent" }, { "index": 4, "account": "Groceries" }]);
    let applied = call(&mut d, "import.apply", &json!({ "proposal": p["id"], "changes": changes }));
    assert_eq!(
        applied["imbalanceSplits"].as_array().unwrap().len(),
        1,
        "the salary went to Imbalance-USD"
    );
    assert_eq!(applied["learned"]["csvAccountMap"].as_array().unwrap().len(), 1);
    let again = preview(&mut d, march, &layout, &json!({}));
    assert_eq!(again["summary"]["add"], 0, "importing the same file again adds nothing");
    call(&mut d, "import.apply", &json!({ "proposal": again["id"] }));

    let learned: Vec<(Guid, Vec<String>)> = applied["learned"]["bayes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                Guid::parse(l["account"].as_str().unwrap()).unwrap(),
                serde_json::from_value(l["tokens"].clone()).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        learned.len(),
        4,
        "the three destinations chosen, and the update's matched transaction's other account"
    );
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));

    let ours = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap().0;
    let source = learned[0].0;
    let mut queries: Vec<(Guid, Vec<String>)> = learned.clone();
    queries.extend([
        (source, vec!["Corner".to_owned()]),
        (source, vec!["Rent".to_owned(), "April".to_owned()]),
        (source, vec!["Bookshop".to_owned()]),
    ]);
    let red_ale: Vec<Option<String>> = queries
        .iter()
        .map(|(s, t)| find_account_bayes(&ours, *s, t).map(|g| g.to_string()))
        .collect();
    let file = TempBook::new_path();
    let body =
        json!({ "bayes": queries.iter().map(|(s, t)| json!({ "source": s.to_string(), "tokens": t })).collect::<Vec<_>>(), "plain": [] });
    std::fs::write(&file.path, body.to_string()).unwrap();
    let theirs = gnucash("import_map.py", &[&book.path_str(), &file.path_str()]);
    assert_eq!(theirs["bayes"], json!(red_ale), "GnuCash's Bayesian lookups agree with red-ale's");
    let name = |g: &Option<String>| g.as_ref().and_then(|g| ours.full_name(Guid::parse(g).unwrap()));
    let found: Vec<Option<String>> = red_ale.iter().map(name).collect();
    assert_eq!(found[4].as_deref(), Some("Groceries"));
    assert_eq!(found[5].as_deref(), Some("Rent"));
    assert_eq!(found[6], None);
    assert_eq!(theirs["features"], json!(["Account GUID based bayesian with flat KVP"]));
    assert_eq!(
        gnucash("opened.py", &[&book.path_str()]),
        json!({ "unknownFeatures": null, "logged": [] })
    );
}
