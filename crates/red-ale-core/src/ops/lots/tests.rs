//! Unit and property tests of lots, capital gains and stock splits. The expected figures are GnuCash 5.17's
//! (`design/notes/engine-commodities-prices-lots.md` §6-7, from oracle runs).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use proptest::prelude::*;

use super::*;
use crate::lot::{gains_split, split_slots};
use crate::ops::commit::{NewSplit, NewTransaction, add_transaction};
use crate::{Account, AccountType, Commodity, Date, FixedEnv, Price};

struct Fx {
    book: Book,
    env: FixedEnv,
    bank: Guid,
    stock: Guid,
    income: Guid,
}

fn n(s: &str) -> Numeric {
    Numeric::parse_any(s).unwrap()
}

fn day(d: u32) -> Date {
    Date::from_ymd(2024, 1, d).unwrap()
}

fn fx() -> Fx {
    let mut env = FixedEnv::new(Time64(1_719_835_200), 0x100);
    let mut book = Book::new_empty(&mut env);
    let usd = CommodityRef::currency("USD");
    let acme = CommodityRef::new("NASDAQ", "ACME");
    book.commodities
        .insert(acme.clone(), Arc::new(Commodity::new(acme.clone(), Some("Acme".into()), 1000)));
    let mut add = |book: &mut Book, name: &str, ty, c: &CommodityRef, scu, parent: Option<Guid>| {
        let mut a = Account::new(env.new_guid(), name, ty, Some(c.clone()), Some(parent.unwrap_or(book.root)));
        a.commodity_scu = Some(scu);
        let g = a.guid;
        book.accounts.insert(g, Arc::new(a));
        g
    };
    let bank = add(&mut book, "Bank", AccountType::Bank, &usd, 100, None);
    let brokerage = add(&mut book, "Brokerage", AccountType::Asset, &usd, 100, None);
    let stock = add(&mut book, "ACME", AccountType::Stock, &acme, 1000, Some(brokerage));
    let income = add(&mut book, "Income", AccountType::Income, &usd, 100, None);
    Fx {
        book,
        env,
        bank,
        stock,
        income,
    }
}

impl Fx {
    fn trade(&mut self, d: u32, shares: &str, value: &str) -> Guid {
        let mut t = NewTransaction::new(CommodityRef::currency("USD"), day(d));
        t.splits = vec![
            NewSplit::new(self.stock, n(value)).with_amount(n(shares)),
            NewSplit::new(self.bank, n(value).neg()),
        ];
        add_transaction(&mut self.book, &mut self.env, t).unwrap()
    }

    fn stock_split(&mut self, d: u32, shares: &str) -> Guid {
        let req = StockSplit {
            account: self.stock,
            date: day(d),
            shares: n(shares),
            description: None,
            currency: None,
            price: None,
            price_currency: None,
            cash: None,
            income_account: None,
            asset_account: None,
            memo: None,
        };
        stock_split(&mut self.book, &mut self.env, &req).unwrap().transaction
    }

    fn scrub(&mut self) {
        scrub_lots(&mut self.book, &mut self.env, self.stock).unwrap();
    }

    fn lots(&self) -> Vec<LotSummary> {
        lot_summaries(&self.book, self.stock).unwrap()
    }

    fn stock_splits_of(&self, txn: Guid) -> Vec<Split> {
        self.book
            .transaction(txn)
            .unwrap()
            .splits
            .iter()
            .filter(|s| s.account == Some(self.stock))
            .cloned()
            .collect()
    }

    fn gains_txns(&self) -> Vec<&Transaction> {
        self.book
            .transactions
            .values()
            .map(AsRef::as_ref)
            .filter(|t| t.description.as_deref() == Some(REALIZED_GAIN_LOSS))
            .collect()
    }
}

fn titles(lots: &[LotSummary]) -> Vec<String> {
    lots.iter().map(|l| l.title.clone().unwrap_or_default()).collect()
}

