//! Recurrence arithmetic: next and nth occurrences and budget period bounds, ported from GnuCash.
//!
//! Ported from `5.17:libgnucash/engine/Recurrence.cpp` (GPL-2.0-or-later):
//! Copyright (C) 2005, Chris Shoemaker <c.shoemaker@cox.net>
//!
//! GnuCash computes on GLib's `GDate`, whose month steps clamp the day to the month's length, as [`Date::add_months`]
//! does. Every step here is checked against `GDate`'s years, and the `guint` wrap of the month count is kept, so the
//! dates agree with GnuCash's for every stored Recurrence its readers accept.

use super::{PeriodType, Recurrence, RecurrenceError, WeekendAdjust};
use crate::Date;
use crate::time::{PeriodBounds, TimeZone, days_in_month};

type Result<T> = std::result::Result<T, RecurrenceError>;

const FRIDAY: u32 = 5;
const SATURDAY: u32 = 6;
const SUNDAY: u32 = 7;

impl Recurrence {
    /// A Recurrence as GnuCash's `recurrenceSet` makes one (`5.17:libgnucash/engine/Recurrence.cpp:87-139`): the
    /// multiplier 0 for `once` and at least 1 otherwise, an end-of-month start moved to its month's last day, a
    /// last-weekday start moved forward by weeks into the month's last week, a fifth-week nth weekday turned into a last
    /// weekday, and the weekend adjustment kept only for month, end of month and year. For Recurrences red-ale creates;
    /// one read from a file keeps its stored fields ([`Recurrence::new`]).
    pub fn normalised(mult: u16, period_type: PeriodType, start: Date, weekend_adjust: WeekendAdjust) -> Self {
        let mult = if period_type == PeriodType::Once { 0 } else { mult.max(1) };
        let (period_type, start) = match period_type {
            PeriodType::EndOfMonth => (period_type, start.end_of_month()),
            PeriodType::LastWeekday => (period_type, into_last_week(start)),
            PeriodType::NthWeekday if (start.day() - 1) / 7 == 4 => (PeriodType::LastWeekday, start),
            _ => (period_type, start),
        };
        let weekend_adjust = if adjusts_for_weekend(period_type) { weekend_adjust } else { WeekendAdjust::None };
        Self {
            mult,
            period_type,
            start,
            weekend_adjust,
        }
    }

    /// The first occurrence strictly after `after`, or `None` when there is none (a `once` Recurrence on or before
    /// `after`): GnuCash's `recurrenceNextInstance` (`5.17:libgnucash/engine/Recurrence.cpp:202-381`), computed from
    /// the stored fields. Before the start, it is the start moved off a weekend, so a scheduled transaction asking from
    /// the day before a Saturday start with weekend adjustment `back` skips the first occurrence.
    pub fn next_instance(&self, after: Date) -> Result<Option<Date>> {
        let start = in_range(self.start)?;
        in_range(after)?;
        let adjusted_start = adjust_for_weekend(self.period_type, self.weekend_adjust, start)?;
        if after < adjusted_start {
            return Ok(Some(adjusted_start));
        }
        let mult = u32::from(self.mult);
        if mult == 0 && self.period_type != PeriodType::Once {
            return Err(RecurrenceError::ZeroMultiplier(self.period_type));
        }
        match self.period_type {
            PeriodType::Once => Ok(None),
            PeriodType::Day => self.next_by_days(after, mult).map(Some),
            PeriodType::Week => self.next_by_days(after, 7 * mult).map(Some),
            PeriodType::Year => self.next_by_months(after, 12 * mult).map(Some),
            PeriodType::Month | PeriodType::EndOfMonth | PeriodType::NthWeekday | PeriodType::LastWeekday => {
                self.next_by_months(after, mult).map(Some)
            }
        }
    }

    /// Occurrence `n`, counted from 0, which is the start itself even on a weekend: GnuCash's `recurrenceNthInstance`
    /// (`5.17:libgnucash/engine/Recurrence.cpp:384-395`), stepping [`Recurrence::next_instance`] from the start. With
    /// weekend adjustment `forward` and a weekend start, occurrence 1 is the adjusted start.
    pub fn nth_instance(&self, n: u32) -> Result<Option<Date>> {
        (0..n).try_fold(Some(self.start), |date, _| date.map_or(Ok(None), |d| self.next_instance(d)))
    }

