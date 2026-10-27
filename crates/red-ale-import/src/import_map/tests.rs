//! GnuCash's import map tests, rebuilt: `5.17:libgnucash/engine/test/gtest-import-map.cpp` (GPL-2.0-or-later,
//! Copyright 2015 John Ralls). Each test names the one it follows. GnuCash's tests also check the edit level and dirty
//! flag; red-ale has neither, and checks the Slots instead.

use std::sync::Arc;

use red_ale_core::book_options::features;
use red_ale_core::{Account, AccountType, Book, CommodityRef, FixedEnv, Guid, SlotValue, Time64};

use super::bayes::{account_probabilities, add_account_bayes, bayes_info, delete_all_bayes_maps, find_account_bayes};
use super::convert::{FLAT_BAYESIAN_FEATURE, check_import_map_data};
use super::*;

/// `ImapTest::SetUp` (`gtest-import-map.cpp:38-81`).
pub(crate) struct Fixture {
    pub book: Book,
    pub bank: Guid,
    pub sav: Guid,
    pub asset2: Guid,
    pub food: Guid,
    pub drink: Guid,
}

pub(crate) fn fixture() -> Fixture {
    let mut env = FixedEnv::new(Time64(1_700_000_000), 0);
    let mut book = Book::new_empty(&mut env);
    let usd = CommodityRef::currency("USD");
    let mut add = |name: &str, ty, parent: Guid| {
        let a = Account::new(red_ale_core::Env::new_guid(&mut env), name, ty, Some(usd.clone()), Some(parent));
        let g = a.guid;
        book.accounts.insert(g, Arc::new(a));
        g
    };
    let root = book.root;
    let asset1 = add("Asset", AccountType::Asset, root);
    let bank = add("Bank", AccountType::Bank, asset1);
    let asset2 = add("Asset-Bank", AccountType::Asset, root);
    let sav = add("Bank", AccountType::Bank, asset2);
    let expense = add("Expense", AccountType::Expense, root);
    let food = add("Food", AccountType::Expense, expense);
    let drink = add("Drink", AccountType::Expense, expense);
    Fixture {
        book,
        bank,
        sav,
        asset2,
        food,
        drink,
    }
}

fn slots(f: &Fixture) -> &red_ale_core::Frame {
    &f.book.account(f.bank).unwrap().slots
}

fn set(f: &mut Fixture, path: &[&str], value: SlotValue) {
    slots_mut(&mut f.book, f.bank).unwrap().set_in(path, value);
}

fn flat(token: &str, guid: Guid) -> String {
    format!("{IMAP_FRAME_BAYES}/{token}/{guid}")
}

fn toks(tokens: &[&str]) -> Vec<String> {
    tokens.iter().map(|t| (*t).to_owned()).collect()
}

/// `t_list1` … `t_list5`, each built by prepending (`gtest-import-map.cpp:224-233`).
fn lists() -> [Vec<String>; 5] {
    [
        toks(&["bar", "foo"]),
        toks(&["waldo", "baz"]),
        toks(&["pepper"]),
        toks(&["salt"]),
        toks(&["sausage", "pork"]),
    ]
}

/// `ImapPlainTest.FindAccount` (`gtest-import-map.cpp:111-125`).
#[test]
fn find_account_plain() {
    let mut f = fixture();
    let (food, drink) = (f.food, f.drink);
    set(&mut f, &[IMAP_FRAME, "foo", "bar"], SlotValue::Guid(food));
    set(&mut f, &[IMAP_FRAME, "baz", "waldo"], SlotValue::Guid(drink));
    set(&mut f, &[IMAP_FRAME, "pepper"], SlotValue::Guid(food));
    set(&mut f, &[IMAP_FRAME, "salt"], SlotValue::Guid(drink));
    assert_eq!(find_account(&f.book, f.bank, Some("foo"), "bar"), Some(food));
    assert_eq!(find_account(&f.book, f.bank, Some("baz"), "waldo"), Some(drink));
    assert_eq!(find_account(&f.book, f.bank, None, "pepper"), Some(food));
    assert_eq!(find_account(&f.book, f.bank, None, "salt"), Some(drink));
    // GnuCash's last case passes a NULL key; red-ale's keys are never null, and a missing one finds nothing.
    assert_eq!(find_account(&f.book, f.bank, Some("salt"), "pepper"), None);
}