#[test]
fn fifo_cuts_the_sale_and_records_gains() {
    let mut f = fx();
    f.trade(1, "10", "100.00");
    f.trade(2, "10", "120.00");
    let sale = f.trade(3, "-15", "-225.00");
    f.scrub();
    let lots = f.lots();
    assert_eq!(titles(&lots), ["Lot 0", "Lot 1"]);
    assert!(lots[0].closed && !lots[1].closed);
    assert_eq!(lots[0].gains, n("50.00"));
    assert_eq!(lots[1].gains, n("15.00"));
    assert_eq!(lots[1].balance, n("5"));
    let pieces = f.stock_splits_of(sale);
    assert_eq!(pieces.len(), 2);
    assert!(pieces[0].value.same_repr(Numeric::new(-15000, 100)) && pieces[0].amount.same_repr(Numeric::new(-10000, 1000)));
    assert!(pieces[1].value.same_repr(Numeric::new(-7500, 100)));
    assert!(is_peer_split_of(&pieces[0], pieces[1].guid) && is_peer_split_of(&pieces[1], pieces[0].guid));
    let gains = f.gains_txns();
    assert_eq!(gains.len(), 2);
    let g = *gains
        .iter()
        .find(|t| t.splits.iter().any(|s| crate::lot::gains_source(s) == Some(pieces[0].guid)))
        .unwrap();
    assert_eq!(g.date_posted, day(3).neutral_time());
    assert!(g.slots.is_empty(), "no date-posted slot on a gains transaction");
    let lot_split = g.splits.iter().find(|s| s.account == Some(f.stock)).unwrap();
    assert!(lot_split.amount.same_repr(Numeric::new(0, 1000)));
    assert_eq!(lot_split.memo.as_deref(), Some(REALIZED_GAIN_LOSS));
    assert_eq!(crate::lot::gains_source(lot_split), Some(pieces[0].guid));
    assert_eq!(gains_split(&pieces[0]), Some(lot_split.guid));
    let orphan = f.book.account_by_full_name("Orphaned Gains-USD").unwrap();
    assert_eq!(orphan.account_type, AccountType::Income);
    assert_eq!(orphan.notes(), Some(ORPHANED_GAINS_NOTES));
    let stock = f.book.account(f.stock).unwrap();
    assert_eq!(
        stock.slots.get("lot-mgmt/gains-acct/CURRENCY::USD").and_then(SlotValue::as_guid),
        Some(orphan.guid)
    );
    assert_eq!(stock.slots.get("lot-mgmt/next-id").and_then(SlotValue::as_int), Some(2));
    assert!(f.book.transactions.values().all(|t| t.is_balanced()));
}

fn is_peer_split_of(s: &Split, peer: Guid) -> bool {
    crate::lot::is_peer_split(s, peer)
}

#[test]
fn cut_values_round_half_up_and_peers_overwrite() {
    let mut f = fx();
    (1..=3).for_each(|d| {
        f.trade(d, "10", "100.00");
    });
    let sale = f.trade(4, "-25", "-375.01");
    f.scrub();
    let pieces = f.stock_splits_of(sale);
    let values: Vec<Numeric> = pieces.iter().map(|s| s.value).collect();
    let mut sorted = values.clone();
    sorted.sort_by(|a, b| a.cmp_value(*b));
    assert_eq!(sorted, [n("-150.01"), n("-150.00"), n("-75.00")]);
    let by_value = |v: &str| pieces.iter().find(|s| s.value == n(v)).unwrap().clone();
    let (first, second, third) = (by_value("-150.00"), by_value("-150.01"), by_value("-75.00"));
    assert!(is_peer_split_of(&first, second.guid));
    assert!(is_peer_split_of(&second, third.guid), "the second cut overwrites the first link");
    assert!(is_peer_split_of(&third, second.guid));
}

