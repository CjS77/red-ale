//! Dates in business import files, in GnuCash's date format preference.
//!
//! GnuCash checks each date with `strptime` in the preference's format string (`isDateValid`,
//! `5.17:gnucash/import-export/bi-import/dialog-bi-import-helper.c:52-66`) and then reads it with `qof_scan_date`
//! (`5.17:libgnucash/engine/gnc-date.cpp:742-988`, GPL-2.0-or-later). red-ale does both in one parser
//! (`design/api/import.md`, "Business documents"), so the date checked is the date stored: glibc's `strptime` for the
//! directives GnuCash's formats and the locales' `D_FMT`s use, then `qof_scan_date`'s year completion.
//!
//! Kept from GnuCash: numbers take as many digits as fit their range (`get_number` in glibc's `strptime`), leading
//! white space before a number is skipped, a space in the format matches any white space, a year below 100 is taken
//! in the century nearest today (`gnc-date.cpp:972-975`), and text after the date is ignored. Deviations: a day the
//! month doesn't have is invalid, where `qof_scan_date` rolls it into the next month (`normalize_struct_tm`); and a
//! digit right after the date makes it invalid, where `strptime` would stop at a two-digit `%y` in `1/15/2024` and
//! GnuCash would store 2020.

use red_ale_core::Date;
use red_ale_locale::Locale;
use serde::{Deserialize, Serialize};

/// GnuCash's date format preference (`QofDateFormat`), as `design/api/import.md` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DateFormat {
    /// `%m/%d/%Y`.
    Us,
    /// `%d/%m/%Y`.
    Uk,
    /// `%d.%m.%Y`.
    Ce,
    /// `%Y-%m-%d`.
    Iso,
    /// The Locale's `D_FMT`.
    #[default]
    Locale,
}

impl DateFormat {
    /// The format by its name.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "us" => Self::Us,
            "uk" => Self::Uk,
            "ce" => Self::Ce,
            "iso" => Self::Iso,
            "locale" => Self::Locale,
            _ => return None,
        })
    }

    /// `qof_date_format_get_string` (`5.17:libgnucash/engine/gnc-date.cpp:504-521`).
    pub fn format_string(self, locale: &Locale) -> &'static str {
        match self {
            Self::Us => "%m/%d/%Y",
            Self::Uk => "%d/%m/%Y",
            Self::Ce => "%d.%m.%Y",
            Self::Iso => "%Y-%m-%d",
            Self::Locale => locale.time().d_fmt,
        }
    }
}

/// What reading a date needs: the format string and the Locale's names, and today for the century of a short year.
#[derive(Debug, Clone, Copy)]
pub struct DateContext<'a> {
    /// The `strptime` format.
    pub format: &'a str,
    /// The Locale, for `%a`, `%A`, `%b` and `%B`.
    pub locale: &'a Locale,
    /// Today.
    pub today: Date,
}

impl<'a> DateContext<'a> {
    /// The context for a preference.
    pub fn new(format: DateFormat, locale: &'a Locale, today: Date) -> Self {
        Self { format: format.format_string(locale), locale, today }
    }
}

/// What `strptime` has read so far.
#[derive(Default)]
struct Fields {
    day: Option<u32>,
    month: Option<u32>,
    year: Option<i32>,
}

/// `normalize_format` (`gnc-date.cpp:705-718`): the `E`, `O` and `-` modifiers removed.
fn normalize_format(format: &str) -> String {
    let mut out = String::with_capacity(format.len());
    let mut after_percent = false;
    format.chars().for_each(|c| {
        if !(after_percent && matches!(c, 'E' | 'O' | '-')) {
            out.push(c);
        }
        after_percent = c == '%' && !after_percent;
    });
    out
}

/// glibc's `get_number`: skip white space, then digits while the value can still grow within `to`, at most `max`.
fn get_number(input: &str, from: u32, to: u32, max: usize) -> Option<(u32, &str)> {
    let rest = input.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let bytes = rest.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mut val = 0_u32;
    let mut used = 0;
    while used < max && bytes.get(used).is_some_and(u8::is_ascii_digit) && (used == 0 || val * 10 <= to) {
        val = val * 10 + u32::from(bytes[used] - b'0');
        used += 1;
    }
    (from..=to).contains(&val).then(|| (val, &rest[used..]))
}

/// The first of `names` that `input` starts with, ignoring ASCII case: its index and the rest.
fn match_name<'s>(input: &'s str, names: &[&str]) -> Option<(usize, &'s str)> {
    names.iter().enumerate().find_map(|(i, n)| {
        let head = input.get(..n.len())?;
        (!n.is_empty() && head.eq_ignore_ascii_case(n)).then(|| (i, &input[n.len()..]))
    })
}

