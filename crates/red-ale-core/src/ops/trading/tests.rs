#![allow(clippy::unwrap_used)]

use proptest::prelude::*;

use super::*;
use crate::book_options::set_trading_accounts;
use crate::ops::commit::{NewSplit, NewTransaction, add_transaction, commit_changed};
use crate::ops::scrub::{Repair, Scope, scrub_imbalance};
use crate::{Commodity, Date, FixedEnv, Time64};

struct Fixture {
    book: Book,
    env: FixedEnv,
    checking: Guid,
    euro: Guid,
    acme: Guid,
    gains: Guid,
}

fn usd() -> CommodityRef {
    CommodityRef::currency("USD")
}

fn eur() -> CommodityRef {
    CommodityRef::currency("EUR")
}

fn acme() -> CommodityRef {
    CommodityRef::new("NASDAQ", "ACME")
}

fn add(book: &mut Book, env: &mut FixedEnv, name: &str, ty: AccountType, c: CommodityRef, parent: Guid) -> Guid {
    let scu = commodity_fraction(book, &c);
    let mut a = Account::new(env.new_guid(), name, ty, Some(c), Some(parent));
    a.commodity_scu = scu;
    let g = a.guid;
    book.accounts.insert(g, Arc::new(a));
    g
}

/// The book of the oracle runs in `design/notes/engine-accounts-transactions.md` §9.6: a root with no commodity and a
/// top-level USD Income account, so the root currency is USD.
fn fixture() -> Fixture {
    let mut env = FixedEnv::new(Time64(1_700_000_000), 0);
    let mut book = Book::new_empty(&mut env);
    book.commodities.insert(acme(), Arc::new(Commodity::new(acme(), Some("Acme".into()), 1000)));
    let root = book.root;
    let assets = add(&mut book, &mut env, "Assets", AccountType::Asset, usd(), root);
    let checking = add(&mut book, &mut env, "Checking", AccountType::Bank, usd(), assets);
    let euro = add(&mut book, &mut env, "Euro", AccountType::Bank, eur(), assets);
    let acme = add(&mut book, &mut env, "ACME", AccountType::Stock, acme(), assets);
    let income = add(&mut book, &mut env, "Income", AccountType::Income, usd(), root);
    let gains = add(&mut book, &mut env, "Gains", AccountType::Income, usd(), income);
    set_trading_accounts(&mut book, true);
    Fixture { book, env, checking, euro, acme, gains }
}

fn n(num: i64, denom: i64) -> Numeric {
    Numeric::new(num, denom)
}

/// An exchange split: `value` in USD cents for `amount` in the account's own units.
fn fx(account: Guid, value: i64, amount: i64, denom: i64) -> NewSplit {
    NewSplit::new(account, n(value, 100)).with_amount(n(amount, denom))
}

fn post(f: &mut Fixture, splits: Vec<NewSplit>) -> DomainResult<Guid> {
    let mut new = NewTransaction::new(usd(), Date::from_ymd(2024, 3, 1).unwrap());
    new.splits = splits;
    add_transaction(&mut f.book, &mut f.env, new)
}

/// Each split as (full account name, value, amount), in stored order.
fn shown(book: &Book, t: Guid) -> Vec<(String, Numeric, Numeric)> {
    book.transaction(t).unwrap().splits.iter().map(|s| (book.full_name(s.account.unwrap()).unwrap(), s.value, s.amount)).collect()
}

fn row(name: &str, value: Numeric, amount: Numeric) -> (String, Numeric, Numeric) {
    (name.to_owned(), value, amount)
}