#[test]
fn stock_splits_stay_out_of_lots_and_gains_use_adjusted_amounts() {
    let mut f = fx();
    f.trade(1, "10", "100.00");
    f.trade(2, "10", "120.00");
    let split_txn = f.stock_split(3, "20");
    let sale = f.trade(4, "-30", "-225.00");
    f.scrub();
    assert!(f.stock_splits_of(split_txn)[0].lot.is_none());
    let pieces = f.stock_splits_of(sale);
    let amounts: Vec<Numeric> = pieces.iter().map(|s| s.amount).collect();
    assert!(amounts.contains(&n("-20")) && amounts.contains(&n("-10")));
    let lots = f.lots();
    assert_eq!(lots.iter().map(|l| l.gains).collect::<Vec<_>>(), [n("50.00"), n("15.00")]);
    assert!(lots[0].closed, "a lot of raw +10 adjusted +20 and -20 closes");
    assert_eq!(lots[1].balance, n("10"));
}

#[test]
fn scrubbing_twice_changes_nothing() {
    let mut f = fx();
    f.trade(1, "10", "100.00");
    f.trade(2, "-4", "-60.00");
    f.trade(3, "-6", "-54.00");
    f.scrub();
    let once = f.book.clone();
    f.scrub();
    assert_eq!(f.book, once);
    assert_eq!(f.lots()[0].gains, n("14.00"), "20.00 on the first sale, -6.00 on the second");
}

#[test]
fn a_changed_sale_updates_its_gains_transaction() {
    let mut f = fx();
    f.trade(1, "10", "100.00");
    let sale = f.trade(2, "-10", "-150.00");
    f.scrub();
    let sale_split = f.stock_splits_of(sale)[0].guid;
    edit_split(&mut f.book, sale_split, |s| s.value = n("-130.00")).unwrap();
    let bank = f
        .book
        .transaction(sale)
        .unwrap()
        .splits
        .iter()
        .find(|s| s.account == Some(f.bank))
        .unwrap()
        .guid;
    edit_split(&mut f.book, bank, |s| {
        s.value = n("130.00");
        s.amount = n("130.00");
    })
    .unwrap();
    f.scrub();
    assert_eq!(f.gains_txns().len(), 1);
    assert_eq!(f.lots()[0].gains, n("30.00"));
}

#[test]
fn merged_pieces_drop_their_gains_and_refill() {
    let mut f = fx();
    f.trade(1, "10", "100.00");
    f.trade(2, "10", "120.00");
    let sale = f.trade(3, "-15", "-225.00");
    f.scrub();
    let lots = f.lots();
    let small = f.stock_splits_of(sale).into_iter().find(|s| s.amount == n("-5")).unwrap();
    edit_split(&mut f.book, small.guid, |s| s.lot = Some(lots[0].guid)).unwrap();
    f.scrub();
    f.scrub();
    let lots = f.lots();
    assert_eq!(lots.iter().map(|l| l.gains).collect::<Vec<_>>(), [n("50.00"), n("15.00")]);
    assert_eq!(
        f.gains_txns().len(),
        2,
        "the merged piece's gains transaction is gone, a new one replaces it"
    );
    assert!(f.book.transactions.values().all(|t| t.is_balanced()));
}

#[test]
fn a_reassigned_sale_keeps_its_one_gains_transaction() {
    // GnuCash 5.17 meets the gains split before the re-added sale, so it does not know the sale's gains split and
    // makes a second gains transaction; red-ale follows the slots and updates the one it has.
    let mut f = fx();
    f.trade(1, "10", "100.00");
    let sale = f.trade(2, "-4", "-60.00");
    f.scrub();
    let sale_split = f.stock_splits_of(sale)[0].guid;
    remove_split(&mut f.book, sale_split).unwrap();
    f.scrub();
    assert_eq!(f.gains_txns().len(), 1);
    assert_eq!(f.lots()[0].gains, n("20.00"));
}

