//! The accounting period: the start and end dates GnuCash's account summary, "Remove old prices" and reports default
//! to. GnuCash keeps the choice in user preferences, not in the book
//! (`5.17:gnucash/gschemas/org.gnucash.GnuCash.window.pages.account.tree.gschema.xml.in`); red-ale keeps it in its own
//! configuration. This module only computes dates.

use std::fmt;
use std::str::FromStr;

use crate::time::PeriodBounds;
use crate::{Book, Date, DomainError, DomainResult, SlotValue, TimeZone};

/// A relative period choice, GnuCash's `GncAccountingPeriod` (`5.17:libgnucash/engine/gnc-accounting-period.h:57-73`).
/// As a start it means the first day of the period; as an end, the last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelativePeriod {
    /// Today.
    Today,
    /// This month.
    Month,
    /// The previous month.
    PreviousMonth,
    /// This calendar quarter.
    Quarter,
    /// The previous calendar quarter.
    PreviousQuarter,
    /// This calendar year (GnuCash's default for both ends).
    Year,
    /// The previous calendar year.
    PreviousYear,
    /// This fiscal year ("accounting period"); needs a fiscal year end.
    FiscalYear,
    /// The previous fiscal year.
    PreviousFiscalYear,
}

const NAMES: [(RelativePeriod, &str); 9] = [
    (RelativePeriod::Today, "today"),
    (RelativePeriod::Month, "month"),
    (RelativePeriod::PreviousMonth, "previous-month"),
    (RelativePeriod::Quarter, "quarter"),
    (RelativePeriod::PreviousQuarter, "previous-quarter"),
    (RelativePeriod::Year, "year"),
    (RelativePeriod::PreviousYear, "previous-year"),
    (RelativePeriod::FiscalYear, "fiscal-year"),
    (RelativePeriod::PreviousFiscalYear, "previous-fiscal-year"),
];

impl RelativePeriod {
    /// Every choice, in GnuCash's enum order.
    pub const ALL: [Self; 9] = [
        Self::Today,
        Self::Month,
        Self::PreviousMonth,
        Self::Quarter,
        Self::PreviousQuarter,
        Self::Year,
        Self::PreviousYear,
        Self::FiscalYear,
        Self::PreviousFiscalYear,
    ];

    /// red-ale's name for the choice.
    pub fn as_str(self) -> &'static str {
        NAMES.iter().find(|(p, _)| *p == self).map_or("", |(_, n)| n)
    }

    /// GnuCash's enum value, as stored in its `start-period` and `end-period` preferences.
    pub fn gnucash_value(self) -> i32 {
        Self::ALL.iter().position(|p| *p == self).and_then(|i| i32::try_from(i).ok()).unwrap_or(-1)
    }

    fn is_fiscal(self) -> bool {
        matches!(self, Self::FiscalYear | Self::PreviousFiscalYear)
    }
}

impl FromStr for RelativePeriod {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        NAMES.iter().find(|(_, n)| *n == s).map(|(p, _)| *p).ok_or_else(|| DomainError::invalid(format!("not a period: {s:?}")))
    }
}

impl fmt::Display for RelativePeriod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One end of the accounting period: a relative choice or a fixed date (GnuCash's `*-choice-relative` /
/// `*-choice-absolute` preferences).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodBound {
    /// Relative to today.
    Relative(RelativePeriod),
    /// A fixed date.
    Absolute(Date),
}

impl FromStr for PeriodBound {
    type Err = DomainError;

    /// A period name or a `YYYY-MM-DD` date.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<RelativePeriod>()
            .map(Self::Relative)
            .or_else(|_| Date::parse(s).map(Self::Absolute))
            .map_err(|_| DomainError::invalid(format!("not a period or a YYYY-MM-DD date: {s:?}")))
    }
}

impl fmt::Display for PeriodBound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Relative(p) => p.fmt(f),
            Self::Absolute(d) => d.fmt(f),
        }
    }
}

/// The last day of a fiscal year, as a month and day (GnuCash keeps a `GDate` and ignores its year).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FiscalYearEnd {
    month: u32,
    day: u32,
}

