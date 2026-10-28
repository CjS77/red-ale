// lib/dates: calendar stepping, date lists and the date fractions reports average with.
//
// Translated from 5.17:bindings/guile/date-utilities.scm (lines 176-499), cited below as DU. Its header:
//
// date-utilities.scm -- date utility functions.
// Bryan Larsen (blarsen@ada-works.com)
// Revised by Christopher Browne
// Improvement to financial year support by Yves-Eric Martin
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License as
// published by the Free Software Foundation; either version 2 of
// the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program; if not, contact:
//
// Free Software Foundation           Voice:  +1-617-542-5942
// 51 Franklin Street, Fifth Floor    Fax:    +1-617-542-2652
// Boston, MA  02110-1301,  USA       gnu@gnu.org
//
// This translation is part of red-ale and is distributed under the GNU General Public License, version 3 or (at your
// option) any later version (GPL-3.0-or-later); see LICENSE.
//
// Times are Time64s. GnuCash's `gnc-localtime` and `gnc-mktime` work in the local time zone; here they are the run
// context's `ctx.local` and `ctx.make_time`, so the functions that need them take `ctx` first. A delta, GnuCash's
// `struct tm` of amounts to add, is a map `#{ year, month, day, hour, minute, second }`.
//
// GnuCash's `gnc:date-get-fraction-func` returns a function; a function a Rhai module returns cannot be called
// later, so it returns a fraction function value that `apply_fraction_func` applies.

/// `gnc:leap-year?` (DU:176-179).
fn leap_year(year) {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// `gnc:days-in-year` (DU:182-183).
fn days_in_year(year) {
    if leap_year(year) { 366 } else { 365 }
}

/// `gnc:days-in-month` (DU:186-190): `()` for a month outside 1 to 12, as GnuCash's `case` has no else.
fn days_in_month(month, year) {
    switch month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if leap_year(year) { 29 } else { 28 },
        _ => (),
    }
}

/// `gnc:date-to-year-fraction` (DU:194-198): years since 1970 as an exact fraction.
fn date_to_year_fraction(ctx, caltime) {
    let lt = ctx.local(caltime);
    (lt.year - 1970) + rational(lt.yearday - 1, days_in_year(lt.year))
}

/// `gnc:date-to-halfyear-fraction` (DU:222-223).
fn date_to_halfyear_fraction(ctx, caltime) {
    date_to_year_fraction(ctx, caltime) * 2
}

/// `gnc:date-to-quarter-fraction` (DU:227-228).
fn date_to_quarter_fraction(ctx, caltime) {
    date_to_month_fraction(ctx, caltime) / 3
}

/// `gnc:date-to-month-fraction` (DU:231-238): months since 1970 as an exact fraction.
fn date_to_month_fraction(ctx, caltime) {
    let lt = ctx.local(caltime);
    12 * (lt.year - 1970) + lt.month - 1 + rational(lt.day - 1, days_in_month(lt.month, lt.year))
}

/// `gnc:date-to-twoweek-fraction` (DU:240-241).
fn date_to_twoweek_fraction(ctx, caltime) {
    date_to_week_fraction(ctx, caltime) / 2
}

/// `gnc:date-to-week-fraction` (DU:250-251), with GnuCash's `weekstart` (DU:244-248) from `ctx.week_start()`.
fn date_to_week_fraction(ctx, caltime) {
    (rational(caltime, 86400) - 1 - ctx.week_start()) / 7
}

/// `gnc:date-to-day-fraction` (DU:258-259): days since 28 February 1970. GnuCash divides by `3600.0`, a double;
/// red-ale divides exactly (`design/reports.md` §12, "Floating point in GnuCash, exact in red-ale").
fn date_to_day_fraction(caltime) {
    rational(caltime, 86400) - 59
}

/// The intervals `gnc:date-get-fraction-func` knows.
private fn fraction_intervals() {
    ["YearDelta", "HalfYearDelta", "QuarterDelta", "MonthDelta", "TwoWeekDelta", "WeekDelta", "DayDelta"]
}

