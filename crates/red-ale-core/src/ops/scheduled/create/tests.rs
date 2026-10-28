//! The note's hand-worked case H6 (`design/notes/engine-scheduled-transactions.md` §9.3), worked from
//! `5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1087-1422`, `5.17:libgnucash/engine/Split.cpp:1243-1300` and
//! `5.17:libgnucash/engine/Scrub.cpp:236-347`, and `test_auto_create_transactions`
//! (`5.17:libgnucash/app-utils/test/test-sx.cpp:225-336`) without its Guile.

use std::sync::Arc;

use super::*;
use crate::book_options::set_trading_accounts;
use crate::formula::FormulaError;
use crate::ops::commit::check_committed;
use crate::ops::scheduled::fixture::{Fixture, T, date, monthly, usd};
use crate::ops::scheduled::{AccountProblem, InstanceState, since_last_run};
use crate::transaction::slot_paths;

fn i(n: i64) -> Variables {
    Variables::from([("i".to_owned(), Numeric::from_int(n))])
}

fn with(mut v: Variables, name: &str, value: Numeric) -> Variables {
    v.insert(name.to_owned(), value);
    v
}

fn create(f: &mut Fixture, sx: Guid, bindings: &Variables) -> Result<Vec<Created>, ScheduledError> {
    create_instance(&mut f.book, &mut f.env, sx, date(2026, 2, 1), bindings, NumberFormat::default())
}

fn values(f: &Fixture, created: &Created) -> Vec<(Guid, Numeric, Numeric)> {
    let t = &f.book.transactions[&created.transaction];
    t.splits.iter().map(|s| (s.account.unwrap(), s.value, s.amount)).collect()
}

fn n(num: i64, denom: i64) -> Numeric {
    Numeric::new(num, denom)
}

/// H6 "plain": 100 from Food to Bank, with num, notes, memos and an action, gives a transaction with the template's
/// fields, posted at the Day-neutral time with `date-posted`, entered now, and `from-sched-xaction` as its only other
/// Slot (`gnc-sx-instance-model.c:1328-1409`; `Transaction.cpp:1913-1950`).
#[test]
fn a_plain_instance_copies_the_template() {
    let mut f = Fixture::new();
    let (food, bank) = (f.food, f.bank);
    let g = f.add(monthly(date(2026, 1, 1)), &usd(), &[&[T(bank, "", "100"), T(food, "100", "")]]);
    let template = *f.book.templates.transactions.keys().next().unwrap();
    let t = Arc::make_mut(&mut f.book.templates.transactions[&template]);
    t.num = Some("CHK".into());
    t.slots.set(slot_paths::NOTES, SlotValue::String("monthly".into()));
    t.slots.set("other", SlotValue::String("not copied".into()));
    t.splits[0].memo = Some("from bank".into());
    t.splits[1].action = Some("Buy".into());
    let created = create(&mut f, g, &i(1)).unwrap();
    assert_eq!(created.len(), 1);
    assert!(created[0].imbalance.is_none());
    let t = &f.book.transactions[&created[0].transaction];
    assert_eq!(
        (t.num.as_deref(), t.description.as_deref(), t.notes()),
        (Some("CHK"), Some("Rent"), Some("monthly"))
    );
    assert_eq!(t.date_posted.to_file_string(), "2026-02-01 10:59:00 +0000");
    assert_eq!(t.slots.get(slot_paths::DATE_POSTED), Some(&SlotValue::Date(date(2026, 2, 1))));
    assert_eq!(t.date_entered, f.env.now);
    assert_eq!(t.slots.get(FROM_SCHEDULED), Some(&SlotValue::Guid(g)));
    assert_eq!(t.slots.iter().count(), 3, "date-posted, notes and from-sched-xaction only");
    assert_eq!(
        values(&f, &created[0]),
        [(food, n(10000, 100), n(10000, 100)), (bank, n(-10000, 100), n(-10000, 100))]
    );
    assert_eq!(
        (t.splits[0].action.as_deref(), t.splits[1].memo.as_deref()),
        (Some("Buy"), Some("from bank"))
    );
    assert!(
        t.splits
            .iter()
            .all(|s| s.slots.is_empty() && s.lot.is_none() && s.reconcile_state == ReconcileState::New)
    );
}

