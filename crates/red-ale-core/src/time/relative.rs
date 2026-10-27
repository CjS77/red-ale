//! Relative dates: GnuCash's `RelativeDatePeriod`, the "today", "start of this month" and similar choices of report
//! date options, turned into instants as `gnc_relative_date_to_time64` does
//! (`5.17:libgnucash/engine/gnc-option-date.cpp:519-597`). Glossary: *Relative date*.

use std::fmt;
use std::str::FromStr;

use super::{Date, LocalTime, Time64, TimeZone, days_in_month};

/// A relative date, in GnuCash's enum order (`5.17:libgnucash/engine/gnc-option-date.hpp:44-78`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)] // Each variant is named by its storage string, in `TABLE`.
pub enum RelativeDate {
    Today,
    OneWeekAgo,
    OneWeekAhead,
    OneMonthAgo,
    OneMonthAhead,
    ThreeMonthsAgo,
    ThreeMonthsAhead,
    SixMonthsAgo,
    SixMonthsAhead,
    OneYearAgo,
    OneYearAhead,
    StartThisMonth,
    EndThisMonth,
    StartPrevMonth,
    EndPrevMonth,
    StartNextMonth,
    EndNextMonth,
    StartCurrentQuarter,
    EndCurrentQuarter,
    StartPrevQuarter,
    EndPrevQuarter,
    StartNextQuarter,
    EndNextQuarter,
    StartCalYear,
    EndCalYear,
    StartPrevYear,
    EndPrevYear,
    StartNextYear,
    EndNextYear,
    StartAccountingPeriod,
    EndAccountingPeriod,
}

/// `RelativeDateType`: whether a date is a period's first or last day, or an offset from now that keeps the time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Start,
    End,
    Last,
    Next,
}

/// `RelativeDateOffset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    None,
    Week,
    Month,
    Quarter,
    Three,
    Six,
    Year,
}

use Kind::{End, Last, Next, Start};
use RelativeDate as R;

/// `reldates` (`5.17:libgnucash/engine/gnc-option-date.cpp:73-323`): period, type, offset and storage string.
const TABLE: [(RelativeDate, Kind, Step, &str); 31] = [
    (R::Today, Last, Step::None, "today"),
    (R::OneWeekAgo, Last, Step::Week, "one-week-ago"),
    (R::OneWeekAhead, Next, Step::Week, "one-week-ahead"),
    (R::OneMonthAgo, Last, Step::Month, "one-month-ago"),
    (R::OneMonthAhead, Next, Step::Month, "one-month-ahead"),
    (R::ThreeMonthsAgo, Last, Step::Three, "three-months-ago"),
    (R::ThreeMonthsAhead, Next, Step::Three, "three-months-ahead"),
    (R::SixMonthsAgo, Last, Step::Six, "six-months-ago"),
    (R::SixMonthsAhead, Next, Step::Six, "six-months-ahead"),
    (R::OneYearAgo, Last, Step::Year, "one-year-ago"),
    (R::OneYearAhead, Next, Step::Year, "one-year-ahead"),
    (R::StartThisMonth, Start, Step::Month, "start-this-month"),
    (R::EndThisMonth, End, Step::Month, "end-this-month"),
    (R::StartPrevMonth, Start, Step::Month, "start-prev-month"),
    (R::EndPrevMonth, End, Step::Month, "end-prev-month"),
    (R::StartNextMonth, Start, Step::Month, "start-next-month"),
    (R::EndNextMonth, End, Step::Month, "end-next-month"),
    (R::StartCurrentQuarter, Start, Step::Quarter, "start-current-quarter"),
    (R::EndCurrentQuarter, End, Step::Quarter, "end-current-quarter"),
    (R::StartPrevQuarter, Start, Step::Quarter, "start-prev-quarter"),
    (R::EndPrevQuarter, End, Step::Quarter, "end-prev-quarter"),
    (R::StartNextQuarter, Start, Step::Quarter, "start-next-quarter"),
    (R::EndNextQuarter, End, Step::Quarter, "end-next-quarter"),
    (R::StartCalYear, Start, Step::Year, "start-cal-year"),
    (R::EndCalYear, End, Step::Year, "end-cal-year"),
    (R::StartPrevYear, Start, Step::Year, "start-prev-year"),
    (R::EndPrevYear, End, Step::Year, "end-prev-year"),
    (R::StartNextYear, Start, Step::Year, "start-next-year"),
    (R::EndNextYear, End, Step::Year, "end-next-year"),
    (R::StartAccountingPeriod, Start, Step::Year, "start-prev-fin-year"),
    (R::EndAccountingPeriod, End, Step::Year, "end-prev-fin-year"),
];

