//! Dates in import files, read with the date format the user chose.
//!
//! Translated from `GncDate::c_formats` and `GncDateImpl::GncDateImpl(const std::string, const std::string)` in
//! `5.17:libgnucash/engine/gnc-datetime.cpp:89-162` and `:707-755` (Copyright 2015 John Ralls
//! <jralls@ceridwen.us>), and from boost's `parse_date` (`boost/date_time/date_parsing.hpp`, Boost 1.83), which
//! GnuCash tries first for the three formats with a year. GPL-2.0-or-later (GnuCash) and BSL-1.0 (Boost); this
//! translation is GPL-3.0-or-later.
//!
//! For `y-m-d`, `d-m-y` and `m-d-y` GnuCash first asks boost to read the text: fields split on `,`, `-`, `.`,
//! space and `/`, the first three taken in the format's order, the month as a number or an English month name
//! (`Aug`, `August`), and the year in 1400–9999. When boost fails GnuCash searches the text with its own regular
//! expression: three runs of ASCII digits separated by any of `-/.'` and space, or eight digits with no separators.
//! A year below 69 is 20xx and one below 100 is 19xx. The yearless formats `d-m` and `m-d` take the current year and
//! refuse text that has one. The search is not anchored, so text around the date is ignored.
//!
//! Numbers from the regular expression reach boost's date as C++ `int`s narrowed to `unsigned short`, so a month
//! of 65537 is January; this is kept.
//!
//! The `Locale` format parses with ICU's default date format for the locale (`gregorian_date_from_locale_string`,
//! `:684-703`). red-ale has no ICU; [`DateContext::locale_date`] supplies that parser, and without one a `Locale`
//! date is an error.

use std::fmt;

use red_ale_core::Date;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A date format of `GncDate::c_formats`, in GnuCash's index order (`gnc-datetime.cpp:95-162`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DateFormat {
    /// Year, month, day (index 0).
    #[default]
    #[serde(rename = "y-m-d")]
    Ymd,
    /// Day, month, year (index 1).
    #[serde(rename = "d-m-y")]
    Dmy,
    /// Month, day, year (index 2).
    #[serde(rename = "m-d-y")]
    Mdy,
    /// Day, month, current year (index 3).
    #[serde(rename = "d-m")]
    Dm,
    /// Month, day, current year (index 4).
    #[serde(rename = "m-d")]
    Md,
    /// The locale's own format (index 5).
    #[serde(rename = "locale")]
    Locale,
}

impl DateFormat {
    /// Every format, in GnuCash's index order.
    pub const ALL: [Self; 6] = [Self::Ymd, Self::Dmy, Self::Mdy, Self::Dm, Self::Md, Self::Locale];

    /// The index GnuCash saves (`DateFormat` in the state file).
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&f| f == self).unwrap_or_default()
    }

    /// The format with GnuCash's index.
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// GnuCash's name for the format (`GncDateFormat::m_fmt`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Ymd => "y-m-d",
            Self::Dmy => "d-m-y",
            Self::Mdy => "m-d-y",
            Self::Dm => "d-m",
            Self::Md => "m-d",
            Self::Locale => "Locale",
        }
    }

    /// The order of the fields, for the formats that have one.
    fn order(self) -> Option<[Field; 3]> {
        Some(match self {
            Self::Ymd => [Field::Year, Field::Month, Field::Day],
            Self::Dmy | Self::Dm => [Field::Day, Field::Month, Field::Year],
            Self::Mdy | Self::Md => [Field::Month, Field::Day, Field::Year],
            Self::Locale => return None,
        })
    }

    fn has_year(self) -> bool {
        matches!(self, Self::Ymd | Self::Dmy | Self::Mdy)
    }
}

/// The locale's date parser: the date the text holds, or why there is none.
pub type LocaleDateParser<'a> = &'a dyn Fn(&str) -> Result<Date, String>;

/// What a date parse needs from outside the text.
pub struct DateContext<'a> {
    /// Today's date in the user's time zone: the year of the yearless formats (`day_clock::local_day`).
    pub today: Date,
    /// The locale's date parser, for [`DateFormat::Locale`].
    pub locale_date: Option<LocaleDateParser<'a>>,
}

impl fmt::Debug for DateContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DateContext").field("today", &self.today).field("locale_date", &self.locale_date.is_some()).finish()
    }
}

