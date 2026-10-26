//! Import maps red-ale learns, checked against GnuCash 5.17: red-ale imports transactions into a book, learning
//! destinations into the Bayesian and plain Import maps (and converting pre-2.7 Bayesian data on the way), saves it,
//! and GnuCash's own `gnc_account_imap_find_account_bayes` and `gnc_account_imap_find_account` (`oracle/import_map.py`)
//! find the same accounts red-ale finds. The book declares only the flat Bayesian feature, and GnuCash opens it
//! without complaint.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::{Book, CommodityRef, Date, Guid, SlotValue, Split, Time64, Transaction};
use red_ale_import::import_map::bayes::find_account_bayes;
use red_ale_import::import_map::{CATEGORY_DESC, find_account};
use red_ale_import::matcher::{ApplyOptions, ImportAction, ImportSettings, ImportedTransaction, apply_batch, match_batch};
use red_ale_locale::Locale;
use red_ale_service::{Session, Settings};
use red_ale_testkit::{TempBook, call, create, oracle};
use serde_json::{Value, json};

fn gnucash(script: &str, args: &[&str]) -> Value {
    oracle::check_version().unwrap();
    let out = oracle::run_script(&oracle::scripts_dir().join(script), args).unwrap_or_else(|e| panic!("{e}"));
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"))
}

fn guid_of(book: &Book, name: &str) -> Guid {
    book.account_by_full_name(name).unwrap().guid
}

/// An imported transaction: one split in `account`, with a memo.
fn imported(n: u8, account: Guid, day: u32, description: &str, memo: &str) -> Transaction {
    let date = Date::from_ymd(2024, 6, day).unwrap();
    let mut split = Split::new(Guid::from_bytes([n; 16]), account, red_ale_core::Numeric::new(-1000 - i64::from(n), 100), red_ale_core::Numeric::new(-1000 - i64::from(n), 100));
    split.memo = Some(memo.to_owned()).filter(|m| !m.is_empty());
    let mut slots = red_ale_core::Frame::new();
    slots.insert_key("date-posted", SlotValue::Date(date));
    Transaction {
        guid: Guid::from_bytes([n.wrapping_add(128); 16]),
        currency: CommodityRef::currency("USD"),
        num: None,
        date_posted: date.neutral_time(),
        date_entered: Time64(1_719_835_200),
        description: Some(description.to_owned()),
        slots,
        splits: vec![split],
    }
}

/// Import `rows` (account, day, description, memo, destination) adding each with its destination chosen by hand, so
/// that red-ale learns each one. Returns the tokens of each.
fn import(session: &mut Session, rows: &[(&str, u32, &str, &str, &str)], use_bayes: bool, seed: u8) -> Vec<(Guid, Vec<String>)> {
    session
        .mutate(|book, env| {
            let settings = ImportSettings { use_bayes, ..ImportSettings::default() };
            let txns = rows
                .iter()
                .zip(seed..)
                .map(|((acc, day, desc, memo, _), n)| ImportedTransaction { transaction: imported(n, guid_of(book, acc), *day, desc, memo), last_split: None })
                .collect();
            let mut batch = match_batch(book, &settings, txns, &Locale::c())?;
            batch.infos.iter_mut().zip(rows).for_each(|(info, row)| {
                info.set_destination(book, Some(guid_of(book, row.4)), true);
                info.set_action(ImportAction::Add);
            });
            apply_batch(book, env, &batch.infos, ApplyOptions { use_bayes, append_text: false })?;
            Ok(batch.infos.iter().map(|i| (i.account().unwrap(), i.tokens.clone())).collect())
        })
        .unwrap()
}