impl FiscalYearEnd {
    /// A month (1-12) and a day valid in that month in a leap year.
    pub fn new(month: u32, day: u32) -> DomainResult<Self> {
        Date::from_ymd(2000, month, day).map(|_| Self { month, day }).ok_or_else(|| DomainError::invalid(format!("not a day of the year: {month:02}-{day:02}")))
    }

    /// Month, 1 to 12.
    pub fn month(self) -> u32 {
        self.month
    }

    /// Day of the month.
    pub fn day(self) -> u32 {
        self.day
    }

    /// This day in `year`; 29 February becomes the 28th in a common year.
    fn in_year(self, year: i32) -> DomainResult<Date> {
        Date::from_ymd(year, self.month, self.day).or_else(|| Date::from_ymd(year, self.month, self.day - 1)).ok_or_else(out_of_range)
    }
}

impl FromStr for FiscalYearEnd {
    type Err = DomainError;

    /// `MM-DD`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || DomainError::invalid(format!("not a month and day (MM-DD): {s:?}"));
        let (m, d) = s.trim().split_once('-').ok_or_else(bad)?;
        Self::new(m.parse().map_err(|_| bad())?, d.parse().map_err(|_| bad())?)
    }
}

impl fmt::Display for FiscalYearEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}-{:02}", self.month, self.day)
    }
}

/// The book's fiscal year end, from its `fy_end` date slot (the QofBook `fy-end` property,
/// `5.17:libgnucash/engine/qofbook.cpp:180-181`, `:219-220`). GnuCash 5.17 never writes it, but keeps it.
pub fn book_fiscal_year_end(book: &Book) -> Option<FiscalYearEnd> {
    book.slots.get("fy_end").and_then(SlotValue::as_date).and_then(|d| FiscalYearEnd::new(d.month(), d.day()).ok())
}

/// The accounting period setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountingPeriod {
    /// Where it starts.
    pub start: PeriodBound,
    /// Where it ends.
    pub end: PeriodBound,
}

impl Default for AccountingPeriod {
    /// This calendar year, GnuCash's default (`start-period` and `end-period` 5).
    fn default() -> Self {
        Self { start: PeriodBound::Relative(RelativePeriod::Year), end: PeriodBound::Relative(RelativePeriod::Year) }
    }
}

fn out_of_range() -> DomainError {
    DomainError::invalid("the period falls outside the supported dates")
}

fn first_of(year: i32, month: u32) -> DomainResult<Date> {
    Date::from_ymd(year, month, 1).ok_or_else(out_of_range)
}

fn quarter_start(d: Date) -> DomainResult<Date> {
    first_of(d.year(), (d.month() - 1) / 3 * 3 + 1)
}

/// `gnc_accounting_period_start_gdate` (`5.17:libgnucash/engine/gnc-accounting-period.c:138-200`), with the helpers
/// of `5.17:libgnucash/engine/gnc-date.cpp:1524-1716`.
fn start_of(p: RelativePeriod, today: Date, fy: Option<FiscalYearEnd>) -> DomainResult<Date> {
    Ok(match p {
        RelativePeriod::Today => today,
        RelativePeriod::Month => first_of(today.year(), today.month())?,
        RelativePeriod::PreviousMonth => first_of(today.year(), today.month())?.add_months(-1),
        RelativePeriod::Quarter => quarter_start(today)?,
        RelativePeriod::PreviousQuarter => quarter_start(today.add_months(-3))?,
        RelativePeriod::Year => first_of(today.year(), 1)?,
        RelativePeriod::PreviousYear => first_of(today.year() - 1, 1)?,
        RelativePeriod::FiscalYear => fiscal_start(today, need_fy(fy)?)?,
        RelativePeriod::PreviousFiscalYear => fiscal_start(today, need_fy(fy)?)?.add_months(-12),
    })
}

