//! Dates in GnuCash's `Locale` date format: ICU's default date pattern for the LC_TIME locale, read strictly.
//!
//! GnuCash parses a `Locale` date with ICU (`gregorian_date_from_locale_string`,
//! `5.17:libgnucash/engine/gnc-datetime.cpp:684-703`): `DateFormat::createDateInstance (kDefault, locale)` for the
//! LC_TIME locale (`:653-681`), which is ICU's medium date pattern (`MMM d, y` under en_US, `d MMM y` under fr_FR,
//! `dd.MM.y` under de_DE), and the whole text must be used. There is no fallback: when ICU refuses the text, the
//! date is an error. red-ale takes the pattern and the month names from ICU4X ([`crate::Locale::icu_date_pattern`]).
//!
//! ICU's parser is lenient by default, and its leniency can't be reproduced exactly. red-ale reads the pattern
//! strictly, with three allowances ICU also makes:
//!
//! - whitespace around the text is ignored, and a space in the pattern (including the no-break spaces CLDR uses)
//!   matches any run of whitespace, or none;
//! - the bidirectional marks CLDR puts in Arabic patterns (U+200E, U+200F, U+061C) are ignored, in the pattern and
//!   in the text;
//! - a text month (`MMM`, `MMMM`) may be the abbreviated or the wide name, in any case, and numbers may be written
//!   in ASCII digits or the locale's own.
//!
//! Where ICU's lenient parser goes further, red-ale refuses: a day the month doesn't have (ICU rolls 30 February over
//! to March), a number where the pattern has a month name, missing or different literal text, and trailing text.
//! A two-digit year in a `y` or `yy` field is taken in the century that starts 80 years before today, as ICU's default
//! century does. GnuCash then makes a boost date of the result, which holds only the years 1400 to 9999.
//!
//! A locale whose calendar is not Gregorian (fa_IR's is Persian) is refused: GnuCash would read its year, month and
//! day as Gregorian ones and get a date centuries away.

use crate::date::CivilDate;
use crate::icu;

/// Why there is no parser for a locale, or why a text is not a date in it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LocaleDateError {
    /// ICU4X has no date pattern for the locale.
    #[error("ICU has no date format for the locale {0}")]
    NoPattern(String),
    /// The locale's calendar is not Gregorian.
    #[error("the locale's dates are in the {calendar} calendar; red-ale reads only Gregorian dates")]
    Calendar {
        /// The calendar, as ICU4X names it.
        calendar: String,
    },
    /// The pattern has a field this parser does not read, or lacks a year, month or day.
    #[error("the locale's date format {pattern:?} has fields red-ale can't read: {fields}")]
    Fields {
        /// The pattern.
        pattern: String,
        /// The fields at fault.
        fields: String,
    },
    /// The text does not follow the pattern.
    #[error("Value can't be parsed into a date using the locale's date format ({pattern}).")]
    NoMatch {
        /// The pattern.
        pattern: String,
    },
    /// The day is not one the month has.
    #[error("Day of month is not valid for year")]
    Day,
    /// The year is outside what GnuCash's dates hold.
    #[error("Year is out of valid range: 1400..9999")]
    Year,
}

/// One part of a UTS 35 date pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    /// `y`, `yy`, …: the year, with the field's width.
    Year(usize),
    /// `M`, `MM` (a number) or `MMM`, `MMMM` (a name); `L` likewise.
    Month { name: bool },
    /// `d` or `dd`.
    Day,
    /// Text to match, quotes resolved and ignorable marks removed.
    Literal(Vec<char>),
}

/// The years GnuCash's dates hold (boost's `greg_year`).
const YEARS: std::ops::RangeInclusive<i32> = 1400..=9999;

/// A parser for one locale's default date pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleDateParser {
    pattern: String,
    items: Vec<Item>,
    /// Every month name, lower case, longest first, with its month.
    months: Vec<(Vec<char>, u8)>,
}

