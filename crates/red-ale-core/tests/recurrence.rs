//! Recurrence arithmetic against GnuCash's own tests (`5.17:libgnucash/engine/test/test-recurrence.c`, GPL-2.0-or-later,
//! Copyright (C) 2005, Chris Shoemaker <c.shoemaker@cox.net>), the worked sequences of
//! `design/notes/engine-scheduled-transactions.md` §2.6 and §9.1, and the budget periods of
//! `design/notes/engine-budgets.md` §1.4.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use red_ale_core::recurrence::next_instance_of_list;
use red_ale_core::time::days_in_month;
use red_ale_core::{Date, PeriodType, Recurrence, RecurrenceError, TimeZone, WeekendAdjust};

use PeriodType as P;
use WeekendAdjust as W;

const FRIDAY: u32 = 5;
const SATURDAY: u32 = 6;
const SUNDAY: u32 = 7;
const MONDAY: u32 = 1;

fn d(s: &str) -> Date {
    Date::parse(s).unwrap()
}

/// `g_date_set_dmy(d, day, month, year)` for years below 1000 too, which `Date::parse` would need padded.
fn dmy(day: u32, month: u32, year: i32) -> Date {
    Date::from_ymd(year, month, day).unwrap()
}

/// GDate's Julian day: 1 is 0001-01-01.
fn julian(j: i64) -> Date {
    dmy(1, 1, 1).add_days(j - 1)
}

fn is_last_of_month(d: Date) -> bool {
    d.day() == days_in_month(d.year(), d.month())
}

/// How `check_valid` compares month and year phases.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    /// As GnuCash does, with C's `%`: a weekend move back into the month before is allowed only as a difference of -1,
    /// and a year never moves. Holds over `test_all`'s ranges, where every reference is before the start.
    GnuCash,
    /// Modulo the multiplier, so a move back is a difference of `mult - 1`, and a year may move as its month does.
    /// GnuCash's own results need this past the start: monthly x2 `back` from Tuesday 2002-10-01, after 2002-10-01, is
    /// Friday 2002-11-29 (Sunday 2002-12-01 moved back), a difference of 1.
    Congruent,
}

/// Whether a difference of `diff` months or years is in phase with `mult`, allowing a weekend move by one.
fn in_phase(diff: i64, mult: i64, wadj: WeekendAdjust, phase: Phase) -> bool {
    let (diff, back, forward) = match phase {
        Phase::GnuCash => (diff % mult, -1, 1),
        Phase::Congruent => (diff.rem_euclid(mult), mult - 1, 1 % mult),
    };
    diff == 0 || (diff == back && wadj == W::Back) || (diff == forward && wadj == W::Forward)
}

/// `check_valid` (`test-recurrence.c:33-202`): the next date is strictly after the reference and in the start's phase,
/// allowing the weekend moves.
fn check_valid(next: Option<Date>, reference: Date, start: Date, mult: u16, pt: PeriodType, wadj: WeekendAdjust, phase: Phase) -> Result<(), String> {
    let expect_none = pt == P::Once && start <= reference;
    let Some(next) = next else {
        return if expect_none { Ok(()) } else { Err("incorrectly invalid".into()) };
    };
    if expect_none {
        return Err("incorrectly valid".into());
    }
    let ensure = |ok: bool, what: &str| if ok { Ok(()) } else { Err(format!("{what}: next {next}")) };
    ensure(reference < next, "next date not strictly later than ref date")?;
    let start_to_next = start.days_until(next);
    let mult = i64::from(mult);
    match pt {
        P::Day | P::Week => ensure(start_to_next % (if pt == P::Week { 7 * mult } else { mult }) == 0, "week or day period phase wrong"),
        P::Once => ensure(start_to_next == 0, "period once not on start date"),
        _ => check_month_phase(next, start, mult, pt, wadj, phase).map_err(|e| format!("{e}: next {next}")),
    }
}

