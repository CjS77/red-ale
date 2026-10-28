//! The note's hand-worked cases H7 and H8 (`design/notes/engine-scheduled-transactions.md` §9.3), worked from
//! `5.17:libgnucash/app-utils/gnc-sx-instance-model.c:482-556`, `:1042-1058`, `:1445-1596` and
//! `5.17:libgnucash/engine/SchedXaction.cpp:885-926`, `:1031-1125`, and `test_state_changes`
//! (`5.17:libgnucash/app-utils/test/test-sx.cpp:152-223`) without its Guile.

use proptest::prelude::*;

use super::*;
use crate::book_options::set_auto_readonly_days;
use crate::ops::scheduled::fixture::{Fixture, T, daily, date, limited, monthly, usd};
use crate::ops::scheduled::{since_last_run, template_account};
use crate::{Numeric, PeriodType, Recurrence, WeekendAdjust};

fn propose(f: &Fixture, today: Date) -> Proposal {
    since_last_run(&f.book, today, NumberFormat::default()).unwrap()
}

fn run(f: &mut Fixture, p: &Proposal, choices: &[Choice]) -> Result<Applied, ScheduledError> {
    apply(&mut f.book, &mut f.env, p, choices, ApplyOptions::default())
}

fn state(index: usize, state: InstanceState) -> Choice {
    Choice {
        index,
        state: Some(state),
        variables: Variables::new(),
    }
}

fn rent(f: &mut Fixture, sx: ScheduledTransaction) -> Guid {
    let (food, bank) = (f.food, f.bank);
    f.add(sx, &usd(), &[&[T(food, "750", ""), T(bank, "", "750")]])
}

fn shown(p: &Proposal) -> Vec<(Date, InstanceState, i32)> {
    p.instances().map(|(_, i)| (i.date, i.state, i.instance_count())).collect()
}

fn temporal(f: &Fixture, g: Guid) -> (Option<Date>, i32, Option<i32>, usize) {
    let sx = f.sx(g);
    (sx.last, sx.instance_count, sx.occurrences.map(|o| o.remaining), sx.deferred.len())
}

use InstanceState::{Ignored, Postponed, Reminder, ToCreate};

/// Creating every due instance creates one transaction each, advances the last occurrence and the Instance count,
/// and leaves nothing to create on the same day (`gnc-sx-instance-model.c:1513-1528`).
#[test]
fn creating_the_due_instances_advances_the_schedule() {
    let mut f = Fixture::new();
    let g = rent(&mut f, monthly(date(2026, 1, 1)));
    let p = propose(&f, date(2026, 3, 15));
    let applied = run(&mut f, &p, &[]).unwrap();
    assert_eq!(
        applied.created.iter().map(|c| c.date).collect::<Vec<_>>(),
        [date(2026, 1, 1), date(2026, 2, 1), date(2026, 3, 1)]
    );
    assert_eq!(f.book.transactions.len(), 3);
    assert_eq!(temporal(&f, g), (Some(date(2026, 3, 1)), 4, None, 0));
    assert_eq!(applied.schedules, [g]);
    assert!(propose(&f, date(2026, 3, 15)).schedules.is_empty());
}

/// H7, the duplicates of §4.8.1 and deviation (a): postpone January and create February in February; March's run
/// proposes January and March (GnuCash: January, February and March); in April, creating the postponed January
/// overwrites the Instance count with its own `i` + 1 before April's sets it again (`:1050`).
#[test]
fn a_postponement_proposes_no_duplicates() {
    let mut f = Fixture::new();
    let g = rent(&mut f, monthly(date(2026, 1, 1)));
    let p = propose(&f, date(2026, 2, 15));
    run(&mut f, &p, &[state(0, Postponed)]).unwrap();
    assert_eq!(temporal(&f, g), (Some(date(2026, 2, 1)), 3, None, 1));
    assert_eq!(
        f.sx(g).deferred[0],
        DeferredInstance {
            last: Some(Date::EPOCH),
            remaining: 0,
            instance_count: 1
        }
    );

    let p = propose(&f, date(2026, 3, 15));
    assert_eq!(shown(&p), [(date(2026, 1, 1), Postponed, 1), (date(2026, 3, 1), ToCreate, 3)]);
    let applied = run(&mut f, &p, &[]).unwrap();
    assert_eq!((applied.postponed.as_slice(), applied.created.len()), ([0].as_slice(), 1));
    assert_eq!(
        temporal(&f, g),
        (Some(date(2026, 3, 1)), 4, None, 1),
        "a postponed instance left postponed changes nothing"
    );

    let p = propose(&f, date(2026, 4, 15));
    assert_eq!(shown(&p), [(date(2026, 1, 1), Postponed, 1), (date(2026, 4, 1), ToCreate, 4)]);
    run(&mut f, &p, &[state(0, ToCreate)]).unwrap();
    assert_eq!(temporal(&f, g), (Some(date(2026, 4, 1)), 5, None, 0));
    assert_eq!(f.book.transactions.len(), 4, "February, March, January and April, once each");
    assert!(propose(&f, date(2026, 4, 15)).schedules.is_empty());
}

