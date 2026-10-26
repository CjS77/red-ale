//! What GnuCash asks ICU rather than the C library: the first day of the week, the interval form of a date range, and
//! the pattern and month names its `Locale` date format parses with. red-ale asks ICU4X with its compiled CLDR data.
//!
//! ICU4X's CLDR is newer than the ICU a GnuCash build links (ICU 74, CLDR 44, for the oracle), so a few locales differ
//! where CLDR changed (`design/reports.md` §14).

use icu_calendar::types::Weekday as IcuWeekday;
use icu_calendar::week::WeekInformation;
use icu_calendar::{AnyCalendarKind, Date, Gregorian};
use icu_datetime::DateTimeFormatter;
use icu_datetime::fieldsets::YMD;
use icu_datetime::fieldsets::enums::DateFieldSet;
use icu_datetime::options::YearStyle;
use icu_datetime::pattern::{DateTimePattern, FixedCalendarDateTimeNames, MonthNameLength};
use icu_datetime::range::DateRangeFormatter;
use icu_locale_core::Locale as IcuLocale;
use writeable::{TryWriteable, Writeable};

use crate::date::{CivilDate, Weekday};

fn locale(tag: &str) -> Option<IcuLocale> {
    tag.parse().ok()
}

/// `gnc_start_of_week` (`5.17:libgnucash/engine/gnc-date.cpp:195-215`): ICU's `Calendar::getFirstDayOfWeek` for the
/// locale, which CLDR sets by territory. Sunday when ICU has no answer.
pub(crate) fn first_weekday(tag: &str) -> Weekday {
    let info = locale(tag).and_then(|l| WeekInformation::try_new((&l).into()).ok());
    info.map_or(Weekday::Sunday, |i| match i.first_weekday {
        IcuWeekday::Monday => Weekday::Monday,
        IcuWeekday::Tuesday => Weekday::Tuesday,
        IcuWeekday::Wednesday => Weekday::Wednesday,
        IcuWeekday::Thursday => Weekday::Thursday,
        IcuWeekday::Friday => Weekday::Friday,
        IcuWeekday::Saturday => Weekday::Saturday,
        IcuWeekday::Sunday => Weekday::Sunday,
    })
}

/// The years GnuCash's dates can hold (`5.17:libgnucash/engine/gnc-datetime.hpp:240`). ICU4X 2.4's range formatter
/// fails on years that need an era name, such as 1 BCE in es_MX (a `debug_assert` in `range/write.rs`), so it is
/// asked only about these.
const GNUCASH_YEARS: std::ops::RangeInclusive<i32> = 1400..=9999;

/// `icu_date_interval_format` (`5.17:libgnucash/engine/gnc-date.cpp:618-659`): ICU's `DateIntervalFormat` for the
/// skeleton `yMd` (`UDAT_YEAR_NUM_MONTH_DAY`), the earlier date first. `None` when ICU cannot format it, where GnuCash
/// falls back to `<from> to <to>`, and for a year GnuCash cannot hold.
pub(crate) fn date_interval(tag: &str, from: CivilDate, to: CivilDate) -> Option<String> {
    let (from, to) = if from > to { (to, from) } else { (from, to) };
    if !GNUCASH_YEARS.contains(&from.year()) || !GNUCASH_YEARS.contains(&to.year()) {
        return None;
    }
    let skeleton = YMD::short().with_year_style(YearStyle::Full);
    let formatter = DateRangeFormatter::try_new((&locale(tag)?).into(), skeleton).ok()?;
    let iso = |d: CivilDate| Date::try_new_iso(d.year(), d.month(), d.day()).ok();
    Some(formatter.format(&iso(from)?, &iso(to)?).write_to_string().into_owned())
}

/// ICU's default date pattern for a locale and the calendar it reads dates in: `DateFormat::createDateInstance
/// (kDefault, locale)` and `Calendar::createInstance (locale)` in `get_icu_resources`
/// (`5.17:libgnucash/engine/gnc-datetime.cpp:653-681`). `kDefault` is ICU's medium style, the field set `YMD::medium()`
/// here. `None` when ICU4X has no formatter for the locale.
pub(crate) fn medium_date_pattern(tag: &str) -> Option<(String, AnyCalendarKind)> {
    let formatter = DateTimeFormatter::try_new((&locale(tag)?).into(), YMD::medium()).ok()?;
    let sample = Date::try_new_iso(2000, 1, 1).ok()?;
    let pattern = formatter.format(&sample).pattern().write_to_string().into_owned();
    Some((pattern, formatter.calendar().kind()))
}

/// The Gregorian month names a pattern's `MMM` and `MMMM` fields print, January first: CLDR's abbreviated and wide
/// format names. ICU's parser accepts either for a text month field.
pub(crate) fn month_names(tag: &str) -> Option<[Vec<String>; 12]> {
    let locale = locale(tag)?;
    let lengths = [(MonthNameLength::Abbreviated, "MMM"), (MonthNameLength::Wide, "MMMM")];
    let per_length: Vec<Vec<String>> = lengths.iter().map(|(length, field)| month_names_of(&locale, *length, field)).collect::<Option<_>>()?;
    let mut out: [Vec<String>; 12] = Default::default();
    out.iter_mut().enumerate().for_each(|(i, names)| {
        *names = per_length.iter().filter_map(|l| l.get(i).cloned()).collect();
        names.dedup();
    });
    Some(out)
}

fn month_names_of(locale: &IcuLocale, length: MonthNameLength, field: &str) -> Option<Vec<String>> {
    let mut names = FixedCalendarDateTimeNames::<Gregorian, DateFieldSet>::try_new(locale.into()).ok()?;
    names.include_month_names(length).ok()?;
    let pattern: DateTimePattern = field.parse().ok()?;
    (1..=12)
        .map(|month| {
            let date = Date::try_new_gregorian(2000, month, 1).ok()?;
            names.with_pattern_unchecked(&pattern).format(&date).try_write_to_string().ok().map(std::borrow::Cow::into_owned)
        })
        .collect()
}
