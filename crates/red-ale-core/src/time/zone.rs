//! The local time zone, and the instants GnuCash turns a calendar date into.
//!
//! GnuCash converts every local time through one process-wide zone (`5.17:libgnucash/engine/gnc-datetime.cpp:68-69`).
//! Here the zone is a value: the service builds it from the environment (see `design/notes/time-zones.md` §1) and
//! hands it to the core through [`crate::Env`]; tests pin one.

use jiff::civil;
use jiff::tz::{self, AmbiguousOffset, Offset};
use jiff::{SignedDuration, Timestamp};
use thiserror::Error;

use super::{Date, Time64};

const HOUR: i64 = 3600;

/// A time zone: the rules that turn an instant into local time and back. Glossary: *Local time zone*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeZone {
    name: String,
    rules: tz::TimeZone,
}

/// Text or data that is not a time zone GnuCash would accept.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("not a time zone: {text:?} ({reason})")]
pub struct ZoneError {
    /// The zone's text or name.
    pub text: String,
    /// Why it was refused.
    pub reason: String,
}

impl ZoneError {
    fn new(text: &str, reason: impl Into<String>) -> Self {
        Self {
            text: text.to_owned(),
            reason: reason.into(),
        }
    }
}

/// A wall-clock time on a local date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalTime {
    /// The local date.
    pub date: Date,
    /// Hour, 0 to 23.
    pub hour: u32,
    /// Minute, 0 to 59.
    pub minute: u32,
    /// Second, 0 to 59.
    pub second: u32,
}

impl LocalTime {
    /// `YYYYMMDDhhmmss`, GnuCash's timestamp for backup and journal names (`GncDateTimeImpl::timestamp`,
    /// `5.17:libgnucash/engine/gnc-datetime.cpp:638-644`).
    pub fn compact(self) -> String {
        format!(
            "{}{:02}{:02}{:02}",
            self.date.to_file_string().replace('-', ""),
            self.hour,
            self.minute,
            self.second
        )
    }
}

impl Default for TimeZone {
    fn default() -> Self {
        Self::utc()
    }
}

impl TimeZone {
    /// UTC.
    pub fn utc() -> Self {
        Self {
            name: "UTC".to_owned(),
            rules: tz::TimeZone::UTC,
        }
    }

    /// A fixed offset, in seconds east of UTC.
    pub fn fixed(seconds_east: i32) -> Result<Self, ZoneError> {
        let offset = Offset::from_seconds(seconds_east).map_err(|e| ZoneError::new(&seconds_east.to_string(), e.to_string()))?;
        Ok(Self {
            name: format!("UTC{offset}"),
            rules: tz::TimeZone::fixed(offset),
        })
    }

    /// A zone from the contents of a TZif file (a zoneinfo file such as `/usr/share/zoneinfo/Pacific/Kiritimati`).
    pub fn from_tzif(name: &str, data: &[u8]) -> Result<Self, ZoneError> {
        let rules = tz::TimeZone::tzif(name, data).map_err(|e| ZoneError::new(name, e.to_string()))?;
        Ok(Self {
            name: name.to_owned(),
            rules,
        })
    }

    /// A zone from a POSIX `TZ` string, read as GnuCash reads it: through Boost's `posix_time_zone`, whose offsets
    /// have the **opposite sign** to POSIX (`XYZ11` is UTC+11). See [`super::posix`].
    pub fn from_gnucash_posix(text: &str) -> Result<Self, ZoneError> {
        let posix = super::posix::to_posix(text).map_err(|reason| ZoneError::new(text, reason))?;
        let rules = tz::TimeZone::posix(&posix).map_err(|e| ZoneError::new(text, e.to_string()))?;
        Ok(Self {
            name: text.to_owned(),
            rules,
        })
    }

    /// The zone's name: an IANA name or path, a `TZ` string, or `UTC±hh:mm`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The offset from UTC at an instant, in seconds east. Instants beyond the years ±9999 count as UTC.
    pub fn offset_at(&self, t: Time64) -> i32 {
        timestamp(t).map_or(0, |ts| self.rules.to_offset(ts).seconds())
    }