#[test]
fn lots_are_made_titled_edited_and_deleted() {
    let mut f = fx();
    let a = create_lot(&mut f.book, &mut f.env, f.stock, None, None).unwrap();
    let b = create_lot(&mut f.book, &mut f.env, f.stock, Some("Mine"), Some("notes")).unwrap();
    assert_eq!(f.book.lot(a).unwrap().title(), Some("Lot 0"));
    assert_eq!(f.book.lot(b).unwrap().title(), Some("Mine"));
    update_lot(&mut f.book, b, None, Some("")).unwrap();
    assert_eq!(f.book.lot(b).unwrap().notes(), None);
    let buy = f.trade(1, "10", "100.00");
    let split = f.stock_splits_of(buy)[0].guid;
    assign_split(&mut f.book, &mut f.env, split, Some(a)).unwrap();
    assert!(matches!(delete_lot(&mut f.book, a, false), Err(DomainError::Conflict(_))));
    assert_eq!(delete_lot(&mut f.book, a, true).unwrap(), [split]);
    assert!(f.book.lot(a).is_none() && f.stock_splits_of(buy)[0].lot.is_none());
    delete_lot(&mut f.book, b, false).unwrap();
}

#[test]
fn an_explicit_lot_takes_what_fits_and_closed_lots_refuse() {
    let mut f = fx();
    let buy = f.trade(1, "10", "100.00");
    let sale = f.trade(2, "-15", "-180.00");
    let lot = create_lot(&mut f.book, &mut f.env, f.stock, None, None).unwrap();
    let buy_split = f.stock_splits_of(buy)[0].guid;
    assign_split(&mut f.book, &mut f.env, buy_split, Some(lot)).unwrap();
    let sale_split = f.stock_splits_of(sale)[0].guid;
    let out = assign_split(&mut f.book, &mut f.env, sale_split, Some(lot)).unwrap();
    let rest = out.rest.unwrap();
    assert_eq!(out.pieces, [(sale_split, lot)]);
    assert!(lot_summary(&f.book, lot).unwrap().closed);
    assert!(matches!(
        assign_split(&mut f.book, &mut f.env, rest, Some(lot)),
        Err(DomainError::Conflict(_))
    ));
    let fifo = assign_split(&mut f.book, &mut f.env, rest, None).unwrap();
    assert_eq!(fifo.new_lots.len(), 1, "no open lot takes a sale, so FIFO opens one");
    assert!(!remove_split(&mut f.book, sale_split).unwrap());
    assert!(remove_split(&mut f.book, rest).unwrap(), "a lot left empty is deleted");
}

#[test]
fn the_viewer_lists_splits_gains_and_free_split_gains() {
    let mut f = fx();
    f.trade(1, "10", "100.00");
    let sale = f.trade(2, "-4", "-60.00");
    f.scrub();
    let lot = f.lots()[0].guid;
    let splits = lot_splits(&f.book, lot).unwrap();
    assert_eq!(
        splits.iter().map(|s| s.kind).collect::<Vec<_>>(),
        [SplitKind::Normal, SplitKind::Normal, SplitKind::Gains]
    );
    assert_eq!(splits[1].gain, Some(n("20.00")));
    let later = f.trade(3, "-2", "-40.00");
    let free = free_splits(&f.book, f.stock, Some(lot)).unwrap();
    assert_eq!(free.len(), 1);
    assert_eq!(free[0].transaction, later);
    assert_eq!(free[0].gain, Some(n("20.00")));
    assert_ne!(sale, later);
}