/// `ImapPlainTest.AddAccount` (`gtest-import-map.cpp:127-158`).
#[test]
fn add_account_plain() {
    let mut f = fixture();
    let (bank, food, drink) = (f.bank, f.food, f.drink);
    assert!(add_account(&mut f.book, bank, Some("foo"), "bar", food).unwrap());
    assert!(add_account(&mut f.book, bank, Some("baz"), "waldo", drink).unwrap());
    assert!(add_account(&mut f.book, bank, None, "pepper", food).unwrap());
    assert!(add_account(&mut f.book, bank, None, "salt", drink).unwrap());
    let before = f.book.clone();
    assert!(
        !add_account(&mut f.book, bank, None, "", drink).unwrap(),
        "an empty key writes nothing"
    );
    assert!(
        add_account(&mut f.book, bank, Some("pork"), "sausage", Guid::from_bytes([9; 16])).is_err(),
        "no added account"
    );
    assert_eq!(f.book, before);
    let s = slots(&f);
    assert_eq!(s.get_in(&[IMAP_FRAME, "foo", "bar"]), Some(&SlotValue::Guid(food)));
    assert_eq!(s.get_in(&[IMAP_FRAME, "baz", "waldo"]), Some(&SlotValue::Guid(drink)));
    assert_eq!(s.get_in(&[IMAP_FRAME, "pepper"]), Some(&SlotValue::Guid(food)));
    assert_eq!(s.get_in(&[IMAP_FRAME, "salt"]), Some(&SlotValue::Guid(drink)));
    assert_eq!(s.get_in(&[IMAP_FRAME, "pork", "sausage"]), None);
}

/// `ImapPlainTest.DeleteAccount` (`gtest-import-map.cpp:160-196`).
#[test]
fn delete_account_plain() {
    let mut f = fixture();
    let (bank, food, drink) = (f.bank, f.food, f.drink);
    add_account(&mut f.book, bank, Some("foo"), "bar", food).unwrap();
    add_account(&mut f.book, bank, Some("foo"), "waldo", drink).unwrap();
    add_account(&mut f.book, bank, None, "pepper", food).unwrap();
    assert!(!delete_account(&mut f.book, bank, None, "nothing").unwrap());

    assert!(delete_account(&mut f.book, bank, Some("foo"), "waldo").unwrap());
    assert_eq!(find_account(&f.book, bank, Some("foo"), "bar"), Some(food));
    assert_eq!(find_account(&f.book, bank, Some("foo"), "waldo"), None);
    assert_eq!(slots(&f).get_in(&[IMAP_FRAME, "foo", "waldo"]), None);

    assert!(delete_account(&mut f.book, bank, Some("foo"), "bar").unwrap());
    assert_eq!(slots(&f).get_in(&[IMAP_FRAME, "foo"]), None, "the emptied category goes");

    assert!(delete_account(&mut f.book, bank, None, "pepper").unwrap());
    assert_eq!(slots(&f).get_in(&[IMAP_FRAME]), None, "the emptied frame goes");
}

