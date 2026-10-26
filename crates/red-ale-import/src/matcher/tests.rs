//! Tests of the matcher. Those rebuilt from GnuCash's `5.17:gnucash/import-export/test/gtest-import-backend.cpp`
//! (GPL-2.0-or-later, Copyright (c) 2020 Christian Gruber) say so; the rest are hand-worked from the lines they cite.

#![allow(clippy::many_single_char_names)] // f(ixture), t(ransaction), e(xisting), s(plit), b(atch), p(rocessed).

use std::sync::Arc;

use proptest::prelude::*;
use red_ale_core::ops::commit::{NewSplit, NewTransaction};
use red_ale_core::ops::scrub::Repair;
use red_ale_core::ops::transactions::post;
use red_ale_core::transaction::slot_paths::{NOTES, ONLINE_ID};
use red_ale_core::{Account, AccountType, Book, CommodityRef, Date, Env, FixedEnv, Guid, Numeric, ReconcileState, SlotValue, Split, Time64, Transaction};
use red_ale_locale::Locale;

use super::batch::resolve_conflicts;
use super::*;
use crate::import_map::bayes::find_account_bayes;
use crate::import_map::{CATEGORY_DESC, CATEGORY_MEMO, add_account, find_account};

struct Fx {
    book: Book,
    env: FixedEnv,
    bank: Guid,
    card: Guid,
    food: Guid,
    drink: Guid,
    eur_cash: Guid,
}

fn usd(cents: i64) -> Numeric {
    Numeric::new(cents, 100)
}

fn day(d: u32) -> Date {
    Date::from_ymd(2024, 3, d).unwrap()
}

fn fx() -> Fx {
    let mut env = FixedEnv::new(Time64(1_711_000_000), 0);
    let mut book = Book::new_empty(&mut env);
    let root = book.root;
    let mut add = |name: &str, ty, commodity: &str| {
        let a = Account::new(env.new_guid(), name, ty, Some(CommodityRef::currency(commodity)), Some(root));
        let g = a.guid;
        book.accounts.insert(g, Arc::new(a));
        g
    };
    let bank = add("Bank", AccountType::Bank, "USD");
    let card = add("Card", AccountType::Credit, "USD");
    let food = add("Food", AccountType::Expense, "USD");
    let drink = add("Drink", AccountType::Expense, "USD");
    let eur_cash = add("Euro", AccountType::Currency, "EUR");
    Fx { book, env, bank, card, food, drink, eur_cash }
}

impl Fx {
    /// An existing transaction: `amount` in `account` against `other`.
    fn existing(&mut self, d: u32, account: Guid, other: Guid, amount: Numeric, desc: &str, num: Option<&str>) -> Guid {
        let mut new = NewTransaction::new(CommodityRef::currency("USD"), day(d));
        new.description = Some(desc.to_owned());
        new.num = num.map(str::to_owned);
        new.splits = vec![NewSplit::new(account, amount), NewSplit::new(other, amount.neg())];
        post(&mut self.book, &mut self.env, new).unwrap()
    }

    fn split_in(&self, t: Guid, account: Guid) -> Guid {
        self.book.transaction(t).unwrap().splits.iter().find(|s| s.account == Some(account)).unwrap().guid
    }

    /// An imported transaction: one split of `amount` in `account`.
    fn imported(&mut self, d: u32, account: Guid, amount: Numeric, desc: &str) -> Transaction {
        let date = day(d);
        let mut split = Split::new(self.env.new_guid(), account, amount, amount);
        split.memo = None;
        let mut slots = red_ale_core::Frame::new();
        slots.insert_key("date-posted", SlotValue::Date(date));
        Transaction {
            guid: self.env.new_guid(),
            currency: CommodityRef::currency("USD"),
            num: None,
            date_posted: date.neutral_time(),
            date_entered: Time64(1_711_000_000),
            description: Some(desc.to_owned()),
            slots,
            splits: vec![split],
        }
    }

    fn info(&mut self, t: Transaction, settings: &ImportSettings) -> TransInfo {
        TransInfo::new(&self.book, settings, 0, t, None, &Locale::c()).unwrap()
    }

    fn batch(&mut self, ts: Vec<Transaction>, settings: &ImportSettings) -> MatchBatch {
        let imported = ts.into_iter().map(|transaction| ImportedTransaction { transaction, last_split: None }).collect();
        match_batch(&self.book, settings, imported, &Locale::c()).unwrap()
    }
}