/// `gnc:date-get-fraction-func` (DU:264-273): the fraction function of `interval` (an interval-choice key such as
/// "MonthDelta"), or `()` for an interval it does not know.
fn date_get_fraction_func(ctx, interval) {
    if fraction_intervals().contains(interval) { #{ fraction_func: interval, ctx: ctx } } else { () }
}

/// Apply a fraction function of `date_get_fraction_func` to `caltime`, or call a closure `|caltime|`.
fn apply_fraction_func(func, caltime) {
    if type_of(func) == "Fn" {
        return func.call(caltime);
    }
    let ctx = func.ctx;
    switch func.fraction_func {
        "YearDelta" => date_to_year_fraction(ctx, caltime),
        "HalfYearDelta" => date_to_halfyear_fraction(ctx, caltime),
        "QuarterDelta" => date_to_quarter_fraction(ctx, caltime),
        "MonthDelta" => date_to_month_fraction(ctx, caltime),
        "TwoWeekDelta" => date_to_twoweek_fraction(ctx, caltime),
        "WeekDelta" => date_to_week_fraction(ctx, caltime),
        "DayDelta" => date_to_day_fraction(caltime),
        _ => throw `not a fraction function: ${func.fraction_func}`,
    }
}

/// `moddate` (DU:276-287): each field of `adate`'s local time combined with `delta`'s by `op` (`|a, b| a + b` or
/// `|a, b| a - b`), then made a time again, normalised as `mktime` normalises (31 January + 1 month is 3 March).
fn moddate(ctx, op, adate, delta) {
    let lt = ctx.local(adate);
    ctx.make_time(#{
        year: op.call(lt.year, delta.year),
        month: op.call(lt.month, delta.month),
        day: op.call(lt.day, delta.day),
        hour: op.call(lt.hour, delta.hour),
        minute: op.call(lt.minute, delta.minute),
        second: op.call(lt.second, delta.second),
    })
}

/// `decdate` (DU:289).
fn decdate(ctx, adate, delta) {
    moddate(ctx, |a, b| a - b, adate, delta)
}

/// `incdate` (DU:290).
fn incdate(ctx, adate, delta) {
    moddate(ctx, |a, b| a + b, adate, delta)
}

/// `end-month?` (DU:293-297): true when the next day is in another month.
private fn end_month(ctx, date) {
    let lt = ctx.local(date);
    let next = ctx.make_time(#{ year: lt.year, month: lt.month, day: lt.day + 1, hour: lt.hour, minute: lt.minute,
                                second: lt.second });
    ctx.local(next).month != lt.month
}

/// `incdate-months` (DU:299-318): `nmonths` months after `date`, on the same day of the month or the month's last
/// day when it has fewer days, and on the last day when `date` is a month's last.
fn incdate_months(ctx, date, nmonths) {
    let lt = ctx.local(date);
    let newmonth = lt.month - 1 + nmonths;
    let new_month_proper = newmonth % 12;
    if new_month_proper < 0 {
        new_month_proper += 12;
    }
    let new_year_proper = lt.year + (newmonth - new_month_proper) / 12;
    let new_mday = lt.day;
    let date_end_month = end_month(ctx, date);
    loop {
        let res = ctx.make_time(#{ year: new_year_proper, month: new_month_proper + 1, day: new_mday, hour: lt.hour,
                                   minute: lt.minute, second: lt.second });
        if ctx.local(res).month != new_month_proper + 1 {
            // The day slipped into the next month: step back.
            new_mday -= 1;
        } else if date_end_month && !end_month(ctx, res) {
            // From a month's end, keep to month ends.
            new_mday += 1;
        } else {
            return res;
        }
    }
}

/// The months of `MonthDeltas` (DU:449-454): 1, 3, 6 or 12 for a delta of whole months or years, `()` for any other.
private fn month_delta(incr) {
    let plain = incr.day == 0 && incr.hour == 0 && incr.minute == 0 && incr.second == 0;
    if !plain {
        return ();
    }
    if incr.year == 0 && [1, 3, 6].contains(incr.month) {
        incr.month
    } else if incr.year == 1 && incr.month == 0 {
        12
    } else {
        ()
    }
}

