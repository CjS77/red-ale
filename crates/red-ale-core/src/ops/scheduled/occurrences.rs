//! A scheduled transaction's temporal state and its next occurrences: `xaccSchedXactionGetNextInstance` and the
//! temporal-state functions of `5.17:libgnucash/engine/SchedXaction.cpp:885-926`, `:1031-1054`.

use crate::recurrence::next_instance_of_list;
use crate::{Date, DeferredInstance, RecurrenceError, ScheduledTransaction};

/// The part of a scheduled transaction that moves as instances are acted on, so instances can be generated without
/// changing it: GnuCash's `SXTmpStateData` (`5.17:libgnucash/engine/SchedXaction.h:130-136`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemporalState {
    /// The last occurrence; `None` once stepping has passed the schedule's end.
    pub last: Option<Date>,
    /// Occurrences remaining. Counts down past zero, as GnuCash's does, when stepped beyond the end.
    pub remaining: i32,
    /// The Instance count: the `i` of the next instance.
    pub instance_count: i32,
}

impl TemporalState {
    /// The scheduled transaction's own state, with an unset last occurrence read as 1970-01-01
    /// (`gnc_sx_create_temporal_state`, `SchedXaction.cpp:1031-1042`).
    pub fn of(sx: &ScheduledTransaction) -> Self {
        Self {
            last: Some(sx.last.unwrap_or(Date::EPOCH)),
            remaining: remaining(sx),
            instance_count: sx.instance_count,
        }
    }

    /// The state a Deferred instance saved.
    pub fn of_deferred(d: &DeferredInstance) -> Self {
        Self {
            last: d.last,
            remaining: d.remaining,
            instance_count: d.instance_count,
        }
    }

    /// This state saved as a Deferred instance.
    pub fn to_deferred(self) -> DeferredInstance {
        DeferredInstance {
            last: self.last,
            remaining: self.remaining,
            instance_count: self.instance_count,
        }
    }
}

/// Whether the scheduled transaction has an occurrence limit: `xaccSchedXactionHasOccurDef`, a total that is not 0
/// (`SchedXaction.cpp:691-694`).
pub fn has_occurrence_limit(sx: &ScheduledTransaction) -> bool {
    sx.occurrences.is_some_and(|o| o.total != 0)
}

/// The scheduled transaction's remaining occurrences, 0 when the file gives none (`xaccSchedXactionGetRemOccur`).
pub(crate) fn remaining(sx: &ScheduledTransaction) -> i32 {
    sx.occurrences.map_or(0, |o| o.remaining)
}

/// The occurrence after `state`, or `None` when there is none: `xaccSchedXactionGetNextInstance`
/// (`SchedXaction.cpp:885-926`).
///
/// The reference is the state's last occurrence, or the day before the start when that is unset or earlier, so a
/// start date is itself an occurrence. With no state (GnuCash's `NULL`) the scheduled transaction's own last occurrence
/// is ignored and the answer is always the first occurrence. An end date ends the schedule after it; otherwise an
/// occurrence limit ends it when the remaining count (the state's, or with no state the scheduled transaction's) is
/// exactly 0. A file holding both an end date and a limit is ended by the date alone.
pub fn next_instance(sx: &ScheduledTransaction, state: Option<&TemporalState>) -> Result<Option<Date>, RecurrenceError> {
    let reference = state
        .and_then(|s| s.last)
        .filter(|last| *last >= sx.start)
        .unwrap_or_else(|| sx.start.add_days(-1));
    let next = next_instance_of_list(&sx.schedule, reference)?;
    let ended = |next: Date| match sx.end {
        Some(end) => next > end,
        None => has_occurrence_limit(sx) && state.map_or_else(|| remaining(sx), |s| s.remaining) == 0,
    };
    Ok(next.filter(|n| !ended(*n)))
}

/// Step `state` past its next occurrence: `gnc_sx_incr_temporal_state` (`SchedXaction.cpp:1044-1054`). The last
/// occurrence becomes the next one (`None` past the end), the remaining count goes down by one when the scheduled
/// transaction has a limit, and the Instance count goes up by one.
pub fn advance(sx: &ScheduledTransaction, state: &mut TemporalState) -> Result<(), RecurrenceError> {
    let next = next_instance(sx, Some(state))?;
    step(sx, state, next);
    Ok(())
}

fn step(sx: &ScheduledTransaction, state: &mut TemporalState, next: Option<Date>) {
    state.last = next;
    if has_occurrence_limit(sx) {
        state.remaining = state.remaining.saturating_sub(1);
    }
    state.instance_count = state.instance_count.saturating_add(1);
}

/// One occurrence of a scheduled transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrence {
    /// Its date.
    pub date: Date,
    /// Its Instance count, the `i` its formulas see.
    pub instance_count: i32,
    /// The temporal state before it.
    pub before: TemporalState,
}

/// The occurrences after a temporal state, in order, each from the state before it, until the schedule ends.
#[derive(Debug, Clone)]
pub struct Upcoming<'a> {
    sx: &'a ScheduledTransaction,
    state: Option<TemporalState>,
}

impl<'a> Upcoming<'a> {
    /// The occurrences after `state`.
    pub fn after(sx: &'a ScheduledTransaction, state: TemporalState) -> Self {
        Self { sx, state: Some(state) }
    }
}

impl Iterator for Upcoming<'_> {
    type Item = Result<Occurrence, RecurrenceError>;

    fn next(&mut self) -> Option<Self::Item> {
        let before = self.state.take()?;
        match next_instance(self.sx, Some(&before)) {
            Ok(Some(date)) => {
                let mut after = before;
                step(self.sx, &mut after, Some(date));
                self.state = Some(after);
                Some(Ok(Occurrence {
                    date,
                    instance_count: before.instance_count,
                    before,
                }))
            }
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

/// The next `count` occurrences of a scheduled transaction from its own temporal state, only those after `after` if
/// given: what the editor's example calendar and `scheduledTransaction.occurrences` show. Each date is
/// [`next_instance`] of the state before it; deferred instances are not occurrences.
pub fn occurrences(sx: &ScheduledTransaction, count: usize, after: Option<Date>) -> Result<Vec<Occurrence>, RecurrenceError> {
    Upcoming::after(sx, TemporalState::of(sx))
        .filter(|o| o.as_ref().map_or(true, |o| after.is_none_or(|a| o.date > a)))
        .take(count)
        .collect()
}

#[cfg(test)]
mod tests;