/// H6 "variables": `amt * 2 + i` with `amt` = 12.5 is 26 for the first instance and 27 for the second; with
/// instance count 0 in the file `i` is 0 (`gnc-sx-instance-model.c:443-456`).
#[test]
fn variables_and_i_enter_the_formulas() {
    let mut f = Fixture::new();
    let (food, bank) = (f.food, f.bank);
    let g = f.add(
        monthly(date(2026, 1, 1)),
        &usd(),
        &[&[T(food, "amt * 2 + i", ""), T(bank, "", "amt * 2 + i")]],
    );
    let amt = n(125, 10);
    [(1, 2600), (2, 2700), (0, 2500)].into_iter().for_each(|(count, cents)| {
        let created = create(&mut f, g, &with(i(count), "amt", amt)).unwrap();
        assert_eq!(values(&f, &created[0])[0].1, n(cents, 100), "i = {count}");
    });
}

/// H6 "rounding": debit 1.005 against credit 1 rounds to 1.01 and -1.00, and the commit adds an Imbalance-USD split of
/// -0.01, creating the account (`Split.cpp:1280-1300`, `Transaction.cpp:1549-1558`).
#[test]
fn rounding_leaves_an_imbalance_split() {
    let mut f = Fixture::new();
    let (food, bank) = (f.food, f.bank);
    let g = f.add(monthly(date(2026, 1, 1)), &usd(), &[&[T(food, "1.005", ""), T(bank, "", "1")]]);
    let built = build_instance(&f.book, f.sx(g), &i(1), NumberFormat::default()).unwrap();
    assert_eq!(built[0].imbalance, Some(n(-1, 100)));
    let created = create(&mut f, g, &i(1)).unwrap();
    let imbalance = created[0].imbalance.unwrap();
    assert_eq!(imbalance.value, n(-1, 100));
    assert_eq!(f.book.account(imbalance.account).map(|a| a.name.as_str()), Some("Imbalance-USD"));
    let t = &f.book.transactions[&created[0].transaction];
    assert!(t.is_balanced());
    assert_eq!(
        t.splits.iter().map(|s| s.value).collect::<Vec<_>>(),
        [n(101, 100), n(-100, 100), n(-1, 100)]
    );
}

/// H6 "thirds" (10/3 each side gives 3.33 and -3.33, balanced) and "marker" (a split with no formula is created with
/// value 0, `gnc-sx-instance-model.c:1169-1194`).
#[test]
fn thirds_balance_and_markers_are_zero() {
    let mut f = Fixture::new();
    let (food, bank, euro) = (f.food, f.bank, f.euro);
    let g = f.add(
        monthly(date(2026, 1, 1)),
        &usd(),
        &[&[T(food, "10/3", ""), T(bank, "", "10/3"), T(euro, "", "")]],
    );
    let created = create(&mut f, g, &i(1)).unwrap();
    assert!(created[0].imbalance.is_none());
    let got = values(&f, &created[0]);
    assert_eq!(
        [got[0], got[2]],
        [(food, n(333, 100), n(333, 100)), (bank, n(-333, 100), n(-333, 100))]
    );
    assert_eq!(
        (got[1].0, got[1].1.is_zero(), got[1].2.is_zero()),
        (euro, true, true),
        "debits first, zero counted as one"
    );
}