impl RelativeDate {
    /// Every relative date, in GnuCash's order.
    pub fn all() -> impl Iterator<Item = Self> {
        TABLE.iter().map(|row| row.0)
    }

    fn row(self) -> (Kind, Step, &'static str) {
        TABLE
            .iter()
            .find(|row| row.0 == self)
            .map_or((Last, Step::None, "today"), |row| (row.1, row.2, row.3))
    }

    /// GnuCash's storage string, such as `start-this-month`.
    pub fn as_str(self) -> &'static str {
        self.row().2
    }

    /// Whether it is the first day of a period (`gnc_relative_date_is_starting`).
    pub fn is_start(self) -> bool {
        self.row().0 == Start
    }

    /// Whether it is the last day of a period (`gnc_relative_date_is_ending`).
    pub fn is_end(self) -> bool {
        self.row().0 == End
    }

    /// `reldate_is_prev`.
    fn is_prev(self) -> bool {
        matches!(
            self,
            R::StartPrevYear | R::EndPrevYear | R::StartPrevQuarter | R::EndPrevQuarter | R::StartPrevMonth | R::EndPrevMonth
        ) || self.row().0 == Last
    }

    /// `reldate_is_next`.
    fn is_next(self) -> bool {
        matches!(
            self,
            R::StartNextYear | R::EndNextYear | R::StartNextQuarter | R::EndNextQuarter | R::StartNextMonth | R::EndNextMonth
        ) || self.row().0 == Next
    }
}

impl FromStr for RelativeDate {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TABLE
            .iter()
            .find(|row| row.3 == s)
            .map(|row| row.0)
            .ok_or_else(|| format!("not a relative date: {s:?}"))
    }
}

impl fmt::Display for RelativeDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The accounting period's bounds as GnuCash holds them: local 00:00:00 of its first day and 23:59:59 of its last
/// (`gnc_accounting_period_fiscal_start` and `_end`, `5.17:libgnucash/engine/gnc-accounting-period.c:60-100`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeriodBounds {
    /// The period's first instant.
    pub start: Time64,
    /// The period's last instant.
    pub end: Time64,
}

/// A `struct tm` as `gnc_relative_date_to_time64` moves it: the month may run outside 0 to 11 and the day outside
/// the month until it is normalised.
#[derive(Debug, Clone, Copy)]
struct Tm {
    /// Years since 1900.
    year: i32,
    /// Month, 0 to 11 once normalised.
    mon: i32,
    mday: i32,
    hour: u32,
    min: u32,
    sec: u32,
}

impl Tm {
    fn from_local(l: LocalTime) -> Self {
        let month = i32::try_from(l.date.month()).unwrap_or(1);
        let mday = i32::try_from(l.date.day()).unwrap_or(1);
        Self {
            year: l.date.year() - 1900,
            mon: month - 1,
            mday,
            hour: l.hour,
            min: l.minute,
            sec: l.second,
        }
    }

    /// `get_last_day_of_month`: the month's length, bringing the month into 0 to 11 first (C division truncates).
    fn last_day_of_month(self) -> i32 {
        let year_delta = self.mon / 12 + if self.mon < 0 { -1 } else { 0 };
        month_days(self.mon - 12 * year_delta, self.year + year_delta)
    }

    fn is_last_day_of_month(self) -> bool {
        self.mday == self.last_day_of_month()
    }

    /// `normalize_reldate_tm` (`5.17:libgnucash/engine/gnc-option-date.cpp:455-485`).
    fn normalize(&mut self) {
        let delta = self.mon / 12 + if self.mon < 0 { -1 } else { 0 };
        self.mon -= 12 * delta;
        self.year += delta;
        while self.mday < 1 {
            self.mon -= 1;
            if self.mon < 0 {
                self.mon = 11;
                self.year -= 1;
            }
            self.mday += month_days(self.mon, self.year);
        }
        while self.mday > month_days(self.mon, self.year) {
            let days = month_days(self.mon, self.year);
            self.mon += 1;
            if self.mon > 11 {
                self.mon = 0;
                self.year += 1;
            }
            self.mday -= days;
        }
    }