impl LocaleDateParser {
    /// The parser for an ICU locale tag (`fr-FR`).
    pub(crate) fn new(tag: &str) -> Result<Self, LocaleDateError> {
        let (pattern, calendar) = icu::medium_date_pattern(tag).ok_or_else(|| LocaleDateError::NoPattern(tag.to_owned()))?;
        if !matches!(calendar, icu_calendar::AnyCalendarKind::Gregorian | icu_calendar::AnyCalendarKind::Iso) {
            return Err(LocaleDateError::Calendar { calendar: format!("{calendar:?}") });
        }
        let items = items(&pattern)?;
        let names = icu::month_names(tag).ok_or_else(|| LocaleDateError::NoPattern(tag.to_owned()))?;
        Ok(Self { pattern, items, months: month_table(&names) })
    }

    /// The pattern, in UTS 35 syntax.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// The date `text` holds. `today` places a two-digit year.
    pub fn parse(&self, text: &str, today: CivilDate) -> Result<CivilDate, LocaleDateError> {
        let input: Vec<char> = text.trim().chars().filter(|c| !is_ignorable(*c)).collect();
        let no_match = || LocaleDateError::NoMatch { pattern: self.pattern.clone() };
        let mut cursor = Cursor { input: &input, pos: 0 };
        let mut fields = Fields::default();
        self.items.iter().try_for_each(|item| cursor.item(item, &self.months, &mut fields)).ok_or_else(no_match)?;
        if cursor.pos != input.len() {
            return Err(no_match());
        }
        let (Some((year, digits, width)), Some(month), Some(day)) = (fields.year, fields.month, fields.day) else {
            return Err(no_match());
        };
        let month = u8::try_from(month).map_err(|_| LocaleDateError::NoMatch { pattern: self.pattern.clone() })?;
        let day = u8::try_from(day).map_err(|_| LocaleDateError::Day)?;
        let year = i32::try_from(year).map_err(|_| LocaleDateError::Year)?;
        let year = if width <= 2 && digits == 2 { two_digit_year(year, month, day, today) } else { year };
        if !(1..=12).contains(&month) {
            return Err(no_match());
        }
        if !YEARS.contains(&year) {
            return Err(LocaleDateError::Year);
        }
        CivilDate::new(year, month, day).ok_or(LocaleDateError::Day)
    }
}

/// The fields read so far: the year with its digit count and field width, the month and the day.
#[derive(Default)]
struct Fields {
    year: Option<(u32, usize, usize)>,
    month: Option<u32>,
    day: Option<u32>,
}

/// Where reading the text has got to.
struct Cursor<'a> {
    input: &'a [char],
    pos: usize,
}

impl Cursor<'_> {
    fn item(&mut self, item: &Item, months: &[(Vec<char>, u8)], fields: &mut Fields) -> Option<()> {
        match item {
            Item::Literal(text) => self.literal(text),
            Item::Year(width) => {
                let (value, digits) = self.number(4)?;
                fields.year = Some((value, digits, *width));
                Some(())
            }
            Item::Month { name: true } => {
                fields.month = Some(u32::from(self.month_name(months)?));
                Some(())
            }
            Item::Month { name: false } => {
                fields.month = Some(self.number(2)?.0);
                Some(())
            }
            Item::Day => {
                fields.day = Some(self.number(2)?.0);
                Some(())
            }
        }
    }

    /// Literal text: a space matches any run of whitespace, or none; anything else must be there, in any case.
    fn literal(&mut self, text: &[char]) -> Option<()> {
        text.iter().try_for_each(|&c| {
            if c.is_whitespace() {
                self.pos += self.input[self.pos..].iter().take_while(|c| c.is_whitespace()).count();
                Some(())
            } else {
                let got = *self.input.get(self.pos)?;
                same_char(got, c).then(|| self.pos += 1)
            }
        })
    }

    /// One to `max` decimal digits, in any script, and how many there were. More digits than `max` is no match.
    fn number(&mut self, max: usize) -> Option<(u32, usize)> {
        let digits: Vec<u32> = self.input[self.pos..].iter().map_while(|c| digit_value(*c)).collect();
        if digits.is_empty() || digits.len() > max {
            return None;
        }
        self.pos += digits.len();
        Some((digits.iter().fold(0, |n, d| n * 10 + d), digits.len()))
    }

    /// The longest month name the text starts with.
    fn month_name(&mut self, months: &[(Vec<char>, u8)]) -> Option<u8> {
        let (end, month) = months.iter().find_map(|(name, month)| self.name_end(name).map(|end| (end, *month)))?;
        self.pos = end;
        Some(month)
    }

    /// Where `name` ends when the text at the cursor starts with it; a space in it matches a run of whitespace.
    fn name_end(&self, name: &[char]) -> Option<usize> {
        name.iter().try_fold(self.pos, |pos, &c| {
            if c.is_whitespace() {
                let run = self.input.get(pos..)?.iter().take_while(|c| c.is_whitespace()).count();
                (run > 0).then_some(pos + run)
            } else {
                same_char(*self.input.get(pos)?, c).then_some(pos + 1)
            }
        })
    }
}

