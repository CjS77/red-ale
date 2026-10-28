//! Applying a Since Last Run proposal with the caller's choices: each instance created, postponed, ignored or left
//! as a reminder, and each schedule advanced as `gnc_sx_instance_model_effect_change` advances it
//! (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1445-1554`; the note §4.4, §4.7–§4.8).

use std::collections::BTreeMap;
use std::sync::Arc;

use super::create::{BuiltTransaction, Created, build_instance, create_built};
use super::error::{Refusal, ScheduledError};
use super::occurrences::{TemporalState, remaining};
use super::proposal::{Instance, InstanceState, Proposal, ScheduleInstances};
use super::template::INSTANCE_COUNT_VARIABLE;
use crate::formula::{NumberFormat, Variables};
use crate::{Book, Date, DeferredInstance, DomainError, Env, Guid, ScheduledTransaction};

/// The caller's change to one instance of a proposal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Choice {
    /// The instance's index in the proposal.
    pub index: usize,
    /// Its new state, or `None` to keep the proposed one.
    pub state: Option<InstanceState>,
    /// Values for its variables.
    pub variables: Variables,
}

/// How to apply a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyOptions {
    /// Act only on the instances of auto-create scheduled transactions, as GnuCash does before it shows its dialog
    /// (`effect_change` with `auto_create_only`, `gnc-sx-instance-model.c:1483-1491`).
    pub auto_create_only: bool,
    /// The Book can hold Deferred instances. An SQL book can't (`5.17:libgnucash/backend/sql/gnc-schedxaction-sql.cpp:51-82`),
    /// so a new postponement there is refused rather than lost on the next load.
    pub can_postpone: bool,
    /// How the Formulas write numbers.
    pub format: NumberFormat,
}

impl Default for ApplyOptions {
    fn default() -> Self {
        Self {
            auto_create_only: false,
            can_postpone: true,
            format: NumberFormat::default(),
        }
    }
}

/// An instance whose transactions were created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedInstance {
    /// Its index in the proposal.
    pub index: usize,
    /// The scheduled transaction.
    pub scheduled: Guid,
    /// The occurrence date.
    pub date: Date,
    /// Its transactions, with the Imbalance split each commit added.
    pub transactions: Vec<Created>,
    /// Dated before the read-only threshold, so read-only by date once created.
    pub read_only_by_date: bool,
}

/// What an apply did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Applied {
    /// Instances created.
    pub created: Vec<CreatedInstance>,
    /// Instances postponed, newly or still.
    pub postponed: Vec<usize>,
    /// Instances ignored.
    pub ignored: Vec<usize>,
    /// Instances left as reminders.
    pub reminders: Vec<usize>,
    /// The scheduled transactions whose state changed, in book order.
    pub schedules: Vec<Guid>,
}

/// One instance with the caller's choice applied.
struct Resolved<'a> {
    instance: &'a Instance,
    state: InstanceState,
    bindings: Variables,
    chosen: bool,
}