    /// The bounds of budget period `n`: local 00:00:00 on occurrence `n`, and local 23:59:59 on the day before
    /// occurrence `n + 1`. GnuCash's `recurrenceGetPeriodTime` (`5.17:libgnucash/engine/Recurrence.cpp:397-418`), which
    /// `gnc_budget_get_period_start_date` and `gnc_budget_get_period_end_date` call
    /// (`5.17:libgnucash/engine/gnc-budget.cpp:647-659`). `None` when occurrence `n + 1` does not exist (`once`).
    pub fn period_bounds(&self, n: u32, zone: &TimeZone) -> Result<Option<PeriodBounds>> {
        let first = self.nth_instance(n)?;
        let following = n.checked_add(1).map_or(Ok(None), |m| self.nth_instance(m))?;
        let (Some(first), Some(following)) = (first, following) else {
            return Ok(None);
        };
        let last = plus_days(following, -1)?;
        Ok(Some(PeriodBounds {
            start: zone.day_start(first),
            end: zone.day_end(last),
        }))
    }

    /// Steps of days or weeks: forward `mult` days, then back to the start's phase (`Recurrence.cpp:319-324`,
    /// `:373-376`).
    fn next_by_days(self, after: Date, mult: u32) -> Result<Date> {
        let next = plus_days(after, i64::from(mult))?;
        plus_days(next, -self.start.days_until(next).rem_euclid(i64::from(mult)))
    }

    /// Steps of months: forward one period, then back to the start's month phase, onto its day, and off a weekend
    /// (`Recurrence.cpp:235-318`, `:337-371`).
    fn next_by_months(self, after: Date, mult: u32) -> Result<Date> {
        let stepped = self.step_months(after, mult)?;
        let months = 12 * (i64::from(stepped.year()) - i64::from(self.start.year())) + i64::from(stepped.month())
            - i64::from(self.start.month());
        // GnuCash holds the count in a `guint` (`:343-347`): a step landing before the start's month would wrap.
        let wrapped = u32::try_from(months.rem_euclid(1 << 32)).map_err(|_| RecurrenceError::OutOfRange(stepped))?;
        let in_phase = plus_months(stepped, -i64::from(wrapped % mult))?;
        adjust_for_weekend(self.period_type, self.weekend_adjust, self.onto_day(in_phase)?)
    }

    /// Step 1 for the month types (`Recurrence.cpp:243-317`): forward `mult` months, or one fewer when this month's
    /// occurrence is still ahead.
    fn step_months(self, after: Date, mult: u32) -> Result<Date> {
        let pt = self.period_type;
        let back = self.weekend_adjust == WeekendAdjust::Back && adjusts_for_weekend(pt);
        let next = match after.weekday() {
            SATURDAY if back => plus_days(after, -1)?,
            SUNDAY if back => plus_days(after, -2)?,
            _ => after,
        };
        if back && next.weekday() == FRIDAY {
            return self.step_from_friday(next, mult);
        }
        let passed = is_last_of_month(next)
            || (matches!(pt, PeriodType::Month | PeriodType::Year) && next.day() >= self.start.day())
            || (matches!(pt, PeriodType::NthWeekday | PeriodType::LastWeekday) && nth_weekday_compare(self.start, next, pt) <= 0);
        plus_months(next, i64::from(if passed { mult } else { mult - 1 }))
    }

    /// Step 1 from a Friday with weekend adjustment `back` (`Recurrence.cpp:251-307`): the Friday may stand for the
    /// Saturday or Sunday after it.
    fn step_from_friday(self, friday: Date, mult: u32) -> Result<Date> {
        let sat = plus_days(friday, 1)?;
        let sun = plus_days(friday, 2)?;
        let start_day = self.start.day();
        let (days, months) = if self.period_type == PeriodType::EndOfMonth {
            let passed = [friday, sat, sun].into_iter().any(is_last_of_month);
            (0, if passed { mult } else { mult - 1 })
        } else if sat.day() == start_day {
            (1, mult)
        } else if sun.day() == start_day {
            (2, mult)
        } else if friday.day() >= start_day || is_last_of_month(friday) {
            (0, mult)
        } else if is_last_of_month(sat) {
            (1, mult)
        } else if is_last_of_month(sun) {
            (2, mult)
        } else {
            (0, mult - 1)
        };
        plus_months(plus_days(friday, days)?, i64::from(months))
    }