    /// The wall-clock time of an instant.
    pub fn to_local(&self, t: Time64) -> Option<LocalTime> {
        let dt = self.rules.to_datetime(timestamp(t)?);
        let date = Date::from_ymd(i32::from(dt.year()), u32::try_from(dt.month()).ok()?, u32::try_from(dt.day()).ok()?)?;
        let part = |n: i8| u32::try_from(n).ok();
        Some(LocalTime {
            date,
            hour: part(dt.hour())?,
            minute: part(dt.minute())?,
            second: part(dt.second())?,
        })
    }

    /// The local date of an instant (`time64_to_gdate`, `5.17:libgnucash/engine/gnc-date.cpp:1284-1296`).
    pub fn local_date(&self, t: Time64) -> Option<Date> {
        self.to_local(t).map(|l| l.date)
    }

    /// The instant of a wall-clock time, as GnuCash makes it (`LDT_from_date_time`,
    /// `5.17:libgnucash/engine/gnc-datetime.cpp:195-229`): a time in a gap is taken an hour later, an ambiguous one
    /// as the later instant. `None` when that fails too, or the date is out of range.
    pub fn instant(&self, date: Date, hour: u32, minute: u32, second: u32) -> Option<Time64> {
        let dt = civil_datetime(date, hour, minute, second)?;
        let one_hour = SignedDuration::from_hours(1);
        let ts = match self.ambiguous_offset(dt) {
            AmbiguousOffset::Unambiguous { offset } => offset.to_timestamp(dt).ok()?,
            AmbiguousOffset::Gap { .. } => self.unambiguous(dt.checked_add(one_hour).ok()?)?,
            AmbiguousOffset::Fold { .. } => self.unambiguous(dt.checked_add(one_hour).ok()?)?.checked_sub(one_hour).ok()?,
        };
        Some(Time64(ts.as_second()))
    }

    fn ambiguous_offset(&self, dt: civil::DateTime) -> AmbiguousOffset {
        self.rules.to_ambiguous_timestamp(dt).offset()
    }

    fn unambiguous(&self, dt: civil::DateTime) -> Option<Timestamp> {
        match self.ambiguous_offset(dt) {
            AmbiguousOffset::Unambiguous { offset } => offset.to_timestamp(dt).ok(),
            AmbiguousOffset::Gap { .. } | AmbiguousOffset::Fold { .. } => None,
        }
    }

    /// [`Self::instant`], or, where GnuCash would fail, the instant before any gap.
    fn instant_or_compatible(&self, date: Date, hour: u32, minute: u32, second: u32, fallback: Time64) -> Time64 {
        self.instant(date, hour, minute, second)
            .or_else(|| {
                let dt = civil_datetime(date, hour, minute, second)?;
                self.rules
                    .to_ambiguous_timestamp(dt)
                    .compatible()
                    .ok()
                    .map(|ts| Time64(ts.as_second()))
            })
            .unwrap_or(fallback)
    }

    /// Local 00:00:00 on a date (`DayPart::start`, `5.17:libgnucash/engine/gnc-datetime.cpp:244-258`).
    pub fn day_start(&self, d: Date) -> Time64 {
        self.instant_or_compatible(d, 0, 0, 0, d.start_utc())
    }

    /// Local 23:59:59 on a date (`DayPart::end`).
    pub fn day_end(&self, d: Date) -> Time64 {
        self.instant_or_compatible(d, 23, 59, 59, d.end_utc())
    }

    /// Local noon on a date: `time64CanonicalDayTime` of any time that day (`5.17:libgnucash/engine/gnc-date.cpp:403-410`).
    pub fn noon(&self, d: Date) -> Time64 {
        self.instant_or_compatible(d, 12, 0, 0, d.start_utc().plus_seconds(12 * HOUR))
    }