/// Text that is not a date in the chosen format. The messages are those GnuCash shows.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DateError {
    /// No date found (`gnc-datetime.cpp:729-730`).
    #[error("Value can't be parsed into a date using the selected date format.")]
    NoMatch,
    /// A year in a yearless format (`gnc-datetime.cpp:733-735`).
    #[error("Value appears to contain a year while the selected format forbids this.")]
    YearForbidden,
    /// A number too large for an `int` (`std::stoi`'s exception).
    #[error("stoi")]
    TooLarge,
    /// boost's `bad_year`.
    #[error("Year is out of valid range: 1400..9999")]
    Year,
    /// boost's `bad_month`.
    #[error("Month number is out of range 1..12")]
    Month,
    /// boost's `bad_day_of_month` for a day above 31 or zero.
    #[error("Day of month value is out of range 1..31")]
    Day,
    /// boost's `bad_day_of_month` for a day the month doesn't have.
    #[error("Day of month is not valid for year")]
    DayOfMonth,
    /// The locale's parser refused the text, or there is none.
    #[error("{0}")]
    Locale(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Year,
    Month,
    Day,
}

/// `GncDate(str, fmt)`: the date `text` holds in `format`.
pub fn parse_date(text: &str, format: DateFormat, ctx: &DateContext) -> Result<Date, DateError> {
    let Some(order) = format.order() else {
        let parse = ctx.locale_date.ok_or_else(|| DateError::Locale("No locale date parser".to_owned()))?;
        return parse(text).map_err(DateError::Locale);
    };
    if format.has_year()
        && let Some(date) = boost_parse_date(text, order)
    {
        return Ok(date);
    }
    let found = search(text.as_bytes(), order, format.has_year()).ok_or(DateError::NoMatch)?;
    if !format.has_year() && found.year.is_some() {
        return Err(DateError::YearForbidden);
    }
    let year = match found.year {
        Some(digits) if format.has_year() => two_digit_year(stoi(digits)?),
        _ => ctx.today.year(),
    };
    make_date(narrow(year), narrow(stoi(found.month)?), narrow(stoi(found.day)?))
}

/// `year < 69` is 20xx and `year < 100` is 19xx (`gnc-datetime.cpp:743-747`).
fn two_digit_year(year: i32) -> i32 {
    if year < 69 {
        year + 2000
    } else if year < 100 {
        year + 1900
    } else {
        year
    }
}

/// `std::stoi` on a run of ASCII digits.
fn stoi(digits: &[u8]) -> Result<i32, DateError> {
    std::str::from_utf8(digits).ok().and_then(|s| s.parse().ok()).ok_or(DateError::TooLarge)
}

/// C++'s implicit `int` to `unsigned short` conversion: modulo 2^16.
fn narrow(n: i32) -> u16 {
    u16::try_from(n.rem_euclid(65_536)).unwrap_or_default()
}

/// `boost::gregorian::date(year, month, day)` with its range checks.
fn make_date(year: u16, month: u16, day: u16) -> Result<Date, DateError> {
    if !(1400..=9999).contains(&year) {
        return Err(DateError::Year);
    }
    if !(1..=12).contains(&month) {
        return Err(DateError::Month);
    }
    if !(1..=31).contains(&day) {
        return Err(DateError::Day);
    }
    Date::from_ymd(i32::from(year), u32::from(month), u32::from(day)).ok_or(DateError::DayOfMonth)
}

/// boost's `parse_date(s, order)`: `None` wherever boost throws.
fn boost_parse_date(text: &str, order: [Field; 3]) -> Option<Date> {
    let mut parts = text.split([',', '-', '.', ' ', '/']).filter(|p| !p.is_empty());
    let (mut year, mut month, mut day) = (0, 0, 0);
    order.iter().zip(parts.by_ref()).try_for_each(|(field, part)| {
        match field {
            Field::Year => year = lexical_u16(part)?,
            Field::Month => month = month_str_to_ushort(part)?,
            Field::Day => day = lexical_u16(part)?,
        }
        Some(())
    })?;
    make_date(year, month, day).ok()
}

/// `boost::lexical_cast<unsigned short>`: an optional `+` and ASCII digits, at most 65535.
fn lexical_u16(s: &str) -> Option<u16> {
    let digits = s.strip_prefix('+').unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// boost's `month_str_to_ushort`: a number when the text starts with a digit, else an English month name or its
/// first three letters in any case; an unknown name is month 13, which then fails.
fn month_str_to_ushort(s: &str) -> Option<u16> {
    const NAMES: [&str; 12] =
        ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];
    if s.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        return lexical_u16(s);
    }
    let lower = s.to_ascii_lowercase();
    let index = NAMES.iter().position(|name| *name == lower || name.get(..3) == Some(lower.as_str()));
    Some(index.and_then(|i| u16::try_from(i + 1).ok()).unwrap_or(13))
}

