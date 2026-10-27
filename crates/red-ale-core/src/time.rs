//! Instants, calendar dates, the local time zone and relative dates.

mod posix;
mod relative;
mod zone;

use std::fmt;

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _, Utc};
use thiserror::Error;

pub use relative::{PeriodBounds, RelativeDate, relative_date};
pub use zone::{LocalTime, TimeZone, ZoneError};

/// An instant in whole seconds since 1970-01-01 00:00:00 UTC: GnuCash's `time64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Time64(pub i64);

/// A calendar date with no time: GnuCash's `GDate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(NaiveDate);

/// Text that is not a GnuCash time or date.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("not a valid {kind}: {text:?}")]
pub struct ParseTimeError {
    /// What was expected.
    pub kind: &'static str,
    /// The text.
    pub text: String,
}

impl Time64 {
    /// The seconds since the epoch.
    pub const fn seconds(self) -> i64 {
        self.0
    }

    fn to_datetime(self) -> Option<DateTime<Utc>> {
        Utc.timestamp_opt(self.0, 0).single()
    }

    /// GnuCash's file format: `YYYY-MM-DD HH:MM:SS +0000`, always in UTC.
    /// `5.17:libgnucash/backend/xml/sixtp-dom-generators.cpp:131` (`time64_to_dom_tree`).
    pub fn to_file_string(self) -> String {
        self.to_datetime()
            .map_or_else(String::new, |dt| format!("{} +0000", dt.format("%Y-%m-%d %H:%M:%S")))
    }

    /// Parse `YYYY-MM-DD HH:MM:SS [+-]HHMM` (the offset is optional and may be absent in old files).
    pub fn parse_file_string(s: &str) -> Result<Self, ParseTimeError> {
        let err = || ParseTimeError {
            kind: "timestamp",
            text: s.to_owned(),
        };
        let s = s.trim();
        let (datetime, offset) = split_offset(s);
        let naive = NaiveDateTime::parse_from_str(datetime.trim(), "%Y-%m-%d %H:%M:%S")
            .or_else(|_| NaiveDate::parse_from_str(datetime.trim(), "%Y-%m-%d").map(|d| d.and_time(NaiveTime::MIN)))
            .map_err(|_| err())?;
        let offset_secs = offset.map(parse_offset).transpose().map_err(|()| err())?.unwrap_or(0);
        Ok(Self(naive.and_utc().timestamp() - offset_secs))
    }

    /// The UTC calendar date of this instant.
    pub fn utc_date(self) -> Option<Date> {
        self.to_datetime().map(|dt| Date(dt.date_naive()))
    }

    /// Add whole seconds.
    #[must_use]
    pub const fn plus_seconds(self, secs: i64) -> Self {
        Self(self.0 + secs)
    }
}

fn split_offset(s: &str) -> (&str, Option<&str>) {
    match s.rfind([' ', '+']) {
        Some(idx) if idx > 10 => {
            let tail = s[idx..].trim();
            if tail.len() == 5 && (tail.starts_with('+') || tail.starts_with('-')) {
                (&s[..idx], Some(tail))
            } else {
                (s, None)
            }
        }
        _ => (s, None),
    }
}

fn parse_offset(o: &str) -> Result<i64, ()> {
    let sign = if o.starts_with('-') { -1 } else { 1 };
    let hours: i64 = o.get(1..3).and_then(|h| h.parse().ok()).ok_or(())?;
    let mins: i64 = o.get(3..5).and_then(|m| m.parse().ok()).ok_or(())?;
    Ok(sign * (hours * 3600 + mins * 60))
}

impl fmt::Display for Time64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_file_string())
    }
}

impl Date {
    /// A date from year, month and day, if valid.
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Self> {
        NaiveDate::from_ymd_opt(year, month, day).map(Self)
    }

    /// Year.
    pub fn year(self) -> i32 {
        self.0.year()
    }

    /// Month, 1 to 12.
    pub fn month(self) -> u32 {
        self.0.month()
    }

    /// Day of month, from 1.
    pub fn day(self) -> u32 {
        self.0.day()
    }

    /// `YYYY-MM-DD`, as GnuCash writes `gdate` elements.
    pub fn to_file_string(self) -> String {
        self.0.format("%Y-%m-%d").to_string()
    }

    /// Parse `YYYY-MM-DD`.
    pub fn parse(s: &str) -> Result<Self, ParseTimeError> {
        NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
            .map(Self)
            .map_err(|_| ParseTimeError {
                kind: "date",
                text: s.to_owned(),
            })
    }

    /// 10:59:00 UTC on this date: GnuCash's *neutral time* in every time zone from UTC-10 to UTC+13
    /// (`gnc_dmy2time64_neutral`). Outside that band GnuCash moves it; code that stores a date the user chose uses
    /// [`TimeZone::neutral_time`] instead.
    pub fn neutral_time(self) -> Time64 {
        Time64(self.0.and_hms_opt(10, 59, 0).map_or(0, |dt| dt.and_utc().timestamp()))
    }

    /// Midnight UTC at the start of this date.
    pub fn start_utc(self) -> Time64 {
        Time64(self.0.and_time(NaiveTime::MIN).and_utc().timestamp())
    }

    /// One second before the next day starts, UTC.
    pub fn end_utc(self) -> Time64 {
        Time64(self.start_utc().0 + 86_399)
    }

    /// The date `days` later (or earlier, if negative).
    #[must_use]
    pub fn add_days(self, days: i64) -> Self {
        self.0.checked_add_signed(Duration::days(days)).map_or(self, Self)
    }

    /// The date `months` later, clamping the day to the month's length.
    #[must_use]
    pub fn add_months(self, months: i32) -> Self {
        let total = self.0.year() * 12 + i32::try_from(self.0.month0()).unwrap_or(0) + months;
        let year = total.div_euclid(12);
        let month = u32::try_from(total.rem_euclid(12)).unwrap_or(0) + 1;
        let day = self.0.day().min(days_in_month(year, month));
        Self::from_ymd(year, month, day).unwrap_or(self)
    }

    /// Days from `self` to `other`.
    pub fn days_until(self, other: Self) -> i64 {
        (other.0 - self.0).num_days()
    }

    /// The last day of this date's month.
    #[must_use]
    pub fn end_of_month(self) -> Self {
        Self::from_ymd(self.year(), self.month(), days_in_month(self.year(), self.month())).unwrap_or(self)
    }
}

/// Days in a month of a year.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    next.and_then(|n| n.pred_opt()).map_or(28, |d| d.day())
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_file_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_strings_round_trip() {
        let t = Time64::parse_file_string("2014-03-02 10:59:00 +0000").unwrap();
        assert_eq!(t.to_file_string(), "2014-03-02 10:59:00 +0000");
        let local = Time64::parse_file_string("2014-03-02 05:59:00 -0500").unwrap();
        assert_eq!(local, t);
        assert_eq!(Date::parse("2020-02-29").unwrap().to_file_string(), "2020-02-29");
    }

    #[test]
    fn neutral_time_is_1059_utc() {
        let d = Date::from_ymd(2024, 1, 31).unwrap();
        assert_eq!(d.neutral_time().to_file_string(), "2024-01-31 10:59:00 +0000");
        assert_eq!(d.add_months(1), Date::from_ymd(2024, 2, 29).unwrap());
    }
}