/// Apply `proposal` to `book` with `choices`, as one edit: every check first, then every transaction created and every
/// schedule advanced. Instances not named keep the proposed state and bindings.
///
/// Refused, before anything changes ([`ScheduledError::Refused`], naming the instances): an unknown index, a second
/// choice for one instance, the state `created`; a value for `i` or for a name that is not a variable; a `toCreate`
/// instance with a variable unbound; a reminder before another state in one schedule, GnuCash's rule, which its dialog
/// keeps by changing other instances' states (deviation 4); a `toCreate` instance dated before the read-only threshold
/// that `choices` does not name as `toCreate` (deviation 3); a new postponement where the Book can't hold one; with
/// `auto_create_only`, a choice for a scheduled transaction that is not auto-create. Then every `toCreate` instance is
/// built, and a Formula that fails or an account that can't take its split refuses the whole apply (deviation 2).
///
/// Then, per scheduled transaction, in order of its instances, from its last occurrence, Instance count and remaining
/// occurrences: an instance proposed `postponed` and now otherwise leaves the deferred list; `toCreate` creates its
/// transactions and advances; a new `postponed` joins the deferred list (sorted by date) and advances; `ignored`
/// advances; a reminder does nothing. Advancing (`increment_sx_state`, `:1042-1058`) makes the instance's date the
/// last occurrence if later, the instance's `i` + 1 the Instance count (overwritten, not maximised), and takes one from
/// the remaining occurrences above zero. A postponed instance left postponed does nothing, where GnuCash advances
/// again on every run it stays postponed (`:1510-1517`). A scheduled transaction no instance changed is not touched.
pub fn apply(
    book: &mut Book,
    env: &mut dyn Env,
    proposal: &Proposal,
    choices: &[Choice],
    options: ApplyOptions,
) -> Result<Applied, ScheduledError> {
    let resolved = resolve(proposal, choices, options)?;
    let acted: Vec<(&ScheduleInstances, Vec<Resolved<'_>>)> = proposal
        .schedules
        .iter()
        .zip(resolved)
        .filter(|(s, _)| !options.auto_create_only || s.auto_create)
        .collect();
    acted.iter().try_for_each(|(_, instances)| check(instances))?;
    let built = acted
        .iter()
        .map(|(s, instances)| build_all(book, s.scheduled, instances, options.format))
        .collect::<Result<Vec<_>, _>>()?;
    let mut applied = Applied::default();
    acted
        .iter()
        .zip(built)
        .try_for_each(|((s, instances), built)| advance_schedule(book, env, s.scheduled, instances, built, &mut applied))?;
    Ok(applied)
}

/// Each schedule's instances with the choices applied, after checking each choice on its own.
fn resolve<'a>(proposal: &'a Proposal, choices: &[Choice], options: ApplyOptions) -> Result<Vec<Vec<Resolved<'a>>>, ScheduledError> {
    let mut by_index: BTreeMap<usize, &Choice> = BTreeMap::new();
    choices.iter().try_for_each(|c| {
        let refuse = |reason, message: &str| {
            Err(ScheduledError::refused(
                reason,
                vec![c.index],
                format!("instance {}: {message}", c.index),
            ))
        };
        let Some((schedule, instance)) = proposal.instance(c.index) else {
            return refuse(Refusal::Index, "no such instance");
        };
        if by_index.insert(c.index, c).is_some() {
            return refuse(Refusal::DuplicateChoice, "chosen twice");
        }
        if c.state == Some(InstanceState::Created) {
            return refuse(Refusal::CreatedState, "created is not a state to choose");
        }
        if options.auto_create_only && !schedule.auto_create {
            return refuse(Refusal::NotAutoCreate, "not an auto-create scheduled transaction");
        }
        let new_postponement = c.state == Some(InstanceState::Postponed) && instance.state != InstanceState::Postponed;
        if new_postponement && !options.can_postpone {
            return refuse(Refusal::SqlDeferredInstances, "this book can't hold postponed instances");
        }
        c.variables
            .iter()
            .try_for_each(|(name, value)| match instance.variables.iter().find(|v| v.name == *name) {
                None => refuse(Refusal::UnknownVariable, &format!("{name} is not one of its variables")),
                Some(v) if !v.editable || name == INSTANCE_COUNT_VARIABLE => refuse(Refusal::NotEditable, &format!("{name} can't be set")),
                Some(_) if !value.is_valid() => refuse(Refusal::UnboundVariable, &format!("{name} has no valid value")),
                Some(_) => Ok(()),
            })
    })?;
    Ok(proposal
        .schedules
        .iter()
        .map(|s| {
            s.instances
                .iter()
                .map(|instance| {
                    let choice = by_index.get(&instance.index);
                    let mut bindings = instance.bindings();
                    bindings.extend(choice.map(|c| c.variables.clone()).unwrap_or_default());
                    Resolved {
                        instance,
                        state: choice.and_then(|c| c.state).unwrap_or(instance.state),
                        bindings,
                        chosen: choice.is_some_and(|c| c.state == Some(InstanceState::ToCreate)),
                    }
                })
                .collect()
        })
        .collect())
}

