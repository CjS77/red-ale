//! Scheduled transactions for tests.

use crate::{Date, Frame, Guid, Occurrences, PeriodType, Recurrence, ScheduledTransaction, WeekendAdjust};

pub fn date(y: i32, m: u32, d: u32) -> Date {
    Date::from_ymd(y, m, d).unwrap()
}

pub fn every(mult: u16, period_type: PeriodType, start: Date) -> Recurrence {
    Recurrence::new(mult, period_type, start, WeekendAdjust::None).unwrap()
}

/// A scheduled transaction never run, as the editor makes one: instance count 1, no end.
pub fn sx(start: Date, schedule: Vec<Recurrence>) -> ScheduledTransaction {
    ScheduledTransaction {
        guid: Guid::from_bytes([0x5c; 16]),
        name: "Rent".into(),
        enabled: true,
        auto_create: false,
        auto_create_notify: false,
        advance_create_days: 0,
        advance_remind_days: 0,
        instance_count: 1,
        start,
        last: None,
        end: None,
        occurrences: None,
        template_account: None,
        schedule,
        deferred: Vec::new(),
        slots: Frame::new(),
    }
}

/// Monthly from `start`.
pub fn monthly(start: Date) -> ScheduledTransaction {
    sx(start, vec![every(1, PeriodType::Month, start)])
}

/// At most `total` occurrences, `remaining` still to come.
pub fn limited(mut sx: ScheduledTransaction, total: i32, remaining: i32) -> ScheduledTransaction {
    sx.occurrences = Some(Occurrences { total, remaining });
    sx
}