/// Two characters equal ignoring case.
fn same_char(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// The marks CLDR puts in patterns for bidirectional text, which ICU skips as pattern whitespace.
fn is_ignorable(c: char) -> bool {
    matches!(c, '\u{200e}' | '\u{200f}' | '\u{061c}')
}

/// The first code point of each run of ten decimal digits ICU4X's locales print: ASCII, Arabic-Indic, extended
/// Arabic-Indic (Persian), N'Ko, Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam,
/// Sinhala, Thai, Lao, Tibetan, Myanmar, Khmer, Mongolian, Javanese, Meetei Mayek and full-width.
const DIGIT_ZEROS: [u32; 23] = [
    0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66, 0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x17e0, 0x1810,
    0xa9d0, 0xabf0, 0xff10,
];

/// A decimal digit's value, in any of the scripts above.
fn digit_value(c: char) -> Option<u32> {
    let code = u32::from(c);
    DIGIT_ZEROS.iter().find(|zero| (**zero..*zero + 10).contains(&code)).map(|zero| code - zero)
}

/// ICU's two-digit year: in the century starting 80 years before today, a year equal to the start's two digits
/// going to the later century when the date falls before the start (`SimpleDateFormat::subParse`).
fn two_digit_year(two: i32, month: u8, day: u8, today: CivilDate) -> i32 {
    let start = today.year() - 80;
    let ambiguous = start.rem_euclid(100);
    let year = start.div_euclid(100) * 100 + two + if two < ambiguous { 100 } else { 0 };
    let before_start = (month, day) < (today.month(), today.day());
    if two == ambiguous && year == start && before_start { year + 100 } else { year }
}

/// The month names as matched: lower case without ignorable marks, longest first so that `juil.` is tried before a
/// shorter name it starts with.
fn month_table(names: &[Vec<String>; 12]) -> Vec<(Vec<char>, u8)> {
    let mut table: Vec<(Vec<char>, u8)> = (1..=12u8)
        .zip(names.iter())
        .flat_map(|(month, list)| list.iter().map(move |n| (n.chars().filter(|c| !is_ignorable(*c)).flat_map(char::to_lowercase).collect(), month)))
        .filter(|(name, _): &(Vec<char>, u8)| !name.is_empty())
        .collect();
    table.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.1.cmp(&b.1)));
    table
}

