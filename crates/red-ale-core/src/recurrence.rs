//! Recurrences: a step repeated from a start date. GnuCash's `Recurrence` (`5.17:libgnucash/engine/Recurrence.h:51-86`).
//! A Budget takes its periods from one; a scheduled transaction's schedule is a list of them.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::Date;

/// The step of a Recurrence. Names are as GnuCash writes them (`5.17:libgnucash/engine/Recurrence.cpp:44-48`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeriodType {
    /// Once, on the start date; not a true period.
    Once,
    /// Every *n* days.
    Day,
    /// Every *n* weeks.
    Week,
    /// Every *n* months, on the start's day of the month.
    Month,
    /// Every *n* months, on the last day of the month.
    EndOfMonth,
    /// Every *n* months, on the start's weekday in the same week of the month (the second Tuesday).
    NthWeekday,
    /// Every *n* months, on the last of the start's weekday in the month.
    LastWeekday,
    /// Every *n* years.
    Year,
}

const PERIOD_NAMES: [(PeriodType, &str); 8] = [
    (PeriodType::Once, "once"),
    (PeriodType::Day, "day"),
    (PeriodType::Week, "week"),
    (PeriodType::Month, "month"),
    (PeriodType::EndOfMonth, "end of month"),
    (PeriodType::NthWeekday, "nth weekday"),
    (PeriodType::LastWeekday, "last weekday"),
    (PeriodType::Year, "year"),
];

impl PeriodType {
    /// Every period type, in GnuCash's order.
    pub const ALL: [Self; 8] = [
        Self::Once,
        Self::Day,
        Self::Week,
        Self::Month,
        Self::EndOfMonth,
        Self::NthWeekday,
        Self::LastWeekday,
        Self::Year,
    ];

    /// The name GnuCash writes.
    pub fn as_str(self) -> &'static str {
        PERIOD_NAMES.iter().find(|(t, _)| *t == self).map_or("", |(_, n)| n)
    }
}

impl fmt::Display for PeriodType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Exact names only, as `recurrencePeriodTypeFromString` matches them (`5.17:libgnucash/engine/Recurrence.cpp:528-536`).
impl FromStr for PeriodType {
    type Err = RecurrenceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PERIOD_NAMES
            .iter()
            .find(|(_, n)| *n == s)
            .map(|(t, _)| *t)
            .ok_or_else(|| RecurrenceError::UnknownPeriodType(s.to_owned()))
    }
}

/// Where an occurrence that falls on a weekend moves to (`5.17:libgnucash/engine/Recurrence.cpp:49-52`). GnuCash's
/// `recurrenceSet` keeps one only for month, end of month and year (`:128-137`); its file readers keep any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WeekendAdjust {
    /// It stays on the weekend.
    #[default]
    None,
    /// To the Friday before.
    Back,
    /// To the Monday after.
    Forward,
}

const WEEKEND_NAMES: [(WeekendAdjust, &str); 3] = [
    (WeekendAdjust::None, "none"),
    (WeekendAdjust::Back, "back"),
    (WeekendAdjust::Forward, "forward"),
];

impl WeekendAdjust {
    /// The name GnuCash writes.
    pub fn as_str(self) -> &'static str {
        WEEKEND_NAMES.iter().find(|(w, _)| *w == self).map_or("", |(_, n)| n)
    }
}

/// Exact names only (`recurrenceWeekendAdjustFromString`, `5.17:libgnucash/engine/Recurrence.cpp:545-553`).
impl FromStr for WeekendAdjust {
    type Err = RecurrenceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        WEEKEND_NAMES
            .iter()
            .find(|(_, n)| *n == s)
            .map(|(w, _)| *w)
            .ok_or_else(|| RecurrenceError::UnknownWeekendAdjust(s.to_owned()))
    }
}

/// Why a Recurrence could not be made.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RecurrenceError {
    /// Not one of GnuCash's eight period type names.
    #[error("unknown recurrence period type {0:?}")]
    UnknownPeriodType(String),
    /// Not one of GnuCash's three weekend adjustment names.
    #[error("unknown weekend adjustment {0:?}")]
    UnknownWeekendAdjust(String),
    /// A repeating Recurrence that never steps.
    #[error("a recurrence every 0 {0} periods never repeats")]
    ZeroMultiplier(PeriodType),
}

/// A Recurrence: every `mult` steps of `period_type` from `start`, moved off weekends by `weekend_adjust`.
///
/// GnuCash's file readers take the fields as written (`5.17:libgnucash/backend/xml/gnc-recurrence-xml-v2.cpp:95-122`),
/// with no phase check, so neither does this: an end-of-month Recurrence may start mid-month. The one value refused is
/// a zero multiplier on a repeating type, which `recurrenceSet` raises to 1 and so never writes; it writes 0 for
/// `once`, whose multiplier means nothing (`5.17:libgnucash/engine/Recurrence.cpp:88-91`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Recurrence {
    mult: u16,
    period_type: PeriodType,
    start: Date,
    weekend_adjust: WeekendAdjust,
}

impl Recurrence {
    /// A Recurrence, refusing a zero multiplier on any period type but [`PeriodType::Once`].
    pub fn new(mult: u16, period_type: PeriodType, start: Date, weekend_adjust: WeekendAdjust) -> Result<Self, RecurrenceError> {
        if mult == 0 && period_type != PeriodType::Once {
            return Err(RecurrenceError::ZeroMultiplier(period_type));
        }
        Ok(Self {
            mult,
            period_type,
            start,
            weekend_adjust,
        })
    }

    /// How many steps of the period type each occurrence is apart.
    pub fn mult(&self) -> u16 {
        self.mult
    }

    /// The step.
    pub fn period_type(&self) -> PeriodType {
        self.period_type
    }

    /// The first occurrence.
    pub fn start(&self) -> Date {
        self.start
    }

    /// Where occurrences on a weekend move to.
    pub fn weekend_adjust(&self) -> WeekendAdjust {
        self.weekend_adjust
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_exactly() {
        PeriodType::ALL
            .iter()
            .for_each(|t| assert_eq!(t.as_str().parse::<PeriodType>(), Ok(*t)));
        assert!(" month".parse::<PeriodType>().is_err());
        assert_eq!("forward".parse::<WeekendAdjust>(), Ok(WeekendAdjust::Forward));
        assert!("None".parse::<WeekendAdjust>().is_err());
    }

    #[test]
    fn only_once_may_have_a_zero_multiplier() {
        let start = Date::from_ymd(2024, 1, 31).unwrap();
        assert!(Recurrence::new(0, PeriodType::Once, start, WeekendAdjust::None).is_ok());
        assert_eq!(
            Recurrence::new(0, PeriodType::Month, start, WeekendAdjust::None),
            Err(RecurrenceError::ZeroMultiplier(PeriodType::Month))
        );
        let r = Recurrence::new(3, PeriodType::EndOfMonth, start, WeekendAdjust::Back).unwrap();
        assert_eq!((r.mult(), r.period_type(), r.start(), r.weekend_adjust()), (3, PeriodType::EndOfMonth, start, WeekendAdjust::Back));
    }
}
