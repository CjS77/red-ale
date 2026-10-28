//! `test_basic`'s occurrence counts (`5.17:libgnucash/app-utils/test/test-sx.cpp:76-81`) and the note's hand-worked
//! case H10 (`design/notes/engine-scheduled-transactions.md` §9.3), worked from
//! `5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1087-1118`, `:1734-1976` and
//! `5.17:libgnucash/engine/SchedXaction.cpp:739-812`.

use std::sync::Arc;

use super::*;
use crate::ops::scheduled::fixture::{Fixture, T, daily, date, limited, monthly, usd};
use crate::{Guid, SlotValue};

fn flow(f: &Fixture, start: Date, end: Date) -> CashFlow {
    cash_flow(&f.book, start, end, NumberFormat::default()).unwrap()
}

/// `test_basic` (`test-sx.cpp:76-81`): a daily schedule from yesterday ending tomorrow occurs 0 times the day before
/// yesterday, once today, twice today and tomorrow, three times from yesterday to tomorrow, once tomorrow and never
/// after.
#[test]
fn test_basic_counts() {
    let today = date(2026, 4, 10);
    let mut s = daily(today.add_days(-1));
    s.end = Some(today.add_days(1));
    let count = |a: i64, b: i64| occurrences_in_range(&s, today.add_days(a), today.add_days(b)).unwrap();
    assert_eq!(
        [count(-2, -2), count(0, 0), count(0, 1), count(-1, 1), count(1, 1), count(2, 2)],
        [0, 1, 2, 3, 1, 0]
    );
}

/// `SchedXaction.cpp:756-760`, `:805-808`: the last occurrence is never counted again; a limit counts its last
/// occurrence (`:795-799`) and none once none remain (`:745-752`).
#[test]
fn counting_starts_after_the_last_occurrence() {
    let mut s = monthly(date(2026, 1, 1));
    s.last = Some(date(2026, 2, 1));
    assert_eq!(occurrences_in_range(&s, date(2026, 1, 1), date(2026, 4, 30)).unwrap(), 2);
    assert_eq!(occurrences_in_range(&s, date(2026, 2, 1), date(2026, 2, 28)).unwrap(), 0);
    let mut l = limited(monthly(date(2026, 1, 1)), 3, 3);
    assert_eq!(occurrences_in_range(&l, date(2025, 1, 1), date(2027, 1, 1)).unwrap(), 3);
    l.occurrences = Some(crate::Occurrences { total: 3, remaining: 0 });
    assert_eq!(occurrences_in_range(&l, date(2025, 1, 1), date(2027, 1, 1)).unwrap(), 0);
}

/// H10: a stored non-zero numeric is used as is; variables, `i` too, count as 0; each split adds count × (debit −
/// credit) to its account; a split in another commodity than the first split's adds 0 and says why; a disabled
/// scheduled transaction adds nothing.
#[test]
fn h10_cash_flow() {
    let mut f = Fixture::new();
    let (food, bank, euro) = (f.food, f.bank, f.euro);
    f.add(
        monthly(date(2026, 1, 1)),
        &usd(),
        &[&[T(food, "nonsense +", ""), T(bank, "", "100")]],
    );
    let template = f.book.templates.transactions[0].guid;
    let t = Arc::make_mut(&mut f.book.templates.transactions[&template]);
    t.splits[0].slots.set_in(
        &[crate::scheduled::SPLIT_FRAME, "debit-numeric"],
        SlotValue::Numeric(Numeric::new(10000, 100)),
    );
    let mut variables = monthly(date(2026, 1, 15));
    variables.guid = Guid::from_bytes([7; 16]);
    f.add(variables, &usd(), &[&[T(food, "amt + i + 5", ""), T(bank, "", "5")]]);
    let mut two = monthly(date(2026, 1, 20));
    two.guid = Guid::from_bytes([8; 16]);
    f.add(two, &usd(), &[&[T(food, "7", ""), T(euro, "", "7")]]);
    let mut off = monthly(date(2026, 1, 1));
    off.guid = Guid::from_bytes([9; 16]);
    off.enabled = false;
    f.add(off, &usd(), &[&[T(food, "1000", ""), T(bank, "", "1000")]]);

    let got = flow(&f, date(2026, 1, 1), date(2026, 3, 31));
    let sum = |a: Guid| got.amounts.get(&a).copied().map(Numeric::reduce);
    assert_eq!(sum(food), Some(Numeric::from_int(3 * 100 + 3 * 5 + 3 * 7)));
    assert_eq!(sum(bank), Some(Numeric::from_int(-(3 * 100 + 3 * 5))));
    assert_eq!(sum(euro), Some(Numeric::ZERO));
    assert_eq!(got.amounts.keys().copied().collect::<Vec<_>>(), [food, bank, euro]);
    assert!(matches!(&got.errors[..], [CashFlowError::NoExchangeRate { from, .. }] if from.mnemonic == "EUR"));
}

/// A Formula that fails even at 0 counts as 0 (`:1127-1139`); an unknown account stops its template transaction's
/// remaining splits (`:1822-1826`).
#[test]
fn failures_count_as_zero() {
    let mut f = Fixture::new();
    let (food, bank) = (f.food, f.bank);
    f.add(
        monthly(date(2026, 1, 1)),
        &usd(),
        &[&[T(food, "1 / i", ""), T(Guid::from_bytes([1; 16]), "", "5"), T(bank, "", "5")]],
    );
    let got = flow(&f, date(2026, 1, 1), date(2026, 1, 31));
    assert_eq!(got.amounts.get(&food).copied(), Some(Numeric::ZERO));
    assert!(!got.amounts.contains_key(&bank));
    assert!(matches!(
        &got.errors[..],
        [CashFlowError::Formula { side: Side::Debit, .. }, CashFlowError::Account(_)]
    ));
}