    /// The day-neutral time of a date: 10:59:00 UTC, moved into the date in zones west of UTC−10 or east of UTC+13
    /// (`DayPart::neutral`, `5.17:libgnucash/engine/gnc-datetime.cpp:260-267`). Glossary: *Day-neutral time*.
    pub fn neutral_time(&self, d: Date) -> Time64 {
        let t = d.neutral_time();
        let offset = i64::from(self.offset_at(t));
        // Boost's `hours()` truncates towards zero, as Rust's division does.
        let hours = offset / HOUR;
        if offset < -10 * HOUR {
            t.plus_seconds(-(hours + 10) * HOUR)
        } else if offset > 13 * HOUR {
            t.plus_seconds((13 - hours) * HOUR)
        } else {
            t
        }
    }

    /// The day-neutral time of an instant's local date (`gnc_time64_get_day_neutral`,
    /// `5.17:libgnucash/engine/gnc-date.cpp:1376-1383`).
    pub fn day_neutral_of(&self, t: Time64) -> Option<Time64> {
        self.local_date(t).map(|d| self.neutral_time(d))
    }

    /// Whether two instants fall on the same local day, compared as GnuCash compares them: local noon of each one's
    /// day (`time64CanonicalDayTime`). Glossary: *Same day* (Prices).
    pub fn same_day(&self, a: Time64, b: Time64) -> bool {
        self.local_date(a).map(|d| self.noon(d)) == self.local_date(b).map(|d| self.noon(d))
    }
}

fn timestamp(t: Time64) -> Option<Timestamp> {
    Timestamp::from_second(t.0).ok()
}