#[test]
fn a_currency_exchange_gets_gnucash_trading_splits_and_tree() {
    let mut f = fixture();
    let (euro, checking) = (f.euro, f.checking);
    let t = post(&mut f, vec![fx(euro, 11000, 10000, 100), NewSplit::new(checking, n(-11000, 100))]).unwrap();
    assert_eq!(
        shown(&f.book, t),
        [
            row("Assets:Euro", n(11000, 100), n(10000, 100)),
            row("Trading:CURRENCY:USD", n(11000, 100), n(11000, 100)),
            row("Assets:Checking", n(-11000, 100), n(-11000, 100)),
            row("Trading:CURRENCY:EUR", n(-11000, 100), n(-10000, 100)),
        ]
    );
    let account = |name: &str| f.book.account_by_full_name(name).unwrap();
    let top = account("Trading");
    let top_props = (top.account_type, top.commodity.clone(), top.commodity_scu, top.is_placeholder());
    assert_eq!(top_props, (AccountType::Trading, Some(usd()), Some(100), true));
    assert_eq!(top.parent, Some(f.book.root));
    let ns = account("Trading:CURRENCY");
    assert_eq!((ns.commodity.clone(), ns.is_placeholder()), (Some(usd()), true));
    let leaf = account("Trading:CURRENCY:EUR");
    assert_eq!((leaf.commodity.clone(), leaf.commodity_scu, leaf.is_placeholder()), (Some(eur()), Some(100), false));
    assert!(crate::ops::commit::check_committed(&f.book, f.book.transaction(t).unwrap()).is_ok());
}

#[test]
fn a_purchase_and_a_gain_match_gnucash() {
    let mut f = fixture();
    let (acme_acc, checking, gains) = (f.acme, f.checking, f.gains);
    let buy = post(&mut f, vec![fx(acme_acc, 50000, 10000, 1000), NewSplit::new(checking, n(-50000, 100))]).unwrap();
    assert_eq!(
        shown(&f.book, buy),
        [
            row("Assets:ACME", n(50000, 100), n(10000, 1000)),
            row("Trading:CURRENCY:USD", n(50000, 100), n(50000, 100)),
            row("Assets:Checking", n(-50000, 100), n(-50000, 100)),
            row("Trading:NASDAQ:ACME", n(-50000, 100), n(-10000, 1000)),
        ]
    );
    let gain = post(&mut f, vec![fx(acme_acc, 2500, 0, 1000), NewSplit::new(gains, n(-2500, 100))]).unwrap();
    assert_eq!(
        shown(&f.book, gain),
        [
            row("Assets:ACME", n(2500, 100), n(0, 1000)),
            row("Trading:CURRENCY:USD", n(2500, 100), n(2500, 100)),
            row("Income:Gains", n(-2500, 100), n(-2500, 100)),
            row("Trading:NASDAQ:ACME", n(-2500, 100), n(0, 1000)),
        ]
    );
    let trading = f.book.descendants(f.book.root).into_iter().filter(|a| a.account_type == AccountType::Trading);
    let names: Vec<String> = trading.map(|a| f.book.full_name(a.guid).unwrap()).collect();
    // GnuCash balances the newest commodity first, so ACME's branch is made before the currency's.
    assert_eq!(names, ["Trading", "Trading:NASDAQ", "Trading:NASDAQ:ACME", "Trading:CURRENCY", "Trading:CURRENCY:USD"]);
}

#[test]
fn single_currency_and_option_off_get_no_trading_splits() {
    let mut f = fixture();
    let (checking, gains, euro) = (f.checking, f.gains, f.euro);
    let t = post(&mut f, vec![NewSplit::new(checking, n(100, 100)), NewSplit::new(gains, n(-100, 100))]).unwrap();
    assert_eq!(f.book.transaction(t).unwrap().splits.len(), 2);
    set_trading_accounts(&mut f.book, false);
    let t = post(&mut f, vec![fx(euro, 11000, 10000, 100), NewSplit::new(checking, n(-11000, 100))]).unwrap();
    assert_eq!(f.book.transaction(t).unwrap().splits.len(), 2);
    assert!(f.book.account_by_full_name("Trading").is_none());
}

#[test]
fn unbalanced_values_are_refused_and_balanced_trading_splits_kept() {
    let mut f = fixture();
    let (euro, checking) = (f.euro, f.checking);
    let err = post(&mut f, vec![fx(euro, 11000, 10000, 100), NewSplit::new(checking, n(-10000, 100))]);
    assert!(matches!(err, Err(DomainError::Unbalanced { .. })));
    let first = post(&mut f, vec![fx(euro, 11000, 10000, 100), NewSplit::new(checking, n(-11000, 100))]).unwrap();
    let (t_usd, t_eur) = {
        let a = |name: &str| f.book.account_by_full_name(name).unwrap().guid;
        (a("Trading:CURRENCY:USD"), a("Trading:CURRENCY:EUR"))
    };
    // A caller's own trading splits that balance stay as given, even at an odd value split between two splits.
    let splits = vec![
        fx(euro, 1000, 900, 100),
        NewSplit::new(checking, n(-1000, 100)),
        NewSplit::new(t_usd, n(1000, 100)),
        fx(t_eur, -1000, -900, 100).with_memo("mine"),
    ];
    let t = post(&mut f, splits).unwrap();
    let memo = f.book.transaction(t).unwrap().splits.iter().find(|s| s.account == Some(t_eur)).unwrap().memo.clone();
    assert_eq!(memo.as_deref(), Some("mine"));
    assert_ne!(first, t);
}

