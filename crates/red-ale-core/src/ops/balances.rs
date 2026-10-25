//! Domain reads: account balances as GnuCash's engine computes them.
//!
//! GnuCash walks an account's splits in split order (`xaccSplitOrder`) and keeps four running totals of their amounts
//! on each split (`xaccAccountRecomputeBalance`, `5.17:libgnucash/engine/Account.cpp:2263-2335`). Every dated balance
//! is read from those: the as-of balance is the running total at the last split posted strictly before the time
//! (`GetBalanceAsOfDate`, `:3505-3537`), and the reports' balances at dates the running total at the last split posted
//! at or before each date (`gnc_account_accumulate_to_dates`, `5.17:bindings/engine.i:196-236`). See
//! `design/notes/engine-accounts-transactions.md` §2.7 and `design/reports.md` §5.

use std::collections::BTreeMap;

use crate::book_options::uses_split_action_for_num;
use crate::ops::commit::{account_scu, commodity_fraction};
use crate::ops::commodities::canonical;
use crate::ops::prices::convert::{PriceAt, convert_balance};
use crate::ops::transactions::order::split_order;
use crate::{Account, Book, CommodityRef, DomainError, DomainResult, Guid, How, Numeric, ReconcileState, Round, Split, Time64, Transaction};

/// An account's running totals, in its commodity (amounts, not values).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Balances {
    /// Every split.
    pub balance: Numeric,
    /// Splits not in state `n`.
    pub cleared: Numeric,
    /// Splits in state `y` or `f`.
    pub reconciled: Numeric,
    /// Splits whose transaction is not a Closing transaction: GnuCash's *noclosing* balance.
    pub noclosing: Numeric,
}

impl Balances {
    /// All zero.
    pub const ZERO: Self = Self { balance: Numeric::ZERO, cleared: Numeric::ZERO, reconciled: Numeric::ZERO, noclosing: Numeric::ZERO };

    /// One of the totals.
    pub fn get(self, total: Total) -> Numeric {
        match total {
            Total::Balance => self.balance,
            Total::Cleared => self.cleared,
            Total::Reconciled => self.reconciled,
            Total::Noclosing => self.noclosing,
        }
    }
}

/// Which running total a read takes: GnuCash's `xaccSplitGetBalance`, `…ClearedBalance`, `…ReconciledBalance` and
/// `…NoclosingBalance` (`5.17:libgnucash/engine/Split.cpp:1316-1338`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Total {
    /// Every split.
    Balance,
    /// Splits not in state `n`.
    Cleared,
    /// Splits in state `y` or `f`.
    Reconciled,
    /// Every split but those of Closing transactions.
    Noclosing,
}

/// An account's splits with their transactions, in split order: GnuCash's `xaccAccountGetSplits`. With the Book's
/// num field source on, split actions stand in for transaction nums (`5.17:libgnucash/engine/Split.cpp:1548-1557`),
/// which reorders splits posted at the same time and so their running balances. An unknown account has none.
pub fn account_splits(book: &Book, account: Guid) -> Vec<(&Transaction, &Split)> {
    let num_action = uses_split_action_for_num(book);
    let mut splits: Vec<(&Transaction, &Split)> = book.splits_of(account).collect();
    splits.sort_by(|(ta, sa), (tb, sb)| split_order(ta, sa, tb, sb, num_action));
    splits
}

/// An account's splits posted from `from` to `to`, both inclusive, in split order, and with `include_descendants`
/// then each descendant's in turn (depth first, children in book order): `gnc_account_foreach_split_between_dates`
/// (`5.17:libgnucash/engine/Account.hpp:51-85`). `None` leaves that end open.
pub fn account_splits_between(
    book: &Book,
    account: Guid,
    from: Option<Time64>,
    to: Option<Time64>,
    include_descendants: bool,
) -> Vec<(&Transaction, &Split)> {
    let descendants = if include_descendants { book.descendants(account).iter().map(|a| a.guid).collect() } else { Vec::new() };
    std::iter::once(account)
        .chain(descendants)
        .flat_map(|a| account_splits(book, a))
        .filter(|(t, _)| from.is_none_or(|f| t.date_posted >= f) && to.is_none_or(|e| t.date_posted <= e))
        .collect()
}