/// One `%` directive.
fn directive<'s>(d: char, input: &'s str, ctx: &DateContext<'_>, f: &mut Fields) -> Option<&'s str> {
    let time = ctx.locale.time();
    match d {
        'd' | 'e' => get_number(input, 1, 31, 2).map(|(v, rest)| {
            f.day = Some(v);
            rest
        }),
        'm' => get_number(input, 1, 12, 2).map(|(v, rest)| {
            f.month = Some(v);
            rest
        }),
        'Y' => get_number(input, 0, 9999, 4).map(|(v, rest)| {
            f.year = i32::try_from(v).ok();
            rest
        }),
        // glibc: 69-99 are 1969-1999, 0-68 are 2000-2068.
        'y' => get_number(input, 0, 99, 2).map(|(v, rest)| {
            f.year = i32::try_from(v).ok().map(|y| if y >= 69 { 1900 + y } else { 2000 + y });
            rest
        }),
        'b' | 'B' | 'h' => match_name(input, &time.mon).or_else(|| match_name(input, &time.abmon)).map(|(i, rest)| {
            f.month = u32::try_from(i + 1).ok();
            rest
        }),
        'a' | 'A' => match_name(input, &time.day).or_else(|| match_name(input, &time.abday)).map(|(_, rest)| rest),
        'n' | 't' => Some(input.trim_start_matches(|c: char| c.is_ascii_whitespace())),
        '%' => input.strip_prefix('%'),
        _ => None,
    }
}

/// `strptime(text, format)`: the fields read and the text left, or `None` when the text doesn't fit the format.
fn strptime<'s>(text: &'s str, ctx: &DateContext<'_>) -> Option<(Fields, &'s str)> {
    let format = normalize_format(ctx.format);
    let mut fields = Fields::default();
    let mut input = text;
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        input = match c {
            '%' => directive(chars.next()?, input, ctx, &mut fields)?,
            c if c.is_ascii_whitespace() => input.trim_start_matches(|c: char| c.is_ascii_whitespace()),
            c => input.strip_prefix(c)?,
        };
    }
    Some((fields, input))
}

/// `qof_scan_date`'s century for a year below 100: the one that puts it nearest today (`gnc-date.cpp:972-975`).
fn complete_year(year: i32, today: Date) -> i32 {
    if year < 100 { year + ((today.year() + 50 - year) / 100) * 100 } else { year }
}

/// A date as GnuCash's business importers check and read it, or `None` when it is not valid.
pub fn scan_date(text: &str, ctx: &DateContext<'_>) -> Option<Date> {
    let (fields, rest) = strptime(text, ctx)?;
    if rest.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    // A field the format doesn't name is today's, as qof_scan_date defaults day and month (gnc-date.cpp:801-804).
    let year = complete_year(fields.year.unwrap_or_else(|| ctx.today.year()), ctx.today);
    Date::from_ymd(year, fields.month.unwrap_or_else(|| ctx.today.month()), fields.day.unwrap_or_else(|| ctx.today.day()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn scan(text: &str, format: &str) -> Option<String> {
        let c = Locale::c();
        let ctx = DateContext { format, locale: &c, today: Date::from_ymd(2026, 10, 26).unwrap() };
        scan_date(text, &ctx).map(Date::to_file_string)
    }

    #[test]
    fn the_four_fixed_formats() {
        assert_eq!(scan("01/15/2024", "%m/%d/%Y").as_deref(), Some("2024-01-15"));
        assert_eq!(scan("15/01/2024", "%d/%m/%Y").as_deref(), Some("2024-01-15"));
        assert_eq!(scan("15.1.2024", "%d.%m.%Y").as_deref(), Some("2024-01-15"));
        assert_eq!(scan("2024-01-15", "%Y-%m-%d").as_deref(), Some("2024-01-15"));
        assert_eq!(scan(" 2024- 1-5", "%Y-%m-%d").as_deref(), Some("2024-01-05"), "white space before a number is skipped");
        assert_eq!(scan("2024-01-15 10:30", "%Y-%m-%d").as_deref(), Some("2024-01-15"), "text after the date is ignored");
    }

    #[test]
    fn invalid_dates() {
        assert_eq!(scan("15/01/2024", "%m/%d/%Y"), None, "month 15");
        assert_eq!(scan("2024-01-15", "%m/%d/%Y"), None);
        assert_eq!(scan("", "%m/%d/%Y"), None);
        assert_eq!(scan("abc", "%Y-%m-%d"), None);
        assert_eq!(scan("02/30/2024", "%m/%d/%Y"), None, "GnuCash rolls this into March");
        assert_eq!(scan("1/15/2024", "%m/%d/%y"), None, "GnuCash reads the year as 2020");
        assert_eq!(scan("45/01/2024", "%d/%m/%Y"), None, "the day takes one digit, then the separator doesn't match");
    }

    #[test]
    fn short_years_take_the_nearest_century() {
        assert_eq!(scan("1/15/24", "%m/%d/%Y").as_deref(), Some("2024-01-15"));
        assert_eq!(scan("1/15/80", "%m/%d/%Y").as_deref(), Some("1980-01-15"));
        assert_eq!(scan("1/15/76", "%m/%d/%Y").as_deref(), Some("2076-01-15"));
        assert_eq!(scan("1/15/68", "%m/%d/%y").as_deref(), Some("2068-01-15"), "strptime's %y");
        assert_eq!(scan("1/15/69", "%m/%d/%y").as_deref(), Some("1969-01-15"));
    }

    #[test]
    fn locale_formats_with_names_and_modifiers() {
        assert_eq!(scan("15 Jan, 2024", "%d %b, %Y").as_deref(), Some("2024-01-15"));
        assert_eq!(scan("Monday 15 january 2024", "%A %e %B %Y").as_deref(), Some("2024-01-15"));
        assert_eq!(scan("5-1-24", "%-d-%-m-%y").as_deref(), Some("2024-01-05"));
        assert_eq!(scan("24/01/15", "%Oy/%Om/%Od").as_deref(), Some("2024-01-15"));
        assert_eq!(scan("2024-01-15", "%Q").as_deref(), None, "an unknown directive");
    }
}