    fn date(self) -> Option<Date> {
        Date::from_ymd(self.year + 1900, u32::try_from(self.mon + 1).ok()?, u32::try_from(self.mday).ok()?)
    }
}

/// `days_in_month(month, year)`: month 0 to 11, years since 1900.
fn month_days(mon: i32, year: i32) -> i32 {
    u32::try_from(mon + 1)
        .ok()
        .map_or(31, |m| i32::try_from(days_in_month(year + 1900, m)).unwrap_or(31))
}

/// The instant a relative date names at `now`, as `gnc_relative_date_to_time64` computes it in the local time zone.
/// `accounting` is the accounting period; quarters count from its first month, and from its first day when that is
/// not the first of a month. `None` when a date falls outside the supported range.
pub fn relative_date(period: RelativeDate, now: Time64, zone: &TimeZone, accounting: PeriodBounds) -> Option<Time64> {
    match period {
        R::Today => return Some(now),
        R::StartAccountingPeriod => return Some(accounting.start),
        R::EndAccountingPeriod => return Some(accounting.end),
        _ => {}
    }
    let mut tm = Tm::from_local(zone.to_local(now)?);
    let acct = Tm::from_local(zone.to_local(accounting.start)?);
    let (kind, step, _) = period.row();
    let offset_quarter = step == Step::Quarter && acct.mday > 1;
    shift(&mut tm, period, step, offset_quarter, acct);
    set_day_and_time(&mut tm, kind, offset_quarter, acct);
    tm.normalize();
    zone.instant(tm.date()?, tm.hour, tm.min, tm.sec)
}

/// The `switch (offset)` of `gnc_relative_date_to_time64` (`5.17:libgnucash/engine/gnc-option-date.cpp:534-584`).
fn shift(tm: &mut Tm, period: RelativeDate, step: Step, offset_quarter: bool, acct: Tm) {
    let (prev, next) = (period.is_prev(), period.is_next());
    let by = |n: i32| {
        if prev {
            -n
        } else if next {
            n
        } else {
            0
        }
    };
    match step {
        Step::None => {}
        Step::Year => {
            tm.year += by(1);
            if period.is_start() {
                tm.mon = 0;
            } else if period.is_end() {
                tm.mon = 11;
            }
        }
        Step::Six => tm.mon += by(6),
        Step::Quarter | Step::Three => {
            if step == Step::Quarter {
                tm.mon -= quarter_delta(*tm, period, offset_quarter, acct);
            }
            tm.mon += by(3);
            if period.is_end() {
                tm.mon += 2;
            }
        }
        Step::Month => tm.mon += by(1),
        Step::Week => tm.mday += by(7),
    }
}

/// How many months `now` is into its quarter, counting quarters from the accounting period's start month.
fn quarter_delta(tm: Tm, period: RelativeDate, offset_quarter: bool, acct: Tm) -> i32 {
    let mut delta = (12 + tm.mon - acct.mon) % 3;
    if offset_quarter {
        if delta == 0 && !tm.is_last_day_of_month() && (acct.is_last_day_of_month() || tm.mday < acct.mday) {
            delta = 3;
        }
        if period.is_end() {
            delta -= 1;
        }
    }
    delta
}