/// A split of an account with the running totals GnuCash keeps on it (`split->balance` and friends).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunningSplit<'a> {
    /// The split's transaction.
    pub transaction: &'a Transaction,
    /// The split.
    pub split: &'a Split,
    /// The split's amount at the account's SCU, as GnuCash holds it once loaded.
    pub amount: Numeric,
    /// The running totals after this split. A stock split GnuCash skips (below) never has them set, so they read as
    /// zero, as they do in GnuCash after loading a book.
    pub running: Balances,
}

fn is_stock_split(s: &Split) -> bool {
    s.slots.get_str("split-type") == Some("stock-split")
}

/// One step of `xaccAccountRecomputeBalance` (`5.17:libgnucash/engine/Account.cpp:2280-2328`). A stock split is
/// skipped, every total and its own running totals untouched, while the balance is zero or when it would make it
/// zero; then this returns false.
fn step(running: &mut Balances, t: &Transaction, s: &Split, amount: Numeric) -> DomainResult<bool> {
    if is_stock_split(s) {
        if running.balance.is_zero() {
            return Ok(false);
        }
        let next = running.balance.add_fixed(amount)?;
        if next.is_zero() {
            return Ok(false);
        }
        running.balance = next;
    } else {
        running.balance = running.balance.add_fixed(amount)?;
    }
    if s.reconcile_state != ReconcileState::New {
        running.cleared = running.cleared.add_fixed(amount)?;
    }
    if matches!(s.reconcile_state, ReconcileState::Reconciled | ReconcileState::Frozen) {
        running.reconciled = running.reconciled.add_fixed(amount)?;
    }
    if !t.is_closing() {
        running.noclosing = running.noclosing.add_fixed(amount)?;
    }
    Ok(true)
}

/// The account's splits with their running totals, and the account's totals after the last one.
fn walk(book: &Book, account: Guid) -> DomainResult<(Vec<RunningSplit<'_>>, Balances)> {
    book.account(account).ok_or_else(|| DomainError::not_found("account", &account))?;
    let scu = account_scu(book, account).ok();
    let mut running = Balances::ZERO;
    let splits = account_splits(book, account)
        .into_iter()
        .map(|(transaction, split)| {
            let amount = scu.map_or(Ok(split.amount), |scu| split.amount.to_denom(scu, Round::HalfUp))?;
            let counted = step(&mut running, transaction, split, amount)?;
            Ok(RunningSplit { transaction, split, amount, running: if counted { running } else { Balances::ZERO } })
        })
        .collect::<DomainResult<Vec<_>>>()?;
    Ok((splits, running))
}

/// An account's splits in split order with their running totals: `xaccAccountRecomputeBalance`.
pub fn running_balances(book: &Book, account: Guid) -> DomainResult<Vec<RunningSplit<'_>>> {
    walk(book, account).map(|(splits, _)| splits)
}

/// An account's totals (`xaccAccountGetBalance` and friends), or, with `before`, the running totals at its last split
/// posted strictly before that instant (`GetBalanceAsOfDate`, `5.17:libgnucash/engine/Account.cpp:3505-3518`): zero
/// when there is none.
pub fn balances(book: &Book, account: Guid, before: Option<Time64>) -> DomainResult<Balances> {
    let (splits, totals) = walk(book, account)?;
    Ok(match before {
        None => totals,
        Some(cut) => splits.iter().rev().find(|s| s.transaction.date_posted < cut).map_or(Balances::ZERO, |s| s.running),
    })
}

/// One total of an account as of `t`: the running total at its last split posted strictly before `t`, zero when there
/// is none. `xaccAccountGetBalanceAsOfDate`, `xaccAccountGetReconciledBalanceAsOfDate` and the static
/// `xaccAccountGetNoclosingBalanceAsOfDate` (`5.17:libgnucash/engine/Account.cpp:3520-3537`).
pub fn balance_as_of(book: &Book, account: Guid, t: Time64, total: Total) -> DomainResult<Numeric> {
    balances(book, account, Some(t)).map(|b| b.get(total))
}