/// `check_valid`'s month-like cases (`test-recurrence.c:55-187`).
fn check_month_phase(next: Date, start: Date, mult: i64, pt: PeriodType, wadj: WeekendAdjust, phase: Phase) -> Result<(), &'static str> {
    let ensure = |ok: bool, what: &'static str| if ok { Ok(()) } else { Err(what) };
    let mut mult = mult;
    if pt == P::Year {
        let diff = i64::from(next.year()) - i64::from(start.year());
        let ok = if phase == Phase::GnuCash { diff % mult == 0 } else { in_phase(diff, mult, wadj, phase) };
        ensure(ok, "year period phase wrong")?;
        mult *= 12;
    }
    if pt == P::EndOfMonth && !is_last_of_month(next) {
        let near = |days: [i64; 2]| days.iter().any(|&k| is_last_of_month(next.add_days(k)));
        ensure(
            match wadj {
                W::None => false,
                W::Back => near([1, 2]),
                W::Forward => near([-1, -2]),
            },
            "end of month phase wrong",
        )?;
    }
    let monthdiff = i64::from(next.month()) - i64::from(start.month()) + 12 * (i64::from(next.year()) - i64::from(start.year()));
    ensure(in_phase(monthdiff, mult, wadj, phase), "month or year phase wrong")?;
    if matches!(pt, P::NthWeekday | P::LastWeekday) {
        ensure(next.weekday() == start.weekday(), "weekday phase wrong")?;
        let (sweek, nweek) = ((start.day() - 1) / 7, (next.day() - 1) / 7);
        let short = sweek == 4 && nweek == 3 && next.day() + 7 > days_in_month(next.year(), next.month());
        return ensure(sweek == nweek || short || (sweek == 3 && nweek == 4 && pt == P::LastWeekday), "week of month phase wrong");
    }
    let (day_start, day_next, weekday) = (start.day(), next.day(), next.weekday());
    let weekday_ok = weekday != SATURDAY && weekday != SUNDAY;
    let moved = |to: u32, by: [i64; 2]| weekday == to && by.iter().any(|&k| next.add_days(k).day() == day_start);
    if day_start < 28 {
        let ok = match wadj {
            W::None => day_start == day_next,
            W::Back => weekday_ok && (day_start == day_next || moved(FRIDAY, [1, 2])),
            W::Forward => weekday_ok && (day_start == day_next || moved(MONDAY, [-1, -2])),
        };
        ensure(ok, "dom don't match")
    } else if pt != P::EndOfMonth && wadj == W::None {
        ensure(day_start == day_next || is_last_of_month(next), "dom don't match and next is not eom")
    } else {
        Ok(())
    }
}

/// `test_specific` (`test-recurrence.c:270-297`): made by `recurrenceSet` with no weekend adjustment.
fn specific(pt: PeriodType, mult: u16, start: (u32, u32, i32), reference: (u32, u32, i32), expected: (u32, u32, i32)) {
    let [start, reference, expected] = [start, reference, expected].map(|(m, dd, y)| dmy(dd, m, y));
    let r = Recurrence::normalised(mult, pt, start, W::None);
    let next = r.next_instance(reference).unwrap();
    check_valid(next, reference, start, mult, pt, W::None, Phase::GnuCash).unwrap();
    assert_eq!(next, Some(expected), "{pt} x{mult} from {start} after {reference}");
}