/// H7, a count of 2 run to its end: two instances created, none remaining, nothing proposed after; the scheduled
/// transaction stays, enabled (the note §4.9).
#[test]
fn a_count_runs_to_its_end() {
    let mut f = Fixture::new();
    let g = rent(&mut f, limited(monthly(date(2026, 1, 1)), 2, 2));
    let p = propose(&f, date(2026, 6, 1));
    assert_eq!(p.summary().to_create, 2);
    run(&mut f, &p, &[]).unwrap();
    assert_eq!(temporal(&f, g), (Some(date(2026, 2, 1)), 3, Some(0), 0));
    assert!(propose(&f, date(2027, 6, 1)).schedules.is_empty());
    assert!(f.sx(g).enabled);
}

/// H7, an old schedule with a 30-day auto read-only threshold: GnuCash creates every instance (Fact 3); red-ale
/// refuses those before the threshold unless the caller chooses them (deviation 3).
#[test]
fn instances_before_the_threshold_need_an_explicit_choice() {
    let mut f = Fixture::new();
    set_auto_readonly_days(&mut f.book, 30).unwrap();
    rent(&mut f, monthly(date(2026, 1, 1)));
    let p = propose(&f, date(2026, 3, 15));
    let err = run(&mut f, &p, &[]).unwrap_err();
    assert!(matches!(err, ScheduledError::Refused { reason: Refusal::ReadOnlyByDate, ref indexes, .. } if indexes == &[0]));
    let err = run(&mut f, &p, &[state(0, ToCreate)]).unwrap_err();
    assert!(matches!(err, ScheduledError::Refused { reason: Refusal::ReadOnlyByDate, ref indexes, .. } if indexes == &[1]));
    let applied = run(&mut f, &p, &[state(0, ToCreate), state(1, ToCreate)]).unwrap();
    let flags: Vec<_> = applied.created.iter().map(|c| c.read_only_by_date).collect();
    assert_eq!(flags, [true, true, false]);
}

/// H7, two monthly recurrences, the 1st and the 15th.
#[test]
fn two_recurrences_create_in_date_order() {
    let mut f = Fixture::new();
    let (first, fifteenth) = (date(2026, 1, 1), date(2026, 1, 15));
    let every = |d| Recurrence::new(1, PeriodType::Month, d, WeekendAdjust::None).unwrap();
    let g = rent(
        &mut f,
        crate::ops::scheduled::fixture::sx(first, vec![every(first), every(fifteenth)]),
    );
    let p = propose(&f, date(2026, 2, 10));
    assert_eq!(
        shown(&p),
        [(first, ToCreate, 1), (fifteenth, ToCreate, 2), (date(2026, 2, 1), ToCreate, 3)]
    );
    run(&mut f, &p, &[]).unwrap();
    assert_eq!(temporal(&f, g), (Some(date(2026, 2, 1)), 4, None, 0));
}