/// H6 "EUR": 100 USD into EUR at `EUR -> USD` = 0.9 is value -100.00 and amount -90.00 EUR; "stock": 100 USD of ACME
/// at `ACME -> USD` = 3 is 33.3333 shares (`split_apply_exchange_rate`, `gnc-sx-instance-model.c:1196-1231`).
#[test]
fn other_commodities_take_the_exchange_rate() {
    let mut f = Fixture::new();
    let (food, euro, shares, bank) = (f.food, f.euro, f.shares, f.bank);
    let g = f.add(
        monthly(date(2026, 1, 1)),
        &usd(),
        &[
            &[T(food, "100", ""), T(euro, "", "100")],
            &[T(shares, "100", ""), T(bank, "", "100")],
        ],
    );
    let bindings = with(with(i(1), "EUR -> USD", n(9, 10)), "ACME -> USD", Numeric::from_int(3));
    let created = create(&mut f, g, &bindings).unwrap();
    assert_eq!(values(&f, &created[0])[1], (euro, n(-10000, 100), n(-9000, 100)));
    assert_eq!(values(&f, &created[1])[0], (shares, n(10000, 100), n(333_333, 10_000)));
}

/// A rate of 0 for a split with a value is refused, where GnuCash leaves an amount of 0 (`gnc-sx-instance-model.c:1218-1224`,
/// `Split.cpp:1246`); with no rate bound, 1 is used (`:1204-1215`).
#[test]
fn a_zero_rate_is_refused_and_a_missing_one_is_one() {
    let mut f = Fixture::new();
    let (food, euro) = (f.food, f.euro);
    let g = f.add(monthly(date(2026, 1, 1)), &usd(), &[&[T(food, "100", ""), T(euro, "", "100")]]);
    let zero = with(i(1), "EUR -> USD", Numeric::ZERO);
    assert!(matches!(
        create(&mut f, g, &zero),
        Err(ScheduledError::Refused {
            reason: Refusal::ZeroRate,
            ..
        })
    ));
    let created = create(&mut f, g, &i(1)).unwrap();
    assert_eq!(values(&f, &created[0])[1].2, n(-10000, 100));
}

/// Deviation (b): a failing Formula refuses the instance, naming the split and side, where GnuCash creates the
/// transaction with 0 (`gnc-sx-instance-model.c:1127-1139`); so do an unknown function and an unbound variable.
#[test]
fn a_failing_formula_creates_nothing() {
    let mut f = Fixture::new();
    let (food, bank) = (f.food, f.bank);
    let g = f.add(
        monthly(date(2026, 1, 1)),
        &usd(),
        &[&[T(food, "1 / 0", ""), T(bank, "", "nosuch(1)")]],
    );
    let err = create(&mut f, g, &i(1)).unwrap_err();
    let ScheduledError::Formula(failure) = err else { panic!("{err:?}") };
    assert_eq!((failure.side, failure.formula.as_str()), (Side::Debit, "1 / 0"));
    assert!(matches!(failure.error, FormulaError::Arithmetic(_)));
    assert_eq!(failure.site.split, f.book.templates.transactions[0].splits[0].guid);
    assert!(f.book.transactions.is_empty());
}

/// red-ale's commit rules hold for every door: a placeholder or unknown account refuses the instance, where GnuCash
/// creates the split or cancels with "Unknown account" (`gnc-sx-instance-model.c:1060-1085`).
#[test]
fn placeholder_and_unknown_accounts_are_refused() {
    let mut f = Fixture::new();
    let (food, bank) = (f.food, f.bank);
    let g = f.add(monthly(date(2026, 1, 1)), &usd(), &[&[T(food, "5", ""), T(bank, "", "5")]]);
    Arc::make_mut(&mut f.book.accounts[&food]).set_placeholder(true);
    let err = create(&mut f, g, &i(1)).unwrap_err();
    assert!(matches!(err, ScheduledError::Account { problem: AccountProblem::Placeholder(a), .. } if a == food));
    f.book.accounts.shift_remove(&food);
    let err = create(&mut f, g, &i(1)).unwrap_err();
    assert!(matches!(err, ScheduledError::Account { problem: AccountProblem::Missing(Some(a)), .. } if a == food));
}