/// The groups a successful search captured.
struct Found<'a> {
    year: Option<&'a [u8]>,
    month: &'a [u8],
    day: &'a [u8],
}

/// `regex_search` with the format's expression: the leftmost position where either alternative matches, the
/// separated form tried first.
fn search(text: &[u8], order: [Field; 3], with_year: bool) -> Option<Found<'_>> {
    (0..=text.len()).find_map(|start| separated(text, start, order, with_year).or_else(|| packed(text, start, order, with_year)))
}

/// `(?<A>[0-9]+)[-/.' ]+(?<B>[0-9]+)[-/.' ]+(?<C>[0-9]+)`, or for the yearless formats
/// `(?<A>[0-9]+)[-/.' ]+(?<B>[0-9]+)(?:[-/.' ]+(?<YEAR>[0-9]+))?`. Digit and separator runs are disjoint classes,
/// so the greedy runs never need to give characters back.
fn separated(text: &[u8], start: usize, order: [Field; 3], with_year: bool) -> Option<Found<'_>> {
    let first = run(text, start, is_digit)?;
    let second = run(text, run(text, first.end, is_date_sep)?.end, is_digit)?;
    let third = run(text, second.end, is_date_sep).and_then(|sep| run(text, sep.end, is_digit));
    let third = if with_year { Some(third?) } else { third };
    Some(assign(order, [Some(&text[first]), Some(&text[second]), third.map(|r| &text[r])]))
}

/// `(?<A>[0-9]{n})(?<B>[0-9]{n})(?<C>[0-9]{n})` with the widths of the format (4-2-2 or 2-2-4), or for the yearless
/// formats `(?<A>[0-9]{2})(?<B>[0-9]{2})(?<YEAR>[0-9]+)?`.
fn packed(text: &[u8], start: usize, order: [Field; 3], with_year: bool) -> Option<Found<'_>> {
    let widths = if order[0] == Field::Year { [4, 2, 2] } else { [2, 2, 4] };
    let fixed = |from: usize, width: usize| {
        let end = from + width;
        text.get(from..end).filter(|s| s.iter().all(u8::is_ascii_digit)).map(|_| from..end)
    };
    let first = fixed(start, widths[0])?;
    let second = fixed(first.end, widths[1])?;
    let third = if with_year { Some(fixed(second.end, widths[2])?) } else { run(text, second.end, is_digit) };
    Some(assign(order, [Some(&text[first]), Some(&text[second]), third.map(|r| &text[r])]))
}

/// The maximal non-empty run of bytes in a class starting at `from`.
fn run(text: &[u8], from: usize, class: fn(u8) -> bool) -> Option<std::ops::Range<usize>> {
    let len = text.get(from..)?.iter().take_while(|&&b| class(b)).count();
    (len > 0).then_some(from..from + len)
}

fn is_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

fn is_date_sep(b: u8) -> bool {
    matches!(b, b'-' | b'/' | b'.' | b'\'' | b' ')
}