/// H8: count 5, Instance count 3, reminding 45 days ahead. Postponing January consumes an occurrence (4 remaining);
/// creating the postponed January later consumes another (3) and sets the Instance count to its `i` + 1 (§4.8.2–3).
#[test]
fn postponing_consumes_an_occurrence_and_creating_it_another() {
    let mut f = Fixture::new();
    let mut s = limited(monthly(date(2026, 1, 1)), 5, 5);
    s.instance_count = 3;
    s.advance_remind_days = 45;
    let g = rent(&mut f, s);
    let p = propose(&f, date(2026, 1, 10));
    assert_eq!(shown(&p), [(date(2026, 1, 1), ToCreate, 3), (date(2026, 2, 1), Reminder, 4)]);
    let applied = run(&mut f, &p, &[state(0, Postponed)]).unwrap();
    assert_eq!(
        (applied.postponed.as_slice(), applied.reminders.as_slice()),
        ([0].as_slice(), [1].as_slice())
    );
    assert_eq!(temporal(&f, g), (Some(date(2026, 1, 1)), 4, Some(4), 1));

    let p = propose(&f, date(2026, 1, 20));
    assert_eq!(
        shown(&p),
        [
            (date(2026, 1, 1), Postponed, 3),
            (date(2026, 2, 1), Reminder, 4),
            (date(2026, 3, 1), Reminder, 5)
        ]
    );
    run(&mut f, &p, &[state(0, ToCreate)]).unwrap();
    assert_eq!(temporal(&f, g), (Some(date(2026, 1, 1)), 4, Some(3), 0));
    assert_eq!(f.book.transactions.len(), 1);
}

/// H8: a postponed instance set to Reminder leaves the deferred list and is never proposed again (§4.8.4).
#[test]
fn a_postponed_instance_set_to_reminder_is_dropped() {
    let mut f = Fixture::new();
    let mut s = monthly(date(2026, 1, 1));
    s.advance_remind_days = 45;
    let g = rent(&mut f, s);
    let p = propose(&f, date(2026, 1, 10));
    run(&mut f, &p, &[state(0, Postponed)]).unwrap();
    let p = propose(&f, date(2026, 1, 20));
    let applied = run(&mut f, &p, &[state(0, Reminder)]).unwrap();
    assert_eq!(applied.reminders, [0, 1, 2]);
    assert_eq!(temporal(&f, g), (Some(date(2026, 1, 1)), 2, None, 0));
    assert_eq!(
        shown(&propose(&f, date(2026, 1, 20))),
        [(date(2026, 2, 1), Reminder, 2), (date(2026, 3, 1), Reminder, 3)]
    );
}

/// `test_state_changes` (`test-sx.cpp:152-223`): four daily instances to create. Choosing to create one, or to postpone
/// one, is allowed and changes no other. Making the second a reminder makes GnuCash turn the third and fourth into
/// reminders (`gnc-sx-instance-model.c:1556-1596`); red-ale refuses it, naming them (deviation 4), and accepts it with
/// them reminders too.
#[test]
fn the_reminder_rule_is_checked_not_applied() {
    let mut f = Fixture::new();
    let today = date(2026, 4, 10);
    rent(&mut f, daily(today));
    let p = propose(&f, today.add_days(3));
    assert_eq!(p.summary().to_create, 4);
    let mut postponed = f.book.clone();
    let applied = apply(
        &mut postponed,
        &mut f.env.clone(),
        &p,
        &[state(2, ToCreate), state(1, Postponed)],
        ApplyOptions::default(),
    )
    .unwrap();
    assert_eq!((applied.created.len(), applied.postponed.as_slice()), (3, [1].as_slice()));
    let err = run(&mut f, &p, &[state(1, Reminder)]).unwrap_err();
    assert!(matches!(err, ScheduledError::Refused { reason: Refusal::ReminderFirst, ref indexes, .. } if indexes == &[1, 2, 3]));
    let applied = run(&mut f, &p, &[state(1, Reminder), state(2, Reminder), state(3, Reminder)]).unwrap();
    assert_eq!((applied.created.len(), applied.reminders.as_slice()), (1, [1, 2, 3].as_slice()));
}

