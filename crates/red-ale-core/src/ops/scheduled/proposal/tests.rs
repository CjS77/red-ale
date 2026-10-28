use super::*;
use crate::book_options::set_auto_readonly_days;
use crate::ops::scheduled::fixture::{Fixture, T, daily, date, every, limited, monthly, sx, usd};
use crate::ops::scheduled::{exchange_rate_variable, template_transactions};
use crate::{DeferredInstance, PeriodType};

fn propose(f: &Fixture, today: Date) -> Proposal {
    since_last_run(&f.book, today, NumberFormat::default()).unwrap()
}

fn shown(p: &Proposal) -> Vec<(usize, Date, InstanceState, i32)> {
    p.instances().map(|(_, i)| (i.index, i.date, i.state, i.instance_count())).collect()
}

fn rent(f: &mut Fixture, sx: ScheduledTransaction) -> Guid {
    let (food, bank) = (f.food, f.bank);
    f.add(sx, &usd(), &[&[T(food, "750", ""), T(bank, "", "750")]])
}

/// `test_empty` (`5.17:libgnucash/app-utils/test/test-sx.cpp:87-101`): no scheduled transactions, no instances.
#[test]
fn no_scheduled_transactions_propose_nothing() {
    let f = Fixture::new();
    assert!(propose(&f, date(2038, 12, 31)).schedules.is_empty());
}

/// `test_basic` (`test-sx.cpp:37-85`): a daily schedule from yesterday, asked for up to tomorrow, has three instances
/// to create: yesterday, today and tomorrow.
#[test]
fn a_daily_schedule_from_yesterday_has_three_to_create_up_to_tomorrow() {
    let mut f = Fixture::new();
    let today = date(2026, 4, 10);
    rent(&mut f, daily(today.add_days(-1)));
    let p = propose(&f, today.add_days(1));
    assert_eq!(p.schedules.len(), 1);
    let states: Vec<_> = p.instances().map(|(_, i)| (i.date, i.state)).collect();
    let to_create = |d: Date| (d, InstanceState::ToCreate);
    assert_eq!(
        states,
        [to_create(today.add_days(-1)), to_create(today), to_create(today.add_days(1))]
    );
}

/// `test_once` (`test-sx.cpp:103-145`): a once-only schedule, within a year either way, asked for up to a year ahead,
/// has one instance on its date.
#[test]
fn a_once_only_schedule_has_one_instance() {
    let today = date(2026, 4, 10);
    [-365, -1, 1, 200, 365].into_iter().for_each(|offset| {
        let mut f = Fixture::new();
        let when = today.add_days(offset);
        rent(&mut f, sx(when, vec![every(0, PeriodType::Once, when)]));
        let p = propose(&f, today.add_days(365));
        assert_eq!(shown(&p), [(0, when, InstanceState::ToCreate, 1)], "{offset}");
    });
}

/// `_gnc_sx_gen_instances` (`gnc-sx-instance-model.c:492-496`, `:520-547`): to create up to today plus the days to
/// create in advance, reminders up to that plus the days to remind.
#[test]
fn days_in_advance_create_then_remind() {
    let mut f = Fixture::new();
    let mut s = monthly(date(2026, 1, 1));
    s.advance_create_days = 3;
    s.advance_remind_days = 30;
    rent(&mut f, s);
    let p = propose(&f, date(2026, 1, 29));
    assert_eq!(
        shown(&p),
        [
            (0, date(2026, 1, 1), InstanceState::ToCreate, 1),
            (1, date(2026, 2, 1), InstanceState::ToCreate, 2),
            (2, date(2026, 3, 1), InstanceState::Reminder, 3),
        ]
    );
}

/// Deviation (a) (`design/api/scheduled-transactions.md`, "Deviations in Since Last Run", 1), against the note's §9.2
/// replay of `gnc-sx-instance-model.c:500-535`: count 6, 4 remaining, last 2025-02-01 after postponing January and
/// creating February. GnuCash proposes the postponed January, then *February* and March from January's state; red-ale
/// proposes January, then March from the schedule's own state.
#[test]
fn instances_after_a_postponed_one_come_from_the_schedules_own_state() {
    let mut f = Fixture::new();
    let mut s = limited(monthly(date(2025, 1, 1)), 6, 4);
    s.last = Some(date(2025, 2, 1));
    s.instance_count = 3;
    s.deferred = vec![DeferredInstance {
        last: Some(Date::EPOCH),
        remaining: 6,
        instance_count: 1,
    }];
    rent(&mut f, s);
    let p = propose(&f, date(2025, 3, 15));
    assert_eq!(
        shown(&p),
        [
            (0, date(2025, 1, 1), InstanceState::Postponed, 1),
            (1, date(2025, 3, 1), InstanceState::ToCreate, 3)
        ]
    );
    assert_eq!(p.instances().next().map(|(_, i)| i.before.remaining), Some(6));
}

/// `gnc-sx-instance-model.c:602-638`: disabled ones are left out; one with nothing due has no entry.
#[test]
fn disabled_and_idle_schedules_have_no_entry() {
    let mut f = Fixture::new();
    let mut off = monthly(date(2026, 1, 1));
    off.enabled = false;
    rent(&mut f, off);
    let mut later = monthly(date(2027, 1, 1));
    later.guid = Guid::from_bytes([7; 16]);
    rent(&mut f, later);
    assert!(propose(&f, date(2026, 6, 1)).schedules.is_empty());
}