/// Name the captured groups by the format's field order.
fn assign(order: [Field; 3], groups: [Option<&[u8]>; 3]) -> Found<'_> {
    let get = |field: Field| order.iter().position(|&f| f == field).and_then(|i| groups[i]);
    Found { year: get(Field::Year), month: get(Field::Month).unwrap_or_default(), day: get(Field::Day).unwrap_or_default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(name: &str) -> DateFormat {
        DateFormat::ALL.into_iter().find(|f| f.name() == name).unwrap()
    }

    fn gnucash_ctx() -> DateContext<'static> {
        DateContext { today: Date::from_ymd(2026, 10, 26).unwrap(), locale_date: None }
    }

    /// `5.17:libgnucash/engine/test/gtest-gnc-datetime.cpp:74-242` (`test_str_format_constructor`): every case
    /// that parses, with today fixed.
    #[test]
    fn gnucash_str_format_constructor_valid() {
        let ctx = gnucash_ctx();
        let y = ctx.today.year();
        let ok: &[(&str, &str, i32, u32, u32)] = &[
            ("y-m-d", "2013-08-01", 2013, 8, 1),
            ("y-m-d", "2013-8-01", 2013, 8, 1),
            ("y-m-d", "2013-08-1", 2013, 8, 1),
            ("y-m-d", "2013-8-1", 2013, 8, 1),
            ("y-m-d", "13-08-01", 2013, 8, 1),
            ("y-m-d", "13-8-01", 2013, 8, 1),
            ("y-m-d", "13-08-1", 2013, 8, 1),
            ("y-m-d", "13-8-1", 2013, 8, 1),
            ("y-m-d", "2009/11/04", 2009, 11, 4),
            ("y-m-d", "1985.3.12", 1985, 3, 12),
            ("y-m-d", "3'6'8", 2003, 6, 8),
            ("y-m-d", "20130801", 2013, 8, 1),
            ("y-m-d", "2013 Aug 1", 2013, 8, 1),
            ("y-m-d", "2013 Aug 01", 2013, 8, 1),
            ("y-m-d", "2013 August 01", 2013, 8, 1),
            ("y-m-d", "2013-August-1", 2013, 8, 1),
            ("y-m-d", "2009/Nov/04", 2009, 11, 4),
            ("y-m-d", "1985.Mar.12", 1985, 3, 12),
            ("d-m-y", "01-08-2013", 2013, 8, 1),
            ("d-m-y", "01-8-2013", 2013, 8, 1),
            ("d-m-y", "1-08-2013", 2013, 8, 1),
            ("d-m-y", "1-8-2013", 2013, 8, 1),
            ("d-m-y", "01-08-13", 2013, 8, 1),
            ("d-m-y", "01-8-13", 2013, 8, 1),
            ("d-m-y", "1-08-13", 2013, 8, 1),
            ("d-m-y", "1-8-13", 2013, 8, 1),
            ("d-m-y", "04/11/2009", 2009, 11, 4),
            ("d-m-y", "12.3.1985", 1985, 3, 12),
            ("d-m-y", "8'6'3", 2003, 6, 8),
            ("d-m-y", "01082013", 2013, 8, 1),
            ("d-m-y", "1 Aug 2013", 2013, 8, 1),
            ("d-m-y", "1 Sep 2013", 2013, 9, 1),
            ("d-m-y", "1 September 2013", 2013, 9, 1),
            ("m-d-y", "08-01-2013", 2013, 8, 1),
            ("m-d-y", "8-01-2013", 2013, 8, 1),
            ("m-d-y", "08-1-2013", 2013, 8, 1),
            ("m-d-y", "8-1-2013", 2013, 8, 1),
            ("m-d-y", "08-01-13", 2013, 8, 1),
            ("m-d-y", "8-01-13", 2013, 8, 1),
            ("m-d-y", "08-1-13", 2013, 8, 1),
            ("m-d-y", "8-1-13", 2013, 8, 1),
            ("m-d-y", "11/04/2009", 2009, 11, 4),
            ("m-d-y", "3.12.1985", 1985, 3, 12),
            ("m-d-y", "6'8'3", 2003, 6, 8),
            ("m-d-y", "08012013", 2013, 8, 1),
            ("m-d-y", "November 4, 2009", 2009, 11, 4),
            ("m-d-y", "Nov 4, 2009", 2009, 11, 4),
            ("d-m", "01-08", y, 8, 1),
            ("d-m", "01-8", y, 8, 1),
            ("d-m", "1-08", y, 8, 1),
            ("d-m", "1-8", y, 8, 1),
            ("d-m", "04/11", y, 11, 4),
            ("d-m", "12.3", y, 3, 12),
            ("d-m", "8'6", y, 6, 8),
            ("d-m", "0108", y, 8, 1),
            ("m-d", "08-01", y, 8, 1),
            ("m-d", "8-01", y, 8, 1),
            ("m-d", "08-1", y, 8, 1),
            ("m-d", "8-1", y, 8, 1),
            ("m-d", "11/04", y, 11, 4),
            ("m-d", "3.12", y, 3, 12),
            ("m-d", "6'8", y, 6, 8),
            ("m-d", "0801", y, 8, 1),
        ];
        ok.iter().for_each(|&(f, s, year, month, day)| {
            assert_eq!(parse_date(s, fmt(f), &ctx), Ok(Date::from_ymd(year, month, day).unwrap()), "{f} {s}");
        });
    }

    /// The same test's invalid cases. GnuCash's -1 (`invalid_argument`) and -2 (`out_of_range`) are both errors
    /// here; the unknown format specifier case has no counterpart, since [`DateFormat`] is closed.
    #[test]
    fn gnucash_str_format_constructor_invalid() {
        let ctx = gnucash_ctx();
        let bad: &[(&str, &str)] = &[
            ("d-m-y", "0 Aug 2013"),
            ("d-m-y", "31 Sep 2013"),
            ("d-m-y", "31 September 2013"),
            ("d-m-y", "31/11/2009"),
            ("d-m-y", "34.3.1985"),
            ("m-d-y", "November 41, 2009"),
            ("m-d-y", "Nov 31, 2009"),
            ("y-m-d", "2013 Aug 0"),
            ("y-m-d", "2013 Feb 30"),
            ("y-m-d", "2013 August 0"),
            ("y-m-d", "2013-June-31"),
            ("y-m-d", "2009/Nov/0"),
            ("y-m-d", "1985.Mar.32"),
            ("d-m-y", "1 Sep 13"),
            ("d-m-y", "1 September 13"),
            ("m-d-y", "November 4, 24"),
            ("m-d-y", "Nov 4, 23"),
            ("m-d-y", "Nov 29, 24"),
            ("y-m-d", "13-June-11"),
            ("y-m-d", "130801"),
            ("d-m-y", "010813"),
            ("m-d-y", "080113"),
            ("y-m-d", "08-01"),
            ("y-m-d", "0801"),
            ("d-m-y", "01-08"),
            ("d-m-y", "0108"),
            ("m-d-y", "08-01"),
            ("m-d-y", "0801"),
            ("d-m", "01-08-2013"),
            ("d-m", "01-08-13"),
            ("d-m", "08-08-08"),
            ("d-m", "01082013"),
            ("d-m", "010813"),
            ("d-m", "20130108"),
            ("m-d", "08-01-2013"),
            ("m-d", "08-01-13"),
            ("m-d", "2013-08-01"),
            ("m-d", "09-08-01"),
            ("m-d", "08012013"),
            ("m-d", "080113"),
            ("m-d", "20130801"),
        ];
        bad.iter().for_each(|&(f, s)| assert!(parse_date(s, fmt(f), &ctx).is_err(), "{f} {s} should fail"));
    }

    #[test]
    fn errors_and_quirks() {
        let ctx = DateContext { today: Date::from_ymd(2026, 1, 1).unwrap(), locale_date: None };
        assert_eq!(parse_date("01-08-2013", DateFormat::Dm, &ctx), Err(DateError::YearForbidden));
        assert_eq!(parse_date("nothing", DateFormat::Ymd, &ctx), Err(DateError::NoMatch));
        assert_eq!(parse_date("31/11/2009", DateFormat::Dmy, &ctx), Err(DateError::DayOfMonth));
        assert_eq!(parse_date("2013-13-01", DateFormat::Ymd, &ctx), Err(DateError::Month));
        assert_eq!(parse_date("99999999999-1-1", DateFormat::Ymd, &ctx), Err(DateError::TooLarge));
        // Text around the date is ignored, and a time after it too.
        assert_eq!(parse_date("on 2013-08-01T10:00", DateFormat::Ymd, &ctx), Ok(Date::from_ymd(2013, 8, 1).unwrap()));
        // boost refuses year 99 (out of 1400..9999); the regex reads it as 1999.
        assert_eq!(parse_date("0099-01-02", DateFormat::Ymd, &ctx), Ok(Date::from_ymd(1999, 1, 2).unwrap()));
        // The month is narrowed to 16 bits.
        assert_eq!(parse_date("2013-65537-02", DateFormat::Ymd, &ctx), Ok(Date::from_ymd(2013, 1, 2).unwrap()));
        assert!(matches!(parse_date("1/2/2013", DateFormat::Locale, &ctx), Err(DateError::Locale(_))));
        let locale = |_: &str| Ok(Date::from_ymd(2000, 2, 3).unwrap());
        let ctx = DateContext { locale_date: Some(&locale), ..ctx };
        assert_eq!(parse_date("x", DateFormat::Locale, &ctx), Ok(Date::from_ymd(2000, 2, 3).unwrap()));
    }

    #[test]
    fn indices() {
        assert_eq!(DateFormat::from_index(2), Some(DateFormat::Mdy));
        assert_eq!(DateFormat::Locale.index(), 5);
        assert_eq!(DateFormat::from_index(6), None);
    }
}