/// `get_transaction_currency` (`gnc-sx-instance-model.c:1245-1308`): a template currency in none of the splits gives
/// way to the first split currency; a transaction whose splits are all in a stock would be in that stock, which red-ale
/// refuses.
#[test]
fn the_transaction_currency_comes_from_the_splits() {
    let mut f = Fixture::new();
    let (food, bank, shares) = (f.food, f.bank, f.shares);
    let eur = crate::CommodityRef::currency("EUR");
    let g = f.add(monthly(date(2026, 1, 1)), &eur, &[&[T(food, "5", ""), T(bank, "", "5")]]);
    let built = build_instance(&f.book, f.sx(g), &i(1), NumberFormat::default()).unwrap();
    assert_eq!(built[0].currency, usd());
    let mut s = monthly(date(2026, 1, 1));
    s.guid = Guid::from_bytes([9; 16]);
    let stock = f.add(s, &eur, &[&[T(shares, "5", ""), T(shares, "", "5")]]);
    let err = build_instance(&f.book, f.sx(stock), &i(1), NumberFormat::default()).unwrap_err();
    assert!(matches!(
        err,
        ScheduledError::Refused {
            reason: Refusal::NotACurrency,
            ..
        }
    ));
}

/// `test_auto_create_transactions` (`test-sx.cpp:225-336`): "123"/"123", "0"/"0", ""/"" and ""/"" with the template
/// currency set each create one transaction. A template in red-ale always has a currency; with only marker splits it is
/// the transaction's (`gnc-sx-instance-model.c:1300-1307`).
#[test]
fn every_test_sx_template_creates_one_transaction() {
    [("123", "123", false), ("0", "0", false), ("", "", false), ("", "", true)]
        .into_iter()
        .for_each(|(debit, credit, txcurr)| {
            let mut f = Fixture::new();
            let (food, bank) = (f.food, f.bank);
            let currency = if txcurr { usd() } else { crate::CommodityRef::currency("EUR") };
            let g = f.add(monthly(date(2026, 1, 1)), &currency, &[&[T(food, debit, ""), T(bank, "", credit)]]);
            let created = create(&mut f, g, &i(1)).unwrap();
            assert_eq!(created.len(), 1, "{debit:?}/{credit:?}");
            let t = &f.book.transactions[&created[0].transaction];
            assert!(t.is_balanced());
            assert_eq!(t.currency, if debit.is_empty() { currency } else { usd() });
        });
}

/// With trading accounts on, the commit gives a two-currency instance its trading splits, as every commit does.
#[test]
fn trading_accounts_balance_each_commodity() {
    let mut f = Fixture::new();
    set_trading_accounts(&mut f.book, true);
    let (food, euro) = (f.food, f.euro);
    let g = f.add(monthly(date(2026, 1, 1)), &usd(), &[&[T(food, "100", ""), T(euro, "", "100")]]);
    let created = create(&mut f, g, &with(i(1), "EUR -> USD", n(9, 10))).unwrap();
    let t = &f.book.transactions[&created[0].transaction];
    assert_eq!(t.splits.len(), 4);
    check_committed(&f.book, t).unwrap();
}

/// The proposal shows what creating an instance would make once every variable has a value, and the problems found
/// without the caller's values.
#[test]
fn the_proposal_previews_and_reports() {
    let mut f = Fixture::new();
    let (food, bank) = (f.food, f.bank);
    f.add(monthly(date(2026, 1, 1)), &usd(), &[&[T(food, "1.005", ""), T(bank, "", "1")]]);
    let p = since_last_run(&f.book, date(2026, 1, 1), NumberFormat::default()).unwrap();
    let instance = &p.schedules[0].instances[0];
    assert_eq!(instance.state, InstanceState::ToCreate);
    let preview = instance.transactions.as_ref().unwrap();
    assert_eq!((preview[0].splits.len(), preview[0].imbalance), (2, Some(n(-1, 100))));
    Arc::make_mut(&mut f.book.accounts[&bank]).set_placeholder(true);
    let p = since_last_run(&f.book, date(2026, 1, 1), NumberFormat::default()).unwrap();
    let instance = &p.schedules[0].instances[0];
    assert!(instance.transactions.is_none());
    assert!(matches!(&instance.errors[..], [ScheduledError::Account { index: Some(0), .. }]));
    assert_eq!(p.summary().errors, 1);
}