/// `gnc-sx-instance-model.c:424-459`: every variable unbound but `i`, the Instance count; an instance count of 0 in
/// the file gives `i` = 0 (the note's H6). The exchange-rate variable names the split's and the transaction's
/// mnemonics (`:300-311`).
#[test]
fn variables_are_unbound_but_i() {
    let mut f = Fixture::new();
    let mut s = monthly(date(2026, 1, 1));
    s.instance_count = 0;
    let (food, euro, bank) = (f.food, f.euro, f.bank);
    f.add(s, &usd(), &[&[T(food, "amt * 2 + i", ""), T(euro, "", "x = amt"), T(bank, "", "")]]);
    let p = propose(&f, date(2026, 2, 1));
    let first = &p.schedules[0].instances[0];
    let names: Vec<_> = first.variables.iter().map(|v| (v.name.as_str(), v.value, v.editable)).collect();
    assert_eq!(
        names,
        [
            ("EUR -> USD", None, true),
            ("amt", None, true),
            ("i", Some(Numeric::from_int(0)), false),
            ("x", None, true),
        ]
    );
    assert_eq!(
        p.schedules[0].instances[1].bindings(),
        Variables::from([("i".to_owned(), Numeric::from_int(1))])
    );
    assert_eq!(first.unbound().collect::<Vec<_>>(), ["EUR -> USD", "amt", "x"]);
    assert_eq!(exchange_rate_variable(&crate::CommodityRef::currency("EUR"), &usd()), "EUR -> USD");
}

/// A marker split's commodity names no exchange-rate variable (`gnc-sx-instance-model.c:357-372`); a formula that
/// does not parse is a problem of the template, its other variables still listed.
#[test]
fn markers_name_no_rate_and_bad_formulas_are_reported() {
    let mut f = Fixture::new();
    let (food, euro, bank) = (f.food, f.euro, f.bank);
    let g = f.add(
        monthly(date(2026, 1, 1)),
        &usd(),
        &[&[T(food, "a +", ""), T(euro, "", ""), T(bank, "", "b")]],
    );
    let (names, errors) = variables(&f.book, f.sx(g), NumberFormat::default());
    assert_eq!(names.into_iter().collect::<Vec<_>>(), ["b", "i"]);
    assert!(matches!(&errors[..], [crate::ops::scheduled::ScheduledError::Formula(e)] if e.formula == "a +"));
}

/// `qof_book_get_autoreadonly_gdate` (`5.17:libgnucash/engine/qofbook.cpp:1006-1019`): instances strictly before
/// today minus the threshold's days are flagged.
#[test]
fn instances_before_the_threshold_are_flagged() {
    let mut f = Fixture::new();
    set_auto_readonly_days(&mut f.book, 30).unwrap();
    rent(&mut f, monthly(date(2026, 1, 1)));
    let p = propose(&f, date(2026, 3, 31));
    assert_eq!(p.threshold, Some(date(2026, 3, 1)));
    let flags: Vec<_> = p.instances().map(|(_, i)| i.read_only_by_date).collect();
    assert_eq!(flags, [true, true, false]);
}

/// Each instance's index runs across the whole proposal, in book order.
#[test]
fn indexes_run_across_schedules() {
    let mut f = Fixture::new();
    rent(&mut f, monthly(date(2026, 1, 1)));
    let mut second = monthly(date(2026, 1, 15));
    second.guid = Guid::from_bytes([8; 16]);
    rent(&mut f, second);
    let p = propose(&f, date(2026, 2, 1));
    let indexes: Vec<_> = p
        .schedules
        .iter()
        .map(|s| s.instances.iter().map(|i| i.index).collect::<Vec<_>>())
        .collect();
    assert_eq!(indexes, [vec![0, 1], vec![2]]);
    assert_eq!(
        p.instance(2).map(|(s, i)| (s.scheduled, i.date)),
        Some((Guid::from_bytes([8; 16]), date(2026, 1, 15)))
    );
}

/// The template account is found by name under the template root (`gnc-sx-instance-model.c:378-388`); its
/// transactions come in split order (`xaccSplitOrder`, posted date first).
#[test]
fn template_transactions_come_in_split_order() {
    let mut f = Fixture::new();
    let (food, bank) = (f.food, f.bank);
    let g = f.add(monthly(date(2026, 1, 1)), &usd(), &[&[T(food, "1", "")], &[T(bank, "", "1")]]);
    let first = *f.book.templates.transactions.keys().next().unwrap();
    std::sync::Arc::make_mut(&mut f.book.templates.transactions[&first]).date_posted = crate::Time64(1_800_000_000);
    let order: Vec<_> = template_transactions(&f.book, f.sx(g)).iter().map(|t| t.guid).collect();
    assert_eq!(order.len(), 2);
    assert_eq!(order[1], first);
    let mut renamed = f.book.clone();
    renamed
        .templates
        .accounts
        .values_mut()
        .for_each(|a| std::sync::Arc::make_mut(a).name = "other".into());
    assert!(template_transactions(&renamed, f.sx(g)).is_empty());
}