/// `test_some` (`test-recurrence.c:361-399`), case by case; arguments are (month, day, year) as GnuCash writes them.
#[test]
fn gnucash_test_some() {
    specific(P::NthWeekday, 1, (4, 1, 2005), (4, 2, 2005), (5, 6, 2005)); // :363
    specific(P::NthWeekday, 1, (7, 14, 2005), (11, 15, 2005), (12, 8, 2005)); // :364
    specific(P::NthWeekday, 1, (7, 14, 2005), (11, 5, 2005), (11, 10, 2005)); // :365
    specific(P::NthWeekday, 1, (4, 1, 2005), (4, 2, 2005), (5, 6, 2005)); // :366
    specific(P::NthWeekday, 1, (4, 1, 2005), (4, 2, 2005), (5, 6, 2005)); // :367
    specific(P::LastWeekday, 1, (4, 29, 2005), (4, 30, 2005), (5, 27, 2005)); // :369
    specific(P::LastWeekday, 1, (4, 29, 2005), (5, 1, 2005), (5, 27, 2005)); // :370
    specific(P::LastWeekday, 1, (4, 29, 2005), (7, 9, 2005), (7, 29, 2005)); // :371
    specific(P::LastWeekday, 1, (4, 29, 2005), (6, 30, 2005), (7, 29, 2005)); // :372
    specific(P::LastWeekday, 1, (4, 29, 2005), (7, 31, 2005), (8, 26, 2005)); // :373
    specific(P::NthWeekday, 2, (4, 27, 2005), (4, 27, 2005), (6, 22, 2005)); // :375
    specific(P::Year, 3, (9, 8, 838), (6, 30, 1094), (9, 8, 1096)); // :378
    specific(P::Year, 2, (9, 8, 838), (6, 30, 1094), (9, 8, 1094)); // :379
    specific(P::Year, 1, (1, 10, 1000), (1, 5, 1002), (1, 10, 1002)); // :380
    specific(P::Month, 1, (1, 12, 1), (2, 6, 1), (2, 12, 1)); // :382
    specific(P::Month, 1, (1, 12, 1), (2, 12, 1), (3, 12, 1)); // :384
    specific(P::Month, 1, (1, 12, 1), (2, 20, 1), (3, 12, 1)); // :385
    specific(P::Month, 1, (1, 30, 1), (2, 28, 1), (3, 30, 1)); // :386
    specific(P::Month, 1, (1, 30, 1), (2, 27, 1), (2, 28, 1)); // :387
    specific(P::Month, 1, (2, 28, 1), (3, 30, 1), (4, 28, 1)); // :388
    specific(P::EndOfMonth, 1, (2, 28, 1), (3, 30, 1), (3, 31, 1)); // :390
    specific(P::EndOfMonth, 5, (4, 30, 1), (4, 21, 1), (4, 30, 1)); // :391
    specific(P::EndOfMonth, 5, (2, 28, 1), (5, 21, 1), (7, 31, 1)); // :392
    specific(P::Year, 7, (6, 8, 199), (9, 10, 1338), (6, 8, 1340)); // :394
    specific(P::Year, 2, (9, 8, 838), (6, 30, 1094), (9, 8, 1094)); // :395
    specific(P::Year, 1, (5, 2, 13), (1, 11, 101), (5, 2, 101)); // :397
    specific(P::Day, 7, (4, 1, 2000), (4, 8, 2000), (4, 15, 2000)); // :398
}