/// `gnc_accounting_period_end_gdate` (`gnc-accounting-period.c:215-290`).
fn end_of(p: RelativePeriod, today: Date, fy: Option<FiscalYearEnd>) -> DomainResult<Date> {
    Ok(match p {
        RelativePeriod::Today => today,
        RelativePeriod::Month => today.end_of_month(),
        RelativePeriod::PreviousMonth => first_of(today.year(), today.month())?.add_days(-1),
        RelativePeriod::Quarter => quarter_start(today)?.add_months(2).end_of_month(),
        RelativePeriod::PreviousQuarter => quarter_start(today.add_months(-3))?.add_months(2).end_of_month(),
        RelativePeriod::Year => first_of(today.year(), 12)?.end_of_month(),
        RelativePeriod::PreviousYear => first_of(today.year() - 1, 12)?.end_of_month(),
        RelativePeriod::FiscalYear => fiscal_end(today, need_fy(fy)?)?,
        RelativePeriod::PreviousFiscalYear => fiscal_end(today, need_fy(fy)?)?.add_months(-12),
    })
}

/// GnuCash returns the epoch for a fiscal period with no fiscal year end (`gnc-accounting-period.c:95-96`, `:188-205`);
/// red-ale refuses instead.
fn need_fy(fy: Option<FiscalYearEnd>) -> DomainResult<FiscalYearEnd> {
    fy.ok_or_else(|| DomainError::invalid("a fiscal-year period needs a fiscal year end (MM-DD)"))
}

/// `gnc_gdate_set_fiscal_year_start` (`5.17:libgnucash/engine/gnc-date.cpp:1650-1672`): the day after this calendar
/// year's fiscal year end if that has passed, else that day less a year. GnuCash moves the start back a year rather
/// than recomputing it, so 29 February becomes the 28th (the oracle confirms it); "previous" subtracts another year.
fn fiscal_start(today: Date, fy: FiscalYearEnd) -> DomainResult<Date> {
    let end = fy.in_year(today.year())?;
    let start = end.add_days(1);
    Ok(if today > end { start } else { start.add_months(-12) })
}

/// `gnc_gdate_set_fiscal_year_end` (`gnc-date.cpp:1674-1695`): this calendar year's fiscal year end, or that day a year
/// later if it has passed.
fn fiscal_end(today: Date, fy: FiscalYearEnd) -> DomainResult<Date> {
    let end = fy.in_year(today.year())?;
    Ok(if today > end { end.add_months(12) } else { end })
}

/// The first and last days of the accounting period that contains `today`. GnuCash turns them into the start of the
/// first day and 23:59:59 of the last, in local time (`gnc-accounting-period.c:60-100`).
pub fn period_dates(p: &AccountingPeriod, today: Date, fy: Option<FiscalYearEnd>) -> DomainResult<(Date, Date)> {
    let start = match p.start {
        PeriodBound::Relative(r) => start_of(r, today, fy)?,
        PeriodBound::Absolute(d) => d,
    };
    let end = match p.end {
        PeriodBound::Relative(r) => end_of(r, today, fy)?,
        PeriodBound::Absolute(d) => d,
    };
    Ok((start, end))
}

/// The accounting period containing `today` as the instants GnuCash uses: local 00:00:00 of its first day and
/// 23:59:59 of its last (`gnc_accounting_period_fiscal_start` and `_end`, `gnc-accounting-period.c:60-100`).
pub fn period_bounds(p: &AccountingPeriod, today: Date, fy: Option<FiscalYearEnd>, zone: &TimeZone) -> DomainResult<PeriodBounds> {
    let (start, end) = period_dates(p, today, fy)?;
    Ok(PeriodBounds { start: zone.day_start(start), end: zone.day_end(end) })
}