/// The checks across one schedule's instances: the reminder rule, unbound variables and the read-only threshold.
fn check(instances: &[Resolved<'_>]) -> Result<(), ScheduledError> {
    let first_reminder = instances.iter().position(|r| r.state == InstanceState::Reminder);
    if let Some(first) = first_reminder {
        let later: Vec<usize> = instances[first..]
            .iter()
            .filter(|r| r.state != InstanceState::Reminder)
            .map(|r| r.instance.index)
            .collect();
        if !later.is_empty() {
            let reminder = instances[first].instance.index;
            let message = format!("reminder {reminder} comes before instances {later:?} in another state, and reminders are not recorded");
            return Err(ScheduledError::refused(
                Refusal::ReminderFirst,
                [vec![reminder], later].concat(),
                message,
            ));
        }
    }
    instances.iter().filter(|r| r.state == InstanceState::ToCreate).try_for_each(|r| {
        let index = r.instance.index;
        if let Some(name) = r.instance.variables.iter().map(|v| &v.name).find(|n| !r.bindings.contains_key(*n)) {
            return Err(ScheduledError::refused(
                Refusal::UnboundVariable,
                vec![index],
                format!("instance {index} needs a value for {name}"),
            ));
        }
        if r.instance.read_only_by_date && !r.chosen {
            let message = format!(
                "instance {index} is dated {}, before the read-only threshold: choose toCreate to create it",
                r.instance.date
            );
            return Err(ScheduledError::refused(Refusal::ReadOnlyByDate, vec![index], message));
        }
        Ok(())
    })
}

/// Every `toCreate` instance of one schedule built, in instance order.
fn build_all(
    book: &Book,
    scheduled: Guid,
    instances: &[Resolved<'_>],
    format: NumberFormat,
) -> Result<Vec<Vec<BuiltTransaction>>, ScheduledError> {
    let sx = book
        .scheduled
        .get(&scheduled)
        .ok_or_else(|| DomainError::not_found("scheduledTransaction", &scheduled))?;
    instances
        .iter()
        .filter(|r| r.state == InstanceState::ToCreate)
        .map(|r| build_instance(book, sx, &r.bindings, format).map_err(|e| e.at(r.instance.index)))
        .collect()
}

/// The schedule's last occurrence, Instance count and remaining occurrences while its instances are applied.
struct Progress {
    last: Option<Date>,
    instance_count: i32,
    remaining: i32,
}

impl Progress {
    /// `increment_sx_state` (`gnc-sx-instance-model.c:1042-1058`).
    fn advance(&mut self, instance: &Instance) {
        if self.last.is_none_or(|l| l <= instance.date) {
            self.last = Some(instance.date);
        }
        self.instance_count = instance.before.instance_count.saturating_add(1);
        if self.remaining > 0 {
            self.remaining -= 1;
        }
    }
}

fn advance_schedule(
    book: &mut Book,
    env: &mut dyn Env,
    scheduled: Guid,
    instances: &[Resolved<'_>],
    built: Vec<Vec<BuiltTransaction>>,
    applied: &mut Applied,
) -> Result<(), ScheduledError> {
    let before = book
        .scheduled
        .get(&scheduled)
        .ok_or_else(|| DomainError::not_found("scheduledTransaction", &scheduled))?
        .clone();
    let mut sx = ScheduledTransaction::clone(&before);
    let mut progress = Progress {
        last: sx.last,
        instance_count: sx.instance_count,
        remaining: remaining(&sx),
    };
    let mut built = built.into_iter();
    instances.iter().try_for_each(|r| -> Result<(), ScheduledError> {
        let (instance, index) = (r.instance, r.instance.index);
        let was_postponed = instance.state == InstanceState::Postponed;
        if was_postponed && r.state != InstanceState::Postponed {
            remove_deferred(&mut sx.deferred, instance.before.last);
        }
        match r.state {
            InstanceState::ToCreate => {
                let transactions =
                    create_built(book, env, scheduled, instance.date, built.next().unwrap_or_default()).map_err(|e| e.at(index))?;
                applied.created.push(CreatedInstance {
                    index,
                    scheduled,
                    date: instance.date,
                    transactions,
                    read_only_by_date: instance.read_only_by_date,
                });
                progress.advance(instance);
            }
            InstanceState::Postponed if !was_postponed => {
                add_deferred(&mut sx.deferred, instance.before);
                applied.postponed.push(index);
                progress.advance(instance);
            }
            InstanceState::Ignored => {
                applied.ignored.push(index);
                progress.advance(instance);
            }
            InstanceState::Postponed => applied.postponed.push(index),
            InstanceState::Reminder | InstanceState::Created => applied.reminders.push(index),
        }
        Ok(())
    })?;
    sx.last = progress.last;
    sx.instance_count = progress.instance_count;
    // `xaccSchedXactionSetRemOccur` keeps the old count when the new one exceeds the total (`SchedXaction.cpp:719-737`).
    if let Some(o) = sx.occurrences.as_mut().filter(|o| progress.remaining <= o.total) {
        o.remaining = progress.remaining;
    }
    if sx != *before {
        applied.schedules.push(scheduled);
        book.scheduled.insert(scheduled, Arc::new(sx));
    }
    Ok(())
}

/// `gnc_sx_add_defer_instance` (`SchedXaction.cpp:1098-1104`): before the first entry not earlier than it, as
/// `g_list_insert_sorted` places it.
fn add_deferred(deferred: &mut Vec<DeferredInstance>, state: TemporalState) {
    let at = deferred.iter().position(|d| !later(state.last, d.last)).unwrap_or(deferred.len());
    deferred.insert(at, state.to_deferred());
}

/// `gnc_sx_remove_defer_instance` (`SchedXaction.cpp:1110-1125`): the first entry with the same last occurrence.
fn remove_deferred(deferred: &mut Vec<DeferredInstance>, last: Option<Date>) {
    if let Some(at) = deferred.iter().position(|d| !later(last, d.last) && !later(d.last, last)) {
        deferred.remove(at);
    }
}

/// `g_date_compare(a, b) > 0`, an unset date comparing equal to any, as GLib's does.
fn later(a: Option<Date>, b: Option<Date>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a > b)
}

#[cfg(test)]
mod tests;