#[test]
fn the_stock_split_assistant_writes_what_gnucash_writes() {
    let mut f = fx();
    f.trade(1, "10", "100.00");
    let mut req = StockSplit {
        account: f.stock,
        date: day(5),
        shares: n("10"),
        description: None,
        currency: None,
        price: Some(n("5.25")),
        price_currency: None,
        cash: Some(n("3.50")),
        income_account: Some(f.income),
        asset_account: Some(f.bank),
        memo: None,
    };
    let out = stock_split(&mut f.book, &mut f.env, &req).unwrap();
    let t = f.book.transaction(out.transaction).unwrap();
    assert_eq!(t.description.as_deref(), Some(DEFAULT_DESCRIPTION_FOR_TEST));
    assert_eq!(t.currency, CommodityRef::currency("USD"), "the Brokerage parent's currency");
    let s1 = &t.splits[0];
    assert!(s1.value.same_repr(Numeric::new(0, 1)));
    assert!(s1.amount.same_repr(Numeric::new(10000, 1000)));
    assert_eq!(s1.action.as_deref(), Some(STOCK_SPLIT_ACTION));
    assert_eq!(s1.slots.get_str(split_slots::SPLIT_TYPE), Some("stock-split"));
    assert_eq!(t.splits[1].memo.as_deref(), Some("Cash in lieu"));
    assert_eq!((t.splits[1].account, t.splits[2].account), (Some(f.bank), Some(f.income)));
    let PriceOutcome::Added(p) = out.price else {
        panic!("price {:?}", out.price)
    };
    let p = f.book.prices.get(&p).unwrap();
    assert_eq!(
        (p.source.as_deref(), p.price_type.as_deref(), p.time),
        (Some("user:stock-split"), Some("unknown"), day(5).neutral_time())
    );
    // A same-day price from the price editor blocks the next one; the transaction is still recorded.
    let editor = Price {
        guid: f.env.new_guid(),
        source: Some("user:price-editor".into()),
        ..(**p).clone()
    };
    f.book.prices.clear();
    f.book.prices.insert(editor.guid, Arc::new(editor.clone()));
    req.cash = None;
    let out = stock_split(&mut f.book, &mut f.env, &req).unwrap();
    assert_eq!(out.price, PriceOutcome::Rejected { existing: editor.guid });
    req.shares = n("0");
    assert!(stock_split(&mut f.book, &mut f.env, &req).is_err());
}

const DEFAULT_DESCRIPTION_FOR_TEST: &str = "Stock Split";

#[test]
fn accounts_without_trades_are_not_scrubbed() {
    let mut f = fx();
    f.trade(1, "10", "100.00");
    let before = f.book.clone();
    scrub_lots(&mut f.book, &mut f.env, f.bank).unwrap();
    assert_eq!(f.book, before);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// After a scrub every transaction balances, every buy and sell is in a lot, the account's shares are unchanged,
    /// each closed lot's values sum to zero, and a second scrub changes nothing.
    #[test]
    fn scrubbed_lots_double_balance(trades in prop::collection::vec((1_i64..40, 100_i64..9_000, any::<bool>()), 1..8)) {
        let mut f = fx();
        let mut held = 0_i64;
        trades.iter().enumerate().for_each(|(i, (qty, price, sell))| {
            let d = u32::try_from(i).unwrap() + 1;
            let qty = if *sell && held >= *qty { -qty } else { *qty };
            held += qty;
            let value = Numeric::new(qty * price, 100);
            f.trade(d, &format!("{qty}"), &format!("{}/100", value.num()));
        });
        let shares = Numeric::sum_fixed(f.book.splits_of(f.stock).map(|(_, s)| s.amount)).unwrap();
        f.scrub();
        prop_assert!(f.book.transactions.values().all(|t| t.is_balanced()));
        prop_assert_eq!(Numeric::sum_fixed(f.book.splits_of(f.stock).map(|(_, s)| s.amount)).unwrap(), shares);
        prop_assert!(f.book.splits_of(f.stock).all(|(_, s)| s.lot.is_some()));
        f.lots().iter().filter(|l| l.closed).for_each(|l| {
            let values = Numeric::sum_fixed(f.book.splits_in_lot(l.guid).map(|(_, s)| s.value)).unwrap();
            assert!(values.is_zero(), "closed lot {:?} has value {values:?}", l.title);
        });
        let once = f.book.clone();
        f.scrub();
        prop_assert_eq!(&f.book, &once);
    }
}