fn plain() -> ImportSettings {
    ImportSettings { use_bayes: false, ..ImportSettings::default() }
}

fn assert_balanced(book: &Book) {
    book.transactions.values().for_each(|t| assert!(t.is_balanced(), "{} unbalanced: {:?}", t.guid, t.imbalance()));
}

// ---------------------------------------------------------------------------------------------------------------
// gtest-import-backend.cpp

/// `ImportBackendTest.CreateTransInfo` (`gtest-import-backend.cpp:203-238`): without Bayesian matching, the destination
/// is looked up by the whole description in the plain map.
#[test]
fn create_trans_info_plain() {
    let mut f = fx();
    let (bank, food) = (f.bank, f.food);
    add_account(&mut f.book, bank, Some(CATEGORY_DESC), "This is the description", food).unwrap();
    let t = f.imported(18, bank, usd(-500), "This is the description");
    let first = t.splits[0].guid;
    let info = f.info(t, &plain());
    assert_eq!(info.trans.splits[0].guid, first);
    assert_eq!(info.dest_account, Some(food));
    assert_eq!(info.dest_source, Some(DestSource::Map));
}

/// `ImportBackendBayesTest.CreateTransInfo` (`gtest-import-backend.cpp:266-326`): the tokens are non-empty, hold no
/// separator, are unique, and include the weekday, `description` and `memo`; the Bayesian map gives the destination.
#[test]
fn create_trans_info_bayes() {
    let mut f = fx();
    let (bank, food) = (f.bank, f.food);
    let mut t = f.imported(18, bank, usd(-500), " test  tokens within   description  tokens  ");
    t.date_posted = Date::from_ymd(2020, 3, 18).unwrap().neutral_time();
    t.splits[0].memo = Some("  test   the memo test ".to_owned());
    let tokens = transaction_tokens(&t, &Locale::c());
    assert!(tokens.iter().all(|t| !t.is_empty() && !t.contains(' ')));
    let mut unique = tokens.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), tokens.len(), "{tokens:?}");
    ["Wednesday", "description", "memo"].iter().for_each(|w| assert!(tokens.iter().any(|t| t == w), "{w}"));

    crate::import_map::bayes::add_account_bayes(&mut f.book, bank, &tokens, food).unwrap();
    let info = f.info(t, &ImportSettings::default());
    assert_eq!(info.dest_account, Some(food));
    assert_eq!(info.dest_source, Some(DestSource::Bayes));
}

// ---------------------------------------------------------------------------------------------------------------
// Tokens

#[test]
fn tokens_in_gnucash_order_with_the_weekday_quirk() {
    let mut f = fx();
    let bank = f.bank;
    let mut t = f.imported(18, bank, usd(-500), "Monday shop shop");
    t.splits[0].memo = Some("card shop".to_owned());
    let mut other = Split::new(Guid::from_bytes([3; 16]), f.food, usd(500), usd(500));
    other.memo = Some("x".to_owned());
    t.splits.push(other);
    // Description: Monday, shop (prepended); weekday Monday again without the check; memos card, x.
    assert_eq!(transaction_tokens(&t, &Locale::c()), ["x", "card", "Monday", "shop", "Monday"]);
    let fr = Locale::by_name("fr_FR").unwrap();
    assert_eq!(transaction_tokens(&t, &fr)[2], "lundi", "the weekday is named in the Locale");
}

#[test]
fn weekday_is_taken_in_utc() {
    let mut f = fx();
    let bank = f.bank;
    let mut t = f.imported(18, bank, usd(-500), "");
    t.date_posted = Time64(day(18).neutral_time().0 + 14 * 3600); // 2024-03-19 00:59 UTC
    assert_eq!(transaction_tokens(&t, &Locale::c()), ["Tuesday"]);
}

/// No glibc full day name is longer than GnuCash's 15-byte `strftime` buffer in the locales red-ale has.
#[test]
fn day_names_fit_gnucash_buffer() {
    ["C", "en_US", "fr_FR", "de_DE", "en_GB"].iter().filter_map(|n| Locale::by_name(n).ok()).for_each(|l| {
        red_ale_locale::Weekday::ALL.iter().for_each(|w| assert!(l.day_name(*w).len() < 16, "{}", l.day_name(*w)));
    });
}

// ---------------------------------------------------------------------------------------------------------------
// Scores

fn score(f: &mut Fx, imported: &Transaction, existing: Guid, settings: &ImportSettings) -> Option<MatchInfo> {
    let t = f.book.transaction(existing).unwrap();
    let s = t.splits.iter().find(|s| s.account == imported.splits[0].account).unwrap();
    split_find_match(&f.book, imported, t, s, settings)
}