/// `ImapBayesTest.FindAccountBayes` (`gtest-import-map.cpp:250-284`).
#[test]
fn find_account_bayes_counts() {
    let mut f = fixture();
    let (bank, food, drink) = (f.bank, f.food, f.drink);
    let s = slots_mut(&mut f.book, bank).unwrap();
    [
        ("foo", food),
        ("bar", food),
        ("baz", drink),
        ("waldo", drink),
        ("pepper", food),
        ("salt", drink),
    ]
    .into_iter()
    .for_each(|(t, g)| {
        s.insert_key(flat(t, g), SlotValue::Integer(42));
    });
    let [l1, l2, l3, l4, l5] = lists();
    assert_eq!(find_account_bayes(&f.book, bank, &l1), Some(food));
    assert_eq!(find_account_bayes(&f.book, bank, &l2), Some(drink));
    assert_eq!(find_account_bayes(&f.book, bank, &l3), Some(food));
    assert_eq!(find_account_bayes(&f.book, bank, &l4), Some(drink));
    assert_eq!(find_account_bayes(&f.book, bank, &l5), None);

    // Only entries whose token matches exactly count.
    let s = slots_mut(&mut f.book, bank).unwrap();
    s.insert_key(flat("pepperwaldo", drink), SlotValue::Integer(42));
    assert_eq!(find_account_bayes(&f.book, bank, &l3), Some(food));
    let s = slots_mut(&mut f.book, bank).unwrap();
    s.insert_key(flat("pepper/waldo", drink), SlotValue::Integer(42));
    assert_eq!(find_account_bayes(&f.book, bank, &l3), Some(food));
}

/// `ImapBayesTest.AddAccountBayes` (`gtest-import-map.cpp:286-331`).
#[test]
fn add_account_bayes_counts() {
    let mut f = fixture();
    let (bank, food, drink) = (f.bank, f.food, f.drink);
    let [l1, l2, l3, l4, l5] = lists();
    add_account_bayes(&mut f.book, bank, &l1, food).unwrap();
    add_account_bayes(&mut f.book, bank, &l2, drink).unwrap();
    add_account_bayes(&mut f.book, bank, &l3, food).unwrap();
    add_account_bayes(&mut f.book, bank, &l4, drink).unwrap();
    let before = f.book.clone();
    assert!(add_account_bayes(&mut f.book, bank, &l5, Guid::from_bytes([9; 16])).is_err());
    assert_eq!(f.book, before);

    let count = |f: &Fixture, t: &str, g: Guid| slots(f).get_key(&flat(t, g)).and_then(SlotValue::as_int);
    assert_eq!(slots(&f).get_key(&format!("{IMAP_FRAME_BAYES}/foo/bar")), None);
    [
        ("foo", food),
        ("bar", food),
        ("baz", drink),
        ("waldo", drink),
        ("pepper", food),
        ("salt", drink),
    ]
    .into_iter()
    .for_each(|(t, g)| assert_eq!(count(&f, t, g), Some(1), "{t}"));
    assert_eq!(count(&f, "baz", food), None);

    add_account_bayes(&mut f.book, bank, &l2, drink).unwrap();
    assert_eq!(count(&f, "baz", drink), Some(2));
    assert_eq!(
        features(&f.book).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        [FLAT_BAYESIAN_FEATURE]
    );
}

/// `ImapBayesTest.ConvertBayesData` (`gtest-import-map.cpp:333-381`).
#[test]
fn convert_bayes_data() {
    let mut f = fixture();
    let (bank, food, drink, asset2, sav) = (f.bank, f.food, f.drink, f.asset2, f.sav);
    let food_text = food.to_string();
    let old: [(&[&str], i64); 7] = [
        (&[IMAP_FRAME_BAYES, "severely", "divided", "token", "Asset-Bank"], 10),
        (&[IMAP_FRAME_BAYES, "salt", "Asset-Bank#Bank"], 10),
        (&[IMAP_FRAME_BAYES, "salt", "Asset>Bank#Bank"], 5),
        (&[IMAP_FRAME_BAYES, "pork", "Expense#Food"], 5),
        (&[IMAP_FRAME_BAYES, "sausage", "Expense#Drink"], 2),
        (&[IMAP_FRAME_BAYES, "foo", "Expense#Food"], 5),
        (&[IMAP_FRAME_BAYES, "salt", &food_text], 10),
    ];
    old.into_iter().for_each(|(path, n)| set(&mut f, path, SlotValue::Integer(n)));
    let [_, _, _, _, l5] = lists();
    let read_only_view = find_account_bayes(&f.book, bank, &toks(&["sausage"]));
    assert_eq!(read_only_view, Some(drink), "a read sees the old data converted");
    assert!(slots(&f).get_key(IMAP_FRAME_BAYES).is_some(), "and changes nothing");

    add_account_bayes(&mut f.book, bank, &l5, drink).unwrap();
    let count = |f: &Fixture, key: &str| slots(f).get_key(key).and_then(SlotValue::as_int);
    assert_eq!(count(&f, &format!("{IMAP_FRAME_BAYES}/severely/divided/token/{asset2}")), Some(10));
    assert_eq!(count(&f, &flat("salt", sav)), Some(10), "Asset-Bank#Bank: separator # guessed");
    assert_eq!(count(&f, &flat("pork", food)), Some(5));
    assert_eq!(count(&f, &flat("sausage", drink)), Some(3), "2, then one more");
    assert_eq!(count(&f, &flat("foo", food)), Some(5));
    assert_eq!(count(&f, &flat("salt", food)), Some(10), "a GUID is kept");
    assert_eq!(
        count(&f, &flat("salt", Guid::NULL)),
        Some(5),
        "an unresolved name becomes the all-zero GUID"
    );
    assert!(slots(&f).get_key(IMAP_FRAME_BAYES).is_none(), "the nested frame is gone");
    assert!(features(&f.book).iter().any(|(n, _)| n == FLAT_BAYESIAN_FEATURE));
    assert!(!check_import_map_data(&mut f.book).unwrap(), "a flat book is not converted again");
}