/// `test_all`'s check on one start and reference (`test-recurrence.c:223-247`): every period type, weekend adjustment
/// and multiplier 0-9 through `recurrenceSet`, then `check_valid` against the normalised fields.
fn check_all_types(start: Date, reference: Date, phase: Phase) -> Result<(), String> {
    let combos = P::ALL.into_iter().flat_map(|pt| [W::None, W::Back, W::Forward].into_iter().flat_map(move |w| (0..10).map(move |m| (pt, w, m))));
    combos.into_iter().try_for_each(|(pt, wadj, mult)| {
        let r = Recurrence::normalised(mult, pt, start, wadj);
        let next = r.next_instance(reference).map_err(|e| e.to_string())?;
        check_valid(next, reference, r.start(), r.mult(), r.period_type(), r.weekend_adjust(), phase)
            .map_err(|e| format!("{e} ({pt} x{mult} {wadj:?} from {start} after {reference})"))
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// `test_all` (`test-recurrence.c:204-252`) over its ranges: starts at Julian days 2003·365 to 2003·365 + 299
    /// (2002-09-02 to 2003-06-28), references random in 1 to 2^19 (0001-01-01 to 1436-06-14), all before the start.
    #[test]
    fn gnucash_test_all(j1 in 731_095_i64..731_395, j2 in 1_i64..=(1 << 19)) {
        prop_assert_eq!(check_all_types(julian(j1), julian(j2), Phase::GnuCash), Ok(()));
    }

    /// The same invariants, with phases compared modulo the multiplier, for references from a week before the start to
    /// eight years after it, where the arithmetic past the start runs.
    #[test]
    fn test_all_invariants_after_the_start(j1 in 731_095_i64..731_395, offset in -7_i64..3000) {
        prop_assert_eq!(check_all_types(julian(j1), julian(j1 + offset), Phase::Congruent), Ok(()));
    }
}

/// Occurrences from the day before the start, as a scheduled transaction asks (`5.17:libgnucash/engine/SchedXaction.cpp:885-926`).
fn first(r: Recurrence, count: usize) -> Vec<String> {
    std::iter::successors(r.next_instance(r.start().add_days(-1)).unwrap(), |prev| r.next_instance(*prev).unwrap())
        .take(count)
        .map(Date::to_file_string)
        .collect()
}

fn dates(list: &str) -> Vec<String> {
    list.split_whitespace().map(str::to_owned).collect()
}

/// `design/notes/engine-scheduled-transactions.md` §2.6, made by `recurrenceSet`.
#[test]
fn worked_sequences() {
    let set = |mult, pt, start, wadj| Recurrence::normalised(mult, pt, d(start), wadj);
    let month_31 = first(set(1, P::Month, "2025-01-31", W::None), 14);
    assert_eq!(month_31[..6], dates("2025-01-31 2025-02-28 2025-03-31 2025-04-30 2025-05-31 2025-06-30"));
    assert_eq!(month_31[13], "2026-02-28");
    let cases = [
        (set(2, P::Month, "2025-01-31", W::None), "2025-01-31 2025-03-31 2025-05-31 2025-07-31 2025-09-30 2025-11-30 2026-01-31"),
        (set(1, P::EndOfMonth, "2025-01-15", W::None), "2025-01-31 2025-02-28 2025-03-31 2025-04-30"),
        (set(1, P::NthWeekday, "2025-01-08", W::None), "2025-01-08 2025-02-12 2025-03-12 2025-04-09 2025-05-14"),
        (set(1, P::NthWeekday, "2025-01-29", W::None), "2025-01-29 2025-02-26 2025-03-26 2025-04-30 2025-05-28"),
        (set(1, P::NthWeekday, "2025-01-28", W::None), "2025-01-28 2025-02-25 2025-03-25 2025-04-22 2025-05-27"),
        (set(1, P::LastWeekday, "2025-01-10", W::None), "2025-01-31 2025-02-28 2025-03-28 2025-04-25 2025-05-30"),
        (set(1, P::Month, "2025-03-01", W::Back), "2025-04-01 2025-05-01 2025-05-30 2025-07-01"),
        (set(1, P::Month, "2025-03-01", W::Forward), "2025-03-03 2025-04-01 2025-05-01 2025-06-02"),
        (set(2, P::Week, "2025-01-01", W::None), "2025-01-01 2025-01-15 2025-01-29 2025-02-12"),
        (set(3, P::Day, "2025-01-30", W::None), "2025-01-30 2025-02-02 2025-02-05"),
        (set(0, P::Once, "2025-03-15", W::None), "2025-03-15"),
    ];
    cases.iter().for_each(|(r, expected)| assert_eq!(first(*r, 20)[..dates(expected).len()], dates(expected), "{r:?}"));
    assert_eq!(first(set(0, P::Once, "2025-03-15", W::None), 20).len(), 1);
    let eom_back = first(set(1, P::EndOfMonth, "2025-01-31", W::Back), 8);
    assert_eq!(eom_back[4..8], dates("2025-05-30 2025-06-30 2025-07-31 2025-08-29"));
    let eom_forward = first(set(1, P::EndOfMonth, "2025-01-31", W::Forward), 8);
    assert_eq!(eom_forward[3..8], dates("2025-04-30 2025-06-02 2025-06-30 2025-07-31 2025-09-01"));
    // A Saturday start with `back` is skipped by a scheduled transaction (§2.4).
    assert_eq!(first(set(1, P::Month, "2025-05-31", W::Back), 1), dates("2025-06-30"));
    assert_eq!(first(set(1, P::Year, "2025-03-01", W::Back), 1), dates("2026-02-27"));
    assert_eq!(first(set(1, P::Month, "2025-05-31", W::Forward), 1), dates("2025-06-02"));
}

/// Stored fields that `recurrenceSet` would never make, computed as GnuCash computes them (note §2.1, §9.1).
#[test]
fn stored_fields_are_not_normalised() {
    let raw = |mult, pt, start, wadj| Recurrence::new(mult, pt, d(start), wadj).unwrap();
    assert_eq!(first(raw(1, P::EndOfMonth, "2025-01-15", W::None), 3), dates("2025-01-15 2025-01-31 2025-02-28"));
    assert_eq!(first(raw(1, P::NthWeekday, "2025-01-29", W::None), 4)[3], "2025-04-23");
    assert_eq!(first(raw(1, P::LastWeekday, "2025-01-10", W::None), 4), dates("2025-01-10 2025-01-17 2025-02-14 2025-03-14"));
    assert_eq!(first(raw(1, P::Week, "2025-03-01", W::Back), 3), first(raw(1, P::Week, "2025-03-01", W::None), 3));
    assert_eq!(raw(1, P::Month, "2025-01-15", W::None).next_instance(d("2025-06-20")), Ok(Some(d("2025-07-15"))));
}

#[test]
fn normalisation_follows_recurrence_set() {
    let set = |mult, pt, start, wadj| {
        let r = Recurrence::normalised(mult, pt, d(start), wadj);
        (r.mult(), r.period_type(), r.start().to_file_string(), r.weekend_adjust())
    };
    assert_eq!(set(0, P::Month, "2025-01-15", W::Back), (1, P::Month, "2025-01-15".into(), W::Back));
    assert_eq!(set(5, P::Once, "2025-01-15", W::Back), (0, P::Once, "2025-01-15".into(), W::None));
    assert_eq!(set(1, P::EndOfMonth, "2024-02-10", W::Forward), (1, P::EndOfMonth, "2024-02-29".into(), W::Forward));
    assert_eq!(set(1, P::LastWeekday, "2025-01-03", W::Back), (1, P::LastWeekday, "2025-01-31".into(), W::None));
    assert_eq!(set(1, P::NthWeekday, "2025-01-29", W::None), (1, P::LastWeekday, "2025-01-29".into(), W::None));
    assert_eq!(set(1, P::NthWeekday, "2025-01-28", W::None), (1, P::NthWeekday, "2025-01-28".into(), W::None));
    assert_eq!(set(2, P::Week, "2025-01-04", W::Forward), (2, P::Week, "2025-01-04".into(), W::None));
}

/// `design/notes/engine-budgets.md` §1.4: periods 0-2 as (first day, last day), in UTC.
#[test]
fn budget_periods() {
    let utc = TimeZone::utc();
    let periods = |mult, pt, start, wadj| -> Vec<String> {
        let r = Recurrence::normalised(mult, pt, d(start), wadj);
        (0..3)
            .map(|n| r.period_bounds(n, &utc).unwrap().unwrap())
            .map(|b| format!("{}..{}", b.start.utc_date().unwrap(), b.end.utc_date().unwrap()))
            .collect()
    };
    let monthly = "2024-01-31..2024-02-28 2024-02-29..2024-03-30 2024-03-31..2024-04-29";
    assert_eq!(periods(1, P::Month, "2024-01-31", W::None), dates(monthly));
    assert_eq!(periods(1, P::EndOfMonth, "2024-01-31", W::None), dates(monthly));
    let eom = "2024-02-29..2024-03-30 2024-03-31..2024-04-29 2024-04-30..2024-05-30";
    assert_eq!(periods(1, P::EndOfMonth, "2024-02-10", W::None), dates(eom));
    let quarterly = "2024-11-30..2025-02-27 2025-02-28..2025-05-29 2025-05-30..2025-08-29";
    assert_eq!(periods(3, P::Month, "2024-11-30", W::None), dates(quarterly));
    let yearly = "2024-02-29..2025-02-27 2025-02-28..2026-02-27 2026-02-28..2027-02-27";
    assert_eq!(periods(1, P::Year, "2024-02-29", W::None), dates(yearly));
    let fortnightly = "2024-01-03..2024-01-16 2024-01-17..2024-01-30 2024-01-31..2024-02-13";
    assert_eq!(periods(2, P::Week, "2024-01-03", W::None), dates(fortnightly));
    let back = "2024-03-02..2024-04-01 2024-04-02..2024-05-01 2024-05-02..2024-05-30";
    assert_eq!(periods(1, P::Month, "2024-03-02", W::Back), dates(back));
    let forward = "2024-03-02..2024-03-03 2024-03-04..2024-04-01 2024-04-02..2024-05-01";
    assert_eq!(periods(1, P::Month, "2024-03-02", W::Forward), dates(forward));
    let b = Recurrence::normalised(1, P::Month, d("2024-01-31"), W::None).period_bounds(0, &utc).unwrap().unwrap();
    assert_eq!((b.start.to_file_string(), b.end.to_file_string()), ("2024-01-31 00:00:00 +0000".into(), "2024-02-28 23:59:59 +0000".into()));
    let once = Recurrence::normalised(0, P::Once, d("2024-01-31"), W::None);
    assert_eq!((once.nth_instance(0), once.nth_instance(3), once.period_bounds(0, &utc)), (Ok(Some(d("2024-01-31"))), Ok(None), Ok(None)));
}

#[test]
fn lists_give_the_earliest() {
    let set = |pt, start| Recurrence::normalised(1, pt, d(start), W::None);
    let semi_monthly = [set(P::Month, "2025-01-01"), set(P::Month, "2025-01-15")];
    assert_eq!(next_instance_of_list(&semi_monthly, d("2025-01-01")), Ok(Some(d("2025-01-15"))));
    assert_eq!(next_instance_of_list(&semi_monthly, d("2025-01-20")), Ok(Some(d("2025-02-01"))));
    let with_once = [set(P::Once, "2025-01-10"), set(P::Month, "2025-01-31")];
    assert_eq!(next_instance_of_list(&with_once, d("2025-01-12")), Ok(Some(d("2025-01-31"))));
    assert_eq!(next_instance_of_list(&[set(P::Once, "2025-01-10")], d("2025-01-12")), Ok(None));
    assert_eq!(next_instance_of_list(&[], d("2025-01-12")), Ok(None));
}

/// Dates past `GDate`'s years are an error, never a wrong date or a panic.
#[test]
fn dates_outside_gdate_are_errors() {
    let r = Recurrence::new(1, P::Month, d("2024-01-31"), W::None).unwrap();
    let last = dmy(31, 12, 65535);
    assert_eq!(r.next_instance(last), Err(RecurrenceError::OutOfRange(dmy(31, 1, 65536))));
    let ancient = Recurrence::new(1, P::Day, Date::from_ymd(0, 6, 1).unwrap(), W::None).unwrap();
    assert!(matches!(ancient.next_instance(d("2024-01-01")), Err(RecurrenceError::OutOfRange(_))));
    let big = Recurrence::new(u16::MAX, P::Year, dmy(1, 1, 2000), W::None).unwrap();
    assert!(matches!(big.next_instance(dmy(1, 1, 2000)), Err(RecurrenceError::OutOfRange(_))));
}