/// One total of an account as of `t` in `commodity` (the account's own when `None`), with its descendants when
/// `include_children`: `xaccAccountGetBalanceAsOfDateInCurrency` and `xaccAccountGetNoclosingBalanceAsOfDateInCurrency`
/// (`5.17:libgnucash/engine/Account.cpp:3682-3696`, `:3746-3856`). Each account's balance is converted at the newest price at or
/// before `t` ([`PriceAt::Before`]); each descendant's is added rounded half up to the commodity's fraction, the
/// account's own is not. Zero for an account with no commodity and none given.
pub fn balance_as_of_in_commodity(
    book: &Book,
    account: Guid,
    t: Time64,
    total: Total,
    commodity: Option<&CommodityRef>,
    include_children: bool,
) -> DomainResult<Numeric> {
    let acc = book.account(account).ok_or_else(|| DomainError::not_found("account", &account))?;
    let Some(target) = commodity.or(acc.commodity.as_ref()).map(canonical) else { return Ok(Numeric::ZERO) };
    let own = converted_as_of(book, acc, t, total, &target)?;
    if !include_children {
        return Ok(own);
    }
    let fraction = commodity_fraction(book, &target).ok_or_else(|| DomainError::not_found("commodity", &target))?;
    book.descendants(account).iter().try_fold(own, |sum, sub| {
        let converted = converted_as_of(book, sub, t, total, &target)?;
        Ok(sum.add(converted, fraction, How::round(Round::HalfUp))?)
    })
}

/// `xaccAccountGetXxxBalanceAsOfDateInCurrency` (`:3634-3647`) and `xaccAccountConvertBalanceToCurrencyAsOfDate`
/// (`:3586-3606`): a zero balance, or one already in `target`, comes back unconverted.
fn converted_as_of(book: &Book, acc: &Account, t: Time64, total: Total, target: &CommodityRef) -> DomainResult<Numeric> {
    let balance = balance_as_of(book, acc.guid, t, total)?;
    match &acc.commodity {
        _ if balance.is_zero() => Ok(balance),
        Some(c) if canonical(c) == *target => Ok(balance),
        Some(c) => convert_balance(book, balance, c, target, PriceAt::Before(t)),
        // GnuCash finds no price for a missing commodity and multiplies by zero.
        None => Ok(Numeric::ZERO),
    }
}

/// How much one total of an account changed from `from` to `to`, in its own commodity, with its descendants when
/// `include_children`: the as-of balance at `to` less that at `from` (`xaccAccountGetBalanceChangeForPeriod` and
/// `xaccAccountGetNoclosingBalanceChangeForPeriod`, `5.17:libgnucash/engine/Account.cpp:3858-3878`). Splits posted at
/// `from` count, splits posted at `to` do not.
pub fn balance_change(book: &Book, account: Guid, from: Time64, to: Time64, total: Total, include_children: bool) -> DomainResult<Numeric> {
    let start = balance_as_of_in_commodity(book, account, from, total, None, include_children)?;
    let end = balance_as_of_in_commodity(book, account, to, total, None, include_children)?;
    Ok(end.sub_fixed(start)?)
}

/// `accumulate_splits_by_dates` (`5.17:bindings/engine.i:195-210`) in one pass: for each of `times`, in the order
/// given, `elt` of the last item posted at or before it, else what the previous time got, starting from `init`. The
/// items must be in split order. Times are meant to ascend; GnuCash's cursor only moves forward, and so does this one,
/// so a time earlier than the one before it repeats that one's result.
pub fn accumulate_to_dates<I, T: Clone>(
    items: &[I],
    posted: impl Fn(&I) -> Time64,
    times: &[Time64],
    init: T,
    mut elt: impl FnMut(&I) -> T,
) -> Vec<T> {
    let mut rest = items.iter().peekable();
    let mut result = init;
    times
        .iter()
        .map(|t| {
            while let Some(item) = rest.next_if(|i| posted(i) <= *t) {
                result = elt(item);
            }
            result.clone()
        })
        .collect()
}

