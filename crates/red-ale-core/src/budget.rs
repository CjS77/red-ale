//! Budgets: an amount and a note per Account and Budget period. GnuCash's `GncBudget`
//! (`5.17:libgnucash/engine/gnc-budget.cpp`).

use crate::{Frame, Guid, Numeric, Recurrence, SlotValue};

/// The name of a budget whose file gives none: GnuCash's default, untranslated (`5.17:libgnucash/engine/gnc-budget.cpp:110`).
pub const DEFAULT_NAME: &str = "Unnamed Budget";

/// The number of periods of a budget whose file gives none, or 0, which GnuCash refuses
/// (`5.17:libgnucash/engine/gnc-budget.cpp:114`, `:472`).
pub const DEFAULT_NUM_PERIODS: u32 = 12;

/// The Frame of the budget's Slots that holds notes (`GNC_BUDGET_NOTES_PATH`, `5.17:libgnucash/engine/gnc-budget.h:98`).
pub const NOTES: &str = "notes";

/// A Budget. Its Budget amounts and Budget notes live in its Slots exactly as GnuCash keeps them: the amount of an
/// Account in period *p* (0-based) at `<account-guid>/<p>`, its note at `notes/<account-guid>/<p>`
/// (`5.17:libgnucash/engine/gnc-budget.cpp:494-508`). Keeping the whole Frame keeps what reads do not see: Slots for
/// periods at or past `num_periods`, for deleted Accounts, of other types, or under other keys.
#[derive(Debug, Clone, PartialEq)]
pub struct Budget {
    /// Identity.
    pub guid: Guid,
    /// Name.
    pub name: String,
    /// Description; may be empty.
    pub description: String,
    /// How many Budget periods it has; never 0.
    pub num_periods: u32,
    /// Gives the start of each Budget period.
    pub recurrence: Recurrence,
    /// Amounts, notes and anything else, as read.
    pub slots: Frame,
}

impl Budget {
    /// The Account's Budget amount in `period`, or `None` when it is unset. An explicit zero is `Some`. As GnuCash
    /// reads it (`get_perioddata`, `5.17:libgnucash/engine/gnc-budget.cpp:671-701`): only below `num_periods`, and
    /// only a numeric Slot.
    pub fn amount(&self, account: Guid, period: u32) -> Option<Numeric> {
        self.period_slot(&[], account, period).and_then(SlotValue::as_numeric)
    }

    /// The Account's Budget note in `period`. An empty note and a Slot that is not a string read as no note, as
    /// GnuCash reads them (`5.17:libgnucash/engine/gnc-budget.cpp:638-645`, `:693-694`).
    pub fn note(&self, account: Guid, period: u32) -> Option<&str> {
        self.period_slot(&[NOTES], account, period)
            .and_then(SlotValue::as_str)
            .filter(|s| !s.is_empty())
    }

    /// True if the Slots hold anything for the Account, in any period, counted or not.
    pub fn mentions(&self, account: Guid) -> bool {
        let key = account.to_string();
        self.slots.get_key(&key).is_some() || self.slots.get_in(&[NOTES, &key]).is_some()
    }

    fn period_slot(&self, prefix: &[&str], account: Guid, period: u32) -> Option<&SlotValue> {
        let (key, period_key) = (account.to_string(), period.to_string());
        let path: Vec<&str> = prefix.iter().copied().chain([key.as_str(), period_key.as_str()]).collect();
        (period < self.num_periods).then(|| self.slots.get_in(&path)).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Date, PeriodType, WeekendAdjust};

    fn budget(slots: Frame) -> Budget {
        let start = Date::from_ymd(2024, 1, 1).unwrap();
        Budget {
            guid: Guid::from_bytes([1; 16]),
            name: "B".into(),
            description: String::new(),
            num_periods: 3,
            recurrence: Recurrence::new(1, PeriodType::Month, start, WeekendAdjust::None).unwrap(),
            slots,
        }
    }

    #[test]
    fn reads_tell_unset_from_zero_and_ignore_hidden_periods() {
        let food = Guid::from_bytes([2; 16]);
        let gone = Guid::from_bytes([3; 16]);
        let f = food.to_string();
        let mut slots = Frame::new();
        slots.set_in(&[&f, "0"], SlotValue::Numeric(Numeric::new(1234, 100)));
        slots.set_in(&[&f, "1"], SlotValue::Numeric(Numeric::new(0, 1)));
        slots.set_in(&[&f, "2"], SlotValue::String("12".into()));
        slots.set_in(&[&f, "5"], SlotValue::Numeric(Numeric::new(5, 1)));
        slots.set_in(&[NOTES, &f, "0"], SlotValue::String("January".into()));
        slots.set_in(&[NOTES, &f, "1"], SlotValue::String(String::new()));
        slots.set_in(&[NOTES, &gone.to_string(), "0"], SlotValue::String("gone".into()));
        let b = budget(slots);
        assert_eq!(b.amount(food, 0), Some(Numeric::new(1234, 100)));
        assert_eq!(b.amount(food, 1), Some(Numeric::new(0, 1)), "an explicit zero is set");
        assert_eq!(b.amount(food, 2), None, "a string is not an amount");
        assert_eq!(b.amount(food, 5), None, "period 5 is past the count");
        assert_eq!(b.note(food, 0), Some("January"));
        assert_eq!(b.note(food, 1), None, "an empty note reads as none");
        assert!(b.mentions(food) && b.mentions(gone));
        assert!(!b.mentions(Guid::from_bytes([4; 16])));
    }
}