/// The checks on each choice, each before anything changes.
#[test]
fn bad_choices_are_refused() {
    let mut f = Fixture::new();
    let (food, bank) = (f.food, f.bank);
    f.add(monthly(date(2026, 1, 1)), &usd(), &[&[T(food, "amt", ""), T(bank, "", "amt")]]);
    let p = propose(&f, date(2026, 1, 1));
    let value = |name: &str, n: Numeric| Choice {
        index: 0,
        state: None,
        variables: Variables::from([(name.to_owned(), n)]),
    };
    let refused = |f: &mut Fixture, choices: &[Choice]| match run(f, &p, choices) {
        Err(ScheduledError::Refused { reason, .. }) => reason,
        other => panic!("{other:?}"),
    };
    assert_eq!(refused(&mut f, &[]), Refusal::UnboundVariable);
    assert_eq!(refused(&mut f, &[state(5, Ignored)]), Refusal::Index);
    assert_eq!(refused(&mut f, &[state(0, Ignored), state(0, Ignored)]), Refusal::DuplicateChoice);
    assert_eq!(refused(&mut f, &[state(0, InstanceState::Created)]), Refusal::CreatedState);
    assert_eq!(refused(&mut f, &[value("i", Numeric::ONE)]), Refusal::NotEditable);
    assert_eq!(refused(&mut f, &[value("nope", Numeric::ONE)]), Refusal::UnknownVariable);
    let sql = ApplyOptions {
        can_postpone: false,
        ..ApplyOptions::default()
    };
    let err = apply(&mut f.book, &mut f.env, &p, &[state(0, Postponed)], sql).unwrap_err();
    assert!(matches!(
        err,
        ScheduledError::Refused {
            reason: Refusal::SqlDeferredInstances,
            ..
        }
    ));
    assert!(f.book.transactions.is_empty());
    let applied = run(&mut f, &p, &[value("amt", Numeric::new(4250, 100))]).unwrap();
    let t = &f.book.transactions[&applied.created[0].transactions[0].transaction];
    assert_eq!(t.splits[0].value, Numeric::new(4250, 100));
}

/// Deviation (b): one failing Formula refuses the whole apply, creating nothing and advancing nothing.
#[test]
fn a_failing_formula_refuses_the_apply() {
    let mut f = Fixture::new();
    let good = rent(&mut f, monthly(date(2026, 1, 1)));
    let mut bad = monthly(date(2026, 1, 1));
    bad.guid = Guid::from_bytes([7; 16]);
    let (food, bank) = (f.food, f.bank);
    f.add(bad, &usd(), &[&[T(food, "pmt(0 : 12 : 1200 : 0 : 0)", ""), T(bank, "", "1")]]);
    let p = propose(&f, date(2026, 1, 1));
    let before = f.book.clone();
    let err = run(&mut f, &p, &[]).unwrap_err();
    assert!(matches!(err, ScheduledError::Formula(ref e) if e.index == Some(1)), "{err:?}");
    assert_eq!(f.book, before);
    run(&mut f, &p, &[state(1, Ignored)]).unwrap();
    assert_eq!(temporal(&f, good), (Some(date(2026, 1, 1)), 2, None, 0));
}

/// `effect_change` with `auto_create_only` (`gnc-sx-instance-model.c:1483-1491`): only auto-create scheduled
/// transactions are acted on; choices for others are refused.
#[test]
fn auto_create_only_acts_on_auto_create_schedules() {
    let mut f = Fixture::new();
    let manual = rent(&mut f, monthly(date(2026, 1, 1)));
    let mut auto = monthly(date(2026, 1, 1));
    auto.guid = Guid::from_bytes([7; 16]);
    auto.auto_create = true;
    let auto = rent(&mut f, auto);
    let p = propose(&f, date(2026, 1, 1));
    let options = ApplyOptions {
        auto_create_only: true,
        ..ApplyOptions::default()
    };
    let err = apply(&mut f.book, &mut f.env, &p, &[state(0, Ignored)], options).unwrap_err();
    assert!(matches!(
        err,
        ScheduledError::Refused {
            reason: Refusal::NotAutoCreate,
            ..
        }
    ));
    let applied = apply(&mut f.book, &mut f.env, &p, &[], options).unwrap();
    assert_eq!((applied.created.len(), applied.schedules.as_slice()), (1, [auto].as_slice()));
    assert_eq!(temporal(&f, manual), (None, 1, None, 0));
}