/// `reldate_set_day_and_time` (`5.17:libgnucash/engine/gnc-option-date.cpp:487-516`).
fn set_day_and_time(tm: &mut Tm, kind: Kind, offset_quarter: bool, acct: Tm) {
    let clamp_to_acct_day = |tm: &mut Tm| {
        if !acct.is_last_day_of_month() && tm.mday > acct.mday {
            tm.mday = acct.mday;
        }
    };
    match kind {
        Start => {
            tm.mday = 1;
            if offset_quarter {
                tm.mday = tm.last_day_of_month();
                clamp_to_acct_day(tm);
            }
            (tm.hour, tm.min, tm.sec) = (0, 0, 0);
        }
        End => {
            tm.mday = tm.last_day_of_month();
            if offset_quarter {
                clamp_to_acct_day(tm);
                tm.mday -= 1;
            }
            (tm.hour, tm.min, tm.sec) = (23, 59, 59);
        }
        Last | Next => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Time64 {
        Time64::parse_file_string(s).unwrap()
    }

    fn year_2024(zone: &TimeZone) -> PeriodBounds {
        PeriodBounds {
            start: zone.day_start(Date::parse("2024-01-01").unwrap()),
            end: zone.day_end(Date::parse("2024-12-31").unwrap()),
        }
    }

    fn rel(p: RelativeDate, now: &str) -> String {
        let utc = TimeZone::utc();
        relative_date(p, at(now), &utc, year_2024(&utc)).unwrap().to_file_string()
    }

    #[test]
    fn names_round_trip_in_gnucash_order() {
        assert_eq!(RelativeDate::all().count(), 31);
        RelativeDate::all().for_each(|p| assert_eq!(p.as_str().parse::<RelativeDate>(), Ok(p)));
        assert_eq!(RelativeDate::all().nth(17), Some(R::StartCurrentQuarter));
    }

    #[test]
    fn months_and_years() {
        assert_eq!(rel(R::StartThisMonth, "2024-03-15 08:00:00"), "2024-03-01 00:00:00 +0000");
        assert_eq!(rel(R::EndThisMonth, "2024-02-15 08:00:00"), "2024-02-29 23:59:59 +0000");
        assert_eq!(rel(R::StartPrevMonth, "2024-01-15 08:00:00"), "2023-12-01 00:00:00 +0000");
        assert_eq!(rel(R::EndNextMonth, "2024-12-15 08:00:00"), "2025-01-31 23:59:59 +0000");
        assert_eq!(rel(R::EndPrevYear, "2024-03-15 08:00:00"), "2023-12-31 23:59:59 +0000");
        assert_eq!(rel(R::StartNextYear, "2024-03-15 08:00:00"), "2025-01-01 00:00:00 +0000");
    }

    #[test]
    fn offsets_keep_the_time_and_roll_over_month_ends() {
        assert_eq!(
            rel(R::OneMonthAgo, "2024-03-31 08:30:00"),
            "2024-03-02 08:30:00 +0000",
            "31 February is 2 March"
        );
        assert_eq!(rel(R::OneWeekAhead, "2024-12-28 08:30:00"), "2025-01-04 08:30:00 +0000");
        assert_eq!(rel(R::SixMonthsAgo, "2024-03-15 08:30:00"), "2023-09-15 08:30:00 +0000");
        assert_eq!(rel(R::Today, "2024-03-15 08:30:00"), "2024-03-15 08:30:00 +0000");
    }

    #[test]
    fn quarters_count_from_the_accounting_period() {
        assert_eq!(rel(R::StartCurrentQuarter, "2024-05-15 08:00:00"), "2024-04-01 00:00:00 +0000");
        assert_eq!(rel(R::EndCurrentQuarter, "2024-05-15 08:00:00"), "2024-06-30 23:59:59 +0000");
        assert_eq!(rel(R::StartPrevQuarter, "2024-02-15 08:00:00"), "2023-10-01 00:00:00 +0000");
        let utc = TimeZone::utc();
        let fy = PeriodBounds {
            start: utc.day_start(Date::parse("2024-02-01").unwrap()),
            end: utc.day_end(Date::parse("2025-01-31").unwrap()),
        };
        let q = |p, now| relative_date(p, at(now), &utc, fy).unwrap().to_file_string();
        assert_eq!(q(R::StartCurrentQuarter, "2024-05-15 08:00:00"), "2024-05-01 00:00:00 +0000");
        assert_eq!(q(R::EndCurrentQuarter, "2024-05-15 08:00:00"), "2024-07-31 23:59:59 +0000");
        let mid = PeriodBounds {
            start: utc.day_start(Date::parse("2024-01-15").unwrap()),
            end: fy.end,
        };
        let m = |p, now| relative_date(p, at(now), &utc, mid).unwrap().to_file_string();
        assert_eq!(m(R::StartCurrentQuarter, "2024-05-20 08:00:00"), "2024-04-15 00:00:00 +0000");
        assert_eq!(m(R::EndCurrentQuarter, "2024-05-20 08:00:00"), "2024-07-14 23:59:59 +0000");
    }

    #[test]
    fn local_time_decides_the_day() {
        let kiritimati = TimeZone::fixed(14 * 3600).unwrap();
        // 2024-03-31 12:00 UTC is 1 April in Kiritimati.
        let t = relative_date(R::StartThisMonth, at("2024-03-31 12:00:00"), &kiritimati, year_2024(&kiritimati)).unwrap();
        assert_eq!(t.to_file_string(), "2024-03-31 10:00:00 +0000", "local midnight on 1 April");
    }
}