/// Check a setting can be computed: a fiscal choice needs a fiscal year end.
pub fn check_period(p: &AccountingPeriod, fy: Option<FiscalYearEnd>) -> DomainResult<()> {
    let fiscal = [p.start, p.end].iter().any(|b| matches!(b, PeriodBound::Relative(r) if r.is_fiscal()));
    if fiscal { need_fy(fy).map(|_| ()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        Date::parse(s).unwrap()
    }

    fn both(p: RelativePeriod, today: &str, fy: Option<FiscalYearEnd>) -> (String, String) {
        let period = AccountingPeriod { start: PeriodBound::Relative(p), end: PeriodBound::Relative(p) };
        let (s, e) = period_dates(&period, d(today), fy).unwrap();
        (s.to_string(), e.to_string())
    }

    #[test]
    fn calendar_periods() {
        let cases = [
            (RelativePeriod::Today, "2024-05-31", "2024-05-31"),
            (RelativePeriod::Month, "2024-05-01", "2024-05-31"),
            (RelativePeriod::PreviousMonth, "2024-04-01", "2024-04-30"),
            (RelativePeriod::Quarter, "2024-04-01", "2024-06-30"),
            (RelativePeriod::PreviousQuarter, "2024-01-01", "2024-03-31"),
            (RelativePeriod::Year, "2024-01-01", "2024-12-31"),
            (RelativePeriod::PreviousYear, "2023-01-01", "2023-12-31"),
        ];
        cases.iter().for_each(|(p, s, e)| assert_eq!(both(*p, "2024-05-31", None), ((*s).to_owned(), (*e).to_owned()), "{p}"));
        assert_eq!(both(RelativePeriod::PreviousMonth, "2024-03-31", None), ("2024-02-01".into(), "2024-02-29".into()));
        assert_eq!(both(RelativePeriod::PreviousQuarter, "2024-01-15", None), ("2023-10-01".into(), "2023-12-31".into()));
    }

    #[test]
    fn fiscal_periods() {
        let fy: FiscalYearEnd = "06-30".parse().unwrap();
        assert_eq!(both(RelativePeriod::FiscalYear, "2024-05-31", Some(fy)), ("2023-07-01".into(), "2024-06-30".into()));
        assert_eq!(both(RelativePeriod::FiscalYear, "2024-06-30", Some(fy)), ("2023-07-01".into(), "2024-06-30".into()));
        assert_eq!(both(RelativePeriod::FiscalYear, "2024-07-01", Some(fy)), ("2024-07-01".into(), "2025-06-30".into()));
        assert_eq!(both(RelativePeriod::PreviousFiscalYear, "2024-07-01", Some(fy)), ("2023-07-01".into(), "2024-06-30".into()));
        let period = AccountingPeriod { start: PeriodBound::Relative(RelativePeriod::FiscalYear), end: PeriodBound::Absolute(d("2024-12-31")) };
        assert!(period_dates(&period, d("2024-05-31"), None).is_err());
        assert!(check_period(&period, None).is_err() && check_period(&AccountingPeriod::default(), None).is_ok());
        assert!("02-30".parse::<FiscalYearEnd>().is_err());
        let feb: FiscalYearEnd = "02-28".parse().unwrap();
        assert_eq!(both(RelativePeriod::FiscalYear, "2024-01-01", Some(feb)), ("2023-02-28".into(), "2024-02-28".into()), "as GnuCash");
        assert_eq!(both(RelativePeriod::FiscalYear, "2025-02-28", Some(feb)), ("2024-03-01".into(), "2025-02-28".into()));
        assert!("02-29".parse::<FiscalYearEnd>().is_ok());
    }

    #[test]
    fn bounds_parse_and_print() {
        assert_eq!("previous-quarter".parse::<PeriodBound>().unwrap(), PeriodBound::Relative(RelativePeriod::PreviousQuarter));
        assert_eq!("2024-01-01".parse::<PeriodBound>().unwrap().to_string(), "2024-01-01");
        assert!("someday".parse::<PeriodBound>().is_err());
        assert_eq!(RelativePeriod::Year.gnucash_value(), 5);
    }

    #[test]
    fn bounds_are_local_day_start_and_end() {
        let kiritimati = TimeZone::fixed(14 * 3600).unwrap();
        let b = period_bounds(&AccountingPeriod::default(), d("2024-06-20"), None, &kiritimati).unwrap();
        assert_eq!(b.start.to_file_string(), "2023-12-31 10:00:00 +0000");
        assert_eq!(b.end.to_file_string(), "2024-12-31 09:59:59 +0000");
    }
}