    /// Step 2's day (`Recurrence.cpp:351-367`): the start's weekday in its week, the last day, or the start's day
    /// clamped to the month.
    fn onto_day(self, in_phase: Date) -> Result<Date> {
        let pt = self.period_type;
        if matches!(pt, PeriodType::NthWeekday | PeriodType::LastWeekday) {
            return plus_days(in_phase, nth_weekday_compare(self.start, in_phase, pt));
        }
        let dim = days_in_month(in_phase.year(), in_phase.month());
        let day = if pt == PeriodType::EndOfMonth || self.start.day() >= dim { dim } else { self.start.day() };
        Date::from_ymd(in_phase.year(), in_phase.month(), day).ok_or(RecurrenceError::OutOfRange(in_phase))
    }
}

/// The earliest next occurrence of any Recurrence in `list` strictly after `after`, or `None` when none has one (or the
/// list is empty): GnuCash's `recurrenceListNextInstance` (`5.17:libgnucash/engine/Recurrence.cpp:432-460`), the next
/// occurrence of a scheduled transaction's schedule.
pub fn next_instance_of_list(list: &[Recurrence], after: Date) -> Result<Option<Date>> {
    list.iter()
        .filter_map(|r| r.next_instance(after).transpose())
        .try_fold(None, |earliest: Option<Date>, next| next.map(|d| Some(earliest.map_or(d, |e| e.min(d)))))
}

/// `recurrenceSet`'s last-weekday phase (`Recurrence.cpp:111-118`): forward by weeks while a week remains in the month.
fn into_last_week(start: Date) -> Date {
    let dim = days_in_month(start.year(), start.month());
    start.add_days(i64::from((dim - start.day()) / 7 * 7))
}

/// The types a weekend adjustment applies to (`Recurrence.cpp:128-137`, `:178`).
fn adjusts_for_weekend(pt: PeriodType) -> bool {
    matches!(pt, PeriodType::Month | PeriodType::EndOfMonth | PeriodType::Year)
}

/// `adjust_for_weekend` (`Recurrence.cpp:176-196`): Saturday back 1 or forward 2, Sunday back 2 or forward 1.
fn adjust_for_weekend(pt: PeriodType, wadj: WeekendAdjust, date: Date) -> Result<Date> {
    if !adjusts_for_weekend(pt) {
        return Ok(date);
    }
    let shift = match (date.weekday(), wadj) {
        (SATURDAY, WeekendAdjust::Back) => -1,
        (SUNDAY, WeekendAdjust::Back) => -2,
        (SATURDAY, WeekendAdjust::Forward) => 2,
        (SUNDAY, WeekendAdjust::Forward) => 1,
        _ => 0,
    };
    plus_days(date, shift)
}

/// `nth_weekday_compare` (`Recurrence.cpp:148-173`): the days from `next` to the start's weekday in the start's week
/// of the month (capped at the fourth, or the last for [`PeriodType::LastWeekday`]) in `next`'s month.
fn nth_weekday_compare(start: Date, next: Date, pt: PeriodType) -> i64 {
    let sd = i64::from(start.day());
    let nd = i64::from(next.day());
    let week = (sd / 7).min(3);
    let week = if week > 0 && sd % 7 == 0 && sd != 28 { week - 1 } else { week };
    let mut matchday = 7 * week + (nd - i64::from(next.weekday()) + i64::from(start.weekday()) + 7) % 7;
    if i64::from(days_in_month(next.year(), next.month())) - matchday >= 7 && pt == PeriodType::LastWeekday {
        matchday += 7;
    }
    if pt == PeriodType::NthWeekday && matchday % 7 == 0 {
        matchday += 7;
    }
    matchday - nd
}

fn is_last_of_month(d: Date) -> bool {
    d.day() == days_in_month(d.year(), d.month())
}

/// A date `GDate` can hold.
fn in_range(d: Date) -> Result<Date> {
    if (1..=65535).contains(&d.year()) { Ok(d) } else { Err(RecurrenceError::OutOfRange(d)) }
}

fn plus_days(d: Date, days: i64) -> Result<Date> {
    in_range(d.add_days(days))
}

/// `g_date_add_months` or `g_date_subtract_months`: the day clamps to the new month's length.
fn plus_months(d: Date, months: i64) -> Result<Date> {
    let months = i32::try_from(months).map_err(|_| RecurrenceError::OutOfRange(d))?;
    in_range(d.add_months(months))
}