/// `ImapBayesTest.import_map_with_delimiters` (`gtest-import-map.cpp:384-395`).
#[test]
fn import_map_with_delimiters() {
    let mut f = fixture();
    let (bank, food) = (f.bank, f.food);
    let tokens = toks(&["one/two/three"]);
    (0..3).for_each(|_| add_account_bayes(&mut f.book, bank, &tokens, food).unwrap());
    assert_eq!(find_account_bayes(&f.book, bank, &tokens), Some(food));
}

/// `ImapBayesTest.get_bayes_info` (`gtest-import-map.cpp:397-418`).
#[test]
fn get_bayes_info() {
    let mut f = fixture();
    let (bank, food) = (f.bank, f.food);
    let tokens = toks(&["one/two/three"]);
    add_account_bayes(&mut f.book, bank, &tokens, food).unwrap();
    assert_eq!(find_account_bayes(&f.book, bank, &tokens), Some(food));
    let infos = bayes_info(&f.book, bank);
    assert_eq!(infos.len(), 1);
    let info = &infos[0];
    assert_eq!(info.source_account, bank);
    assert_eq!(info.map_account, Some(food));
    assert_eq!(info.head, format!("{IMAP_FRAME_BAYES}/one/two/three/{food}"));
    assert_eq!(info.match_string, "one/two/three");
    assert_eq!(info.count, Some(1));
}

/// Hand-worked: two accounts share a token; the probability is computed as GnuCash computes it, truncated, and the
/// threshold is inclusive.
#[test]
fn probabilities_combine_shared_tokens_and_truncate() {
    let mut f = fixture();
    let (bank, food, drink) = (f.bank, f.food, f.drink);
    let s = slots_mut(&mut f.book, bank).unwrap();
    // "shop": food 9, drink 1. "Monday": food 1, drink 1.
    s.insert_key(flat("shop", food), SlotValue::Integer(9));
    s.insert_key(flat("shop", drink), SlotValue::Integer(1));
    s.insert_key(flat("Monday", food), SlotValue::Integer(1));
    s.insert_key(flat("Monday", drink), SlotValue::Integer(1));
    let probs = account_probabilities(&f.book, bank, &toks(&["shop"]));
    // Food: 0.9 / (0.9 + 0.1) × 100000 = 90000 (0.9 + 0.1 is exactly 1.0 in doubles).
    let food_first = if food < drink { [food, drink] } else { [drink, food] };
    assert_eq!(
        probs.iter().map(|(g, _)| g.clone()).collect::<Vec<_>>(),
        food_first.map(|g| g.to_string()),
        "accounts in key order"
    );
    assert!(probs.contains(&(food.to_string(), 90_000)));
    assert!(probs.contains(&(drink.to_string(), 10_000)));
    assert_eq!(
        find_account_bayes(&f.book, bank, &toks(&["shop"])),
        Some(food),
        "90000 reaches the threshold"
    );
    // With Monday: food 0.9 × 0.5 / (0.45 + 0.05) = 0.9; the same.
    assert_eq!(find_account_bayes(&f.book, bank, &toks(&["shop", "Monday"])), Some(food));
    // Monday alone: 50000 each; a tie goes to the first, but it misses the threshold.
    assert_eq!(find_account_bayes(&f.book, bank, &toks(&["Monday"])), None);
    // A token counted twice multiplies twice: 0.9² / (0.81 + 0.01) = 0.98780…, truncated.
    let twice = account_probabilities(&f.book, bank, &toks(&["shop", "shop"]));
    assert!(twice.contains(&(food.to_string(), 98_780)));
}