/// `gnc:make-date-interval-list` (DU:323-349): intervals `[from, to]` from `startdate` in steps of `incr`, each
/// ending a second before the next starts, the last at `enddate`, so it may be shorter. Month steps count from
/// `startdate` with `incdate_months`, as `make_date_list` does. The dates are swapped when `enddate` is before
/// `startdate`.
fn make_date_interval_list(ctx, startdate, enddate, incr) {
    if enddate < startdate {
        let saved_enddate = enddate;
        enddate = startdate;
        startdate = saved_enddate;
    }
    let months = month_delta(incr);
    let result = [];
    let date = startdate;
    let idx = 0;
    while date < enddate {
        let curr = if months != () { incdate_months(ctx, startdate, months * idx) } else { date };
        let next = if months != () { incdate_months(ctx, startdate, months * (idx + 1)) } else { incdate(ctx, date, incr) };
        result.push([curr, if next < enddate { decdate(ctx, next, delta("second", 1)) } else { enddate }]);
        date = next;
        idx += 1;
    }
    result
}

/// `gnc:make-date-list` (DU:356-381): times from `startdate` in steps of `incr`, then `enddate` itself, so the last
/// step may be shorter. Month steps count from `startdate` with `incdate_months`, so month ends stay month ends. The
/// dates are swapped when `enddate` is before `startdate`.
fn make_date_list(ctx, startdate, enddate, incr) {
    if enddate < startdate {
        let saved_enddate = enddate;
        enddate = startdate;
        startdate = saved_enddate;
    }
    let months = month_delta(incr);
    let result = [];
    let date = startdate;
    let idx = 0;
    while date < enddate {
        if months != () {
            result.push(incdate_months(ctx, startdate, months * idx));
            date = incdate_months(ctx, startdate, months * (idx + 1));
        } else {
            result.push(date);
            date = incdate(ctx, date, incr);
        }
        idx += 1;
    }
    result.push(enddate);
    result
}

/// A delta with every field zero, GnuCash's `make-zdate` (DU:384-395).
private fn zdate() {
    #{ year: 0, month: 0, day: 0, hour: 0, minute: 0, second: 0 }
}

/// A delta of `n` in one `field`, as DU:398-447 make each.
private fn delta(field, n) {
    let d = zdate();
    d[field] = n;
    d
}

// The deltas of DU:398-447 by GnuCash's names: `dates::MonthDelta` and so on.
export const SecDelta = delta("second", 1);
export const DayDelta = delta("day", 1);
export const WeekDelta = delta("day", 7);
export const TwoWeekDelta = delta("day", 14);
export const MonthDelta = delta("month", 1);
export const QuarterDelta = delta("month", 3);
export const HalfYearDelta = delta("month", 6);
export const YearDelta = delta("year", 1);
export const ThirtyDayDelta = delta("day", 30);
export const NinetyDayDelta = delta("day", 90);

/// `gnc:deltasym-to-delta` (DU:470-474): the delta named `ds` ("MonthDelta", an interval-choice key), `()` for
/// another name.
fn deltasym_to_delta(ds) {
    // A function cannot read the module's constants, so it makes each delta again.
    switch ds {
        "SecDelta" => delta("second", 1),
        "DayDelta" => delta("day", 1),
        "WeekDelta" => delta("day", 7),
        "TwoWeekDelta" => delta("day", 14),
        "MonthDelta" => delta("month", 1),
        "QuarterDelta" => delta("month", 3),
        "HalfYearDelta" => delta("month", 6),
        "YearDelta" => delta("year", 1),
        "ThirtyDayDelta" => delta("day", 30),
        "NinetyDayDelta" => delta("day", 90),
        _ => (),
    }
}

/// `gnc:time64-start-day-time` (DU:479-485): local 00:00:00 of `t64`'s day.
fn time64_start_day_time(ctx, t64) {
    ctx.start_of_day(t64)
}

/// `gnc:time64-end-day-time` (DU:487-493): local 23:59:59 of `t64`'s day.
fn time64_end_day_time(ctx, t64) {
    ctx.end_of_day(t64)
}

/// `gnc:time64-previous-day` (DU:495-496): `t64` a day earlier at the same local time of day, through `decdate`.
fn time64_previous_day(ctx, t64) {
    decdate(ctx, t64, delta("day", 1))
}

/// `gnc:time64-next-day` (DU:498-499): `t64` a day later at the same local time of day, through `incdate`.
fn time64_next_day(ctx, t64) {
    incdate(ctx, t64, delta("day", 1))
}