#[test]
fn every_scoring_branch() {
    let mut f = fx();
    let (bank, food) = (f.bank, f.food);
    let s = ImportSettings { match_threshold: -100, ..ImportSettings::default() };
    let e = f.existing(10, bank, food, usd(-2000), "Grocer", Some("101"));
    let mut t = f.imported(10, bank, usd(-2000), "");
    let m = score(&mut f, &t, e, &s).unwrap();
    assert_eq!((m.probability, m.update_proposed), (6, false), "amount +3, same day +3");
    t.date_posted = day(14).neutral_time();
    assert_eq!(score(&mut f, &t, e, &s).unwrap().probability, 5, "4 days: +2");
    assert!(score(&mut f, &t, e, &s).unwrap().update_proposed);
    t.date_posted = day(24).neutral_time();
    assert_eq!(score(&mut f, &t, e, &s).unwrap().probability, 3, "14 days: 0");
    t.date_posted = day(25).neutral_time();
    assert_eq!(score(&mut f, &t, e, &s).unwrap().probability, -2, "15 days: −5");
    t.date_posted = day(10).neutral_time();
    t.splits[0].amount = usd(-2150);
    assert_eq!(score(&mut f, &t, e, &s).unwrap().probability, 5, "within the ATM fee: +2");
    t.splits[0].amount = usd(-2300);
    assert_eq!(score(&mut f, &t, e, &s).unwrap().probability, -2, "beyond: −5");
    t.splits[0].amount = usd(-2000);
    t.num = Some("101".into());
    assert_eq!(score(&mut f, &t, e, &s).unwrap().probability, 10, "equal number +4");
    t.num = Some("102".into());
    assert_eq!(score(&mut f, &t, e, &s).unwrap().probability, 4, "different number −2");
    t.num = None;
    t.description = Some("GROCER".into());
    assert_eq!(score(&mut f, &t, e, &s).unwrap().probability, 8, "equal description, case-folded +2");
    t.description = Some("Grocery".into());
    assert_eq!(score(&mut f, &t, e, &s).unwrap().probability, 7, "half-prefix +1");
    let s1 = ImportSettings::default();
    t.date_posted = day(25).neutral_time();
    t.splits[0].amount = usd(-9999);
    assert_eq!(score(&mut f, &t, e, &s1), None, "below match-threshold");
}

#[test]
fn memo_is_scored_on_the_imported_split() {
    let mut f = fx();
    let (bank, food) = (f.bank, f.food);
    let s = ImportSettings::default();
    let e = f.existing(10, bank, food, usd(-2000), "", None);
    let mut t = f.imported(10, bank, usd(-2000), "");
    let mut book_t = f.book.transaction(e).unwrap().clone();
    book_t.splits.iter_mut().filter(|s| s.account == Some(bank)).for_each(|s| s.memo = Some("REF 1".into()));
    f.book.transactions.insert(e, Arc::new(book_t));
    t.splits[0].memo = Some("ref 1".into());
    assert_eq!(score(&mut f, &t, e, &s).unwrap().probability, 8);
}

// ---------------------------------------------------------------------------------------------------------------
// Actions

fn with_matches(scores: &[(i32, bool)]) -> TransInfo {
    let mut f = fx();
    let bank = f.bank;
    let t = f.imported(10, bank, usd(-100), "");
    let mut info = f.info(t, &ImportSettings::default());
    info.matches = scores
        .iter()
        .enumerate()
        .map(|(i, (p, u))| MatchInfo {
            split: Guid::from_bytes([u8::try_from(i).unwrap() + 1; 16]),
            transaction: Guid::from_bytes([u8::try_from(i).unwrap() + 100; 16]),
            probability: *p,
            update_proposed: *u,
        })
        .collect();
    info
}

#[test]
fn init_matches_chooses_as_gnucash() {
    let s = ImportSettings::default();
    let act = |scores: &[(i32, bool)], s: &ImportSettings| {
        let mut i = with_matches(scores);
        i.init_matches(s);
        i.action
    };
    assert_eq!(act(&[(6, false)], &s), ImportAction::Clear);
    assert_eq!(act(&[(6, true)], &s), ImportAction::Update);
    assert_eq!(act(&[(6, true)], &ImportSettings { enable_update: false, ..s.clone() }), ImportAction::Clear);
    assert_eq!(act(&[(3, false)], &s), ImportAction::Add);
    assert_eq!(act(&[], &s), ImportAction::Add);
    assert_eq!(act(&[(4, false)], &s), ImportAction::Skip);
    assert_eq!(act(&[(5, false)], &ImportSettings { enable_skip: false, ..s.clone() }), ImportAction::Update, "update whatever update_proposed says");
    assert_eq!(act(&[(5, false)], &ImportSettings { enable_skip: false, enable_update: false, ..s.clone() }), ImportAction::Add);
}