fn civil_datetime(date: Date, hour: u32, minute: u32, second: u32) -> Option<civil::DateTime> {
    let narrow = |n: u32| i8::try_from(n).ok();
    let day = civil::Date::new(i16::try_from(date.year()).ok()?, narrow(date.month())?, narrow(date.day())?).ok()?;
    civil::DateTime::new(
        day.year(),
        day.month(),
        day.day(),
        narrow(hour)?,
        narrow(minute)?,
        narrow(second)?,
        0,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> Date {
        Date::from_ymd(y, m, d).unwrap()
    }

    fn file(t: Time64) -> String {
        t.to_file_string()
    }

    /// `America/New_York`'s rules, so the tests need no zoneinfo files.
    fn new_york() -> TimeZone {
        TimeZone::from_gnucash_posix("EST-5EDT,M3.2.0,M11.1.0").unwrap()
    }

    #[test]
    fn neutral_time_moves_only_outside_utc_minus_10_to_plus_13() {
        let d = day(2024, 6, 20);
        let at = |hours: i32| file(TimeZone::fixed(hours * 3600).unwrap().neutral_time(d));
        assert_eq!(at(0), "2024-06-20 10:59:00 +0000");
        assert_eq!(at(-10), "2024-06-20 10:59:00 +0000");
        assert_eq!(at(13), "2024-06-20 10:59:00 +0000");
        assert_eq!(at(-11), "2024-06-20 11:59:00 +0000", "Pacific/Pago_Pago, as the oracle gives");
        assert_eq!(at(-12), "2024-06-20 12:59:00 +0000");
        assert_eq!(at(14), "2024-06-20 09:59:00 +0000", "Pacific/Kiritimati, as the oracle gives");
        let chatham_summer = TimeZone::fixed(13 * 3600 + 45 * 60).unwrap();
        assert_eq!(
            file(chatham_summer.neutral_time(d)),
            "2024-06-20 10:59:00 +0000",
            "13:45 truncates to 13 hours"
        );
    }

    #[test]
    fn neutral_time_falls_on_its_date_in_every_whole_hour_zone() {
        (-12..=14).for_each(|h| {
            let zone = TimeZone::fixed(h * 3600).unwrap();
            let d = day(2024, 2, 29);
            assert_eq!(zone.local_date(zone.neutral_time(d)), Some(d), "UTC{h:+}");
        });
    }

    #[test]
    fn day_start_and_end_are_local() {
        let kiritimati = TimeZone::fixed(14 * 3600).unwrap();
        assert_eq!(file(kiritimati.day_start(day(2024, 6, 20))), "2024-06-19 10:00:00 +0000");
        assert_eq!(file(kiritimati.day_end(day(2024, 6, 20))), "2024-06-20 09:59:59 +0000");
        let pago = TimeZone::fixed(-11 * 3600).unwrap();
        assert_eq!(file(pago.day_start(day(2024, 6, 20))), "2024-06-20 11:00:00 +0000");
        assert_eq!(file(pago.day_end(day(2024, 6, 20))), "2024-06-21 10:59:59 +0000");
        assert_eq!(file(TimeZone::utc().day_end(day(2024, 6, 20))), "2024-06-20 23:59:59 +0000");
    }

    #[test]
    fn gaps_move_an_hour_later_and_folds_take_the_later_instant() {
        let ny = new_york();
        // 2024-03-10 02:30 does not exist in New York: GnuCash takes 03:30 EDT.
        assert_eq!(file(ny.instant(day(2024, 3, 10), 2, 30, 0).unwrap()), "2024-03-10 07:30:00 +0000");
        // 2024-11-03 01:30 happens twice: GnuCash takes the second, 01:30 EST.
        assert_eq!(file(ny.instant(day(2024, 11, 3), 1, 30, 0).unwrap()), "2024-11-03 06:30:00 +0000");
        assert_eq!(file(ny.day_start(day(2024, 7, 4))), "2024-07-04 04:00:00 +0000");
        assert_eq!(file(ny.day_start(day(2024, 1, 4))), "2024-01-04 05:00:00 +0000");
    }

    #[test]
    fn local_dates_and_same_day() {
        let kiritimati = TimeZone::fixed(14 * 3600).unwrap();
        let t = Time64::parse_file_string("2024-06-19 10:30:00 +0000").unwrap();
        assert_eq!(kiritimati.local_date(t), Some(day(2024, 6, 20)));
        assert_eq!(TimeZone::utc().local_date(t), Some(day(2024, 6, 19)));
        let later = Time64::parse_file_string("2024-06-20 09:00:00 +0000").unwrap();
        assert!(kiritimati.same_day(t, later), "different UTC days, one local day");
        assert!(!TimeZone::utc().same_day(t, later));
        assert_eq!(kiritimati.to_local(t).unwrap().compact(), "20240620003000");
    }

    #[test]
    fn tzif_data_is_read() {
        // A TZif version 1 file for a zone fixed at UTC+14 with no transitions.
        let mut data = Vec::new();
        data.extend_from_slice(b"TZif\0");
        data.extend_from_slice(&[0; 15]);
        [0_u32, 0, 0, 0, 1, 4].iter().for_each(|n| data.extend_from_slice(&n.to_be_bytes()));
        data.extend_from_slice(&50_400_i32.to_be_bytes());
        data.extend_from_slice(&[0, 0]);
        data.extend_from_slice(b"+14\0");
        let zone = TimeZone::from_tzif("Pacific/Kiritimati", &data).unwrap();
        assert_eq!(zone.offset_at(Time64(1_718_880_000)), 50_400);
        assert_eq!(zone.name(), "Pacific/Kiritimati");
        assert!(TimeZone::from_tzif("junk", b"not a zone").is_err());
    }

    proptest::proptest! {
        /// Every fixed zone GnuCash's neutral time is designed for (UTC−12 to UTC+13, in quarter hours) keeps the
        /// date, and the day's start, neutral time and end are in order, a day apart.
        #[test]
        fn neutral_time_lies_inside_the_local_day(days in 0_i64..73_000, quarters in -48_i32..=52) {
            let zone = TimeZone::fixed(quarters * 900).unwrap();
            let d = day(1900, 1, 1).add_days(days);
            let (start, neutral, end) = (zone.day_start(d), zone.neutral_time(d), zone.day_end(d));
            proptest::prop_assert_eq!(zone.local_date(neutral), Some(d));
            proptest::prop_assert!(start < neutral && neutral < end);
            proptest::prop_assert_eq!(end.0 - start.0, 86_399);
        }
    }
}
