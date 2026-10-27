//! A Budget's actuals and the Estimate dialog's amounts from history.

use super::{Cell, check_account, check_signs, get, get_mut, period_dates};
use crate::ops::balances::{Total, balance_as_of, balance_change};
use crate::ops::commit::commodity_fraction;
use crate::ops::commodities::canonical;
use crate::ops::prices::convert::{PriceAt, convert_balance};
use crate::time::PeriodBounds;
use crate::{
    Book, Budget, DENOM_AUTO, Date, DenomMode, DomainError, DomainResult, Guid, How, Numeric, Recurrence, Round, SlotValue, TimeZone,
};

/// The bounds of periods `0..count` of a Recurrence: local 00:00:00 of each period's first day and local 23:59:59 of
/// its last (`recurrenceGetPeriodTime`, `5.17:libgnucash/engine/Recurrence.cpp:397-418`).
pub fn period_times(recurrence: &Recurrence, count: u32, zone: &TimeZone) -> DomainResult<Vec<PeriodBounds>> {
    Ok(period_dates(recurrence, count)?
        .into_iter()
        .map(|(first, last)| PeriodBounds {
            start: zone.day_start(first),
            end: zone.day_end(last),
        })
        .collect())
}

/// The account's actual figure in a Budget period: `gnc_budget_get_account_period_actual_value`
/// (`5.17:libgnucash/engine/gnc-budget.cpp:661-669`), which is `xaccAccountGetNoclosingBalanceChangeInCurrencyForPeriod`
/// over the period's bounds (`5.17:libgnucash/engine/Account.cpp:3903-3923`). The account's own no-closing change
/// between its balances strictly before each bound, plus each descendant's change converted to the account's
/// commodity at the newest price at or before the period's end and added half up to that commodity's fraction, one
/// descendant at a time (`:3887-3901`). Any period may be asked for, as GnuCash's reports do past the count.
pub fn actual(book: &Book, budget: &Budget, account: Guid, period: u32, zone: &TimeZone) -> DomainResult<Numeric> {
    let bounds = period_times(&budget.recurrence, period.saturating_add(1), zone)?;
    let Some(b) = bounds.last() else {
        return Ok(Numeric::ZERO);
    };
    let own_change = |g: Guid| -> DomainResult<Numeric> {
        let before = balance_as_of(book, g, b.start, Total::Noclosing)?;
        Ok(balance_as_of(book, g, b.end, Total::Noclosing)?.sub_fixed(before)?)
    };
    let acc = book.account(account).ok_or_else(|| DomainError::not_found("account", &account))?;
    let own = own_change(account)?;
    let Some(target) = acc.commodity.as_ref().map(canonical) else {
        return Ok(own);
    };
    let fraction = commodity_fraction(book, &target).ok_or_else(|| DomainError::not_found("commodity", &target))?;
    book.descendants(account).iter().try_fold(own, |sum, sub| {
        let change = own_change(sub.guid)?;
        let converted = match &sub.commodity {
            _ if change.is_zero() => change,
            Some(c) if canonical(c) == target => change,
            Some(c) => convert_balance(book, change, c, &target, PriceAt::Before(b.end))?,
            None => Numeric::ZERO,
        };
        Ok(sum.add(converted, fraction, How::round(Round::HalfUp))?)
    })
}

/// What the Estimate dialog asks (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:1052-1065`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Estimate {
    /// The accounts to estimate, at least one.
    pub accounts: Vec<Guid>,
    /// The start of history period 0.
    pub start: Date,
    /// Significant digits to round to, 1 to [`MAX_SIGNIFICANT_DIGITS`].
    pub significant_digits: u8,
    /// Store the average over all history periods in every period.
    pub use_average: bool,
}

/// The Significant Digits spin button's range (`5.17:gnucash/gtkbuilder/gnc-plugin-page-budget.glade:5-11`).
pub const MAX_SIGNIFICANT_DIGITS: u8 = 12;

/// Fill the accounts' amounts from history as the Estimate dialog does (`estimate_budget_helper`,
/// `5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:955-1005`): the history Recurrence is the Budget's from `start`,
/// through `recurrenceSet` (`:1072-1077`); each account's no-closing balance change with its descendants
/// (`xaccAccountGetNoclosingBalanceChangeForPeriod`, `5.17:libgnucash/engine/Account.cpp:3869-3878`) over history
/// period *i* goes to Budget period *i*, rounded half up to significant digits; with `use_average`, the change over
/// every history period divided by the count, likewise rounded, goes to every period. Every amount is overwritten.
/// Returns the cells written. An overflow refuses the whole estimate, where GnuCash skips the period.
pub fn estimate(book: &mut Book, guid: Guid, e: &Estimate, zone: &TimeZone) -> DomainResult<Vec<(Cell, Numeric)>> {
    check_signs(book)?;
    if e.accounts.is_empty() {
        return Err(DomainError::invalid("You must select at least one account to estimate."));
    }
    if !(1..=MAX_SIGNIFICANT_DIGITS).contains(&e.significant_digits) {
        return Err(DomainError::invalid(format!("significant digits are 1 to {MAX_SIGNIFICANT_DIGITS}")));
    }
    e.accounts.iter().try_for_each(|a| check_account(book, *a))?;
    let budget = get(book, guid)?;
    let r = budget.recurrence;
    let history = Recurrence::normalised(r.mult(), r.period_type(), e.start, r.weekend_adjust());
    let bounds = period_times(&history, budget.num_periods, zone)?;
    let how = How::new(DenomMode::SigFigs(e.significant_digits), Round::HalfUp);
    let mut seen = std::collections::BTreeSet::new();
    let accounts: Vec<Guid> = e.accounts.iter().copied().filter(|a| seen.insert(*a)).collect();
    let cells = accounts
        .iter()
        .map(|a| estimate_account(book, *a, &bounds, e.use_average, how))
        .collect::<DomainResult<Vec<_>>>()?
        .concat();
    let budget = get_mut(book, guid)?;
    cells.iter().for_each(|(c, n)| {
        budget
            .slots
            .set_in(&[&c.account.to_string(), &c.period.to_string()], SlotValue::Numeric(*n));
    });
    Ok(cells)
}

fn estimate_account(book: &Book, account: Guid, bounds: &[PeriodBounds], average: bool, how: How) -> DomainResult<Vec<(Cell, Numeric)>> {
    let change = |from, to| balance_change(book, account, from, to, Total::Noclosing, true);
    let cell = |period: usize, n| (Cell { account, period: u32::try_from(period).unwrap_or(u32::MAX) }, n);
    match (average, bounds.first(), bounds.last()) {
        (true, Some(first), Some(last)) => {
            let count = i64::try_from(bounds.len()).map_err(|_| crate::NumericError::Overflow)?;
            let mean = change(first.start, last.end)?.div(Numeric::from_int(count), DENOM_AUTO, how)?;
            Ok((0..bounds.len()).map(|p| cell(p, mean)).collect())
        }
        _ => bounds
            .iter()
            .enumerate()
            .map(|(p, b)| Ok(cell(p, change(b.start, b.end)?.convert(DENOM_AUTO, how)?)))
            .collect(),
    }
}