#[test]
fn gnucash_finds_the_accounts_red_ale_learned() {
    let (book, mut d) = create();
    [("Bank", "BANK"), ("Card", "CREDIT"), ("Food", "EXPENSE"), ("Drink", "EXPENSE"), ("Rent", "EXPENSE")].iter().for_each(|(name, ty)| {
        call(&mut d, "account.create", &json!({ "name": name, "type": ty, "commodity": "USD" }));
    });
    call(&mut d, "book.close", &json!({ "save": true }));

    let mut session = Session::open(&book.path_str(), false, Settings::default()).unwrap();
    // Pre-2.7 data, nested and by full name: red-ale converts it at its first Bayesian write.
    session
        .mutate(|b, _| {
            let bank = guid_of(b, "Bank");
            let acc = std::sync::Arc::make_mut(b.accounts.get_mut(&bank).unwrap());
            acc.slots.set_in(&["import-map-bayes", "latte", "Drink"], SlotValue::Integer(5));
            Ok(())
        })
        .unwrap();
    let grocer: Vec<(&str, u32, &str, &str, &str)> = (1..=9).map(|day| ("Bank", day, "Corner Grocer", "", "Food")).collect();
    let mut rows = grocer;
    rows.extend([
        ("Bank", 10, "Corner Pub", "card 77", "Drink"),
        ("Bank", 11, "Monday Club Monday", "", "Drink"),
        ("Bank", 12, "Landlord one/two", "rent/june", "Rent"),
    ]);
    let mut learned = import(&mut session, &rows, true, 1);
    let plain = import(&mut session, &[("Card", 3, "Coffee Cart", "espresso", "Drink")], false, 50);
    session.save(None).unwrap();
    session.close(false).unwrap();

    let ours = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap().0;
    let bank = guid_of(&ours, "Bank");
    let tokens = |t: &[&str]| t.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    learned.extend([
        (bank, tokens(&["Corner"])),
        (bank, tokens(&["Corner", "Pub"])),
        (bank, tokens(&["latte"])),
        (bank, tokens(&["Monday"])),
        (bank, tokens(&["one/two", "rent/june"])),
        (bank, tokens(&["nothing"])),
    ]);
    let red_ale: Vec<Option<String>> = learned.iter().map(|(s, t)| find_account_bayes(&ours, *s, t).map(|g| g.to_string())).collect();
    let card = plain[0].0;
    let red_ale_plain = find_account(&ours, card, Some(CATEGORY_DESC), "Coffee Cart").map(|g| g.to_string());

    let queries = json!({
        "bayes": learned.iter().map(|(s, t)| json!({ "source": s.to_string(), "tokens": t })).collect::<Vec<_>>(),
        "plain": [{ "source": card.to_string(), "category": CATEGORY_DESC, "key": "Coffee Cart" }],
    });
    let queries_file = TempBook::new_path();
    std::fs::write(&queries_file.path, queries.to_string()).unwrap();
    let theirs = gnucash("import_map.py", &[&book.path_str(), &queries_file.path_str()]);

    assert_eq!(theirs["bayes"], json!(red_ale), "GnuCash's Bayesian lookups agree with red-ale's");
    assert_eq!(theirs["plain"], json!([red_ale_plain]));
    let name = |g: &Option<String>| g.as_ref().map(|g| ours.full_name(Guid::parse(g).unwrap()).unwrap());
    let found: Vec<Option<String>> = red_ale.iter().map(name).collect();
    let n = found.len();
    assert_eq!(found[0].as_deref(), Some("Food"), "a grocer transaction");
    assert_eq!(found[n - 6].as_deref(), Some("Food"), "Corner: Food 9 of 10, exactly 90000");
    assert_eq!(found[n - 4].as_deref(), Some("Drink"), "the converted pre-2.7 entry");
    assert_eq!(found[n - 2].as_deref(), Some("Rent"), "tokens holding '/'");
    assert_eq!(found[n - 1], None);
    assert_eq!(name(&red_ale_plain).as_deref(), Some("Drink"));
    assert_eq!(theirs["features"], json!(["Account GUID based bayesian with flat KVP"]));
    assert_eq!(gnucash("opened.py", &[&book.path_str()]), json!({ "unknownFeatures": null, "logged": [] }));
}
