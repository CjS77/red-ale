use proptest::prelude::*;

use super::*;
use crate::ops::scheduled::fixture::{date, every, limited, monthly, sx};
use crate::{PeriodType, Recurrence, WeekendAdjust};

fn dates(sx: &ScheduledTransaction, count: usize) -> Vec<Date> {
    occurrences(sx, count, None).unwrap().iter().map(|o| o.date).collect()
}

/// `SchedXaction.cpp:899-907`: a start date is itself the first occurrence, from the day before it.
#[test]
fn the_start_is_the_first_occurrence() {
    let s = monthly(date(2025, 1, 15));
    assert_eq!(next_instance(&s, None).unwrap(), Some(date(2025, 1, 15)));
    assert_eq!(dates(&s, 3), [date(2025, 1, 15), date(2025, 2, 15), date(2025, 3, 15)]);
    let first = occurrences(&s, 2, None).unwrap();
    assert_eq!(first.iter().map(|o| o.instance_count).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(first[0].before, TemporalState::of(&s));
    assert_eq!(first[1].before.last, Some(date(2025, 1, 15)));
}

/// The note's §9.2 "Weekend": monthly from Saturday 2025-05-31 adjusted back first occurs on 2025-06-30, as GnuCash
/// 5.17 computed it (oracle, `recurrenceNextInstance` from the day before the start, `Recurrence.cpp:202-381`).
#[test]
fn a_weekend_start_adjusted_back_skips_the_first_occurrence() {
    let start = date(2025, 5, 31);
    let s = sx(
        start,
        vec![Recurrence::new(1, PeriodType::Month, start, WeekendAdjust::Back).unwrap()],
    );
    assert_eq!(next_instance(&s, None).unwrap(), Some(date(2025, 6, 30)));
    assert_eq!(next_instance(&s, Some(&TemporalState::of(&s))).unwrap(), Some(date(2025, 6, 30)));
}

/// §9.2 "Ends": a count of 2 with none remaining has no next occurrence, with or without a state
/// (`SchedXaction.cpp:917-924`).
#[test]
fn no_occurrence_once_none_remain() {
    let mut s = limited(monthly(date(2025, 1, 1)), 2, 0);
    s.last = Some(date(2025, 2, 1));
    assert_eq!(next_instance(&s, None).unwrap(), None);
    assert_eq!(next_instance(&s, Some(&TemporalState::of(&s))).unwrap(), None);
    assert!(dates(&s, 5).is_empty());
}

/// §9.2 "EndDate": no state gives the first occurrence whatever the last one was; the state gives none after the end.
#[test]
fn an_end_date_ends_the_schedule_after_it() {
    let mut s = monthly(date(2025, 1, 1));
    s.end = Some(date(2025, 2, 1));
    s.last = Some(date(2025, 2, 1));
    assert_eq!(next_instance(&s, None).unwrap(), Some(date(2025, 1, 1)));
    assert_eq!(next_instance(&s, Some(&TemporalState::of(&s))).unwrap(), None);
    s.last = None;
    assert_eq!(dates(&s, 5), [date(2025, 1, 1), date(2025, 2, 1)]);
}

/// §9.2 "Defer": count 6, 4 remaining, last 2025-02-01, and the deferred instance `(1970-01-01, 6, 1)` that
/// postponing January leaves; GnuCash gives 2025-01-01 with no state, 2025-03-01 from the state and 2025-01-01 from the
/// deferred instance.
#[test]
fn a_deferred_instance_steps_from_its_own_state() {
    let mut s = limited(monthly(date(2025, 1, 1)), 6, 4);
    s.last = Some(date(2025, 2, 1));
    s.instance_count = 3;
    let deferred = DeferredInstance {
        last: Some(Date::EPOCH),
        remaining: 6,
        instance_count: 1,
    };
    assert_eq!(next_instance(&s, None).unwrap(), Some(date(2025, 1, 1)));
    assert_eq!(next_instance(&s, Some(&TemporalState::of(&s))).unwrap(), Some(date(2025, 3, 1)));
    assert_eq!(
        next_instance(&s, Some(&TemporalState::of_deferred(&deferred))).unwrap(),
        Some(date(2025, 1, 1))
    );
    let left = occurrences(&s, 10, None).unwrap();
    assert_eq!(left.len(), 4, "the remaining count, not the total");
    assert_eq!(left.last().map(|o| (o.date, o.instance_count)), Some((date(2025, 6, 1), 6)));
    assert_eq!(TemporalState::of_deferred(&deferred).to_deferred(), deferred);
}

/// `SchedXaction.cpp:909-924`: with both an end date and a limit, only the end date is tested.
#[test]
fn an_end_date_outranks_an_occurrence_limit() {
    let mut s = limited(monthly(date(2025, 1, 1)), 2, 2);
    s.end = Some(date(2025, 4, 1));
    assert_eq!(dates(&s, 10).len(), 4);
    s.end = None;
    assert_eq!(dates(&s, 10).len(), 2);
    s.occurrences = Some(crate::Occurrences { total: 0, remaining: 0 });
    assert_eq!(dates(&s, 3).len(), 3, "a total of 0 is no limit (`SchedXaction.cpp:691-694`)");
}

/// A last occurrence before the start counts from the day before the start (`SchedXaction.cpp:899-907`), and an
/// unset one reads 1970-01-01 (`:1031-1042`).
#[test]
fn a_last_occurrence_before_the_start_is_ignored() {
    let mut s = monthly(date(2025, 1, 31));
    s.last = Some(date(2020, 6, 30));
    assert_eq!(dates(&s, 2), [date(2025, 1, 31), date(2025, 2, 28)]);
    assert_eq!(TemporalState::of(&monthly(date(2025, 1, 1))).last, Some(Date::EPOCH));
}

/// `test_once` (`5.17:libgnucash/app-utils/test/test-sx.cpp:103-145`): a once-only schedule occurs on its date only.
#[test]
fn a_once_only_schedule_occurs_once() {
    let when = date(2026, 3, 9);
    let s = sx(when, vec![every(0, PeriodType::Once, when)]);
    assert_eq!(dates(&s, 5), [when]);
}

/// `recurrenceListNextInstance` (`Recurrence.cpp:432-460`): the earliest of the Recurrences', the note's H7 case of
/// the 1st and 15th; an empty schedule never occurs.
#[test]
fn two_recurrences_interleave() {
    let (first, fifteenth) = (date(2025, 1, 1), date(2025, 1, 15));
    let s = sx(
        first,
        vec![every(1, PeriodType::Month, first), every(1, PeriodType::Month, fifteenth)],
    );
    assert_eq!(dates(&s, 4), [first, fifteenth, date(2025, 2, 1), date(2025, 2, 15)]);
    assert!(dates(&sx(first, Vec::new()), 3).is_empty());
}

#[test]
fn after_skips_earlier_occurrences_and_keeps_their_count() {
    let s = monthly(date(2025, 1, 10));
    let later = occurrences(&s, 2, Some(date(2025, 3, 10))).unwrap();
    assert_eq!(
        later.iter().map(|o| (o.date, o.instance_count)).collect::<Vec<_>>(),
        [(date(2025, 4, 10), 4), (date(2025, 5, 10), 5)]
    );
}

#[test]
fn advance_steps_as_the_iterator_does() {
    let s = limited(monthly(date(2025, 1, 10)), 3, 3);
    let mut state = TemporalState::of(&s);
    advance(&s, &mut state).unwrap();
    assert_eq!(state, occurrences(&s, 2, None).unwrap()[1].before);
    advance(&s, &mut state).unwrap();
    advance(&s, &mut state).unwrap();
    assert_eq!((state.last, state.remaining, state.instance_count), (Some(date(2025, 3, 10)), 0, 4));
    advance(&s, &mut state).unwrap();
    assert_eq!(
        (state.last, state.remaining, state.instance_count),
        (None, -1, 5),
        "past the end, as GnuCash steps"
    );
}

fn period_type() -> impl Strategy<Value = PeriodType> {
    prop::sample::select(PeriodType::ALL.to_vec())
}

fn weekend_adjust() -> impl Strategy<Value = WeekendAdjust> {
    prop::sample::select(vec![WeekendAdjust::None, WeekendAdjust::Back, WeekendAdjust::Forward])
}

fn recurrence() -> impl Strategy<Value = Recurrence> {
    (1u16..=3, period_type(), 0i64..3000, weekend_adjust())
        .prop_map(|(mult, pt, day, wadj)| Recurrence::normalised(mult, pt, date(2020, 1, 1).add_days(day), wadj))
}

proptest! {
    /// Next occurrences never go backwards: each is after the one before, and after the last occurrence acted on.
    #[test]
    fn occurrences_never_go_backwards(
        schedule in prop::collection::vec(recurrence(), 1..3),
        start in 0i64..3000,
        last in prop::option::of(0i64..4000),
    ) {
        let mut s = sx(date(2020, 1, 1).add_days(start), schedule);
        s.last = last.map(|d| date(2020, 1, 1).add_days(d));
        let found = occurrences(&s, 30, None).unwrap();
        prop_assert!(found.windows(2).all(|w| w[0].date < w[1].date && w[1].instance_count == w[0].instance_count + 1));
        let floor = s.last.filter(|l| *l >= s.start).unwrap_or_else(|| s.start.add_days(-1));
        prop_assert!(found.iter().all(|o| o.date > floor));
    }
}