/// One running total of an account at each of `times`: the total at its last split posted at or before the time, or
/// `None` before its first split. `gnc-account-accumulate-to-dates` with `xaccSplitGetBalance` or
/// `xaccSplitGetNoclosingBalance`, as `gnc:account-accumulate-at-dates` calls it
/// (`5.17:gnucash/report/report-utilities.scm:505-512`); see [`accumulate_to_dates`] for the order of `times`.
pub fn running_totals_at(book: &Book, account: Guid, times: &[Time64], total: Total) -> DomainResult<Vec<Option<Numeric>>> {
    let splits = running_balances(book, account)?;
    Ok(accumulate_to_dates(&splits, |s| s.transaction.date_posted, times, None, |s| Some(s.running.get(total))))
}

/// An account's balance at each of `times` as `gnc:account-get-balances-at-dates` adds it
/// (`5.17:gnucash/report/report-utilities.scm:478-488`): the exact sum of the amounts of its splits posted at or before
/// the time, zero before the first. Unlike the running total it adds stock splits GnuCash's engine skips.
pub fn balances_at_dates(book: &Book, account: Guid, times: &[Time64]) -> DomainResult<Vec<Numeric>> {
    let mut sum = Numeric::ZERO;
    let sums = running_balances(book, account)?
        .iter()
        .map(|s| {
            sum = sum.add_exact(s.amount)?;
            Ok((s.transaction.date_posted, sum))
        })
        .collect::<DomainResult<Vec<(Time64, Numeric)>>>()?;
    Ok(accumulate_to_dates(&sums, |(t, _)| *t, times, Numeric::ZERO, |(_, s)| *s))
}

