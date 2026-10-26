//! What GnuCash asks ICU rather than the C library: the first day of the week and the interval form of a date range.
//! red-ale asks ICU4X with its compiled CLDR data.
//!
//! ICU4X's CLDR is newer than the ICU a GnuCash build links (ICU 74, CLDR 44, for the oracle), so a few locales differ
//! where CLDR changed (`design/reports.md` §14).

use icu_calendar::Date;
use icu_calendar::types::Weekday as IcuWeekday;
use icu_calendar::week::WeekInformation;
use icu_datetime::fieldsets::YMD;
use icu_datetime::options::YearStyle;
use icu_datetime::range::DateRangeFormatter;
use icu_locale_core::Locale as IcuLocale;
use writeable::Writeable;

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