/// A schedule with only reminders is not touched (`:1467-1471`, `:1550-1552`); one with no template transactions
/// creates nothing and advances (`:1424-1443`).
#[test]
fn reminders_touch_nothing_and_empty_templates_still_advance() {
    let mut f = Fixture::new();
    let mut s = monthly(date(2026, 2, 1));
    s.advance_remind_days = 40;
    let g = rent(&mut f, s);
    let p = propose(&f, date(2026, 1, 1));
    let applied = run(&mut f, &p, &[]).unwrap();
    assert!(applied.schedules.is_empty());
    assert_eq!(temporal(&f, g), (None, 1, None, 0));
    let mut empty = monthly(date(2026, 1, 1));
    empty.guid = Guid::from_bytes([7; 16]);
    let empty = f.add(empty, &usd(), &[]);
    assert!(template_account(&f.book, f.sx(empty)).is_some());
    let p = propose(&f, date(2026, 1, 1));
    let applied = run(&mut f, &p, &[]).unwrap();
    assert!(applied.created[0].transactions.is_empty());
    assert_eq!(temporal(&f, empty), (Some(date(2026, 1, 1)), 2, None, 0));
}

/// `gnc_sx_add_defer_instance` keeps the deferred list sorted by date, a new one before an equal one.
#[test]
fn deferred_instances_stay_sorted() {
    let at = |d: Option<Date>| DeferredInstance {
        last: d,
        remaining: 0,
        instance_count: 0,
    };
    let state = |d: Option<Date>| TemporalState {
        last: d,
        remaining: 0,
        instance_count: 1,
    };
    let mut list = vec![at(Some(date(2026, 1, 1))), at(Some(date(2026, 3, 1)))];
    add_deferred(&mut list, state(Some(date(2026, 2, 1))));
    add_deferred(&mut list, state(Some(date(2026, 3, 1))));
    let order: Vec<_> = list.iter().map(|d| (d.last, d.instance_count)).collect();
    assert_eq!(
        order,
        [
            (Some(date(2026, 1, 1)), 0),
            (Some(date(2026, 2, 1)), 1),
            (Some(date(2026, 3, 1)), 1),
            (Some(date(2026, 3, 1)), 0)
        ]
    );
    remove_deferred(&mut list, Some(date(2026, 3, 1)));
    assert_eq!(list[2].instance_count, 0, "the first equal one goes");
}

fn schedule() -> impl Strategy<Value = ScheduledTransaction> {
    let types = prop::sample::select(vec![
        PeriodType::Day,
        PeriodType::Week,
        PeriodType::Month,
        PeriodType::EndOfMonth,
        PeriodType::Year,
    ]);
    (1u16..=3, types, 0i64..400, 0i32..10, 0i32..20, prop::option::of(1i32..8)).prop_map(|(mult, pt, start, create, remind, count)| {
        let start = date(2025, 1, 1).add_days(start);
        let mut s = crate::ops::scheduled::fixture::sx(start, vec![Recurrence::normalised(mult, pt, start, WeekendAdjust::Back)]);
        s.advance_create_days = create;
        s.advance_remind_days = remind;
        limited(s, count.unwrap_or(0), count.unwrap_or(0))
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// One apply of N instances to create creates N transactions, all balanced; proposing again on the same day
    /// proposes nothing to create.
    #[test]
    fn applying_then_proposing_again_creates_nothing_more(
        schedules in prop::collection::vec(schedule(), 1..4),
        amounts in prop::collection::vec(1i64..100_000, 4),
        today in 0i64..600,
    ) {
        let mut f = Fixture::new();
        let (food, bank) = (f.food, f.bank);
        schedules.into_iter().zip(amounts).enumerate().for_each(|(n, (mut s, cents))| {
            s.guid = Guid::from_bytes([u8::try_from(n + 1).unwrap(); 16]);
            let debit = format!("{cents}/1000 * i");
            f.add(s, &usd(), &[&[T(food, &debit, ""), T(bank, "", &debit)]]);
        });
        let today = date(2025, 1, 1).add_days(today);
        let p = propose(&f, today);
        let to_create = p.summary().to_create;
        let applied = run(&mut f, &p, &[]).unwrap();
        prop_assert_eq!(applied.created.len(), to_create);
        prop_assert_eq!(f.book.transactions.len(), to_create);
        prop_assert!(f.book.transactions.values().all(|t| t.is_balanced()));
        prop_assert_eq!(propose(&f, today).summary().to_create, 0);
    }
}
