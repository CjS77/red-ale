//! The scheduled transactions' cash flow over a date range, which the forecast reports read
//! (`gnc_sx_all_instantiate_cashflow_all`, `5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1715-1976`; the note
//! §3.9, `design/notes/engine-budgets.md` §4.10).

use std::ops::ControlFlow;

use indexmap::IndexMap;

use super::error::{ScheduledError, Side, Site};
use super::occurrences::{TemporalState, advance, has_occurrence_limit, remaining};
use super::template::{formulas, site, split_account, template_account, template_transactions};
use crate::formula::{FormulaError, NumberFormat, Variables, evaluate, free_variables};
use crate::{
    Book, CommodityRef, DENOM_AUTO, Date, Guid, How, Numeric, RecurrenceError, Round, ScheduledTransaction, Split, TemplateSplit, Transaction,
};

/// How many occurrences of a scheduled transaction fall in `start..=end`, counting on from its last occurrence, which
/// is itself never counted: `gnc_sx_get_num_occur_daterange` (`5.17:libgnucash/engine/SchedXaction.cpp:739-812`).
/// Deferred instances, the days in advance and auto-create play no part.
pub fn occurrences_in_range(sx: &ScheduledTransaction, start: Date, end: Date) -> Result<u32, RecurrenceError> {
    let limit = has_occurrence_limit(sx);
    if (limit && remaining(sx) <= 0) || sx.end.is_some_and(|e| e < start) {
        return Ok(0);
    }
    let mut state = TemporalState::of(sx);
    let count_first = state.last.is_none_or(|l| l < start);
    while state.last.is_some_and(|l| l < start) {
        advance(sx, &mut state)?;
        if limit && state.remaining < 0 {
            return Ok(0);
        }
    }
    let in_range = |s: &TemporalState| {
        s.last.is_some_and(|l| l <= end && sx.end.is_none_or(|e| l <= e)) && (!limit || s.remaining >= 0)
    };
    let mut count = 0u32;
    while in_range(&state) {
        count += 1;
        advance(sx, &mut state)?;
    }
    Ok(if count_first { count } else { count.saturating_sub(1) })
}

/// Why part of the cash flow counts as 0, where GnuCash reports the same error and carries on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CashFlowError {
    /// A Formula that does not evaluate, even with every variable 0; it counts as 0 (`:1127-1139`).
    Formula {
        /// Where.
        site: Site,
        /// Which side.
        side: Side,
        /// Why.
        error: FormulaError,
    },
    /// A template split whose account is unknown: it and the rest of its template transaction count for nothing
    /// (`:1822-1826`).
    Account(ScheduledError),
    /// A split in another commodity than the template transaction's first split: "No exchange rate available in SX
    /// [%s] for %s -> %s, value is zero." (`:1869-1878`).
    NoExchangeRate {
        /// Where.
        site: Site,
        /// The split's account commodity.
        from: CommodityRef,
        /// The first split's.
        to: CommodityRef,
    },
    /// The amount overflowed; it counts as 0 (`:1860-1867`).
    Overflow(Site),
}

/// What the scheduled transactions add to each account over a date range.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CashFlow {
    /// The sum per real account, in its commodity, debits positive, in the order first reached.
    pub amounts: IndexMap<Guid, Numeric>,
    /// What counted as 0, and why.
    pub errors: Vec<CashFlowError>,
}

/// Every enabled scheduled transaction's cash flow from `start` to `end`, both inclusive
/// (`gnc_sx_all_instantiate_cashflow_all`, `gnc-sx-instance-model.c:1888-1976`): for each with
/// [`occurrences_in_range`] `n` > 0, each template split adds `n` × (debit − credit) to its real account, at the
/// difference's denominator, half up, summed exactly. With no bindings, each side is its stored numeric when that is
/// valid and not zero, else its Formula with every variable, `i` too, at 0 (`_get_sx_formula_value`, `:1087-1148`).
pub fn cash_flow(book: &Book, start: Date, end: Date, format: NumberFormat) -> Result<CashFlow, RecurrenceError> {
    let mut flow = CashFlow::default();
    book.scheduled.values().filter(|sx| sx.enabled && template_account(book, sx).is_some()).try_for_each(|sx| {
        let count = occurrences_in_range(sx, start, end)?;
        if count > 0 {
            template_transactions(book, sx).into_iter().for_each(|t| add_transaction(book, sx, t, count, format, &mut flow));
        }
        Ok(())
    })?;
    Ok(flow)
}

fn add_transaction(book: &Book, sx: &ScheduledTransaction, t: &Transaction, count: u32, format: NumberFormat, flow: &mut CashFlow) {
    let mut first: Option<CommodityRef> = None;
    let _stopped = t.splits.iter().try_for_each(|split| {
        let (account, commodity) = match split_account(book, sx, t, split) {
            Ok(found) => found,
            Err(e) => {
                flow.errors.push(CashFlowError::Account(e));
                return ControlFlow::Break(());
            }
        };
        let first = first.get_or_insert_with(|| commodity.clone());
        let at = site(sx, t, split);
        let once = side(split, Side::Debit, at, format, flow).sub_fixed(side(split, Side::Credit, at, format, flow));
        let total = once.and_then(|d| d.mul(Numeric::from_int(i64::from(count)), d.denom(), How::round(Round::HalfUp)));
        let amount = match total {
            Ok(_) if commodity != first => {
                let (from, to) = (commodity.clone(), first.clone());
                flow.errors.push(CashFlowError::NoExchangeRate { site: at, from, to });
                Numeric::ZERO
            }
            Ok(amount) => amount,
            Err(_) => {
                flow.errors.push(CashFlowError::Overflow(at));
                Numeric::ZERO
            }
        };
        let entry = flow.amounts.entry(account.guid).or_insert(Numeric::ZERO);
        match entry.add(amount, DENOM_AUTO, How::REDUCE_NEVER) {
            Ok(sum) => *entry = sum,
            Err(_) => flow.errors.push(CashFlowError::Overflow(at)),
        }
        ControlFlow::Continue(())
    });
}

/// One side's value with no bindings.
fn side(split: &Split, wanted: Side, at: Site, format: NumberFormat, flow: &mut CashFlow) -> Numeric {
    let frame = TemplateSplit::of(split);
    let stored = frame.and_then(|f| match wanted {
        Side::Debit => f.debit_numeric(),
        Side::Credit => f.credit_numeric(),
    });
    if let Some(n) = stored.filter(|n| n.is_valid() && !n.is_zero()) {
        return n;
    }
    let Some((_, text)) = formulas(split).into_iter().find(|(s, _)| *s == wanted) else {
        return Numeric::ZERO;
    };
    let zeroes = |names: Vec<String>| names.into_iter().map(|n| (n, Numeric::ZERO)).collect::<Variables>();
    match free_variables(text, format).and_then(|names| evaluate(text, format, &zeroes(names))) {
        Ok(n) => n,
        Err(error) => {
            flow.errors.push(CashFlowError::Formula { site: at, side: wanted, error });
            Numeric::ZERO
        }
    }
}

#[cfg(test)]
mod tests;