/// Hand-worked: a GUID naming no account finds none, and the plain lookup ignores non-GUID values.
#[test]
fn unknown_targets_find_nothing() {
    let mut f = fixture();
    let bank = f.bank;
    let ghost = Guid::from_bytes([7; 16]);
    let s = slots_mut(&mut f.book, bank).unwrap();
    s.insert_key(flat("x", ghost), SlotValue::Integer(5));
    s.set_in(&[IMAP_FRAME, CATEGORY_DESC, "y"], SlotValue::String("not a guid".into()));
    assert_eq!(find_account_bayes(&f.book, bank, &toks(&["x"])), None);
    assert_eq!(find_account(&f.book, bank, Some(CATEGORY_DESC), "y"), None);
    assert!(plain_info(&f.book, bank, Some(CATEGORY_DESC)).is_empty());
    assert_eq!(bayes_info(&f.book, bank)[0].map_account, None);
}

/// `gnc_account_imap_find_any` over the sorted tree, and the listings and deletions the import map editor uses.
#[test]
fn find_any_listing_and_deleting() {
    let mut f = fixture();
    let (bank, food, drink) = (f.bank, f.food, f.drink);
    add_account(&mut f.book, drink, Some(CATEGORY_CSV_ACCOUNT_MAP), "Pub", drink).unwrap();
    add_account(&mut f.book, food, Some(CATEGORY_CSV_ACCOUNT_MAP), "Pub", food).unwrap();
    assert_eq!(
        find_any(&f.book, Some(CATEGORY_CSV_ACCOUNT_MAP), "Pub"),
        Some(drink),
        "Drink sorts before Food"
    );
    add_account(&mut f.book, bank, Some(CATEGORY_DESC), "b", food).unwrap();
    add_account(&mut f.book, bank, Some(CATEGORY_DESC), "a", drink).unwrap();
    let listed: Vec<(String, Option<Guid>)> = plain_info(&f.book, bank, Some(CATEGORY_DESC))
        .into_iter()
        .map(|i| (i.match_string, i.map_account))
        .collect();
    assert_eq!(listed, [("a".to_owned(), Some(drink)), ("b".to_owned(), Some(food))]);
    assert!(delete_map_entry(&mut f.book, bank, IMAP_FRAME, Some(CATEGORY_DESC), Some("a"), false).unwrap());
    assert!(delete_map_entry(&mut f.book, bank, IMAP_FRAME, Some(CATEGORY_DESC), None, true).unwrap());
    assert!(
        slots(&f).get_in(&[IMAP_FRAME, CATEGORY_DESC]).is_some(),
        "a category still holding entries is kept"
    );

    add_account_bayes(&mut f.book, bank, &toks(&["p", "q"]), food).unwrap();
    let head = flat("p", food);
    assert!(delete_map_entry(&mut f.book, bank, &head, None, None, false).unwrap());
    assert_eq!(bayes_info(&f.book, bank).len(), 1);
    assert_eq!(delete_all_bayes_maps(&mut f.book, bank).unwrap(), 1);
    assert!(bayes_info(&f.book, bank).is_empty());
    assert!(slots(&f).get_in(&[IMAP_FRAME, CATEGORY_DESC, "b"]).is_some(), "plain entries stay");
}