#[test]
fn sort_is_stable_and_descending() {
    let mut i = with_matches(&[(2, false), (5, false), (2, true), (5, true)]);
    i.init_matches(&ImportSettings::default());
    let order: Vec<(i32, bool)> = i.matches.iter().map(|m| (m.probability, m.update_proposed)).collect();
    assert_eq!(order, [(5, false), (5, true), (2, false), (2, true)]);
    assert_eq!(i.selected_match, Some(i.matches[0].split));
}

#[test]
fn conflicts_resolve_greedily_without_recomputing_the_action() {
    // Rows in GnuCash order. Rows 0 and 1 want transaction A; row 1 scores higher and keeps it.
    let a = Guid::from_bytes([200; 16]);
    let b = Guid::from_bytes([201; 16]);
    let mk = |tops: &[(Guid, i32)]| {
        let mut i = with_matches(&[]);
        i.matches = tops
            .iter()
            .enumerate()
            .map(|(n, (t, p))| MatchInfo { split: Guid::from_bytes([u8::try_from(n).unwrap() + 1; 16]), transaction: *t, probability: *p, update_proposed: false })
            .collect();
        i.init_matches(&ImportSettings::default());
        i
    };
    let mut rows = vec![mk(&[(a, 7), (b, 4)]), mk(&[(a, 9)]), mk(&[(b, 6)])];
    assert_eq!(rows[0].action, ImportAction::Clear);
    resolve_conflicts(&mut rows);
    assert_eq!(rows[1].matches[0].transaction, a, "the best score keeps it");
    assert!(rows[0].matches.is_empty(), "row 0 lost A, then lost B to row 2 (6 > 4)");
    assert_eq!(rows[0].action, ImportAction::Add, "a row left with no match is added");
    assert_eq!(rows[2].matches[0].transaction, b);

    let mut tie = vec![mk(&[(a, 7), (b, 2)]), mk(&[(a, 7)])];
    resolve_conflicts(&mut tie);
    assert_eq!(tie[0].matches[0].transaction, a, "a tie goes to the earlier row");
    assert_eq!(tie[1].action, ImportAction::Add);
    assert_eq!(tie[0].action, ImportAction::Clear);

    let mut demoted = vec![mk(&[(a, 9)]), mk(&[(a, 7), (b, 4)])];
    resolve_conflicts(&mut demoted);
    assert_eq!(demoted[1].matches[0].probability, 4);
    assert_eq!(demoted[1].action, ImportAction::Clear, "the action is not recomputed for the weaker match");
}

// ---------------------------------------------------------------------------------------------------------------
// Batches

#[test]
fn batch_finds_candidates_in_the_window_and_account() {
    let mut f = fx();
    let (bank, card, food) = (f.bank, f.card, f.food);
    let near = f.existing(5, bank, food, usd(-1000), "Shop", None);
    let other_account = f.existing(5, card, food, usd(-1000), "Shop", None);
    let mut far = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, 1, 10).unwrap());
    far.splits = vec![NewSplit::new(bank, usd(-1000)), NewSplit::new(food, usd(1000))];
    let far = post(&mut f.book, &mut f.env, far).unwrap();
    let t = f.imported(5, bank, usd(-1000), "Shop");
    let b = f.batch(vec![t], &ImportSettings::default());
    let found: Vec<Guid> = b.infos[0].matches.iter().map(|m| m.transaction).collect();
    assert_eq!(found, [near], "{other_account} is in another account; {far} is 55 days before");
    assert_eq!(b.infos[0].action, ImportAction::Clear);
    let mut later = f.imported(5, bank, usd(-1000), "Shop");
    later.date_posted = Date::from_ymd(2024, 2, 21).unwrap().neutral_time();
    let b = f.batch(vec![later], &ImportSettings { match_threshold: -100, ..ImportSettings::default() });
    assert!(b.infos[0].matches.iter().any(|m| m.transaction == far), "42 days after the far one: inside the window");
}