/// Balances of an account and its sub-accounts, as `xaccAccountGetXxxBalanceInCurrencyRecursive` sums them
/// (`5.17:libgnucash/engine/Account.cpp:3669-3740`): each sub-account's balance is added rounded half up to the
/// commodity's fraction. GnuCash converts a sub-account in another commodity at the latest price; `account.balance`
/// adds nothing for it (as GnuCash does when no price exists) and reports its balances unconverted, by commodity.
pub fn balances_with_subaccounts(
    book: &Book,
    guid: Guid,
    before: Option<Time64>,
) -> DomainResult<(Balances, BTreeMap<CommodityRef, Balances>)> {
    let acc = book.account(guid).ok_or_else(|| DomainError::not_found("account", &guid))?;
    let own = balances(book, guid, before)?;
    let Some(commodity) = acc.commodity.clone() else { return Ok((own, BTreeMap::new())) };
    let fraction = commodity_fraction(book, &commodity).ok_or_else(|| DomainError::not_found("commodity", &commodity))?;
    let add = |a: Numeric, b: Numeric| a.add(b, fraction, How::round(Round::HalfUp));
    let mut unconverted: BTreeMap<CommodityRef, Balances> = BTreeMap::new();
    let total = book.descendants(guid).iter().try_fold(own, |total, sub| -> DomainResult<Balances> {
        let b = balances(book, sub.guid, before)?;
        let same = sub.commodity.as_ref() == Some(&commodity) || b.balance.is_zero() && b.cleared.is_zero() && b.reconciled.is_zero();
        let counted = if same {
            b
        } else {
            let other = sub.commodity.clone().unwrap_or_else(|| commodity.clone());
            let entry = unconverted.entry(other).or_insert(Balances::ZERO);
            *entry = Balances {
                balance: entry.balance.add_exact(b.balance)?,
                cleared: entry.cleared.add_exact(b.cleared)?,
                reconciled: entry.reconciled.add_exact(b.reconciled)?,
                noclosing: entry.noclosing.add_exact(b.noclosing)?,
            };
            Balances::ZERO
        };
        Ok(Balances {
            balance: add(total.balance, counted.balance)?,
            cleared: add(total.cleared, counted.cleared)?,
            reconciled: add(total.reconciled, counted.reconciled)?,
            noclosing: add(total.noclosing, counted.noclosing)?,
        })
    })?;
    Ok((total, unconverted))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use proptest::prelude::*;

    use super::*;
    use crate::ops::accounts::{AccountEdit, create_account};
    use crate::ops::commit::{NewSplit, NewTransaction, add_transaction};
    use crate::transaction::slot_paths;
    use crate::{AccountType, Date, FixedEnv, Price, SlotValue};

    struct Fx {
        book: Book,
        env: FixedEnv,
    }

    fn day(d: u32) -> Time64 {
        Date::from_ymd(2024, 1, d).unwrap().neutral_time()
    }

    fn usd(cents: i64) -> Numeric {
        Numeric::new(cents, 100)
    }

    impl Fx {
        fn new() -> Self {
            let mut env = FixedEnv::new(Time64(1_700_000_000), 0);
            let book = Book::new_empty(&mut env);
            Self { book, env }
        }

        fn add_in(&mut self, parent: Option<Guid>, name: &str, t: AccountType, currency: &str) -> Guid {
            let edit = AccountEdit {
                name: Some(name.into()),
                account_type: Some(t),
                commodity: Some(CommodityRef::currency(currency)),
                ..AccountEdit::default()
            };
            let parent = parent.unwrap_or(self.book.root);
            create_account(&mut self.book, &mut self.env, parent, &edit).unwrap()
        }

        fn add(&mut self, parent: Option<Guid>, name: &str, t: AccountType) -> Guid {
            self.add_in(parent, name, t, "USD")
        }

        fn post_in(&mut self, currency: &str, a: Guid, b: Guid, cents: i64, d: u32, num: Option<&str>) -> Guid {
            let mut t = NewTransaction::new(CommodityRef::currency(currency), Date::from_ymd(2024, 1, d).unwrap());
            t.num = num.map(Into::into);
            t.splits = vec![NewSplit::new(a, usd(cents)).with_action(num.unwrap_or_default()), NewSplit::new(b, usd(-cents))];
            add_transaction(&mut self.book, &mut self.env, t).unwrap()
        }

        fn post(&mut self, a: Guid, b: Guid, cents: i64, d: u32) -> Guid {
            self.post_in("USD", a, b, cents, d, None)
        }

        fn txn(&mut self, g: Guid) -> &mut Transaction {
            Arc::make_mut(self.book.transactions.get_mut(&g).unwrap())
        }

        fn price(&mut self, n: u8, commodity: &str, currency: &str, d: u32, value: Numeric) {
            let p = Price {
                guid: Guid::from_bytes([n; 16]),
                commodity: CommodityRef::currency(commodity),
                currency: CommodityRef::currency(currency),
                time: day(d),
                source: None,
                price_type: None,
                value,
            };
            self.book.prices.insert(p.guid, Arc::new(p));
        }
    }

    fn running(f: &Fx, account: Guid, total: Total) -> Vec<Numeric> {
        running_balances(&f.book, account).unwrap().iter().map(|s| s.running.get(total)).collect()
    }

    #[test]
    fn balances_follow_reconcile_states_and_dates() {
        let mut f = Fx::new();
        let bank = f.add(None, "Bank", AccountType::Bank);
        let sub = f.add(Some(bank), "Sub", AccountType::Bank);
        let food = f.add(None, "Food", AccountType::Expense);
        f.post(bank, food, 1000, 2);
        let t = f.post(bank, food, 250, 5);
        f.post(sub, food, 5, 6);
        f.txn(t).splits.iter_mut().for_each(|s| s.reconcile_state = ReconcileState::Reconciled);
        let all = balances(&f.book, bank, None).unwrap();
        assert_eq!((all.balance, all.cleared, all.reconciled), (usd(1250), usd(250), usd(250)));
        let before = balances(&f.book, bank, Some(Date::from_ymd(2024, 1, 5).unwrap().start_utc())).unwrap();
        assert_eq!(before.balance, usd(1000));
        let (total, other) = balances_with_subaccounts(&f.book, bank, None).unwrap();
        assert_eq!(total.balance, usd(1255));
        assert!(other.is_empty());
    }

    #[test]
    fn the_noclosing_total_leaves_closing_transactions_out() {
        let mut f = Fx::new();
        let income = f.add(None, "Income", AccountType::Income);
        let equity = f.add(None, "Equity", AccountType::Equity);
        let bank = f.add(None, "Bank", AccountType::Bank);
        f.post(bank, income, 500, 2);
        let closing = f.post(income, equity, 500, 3);
        f.txn(closing).slots.set(slot_paths::BOOK_CLOSING, SlotValue::Integer(1));
        f.post(bank, income, 200, 3);
        assert_eq!(running(&f, income, Total::Balance), [usd(-500), usd(-700), usd(-200)], "the closing transaction sorts last on its day");
        assert_eq!(running(&f, income, Total::Noclosing), [usd(-500), usd(-700), usd(-700)]);
        assert_eq!(balance_as_of(&f.book, income, day(4), Total::Noclosing).unwrap(), usd(-700));
        assert_eq!(balance_change(&f.book, income, day(2), day(4), Total::Noclosing, false).unwrap(), usd(-700), "posted at `from` counts");
        assert_eq!(balance_change(&f.book, income, day(3), day(4), Total::Balance, false).unwrap(), usd(300));
    }

    #[test]
    fn as_of_is_strictly_before_and_at_dates_is_at_or_before() {
        let mut f = Fx::new();
        let bank = f.add(None, "Bank", AccountType::Bank);
        let food = f.add(None, "Food", AccountType::Expense);
        f.post(bank, food, 100, 2);
        f.post(bank, food, 200, 4);
        assert_eq!(balance_as_of(&f.book, bank, day(4), Total::Balance).unwrap(), usd(100));
        assert!(balance_as_of(&f.book, bank, day(2), Total::Balance).unwrap().is_zero());
        let times = [day(1), day(2), day(3), day(4), day(9)];
        let at = running_totals_at(&f.book, bank, &times, Total::Balance).unwrap();
        assert_eq!(at, [None, Some(usd(100)), Some(usd(100)), Some(usd(300)), Some(usd(300))]);
        assert_eq!(balances_at_dates(&f.book, bank, &times).unwrap(), [Numeric::ZERO, usd(100), usd(100), usd(300), usd(300)]);
        let unsorted = [day(4), day(2), day(9)];
        let at = running_totals_at(&f.book, bank, &unsorted, Total::Balance).unwrap();
        assert_eq!(at, [Some(usd(300)); 3], "the cursor only moves forward");
    }

    #[test]
    fn same_day_running_totals_follow_the_num_field_source() {
        let mut f = Fx::new();
        let bank = f.add(None, "Bank", AccountType::Bank);
        let food = f.add(None, "Food", AccountType::Expense);
        f.post_in("USD", bank, food, 500, 2, Some("10"));
        f.post_in("USD", bank, food, 700, 2, Some("2"));
        assert_eq!(running(&f, bank, Total::Balance), [usd(700), usd(1200)], "num 2 before num 10");
        let actions = f.book.transactions.keys().copied().collect::<Vec<_>>();
        f.txn(actions[0]).splits[0].action = Some("1".into());
        f.txn(actions[1]).splits[0].action = Some("9".into());
        crate::book_options::set_split_action_for_num(&mut f.book, true);
        assert_eq!(running(&f, bank, Total::Balance), [usd(500), usd(1200)], "actions stand in for nums");
    }

    #[test]
    fn a_skipped_stock_split_reads_as_zero() {
        let mut f = Fx::new();
        let bank = f.add(None, "Bank", AccountType::Bank);
        let food = f.add(None, "Food", AccountType::Expense);
        f.post(bank, food, 1000, 2);
        let split = f.post(bank, food, -1000, 3);
        let s = f.txn(split).splits.iter_mut().find(|s| s.account == Some(bank)).unwrap();
        s.slots.set("split-type", SlotValue::String("stock-split".into()));
        f.post(bank, food, 500, 4);
        assert_eq!(running(&f, bank, Total::Balance), [usd(1000), Numeric::ZERO, usd(1500)], "it would make the balance zero");
        assert_eq!(balances(&f.book, bank, None).unwrap().balance, usd(1500));
        assert!(balance_as_of(&f.book, bank, day(4), Total::Balance).unwrap().is_zero());
        assert_eq!(balances_at_dates(&f.book, bank, &[day(3), day(4)]).unwrap(), [Numeric::ZERO, usd(500)], "the reports' sum adds it");
    }

    #[test]
    fn balances_in_a_commodity_convert_each_account_at_the_price_before() {
        let mut f = Fx::new();
        let assets = f.add(None, "Assets", AccountType::Asset);
        let bank = f.add(Some(assets), "Bank", AccountType::Bank);
        let euro = f.add_in(Some(assets), "Euro", AccountType::Bank, "EUR");
        let food = f.add(None, "Food", AccountType::Expense);
        let euro_food = f.add_in(None, "Euro food", AccountType::Expense, "EUR");
        f.post(bank, food, 1000, 2);
        f.post_in("EUR", euro, euro_food, 333, 2, None);
        f.price(1, "EUR", "USD", 3, Numeric::new(11, 10));
        f.price(2, "EUR", "USD", 6, Numeric::new(12, 10));
        let in_usd = |t, children| balance_as_of_in_commodity(&f.book, assets, t, Total::Balance, None, children).unwrap();
        assert!(in_usd(day(5), false).is_zero());
        assert_eq!(in_usd(day(5), true), usd(1366), "1000 + 3.33 × 1.1 = 3.663, rounded half up");
        assert_eq!(in_usd(day(7), true), usd(1400), "3.33 × 1.2 = 3.996");
        assert_eq!(in_usd(Time64(day(3).0 - 1), true), usd(1000), "no price before the time: nothing added");
        let eur = CommodityRef::currency("EUR");
        assert_eq!(balance_as_of_in_commodity(&f.book, euro, day(7), Total::Balance, Some(&eur), false).unwrap(), usd(333));
        assert_eq!(balance_change(&f.book, assets, day(5), day(7), Total::Balance, true).unwrap(), usd(34), "the price moved");
    }

    #[test]
    fn splits_between_dates_take_descendants_after_the_account() {
        let mut f = Fx::new();
        let assets = f.add(None, "Assets", AccountType::Asset);
        let bank = f.add(Some(assets), "Bank", AccountType::Bank);
        let food = f.add(None, "Food", AccountType::Expense);
        let a = f.post(bank, food, 100, 2);
        let b = f.post(assets, food, 200, 3);
        let c = f.post(bank, food, 300, 4);
        let found = |from, to, children| -> Vec<Guid> {
            account_splits_between(&f.book, assets, from, to, children).iter().map(|(t, _)| t.guid).collect()
        };
        assert_eq!(found(None, None, true), [b, a, c]);
        assert_eq!(found(Some(day(3)), Some(day(4)), true), [b, c], "both ends inclusive");
        assert_eq!(found(None, None, false), [b]);
    }

    proptest! {
        #[test]
        fn at_dates_agree_with_as_of_a_second_later(
            days in proptest::collection::vec((1u32..28, -5000i64..5000), 1..12),
            probes in proptest::collection::vec(0u32..30, 1..8),
        ) {
            let mut f = Fx::new();
            let bank = f.add(None, "Bank", AccountType::Bank);
            let food = f.add(None, "Food", AccountType::Expense);
            days.iter().filter(|(_, c)| *c != 0).for_each(|(d, c)| { f.post(bank, food, *c, *d); });
            let start = Date::from_ymd(2024, 1, 1).unwrap();
            let mut times: Vec<Time64> = probes.iter().map(|d| start.add_days(i64::from(*d)).neutral_time()).collect();
            times.sort();
            let at = running_totals_at(&f.book, bank, &times, Total::Balance).unwrap();
            let sums = balances_at_dates(&f.book, bank, &times).unwrap();
            times.iter().zip(at.iter().zip(sums)).for_each(|(t, (r, s))| {
                let as_of = balance_as_of(&f.book, bank, Time64(t.0 + 1), Total::Balance).unwrap();
                assert_eq!(r.unwrap_or(Numeric::ZERO), as_of);
                assert_eq!(s, as_of);
            });
        }
    }
}