#[test]
fn trading_splits_are_rebuilt_when_an_edit_unbalances_them() {
    let mut f = fixture();
    let (euro, checking) = (f.euro, f.checking);
    let g = post(&mut f, vec![fx(euro, 11000, 10000, 100), NewSplit::new(checking, n(-11000, 100))]).unwrap();
    let mut t = f.book.transaction(g).unwrap().clone();
    let old: Vec<Guid> = t.splits.iter().filter(|s| is_trading_split(&f.book, s)).map(|s| s.guid).collect();
    t.splits.iter_mut().filter(|s| s.account == Some(euro)).for_each(|s| s.amount = n(9000, 100));
    let out = commit(&mut f.book, &mut f.env, &mut t).unwrap().unwrap();
    assert_eq!(out.removed.iter().map(|s| s.guid).collect::<Vec<_>>(), old);
    assert_eq!(out.added.len(), 2);
    let eur_split = t.splits.iter().find(|s| commodity_of(&f.book, s) == Some(eur()) && is_trading_split(&f.book, s)).unwrap();
    assert_eq!((eur_split.value, eur_split.amount), (n(-11000, 100), n(-9000, 100)));
}

#[test]
fn a_rate_change_that_leaves_amounts_balanced_keeps_the_old_trading_splits() {
    let mut f = fixture();
    let (euro, acme_acc) = (f.euro, f.acme);
    let g = post(&mut f, vec![fx(acme_acc, 11000, 2000, 1000), fx(euro, -11000, -10000, 100)]).unwrap();
    let mut t = f.book.transaction(g).unwrap().clone();
    t.splits.iter_mut().filter(|s| s.account == Some(acme_acc)).for_each(|s| s.value = n(12000, 100));
    t.splits.iter_mut().filter(|s| s.account == Some(euro)).for_each(|s| s.value = n(-12000, 100));
    assert!(is_balanced(&f.book, &t).unwrap(), "GnuCash checks amounts per commodity, not values");
    let before = t.clone();
    assert!(commit(&mut f.book, &mut f.env, &mut t).unwrap().is_none());
    assert_eq!(t, before);
}

#[test]
fn lookup_ignores_the_top_name_and_the_leaf_name() {
    let mut f = fixture();
    let root = f.book.root;
    let top = add(&mut f.book, &mut f.env, "Handel", AccountType::Trading, usd(), root);
    let ns = add(&mut f.book, &mut f.env, "CURRENCY", AccountType::Trading, usd(), top);
    let leaf = add(&mut f.book, &mut f.env, "Euros", AccountType::Trading, eur(), ns);
    let mut created = Vec::new();
    assert_eq!(trading_account(&mut f.book, &mut f.env, &eur(), &mut created).unwrap(), leaf);
    assert!(created.is_empty());
    let usd_leaf = trading_account(&mut f.book, &mut f.env, &usd(), &mut created).unwrap();
    assert_eq!(f.book.full_name(usd_leaf).as_deref(), Some("Handel:CURRENCY:USD"));
    assert_eq!(created, [usd_leaf]);
}

#[test]
fn without_a_root_currency_trading_parents_have_no_commodity() {
    let mut env = FixedEnv::new(Time64(0), 0);
    let mut book = Book::new_empty(&mut env);
    let mut created = Vec::new();
    let leaf = trading_account(&mut book, &mut env, &eur(), &mut created).unwrap();
    assert_eq!(created.len(), 3);
    assert!(book.account_by_full_name("Trading").unwrap().commodity.is_none());
    assert_eq!(book.account(leaf).unwrap().commodity, Some(eur()));
}