#[test]
fn online_ids_drop_duplicates_and_exclude_candidates() {
    let mut f = fx();
    let (bank, food) = (f.bank, f.food);
    let e = f.existing(5, bank, food, usd(-1000), "Shop", None);
    let s = f.split_in(e, bank);
    let mut t = (*f.book.transactions[&e]).clone();
    t.split_mut(s).unwrap().slots.insert_key(ONLINE_ID, SlotValue::String("ID1".into()));
    f.book.transactions.insert(e, Arc::new(t));
    let mut dup = f.imported(5, bank, usd(-1000), "Shop");
    dup.splits[0].slots.insert_key(ONLINE_ID, SlotValue::String("ID1".into()));
    let fresh = f.imported(5, bank, usd(-1000), "Shop");
    let b = f.batch(vec![dup, fresh], &ImportSettings::default());
    assert_eq!(b.duplicates, [0]);
    assert_eq!(b.infos.len(), 1);
    assert_eq!(b.infos[0].index, 1);
    assert!(b.infos[0].matches.is_empty(), "a split with an online_id is never a candidate");
}

#[test]
fn batch_works_in_reverse_arrival_order() {
    let mut f = fx();
    let (bank, food) = (f.bank, f.food);
    let e = f.existing(5, bank, food, usd(-1000), "Shop", None);
    let first = f.imported(5, bank, usd(-1000), "Shop");
    let second = f.imported(5, bank, usd(-1000), "Shop");
    let b = f.batch(vec![first, second], &ImportSettings::default());
    assert_eq!(b.infos.iter().map(|i| i.index).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(b.infos[1].matches[0].transaction, e, "the last to arrive is GnuCash's first row and wins the tie");
    assert!(b.infos[0].matches.is_empty());
}

// ---------------------------------------------------------------------------------------------------------------
// Destination amounts

#[test]
fn dest_amount_follows_currency_price_and_given_amount() {
    let mut f = fx();
    let (bank, food, eur) = (f.bank, f.food, f.eur_cash);
    let t = f.imported(5, bank, usd(-1000), "x");
    let mut info = f.info(t, &plain());
    assert!(info.dest_value.same_value(usd(1000)));
    assert!(info.dest_amount.is_zero(), "no destination: no amount");
    info.set_destination(&f.book, Some(food), true);
    assert!(info.dest_amount.same_value(usd(1000)), "same currency: amount = value");
    info.set_destination(&f.book, Some(eur), true);
    assert!(info.dest_amount.is_zero(), "another commodity, no price");
    info.set_price(&f.book, Numeric::new(125, 100));
    assert!(info.dest_amount.same_value(usd(800)), "value ÷ price: 10.00 / 1.25");
    info.set_dest_amount(&f.book, Numeric::new(900, 100));
    assert!(info.dest_amount.same_value(usd(900)), "a given amount is kept");
    let t = f.imported(5, bank, usd(-1000), "x");
    let last = LastSplitInfo { price: None, amount: Some(Numeric::new(950, 100)), account: Some(eur) };
    let info = TransInfo::new(&f.book, &plain(), 0, t, Some(&last), &Locale::c()).unwrap();
    assert_eq!((info.dest_account, info.dest_source), (Some(eur), Some(DestSource::File)));
    assert!(info.dest_amount.same_value(Numeric::new(950, 100)));
}

// ---------------------------------------------------------------------------------------------------------------
// Applying

fn opts(use_bayes: bool, append_text: bool) -> ApplyOptions {
    ApplyOptions { use_bayes, append_text }
}

#[test]
fn add_balances_with_the_destination_and_learns_nothing_automatic() {
    let mut f = fx();
    let (bank, food) = (f.bank, f.food);
    add_account(&mut f.book, bank, Some(CATEGORY_DESC), "Shop", food).unwrap();
    let t = f.imported(5, bank, usd(-1000), "Shop");
    let info = f.info(t, &plain());
    assert_eq!(info.action, ImportAction::Add);
    let before = f.book.account(bank).unwrap().slots.clone();
    let p = process_trans_item(&mut f.book, &mut f.env, &info, opts(false, false)).unwrap();
    assert!(p.applied && p.learned.is_empty() && p.repairs.is_empty());
    let added = f.book.transaction(p.transaction.unwrap()).unwrap();
    assert_eq!(added.splits.len(), 2);
    let near = added.splits.iter().find(|s| s.account == Some(bank)).unwrap();
    assert_eq!((near.reconcile_state, near.reconcile_date), (ReconcileState::Cleared, Some(f.env.now)));
    let other = added.splits.iter().find(|s| s.account == Some(food)).unwrap();
    assert!(other.value.same_value(usd(1000)) && other.memo.is_none() && other.reconcile_state == ReconcileState::New);
    assert_eq!(f.book.account(bank).unwrap().slots, before);
    assert_balanced(&f.book);
}

#[test]
fn add_without_destination_makes_an_imbalance_split() {
    let mut f = fx();
    let bank = f.bank;
    let t = f.imported(5, bank, usd(-1000), "Unknown");
    let info = f.info(t, &ImportSettings::default());
    let p = process_trans_item(&mut f.book, &mut f.env, &info, opts(true, false)).unwrap();
    assert!(matches!(&p.repairs[..], [Repair::AccountCreated { name, .. }, Repair::BalanceSplitAdded { .. }] if name == "Imbalance-USD"));
    assert_balanced(&f.book);
}

#[test]
fn add_to_a_priced_account_records_an_import_price_or_warns() {
    let mut f = fx();
    let (bank, eur) = (f.bank, f.eur_cash);
    let t = f.imported(5, bank, usd(-1000), "fx");
    let mut info = f.info(t, &plain());
    info.set_destination(&f.book, Some(eur), true);
    let p = process_trans_item(&mut f.book, &mut f.env, &info, opts(false, false)).unwrap();
    assert_eq!(p.warnings, [ProcessWarning::MissingRate], "no amount: the value at a rate of 1");
    let t = f.imported(6, bank, usd(-1000), "fx");
    let mut info = f.info(t, &plain());
    info.set_destination(&f.book, Some(eur), true);
    info.set_dest_amount(&f.book, Numeric::new(900, 100));
    let p = process_trans_item(&mut f.book, &mut f.env, &info, opts(false, false)).unwrap();
    assert!(p.warnings.is_empty());
    let [red_ale_core::ops::transactions::transfer::PriceChange::Created(price)] = p.prices[..] else { panic!("{:?}", p.prices) };
    let price = &f.book.prices[&price];
    assert_eq!(price.source.as_deref(), Some("user:split-import"));
    assert_eq!(price.commodity, CommodityRef::currency("EUR"));
    assert_balanced(&f.book);
}

#[test]
fn clear_reconciles_copies_online_id_and_learns_the_other_account() {
    let mut f = fx();
    let (bank, food) = (f.bank, f.food);
    let e = f.existing(5, bank, food, usd(-1000), "Shop", None);
    let s = f.split_in(e, bank);
    let mut t = f.imported(5, bank, usd(-1000), "Shop");
    t.splits[0].slots.insert_key(ONLINE_ID, SlotValue::String("BANK-9".into()));
    let b = f.batch(vec![t], &ImportSettings::default());
    let info = &b.infos[0];
    assert_eq!(info.action, ImportAction::Clear);
    let p = process_trans_item(&mut f.book, &mut f.env, info, opts(true, false)).unwrap();
    assert_eq!(p.transaction, Some(e));
    let split = f.book.transaction(e).unwrap().split(s).unwrap();
    assert_eq!((split.reconcile_state, split.reconcile_date), (ReconcileState::Cleared, Some(f.env.now)));
    assert_eq!(split.slots.get_key(ONLINE_ID), Some(&SlotValue::String("BANK-9".into())));
    assert!(matches!(&p.learned[..], [Learned::Bayes { target, .. }] if *target == food));
    assert_eq!(find_account_bayes(&f.book, bank, &info.tokens), Some(food));
    assert_eq!(f.book.transactions.len(), 1, "nothing added");
}

#[test]
fn clear_redates_a_reconciled_split_and_skips_learning_with_three_splits() {
    let mut f = fx();
    let (bank, food, drink) = (f.bank, f.food, f.drink);
    let mut new = NewTransaction::new(CommodityRef::currency("USD"), day(5));
    new.splits = vec![NewSplit::new(bank, usd(-1000)), NewSplit::new(food, usd(600)), NewSplit::new(drink, usd(400))];
    let e = post(&mut f.book, &mut f.env, new).unwrap();
    let s = f.split_in(e, bank);
    let mut t = (*f.book.transactions[&e]).clone();
    let split = t.split_mut(s).unwrap();
    split.reconcile_state = ReconcileState::Reconciled;
    split.reconcile_date = Some(Time64(5));
    f.book.transactions.insert(e, Arc::new(t));
    let imp = f.imported(5, bank, usd(-1000), "");
    let b = f.batch(vec![imp], &ImportSettings::default());
    let p = process_trans_item(&mut f.book, &mut f.env, &b.infos[0], opts(true, false)).unwrap();
    let split = f.book.transaction(e).unwrap().split(s).unwrap();
    assert_eq!((split.reconcile_state, split.reconcile_date), (ReconcileState::Reconciled, Some(f.env.now)), "y stays y, re-dated");
    assert!(p.learned.is_empty(), "no single other split");
}

#[test]
fn update_changes_the_match_from_the_import() {
    let mut f = fx();
    let (bank, food) = (f.bank, f.food);
    let e = f.existing(5, bank, food, usd(-1000), "Shop", None);
    let mut t = (*f.book.transactions[&e]).clone();
    t.slots.insert_key(NOTES, SlotValue::String("kept".into()));
    f.book.transactions.insert(e, Arc::new(t));
    let mut imp = f.imported(7, bank, usd(-1050), "SHOP 123");
    imp.splits[0].memo = Some("card".into());
    let b = f.batch(vec![imp], &ImportSettings::default());
    let mut info = b.infos[0].clone();
    assert_eq!(info.matches[0].probability, 5, "+2 ATM, +2 date, +1 description half-prefix");
    info.set_action(ImportAction::Update);
    let p = process_trans_item(&mut f.book, &mut f.env, &info, opts(true, false)).unwrap();
    let t = f.book.transaction(e).unwrap();
    assert_eq!(t.date_posted, day(7).neutral_time());
    assert_eq!(t.slots.get_key("date-posted"), Some(&SlotValue::Date(day(7))));
    let near = t.splits.iter().find(|s| s.account == Some(bank)).unwrap();
    let other = t.splits.iter().find(|s| s.account == Some(food)).unwrap();
    assert!(near.amount.same_value(usd(-1050)) && near.value.same_value(usd(-1050)));
    assert!(other.amount.same_value(usd(1050)) && other.value.same_value(usd(1050)), "the other split takes the balancing value");
    assert_eq!(near.memo.as_deref(), Some("card"));
    assert_eq!(near.reconcile_state, ReconcileState::Cleared);
    assert_eq!(t.description.as_deref(), Some("SHOP 123"));
    assert_eq!(t.notes(), Some("kept"), "no imported notes: GnuCash's null leaves them");
    assert!(p.warnings.is_empty());
    assert_balanced(&f.book);
}

#[test]
fn update_leaves_a_cleared_other_split_and_balances_with_imbalance() {
    let mut f = fx();
    let (bank, food) = (f.bank, f.food);
    let e = f.existing(5, bank, food, usd(-1000), "Shop", None);
    let o = f.split_in(e, food);
    let mut t = (*f.book.transactions[&e]).clone();
    t.split_mut(o).unwrap().reconcile_state = ReconcileState::Cleared;
    f.book.transactions.insert(e, Arc::new(t));
    let imp = f.imported(5, bank, usd(-1100), "Shop");
    let b = f.batch(vec![imp], &ImportSettings::default());
    let mut info = b.infos[0].clone();
    info.set_action(ImportAction::Update);
    let p = process_trans_item(&mut f.book, &mut f.env, &info, opts(true, false)).unwrap();
    assert_eq!(p.warnings, [ProcessWarning::OtherSplitNotUpdated]);
    assert!(p.repairs.iter().any(|r| matches!(r, Repair::BalanceSplitAdded { value, .. } if value.same_value(usd(100)))));
    assert_balanced(&f.book);
}

#[test]
fn update_appends_text_as_gnucash() {
    let cases: [(Option<&str>, Option<&str>, Option<&str>); 6] = [
        (None, Some("new"), Some("new")),
        (Some("old"), None, None),
        (Some("old"), Some("new"), Some("old|new")),
        (Some("old new"), Some("new"), None),
        (Some("abc"), Some("abcd"), Some("abc|abcd")),
        (Some("Cafe\u{301}"), Some("Café"), None),
    ];
    cases.iter().for_each(|(m, i, want)| assert_eq!(process::maybe_append(*m, *i).as_deref(), *want, "{m:?} {i:?}"));
}

#[test]
fn update_without_a_match_is_ignored() {
    let mut f = fx();
    let bank = f.bank;
    let t = f.imported(5, bank, usd(-1000), "x");
    let mut info = f.info(t, &ImportSettings::default());
    info.set_action(ImportAction::Clear);
    let before = f.book.clone();
    let p = process_trans_item(&mut f.book, &mut f.env, &info, opts(true, false)).unwrap();
    assert_eq!((p.applied, &p.warnings[..]), (false, &[ProcessWarning::NoMatch][..]));
    assert_eq!(f.book, before);
}

#[test]
fn apply_batch_saves_append_text_and_learns_chosen_destinations() {
    let mut f = fx();
    let (bank, card, food, drink) = (f.bank, f.card, f.food, f.drink);
    let a = f.imported(5, card, usd(-1000), "Pub");
    let b = f.imported(6, bank, usd(-500), "Bakery");
    let mut batch = f.batch(vec![a, b], &plain());
    batch.infos[0].set_destination(&f.book, Some(drink), true);
    batch.infos[0].set_action(ImportAction::Skip);
    batch.infos[1].set_destination(&f.book, Some(food), true);
    let out = apply_batch(&mut f.book, &mut f.env, &batch.infos, opts(false, true)).unwrap();
    assert_eq!(out.iter().map(|p| p.index).collect::<Vec<_>>(), [0, 1]);
    assert!(append_text(&f.book, bank), "saved on the first row's account: the last to arrive");
    assert!(!append_text(&f.book, card));
    assert_eq!(find_account(&f.book, card, Some(CATEGORY_DESC), "Pub"), Some(drink), "learned though skipped, as GnuCash");
    assert_eq!(find_account(&f.book, bank, Some(CATEGORY_DESC), "Bakery"), Some(food));
    assert_eq!(find_account(&f.book, bank, Some(CATEGORY_MEMO), "Bakery"), None, "no memo, no memo entry");
    assert!(!out[0].applied && out[1].applied);
    set_append_text(&mut f.book, bank, false).unwrap();
    assert!(f.book.account(bank).unwrap().slots.get_key(process::APPEND_TEXT).is_none());
    assert_balanced(&f.book);
}

// ---------------------------------------------------------------------------------------------------------------
// Properties

fn arb_text() -> impl Strategy<Value = Option<String>> {
    prop::option::of("[ a-zA-Zß0-9/Ä]{0,12}")
}

proptest! {
    /// Scoring never panics, whatever the amounts, dates and text.
    #[test]
    fn scoring_never_panics(
        an in any::<i64>(), ad in 1..i64::MAX, bn in any::<i64>(), bd in 1..i64::MAX,
        ta in any::<i64>(), tb in any::<i64>(), na in arb_text(), nb in arb_text(), da in arb_text(), db in arb_text(),
    ) {
        let mut f = fx();
        let (bank, food) = (f.bank, f.food);
        let mut t = f.imported(5, bank, Numeric::new(an, ad), "");
        t.date_posted = Time64(ta);
        t.num = na;
        t.description = da;
        let mut e = t.clone();
        e.guid = Guid::from_bytes([77; 16]);
        e.date_posted = Time64(tb);
        e.num = nb;
        e.description = db;
        e.splits[0].amount = Numeric::new(bn, bd);
        e.splits.push(Split::new(Guid::from_bytes([78; 16]), food, Numeric::ZERO, Numeric::ZERO));
        let _ = split_find_match(&f.book, &t, &e, &e.splits[0], &ImportSettings::default());
        let _ = transaction_tokens(&t, &Locale::c());
    }

    /// Applying any action to any imported amount succeeds and keeps the Book balanced.
    #[test]
    fn apply_keeps_the_book_balanced(cents in -1_000_000_i64..1_000_000, existing in -1_000_000_i64..1_000_000, action in 0_usize..4, dest in 0_usize..3, days in 0_u32..20) {
        let mut f = fx();
        let (bank, food) = (f.bank, f.food);
        f.existing(5, bank, food, usd(existing), "Shop", None);
        let t = f.imported(5 + days % 10, bank, usd(cents), "Shop");
        let mut b = f.batch(vec![t], &ImportSettings { match_threshold: -100, ..ImportSettings::default() });
        let info = &mut b.infos[0];
        info.set_action([ImportAction::Add, ImportAction::Clear, ImportAction::Update, ImportAction::Skip][action]);
        let target = [None, Some(food), Some(f.eur_cash)][dest];
        info.set_destination(&f.book, target, true);
        let applied = apply_batch(&mut f.book, &mut f.env, &b.infos, opts(dest % 2 == 0, days % 2 == 0));
        prop_assert!(applied.is_ok(), "{applied:?}");
        assert_balanced(&f.book);
    }
}
