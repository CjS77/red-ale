//! Calendar dates as a locale writes them: `strftime` over the directives glibc's `D_FMT`s use, after GnuCash's
//! normalisation of the format.

use std::fmt::Write as _;

use crate::data::Time;

/// A day of the proleptic Gregorian calendar, valid on construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDate {
    year: i32,
    month: u8,
    day: u8,
}

impl CivilDate {
    /// The date, or `None` when there is no such day.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return None,
        };
        (1..=days).contains(&day).then_some(Self { year, month, day })
    }

    /// The year.
    pub fn year(self) -> i32 {
        self.year
    }

    /// The month, 1 to 12.
    pub fn month(self) -> u8 {
        self.month
    }

    /// The day of the month, from 1.
    pub fn day(self) -> u8 {
        self.day
    }

    /// The day of the week.
    pub fn weekday(self) -> Weekday {
        // Days since 1970-01-01 (Howard Hinnant's `days_from_civil`), a Thursday.
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        Weekday::from_sunday((days + 4).rem_euclid(7))
    }
}

/// A day of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)]
pub enum Weekday {
    Sunday,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
}

impl Weekday {
    /// Every day, from Sunday, as `struct tm`'s `tm_wday` counts them.
    pub const ALL: [Weekday; 7] = [
        Self::Sunday,
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
    ];

    /// `tm_wday`: 0 for Sunday to 6 for Saturday.
    pub fn days_from_sunday(self) -> usize {
        self as usize
    }

    fn from_sunday(n: i64) -> Self {
        usize::try_from(n)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
            .unwrap_or(Self::Sunday)
    }
}

/// GnuCash's `normalize_format` (`5.17:libgnucash/engine/gnc-datetime.cpp:517-533`): Boost's date facets do not
/// support the `E`, `O` and `-` modifiers, so GnuCash drops them and `%-d` prints as `%d`, zero-padded.
fn normalize(format: &str) -> String {
    let mut after_percent = false;
    format
        .chars()
        .filter(|&c| {
            let dropped = after_percent && matches!(c, 'E' | 'O' | '-');
            after_percent = c == '%';
            !dropped
        })
        .collect()
}

/// `date` under `format` with LC_TIME's names, as `GncDate::format` prints it
/// (`5.17:libgnucash/engine/gnc-datetime.cpp:764-777`): `%d` `%e` `%m` `%y` `%Y` `%a` `%A` `%b` `%B` and `%%`. Any
/// other directive is copied as it is. `%Y` has four digits at least, as red-ale's other date formats.
pub(crate) fn format(time: &Time, format_text: &str, date: CivilDate) -> String {
    let normalized = normalize(format_text);
    let mut out = String::with_capacity(normalized.len() + 8);
    let mut chars = normalized.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some(d) => directive(&mut out, time, d, date),
            None => out.push('%'),
        }
    }
    out
}

fn directive(out: &mut String, time: &Time, d: char, date: CivilDate) {
    let (year, month, day) = (date.year, usize::from(date.month), date.day);
    let weekday = date.weekday().days_from_sunday();
    let name = |names: &[&'static str]| names.get(month.saturating_sub(1)).copied().unwrap_or_default();
    // Writing to a String cannot fail.
    let _ = match d {
        'd' => write!(out, "{day:02}"),
        'e' => write!(out, "{day:2}"),
        'm' => write!(out, "{month:02}"),
        'y' => write!(out, "{:02}", year.rem_euclid(100)),
        'Y' => write!(out, "{year:04}"),
        'a' => write!(out, "{}", time.abday[weekday]),
        'A' => write!(out, "{}", time.day[weekday]),
        'b' | 'h' => write!(out, "{}", name(&time.abmon)),
        'B' => write!(out, "{}", name(&time.mon)),
        '%' => write!(out, "%"),
        other => write!(out, "%{other}"),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekdays_are_counted_from_the_epoch() {
        let d = |y, m, day| CivilDate::new(y, m, day).unwrap().weekday();
        assert_eq!(d(1970, 1, 1), Weekday::Thursday);
        assert_eq!(d(2026, 10, 26), Weekday::Monday);
        assert_eq!(d(2000, 2, 29), Weekday::Tuesday);
        assert_eq!(d(1600, 3, 1), Weekday::Wednesday);
        assert_eq!(d(-1, 12, 31), Weekday::Friday);
    }

    #[test]
    fn invalid_dates_are_refused() {
        assert!(CivilDate::new(2026, 2, 29).is_none());
        assert!(CivilDate::new(2024, 2, 29).is_some());
        assert!(CivilDate::new(1900, 2, 29).is_none());
        assert!(CivilDate::new(2026, 13, 1).is_none());
        assert!(CivilDate::new(2026, 4, 31).is_none());
        assert!(CivilDate::new(2026, 4, 0).is_none());
    }

    #[test]
    fn modifiers_are_dropped_as_gnucash_drops_them() {
        assert_eq!(normalize("%-d/%-m/%y"), "%d/%m/%y");
        assert_eq!(normalize("%Oy/%Om/%Od"), "%y/%m/%d");
        // GnuCash's filter treats the second `%` of `%%` as opening a directive too.
        assert_eq!(normalize("%%-d"), "%%d");
        assert_eq!(normalize("a-b"), "a-b");
    }
}