/// The items of a UTS 35 pattern: runs of an ASCII letter are fields, text in single quotes is literal (`''` is a
/// quote), anything else is literal. Exactly one year, month and day field are needed, and no other field.
fn items(pattern: &str) -> Result<Vec<Item>, LocaleDateError> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut items = Vec::new();
    let mut literal = Vec::new();
    let mut unknown = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            let (text, next) = quoted(&chars, i);
            literal.extend(text);
            i = next;
        } else if c.is_ascii_alphabetic() {
            let count = chars[i..].iter().take_while(|&&d| d == c).count();
            if !literal.is_empty() {
                items.push(Item::Literal(std::mem::take(&mut literal)));
            }
            match field(c, count) {
                Some(item) => items.push(item),
                None => unknown.push(c.to_string().repeat(count)),
            }
            i += count;
        } else {
            if !is_ignorable(c) {
                literal.push(c);
            }
            i += 1;
        }
    }
    if !literal.is_empty() {
        items.push(Item::Literal(literal));
    }
    let count = |f: fn(&Item) -> bool| items.iter().filter(|i| f(i)).count();
    let complete = count(|i| matches!(i, Item::Year(_))) == 1 && count(|i| matches!(i, Item::Month { .. })) == 1 && count(|i| *i == Item::Day) == 1;
    if !unknown.is_empty() || !complete {
        let fields = if unknown.is_empty() { "not one each of year, month and day".to_owned() } else { unknown.join(", ") };
        return Err(LocaleDateError::Fields { pattern: pattern.to_owned(), fields });
    }
    Ok(items)
}

/// A quoted part starting at `start`: its text and the index after it. `''` is a quote, inside quotes or out.
fn quoted(chars: &[char], start: usize) -> (Vec<char>, usize) {
    if chars.get(start + 1) == Some(&'\'') {
        return (vec!['\''], start + 2);
    }
    let mut text = Vec::new();
    let mut i = start + 1;
    while i < chars.len() {
        match (chars[i], chars.get(i + 1)) {
            ('\'', Some('\'')) => {
                text.push('\'');
                i += 2;
            }
            ('\'', _) => return (text, i + 1),
            (c, _) => {
                text.push(c);
                i += 1;
            }
        }
    }
    (text, i)
}

/// The item a field of `count` letters `c` is, if this parser reads it.
fn field(c: char, count: usize) -> Option<Item> {
    match (c, count) {
        ('y', _) => Some(Item::Year(count)),
        ('M' | 'L', 1..=2) => Some(Item::Month { name: false }),
        ('M' | 'L', 3..=4) => Some(Item::Month { name: true }),
        ('d', 1..=2) => Some(Item::Day),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u8, d: u8) -> CivilDate {
        CivilDate::new(y, m, d).unwrap()
    }

    fn today() -> CivilDate {
        date(2026, 10, 26)
    }

    #[test]
    fn reads_patterns() {
        assert_eq!(items("MMM d, y").unwrap().len(), 5);
        let pt = items("d 'de' MMM 'de' y").unwrap();
        assert_eq!(pt[1], Item::Literal(" de ".chars().collect()));
        assert_eq!(items("y MMM d'(a)'").unwrap().last(), Some(&Item::Literal("(a)".chars().collect())));
        assert_eq!(items("h:mm 'o''clock' y M d").unwrap_err().to_string(), "the locale's date format \"h:mm 'o''clock' y M d\" has fields red-ale can't read: h, mm");
        assert!(items("d MMM").is_err(), "no year");
        assert_eq!(quoted(&"''x".chars().collect::<Vec<_>>(), 0), (vec!['\''], 2));
        assert_eq!(quoted(&"'it''s'".chars().collect::<Vec<_>>(), 0), ("it's".chars().collect(), 7));
    }

    #[test]
    fn two_digit_years_follow_icus_default_century() {
        assert_eq!(two_digit_year(24, 2, 5, today()), 2024);
        assert_eq!(two_digit_year(50, 2, 5, today()), 1950);
        assert_eq!(two_digit_year(45, 2, 5, today()), 2045);
        assert_eq!(two_digit_year(46, 12, 1, today()), 1946, "after the century start");
        assert_eq!(two_digit_year(46, 1, 1, today()), 2046, "before it");
    }

    #[test]
    fn digits_in_other_scripts() {
        assert_eq!(digit_value('7'), Some(7));
        assert_eq!(digit_value('\u{0665}'), Some(5));
        assert_eq!(digit_value('\u{06f9}'), Some(9));
        assert_eq!(digit_value('\u{096b}'), Some(5));
        assert_eq!(digit_value('x'), None);
    }
}