#[test]
fn the_session_commit_rebalances_changed_transactions_only() {
    let mut f = fixture();
    let (euro, checking) = (f.euro, f.checking);
    set_trading_accounts(&mut f.book, false);
    let a = post(&mut f, vec![fx(euro, 11000, 10000, 100), NewSplit::new(checking, n(-11000, 100))]).unwrap();
    let b = post(&mut f, vec![fx(euro, 2200, 2000, 100), NewSplit::new(checking, n(-2200, 100))]).unwrap();
    set_trading_accounts(&mut f.book, true);
    let before = f.book.clone();
    Arc::make_mut(f.book.transactions.get_mut(&a).unwrap()).splits[0].memo = Some("edited".into());
    commit_changed(&before, &mut f.book, &mut f.env).unwrap();
    assert_eq!(f.book.transaction(a).unwrap().splits.len(), 4);
    assert_eq!(f.book.transaction(b).unwrap().splits.len(), 2, "untouched transactions keep their splits, as in GnuCash");
}

#[test]
fn the_imbalance_scrub_runs_the_trading_branch() {
    let mut f = fixture();
    let (euro, checking) = (f.euro, f.checking);
    set_trading_accounts(&mut f.book, false);
    let g = post(&mut f, vec![fx(euro, 11000, 10000, 100), NewSplit::new(checking, n(-11000, 100))]).unwrap();
    Arc::make_mut(f.book.transactions.get_mut(&g).unwrap()).splits[0].value = n(11500, 100);
    set_trading_accounts(&mut f.book, true);
    let repairs = scrub_imbalance(&mut f.book, &mut f.env, Scope::Transaction(g)).unwrap();
    let kinds: Vec<&str> = repairs
        .iter()
        .map(|r| match r {
            Repair::AccountCreated { .. } => "created",
            Repair::BalanceSplitAdded { .. } => "balance",
            Repair::TradingSplitAdded { .. } => "trading",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["created", "balance", "created", "created", "created", "created", "trading", "trading"]);
    let t = f.book.transaction(g).unwrap();
    assert!(is_balanced(&f.book, t).unwrap());
    assert!(scrub_imbalance(&mut f.book, &mut f.env, Scope::Transaction(g)).unwrap().is_empty(), "a second scrub finds nothing");
}

/// A split of a generated transaction: which account, value in cents, amount in the account's units.
fn arb_split() -> impl Strategy<Value = (usize, i64, i64)> {
    (0usize..4, -1_000_000i64..1_000_000, -1_000_000i64..1_000_000)
}

proptest! {
    /// Every committed multi-commodity transaction balances with trading accounts: non-trading and trading values sum
    /// to zero, and so does every commodity's amount.
    #[test]
    fn every_committed_transaction_balances_per_commodity(splits in proptest::collection::vec(arb_split(), 1..6)) {
        let mut f = fixture();
        let accounts = [f.checking, f.euro, f.acme, f.gains];
        let mut new: Vec<NewSplit> = splits
            .iter()
            .filter(|(_, v, _)| *v != 0)
            .map(|(i, v, a)| {
                let s = NewSplit::new(accounts[*i], n(*v, 100));
                match *i { 1 => s.with_amount(n(*a, 100)), 2 => s.with_amount(n(*a, 1000)), _ => s }
            })
            .collect();
        let total: i64 = new.iter().map(|s| s.value.num()).sum();
        new.push(NewSplit::new(f.checking, n(-total, 100)));
        new.retain(|s| !s.value.is_zero());
        prop_assume!(!new.is_empty());
        let g = post(&mut f, new).unwrap();
        let t = f.book.transaction(g).unwrap();
        prop_assert!(is_balanced(&f.book, t).unwrap());
        let (other, trading) = value_sums(&f.book, t).unwrap();
        prop_assert!(other.is_zero() && trading.is_zero());
        for c in [usd(), eur(), acme()] {
            let mut in_c = t.splits.iter().filter(|s| commodity_of(&f.book, s) == Some(c.clone()));
            let sum = in_c.try_fold(Numeric::ZERO, |acc, s| acc.add_exact(s.amount)).unwrap();
            prop_assert!(sum.is_zero(), "{c} amounts sum to {sum:?}");
        }
    }
}
