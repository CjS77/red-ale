//! The Since Last Run proposal: the instances of every enabled scheduled transaction that are due, or postponed, or
//! coming up for a reminder (`_gnc_sx_gen_instances`, `5.17:libgnucash/app-utils/gnc-sx-instance-model.c:482-556`).

use std::collections::BTreeSet;

use super::create::{BuiltTransaction, build_instance, check_account};
use super::error::ScheduledError;
use super::occurrences::{TemporalState, Upcoming, next_instance};
use super::template::{INSTANCE_COUNT_VARIABLE, template_transactions, variables};
use crate::book_options::auto_readonly_threshold;
use crate::formula::{NumberFormat, Variables};
use crate::{Book, Date, Guid, Numeric, RecurrenceError, ScheduledTransaction};

/// What applying an instance does: GnuCash's `GncSxInstanceState` (`gnc-sx-instance-model.h:52-60`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstanceState {
    /// Create its transactions, then advance the schedule.
    ToCreate,
    /// Nothing: reminders are not recorded, so it is proposed again next time.
    Reminder,
    /// Save it as a Deferred instance, then advance the schedule.
    Postponed,
    /// Advance the schedule without creating anything.
    Ignored,
    /// Created; only in results, never chosen.
    Created,
}

impl InstanceState {
    /// The name on the wire: `toCreate`, `reminder`, `postponed`, `ignored` or `created`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToCreate => "toCreate",
            Self::Reminder => "reminder",
            Self::Postponed => "postponed",
            Self::Ignored => "ignored",
            Self::Created => "created",
        }
    }
}

/// A Variable of an instance and its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    /// Its name.
    pub name: String,
    /// Its value; `None` while unbound.
    pub value: Option<Numeric>,
    /// False for `i`, which is the Instance count.
    pub editable: bool,
}

/// One occurrence of a scheduled transaction as Since Last Run acts on it (`GncSxInstance`,
/// `gnc-sx-instance-model.h:69-77`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// Its position across the whole proposal: the key choices name it by.
    pub index: usize,
    /// The occurrence date.
    pub date: Date,
    /// The proposed state: postponed, to create or reminder.
    pub state: InstanceState,
    /// The temporal state before it; its Instance count is the instance's `i`.
    pub before: TemporalState,
    /// Every Variable, sorted by name, `i` bound.
    pub variables: Vec<Variable>,
    /// Dated before the auto read-only threshold: creating it needs the caller's explicit choice.
    pub read_only_by_date: bool,
    /// What creating it as it stands would fail on, found without the caller's values: Formulas that don't parse or
    /// don't evaluate, and template splits whose account can't take the split.
    pub errors: Vec<ScheduledError>,
    /// When every variable has a value: the transactions creating it would make. Otherwise `None`.
    pub transactions: Option<Vec<BuiltTransaction>>,
}

impl Instance {
    /// The Instance count, `i`.
    pub fn instance_count(&self) -> i32 {
        self.before.instance_count
    }

    /// The bound variables, as formulas read them.
    pub fn bindings(&self) -> Variables {
        self.variables.iter().filter_map(|v| v.value.map(|n| (v.name.clone(), n))).collect()
    }

    /// The names of the variables still unbound.
    pub fn unbound(&self) -> impl Iterator<Item = &str> {
        self.variables.iter().filter(|v| v.value.is_none()).map(|v| v.name.as_str())
    }
}

/// The instances of one scheduled transaction (`GncSxInstances`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleInstances {
    /// The scheduled transaction.
    pub scheduled: Guid,
    /// Its name.
    pub name: String,
    /// It creates its instances without asking.
    pub auto_create: bool,
    /// And says so.
    pub notify: bool,
    /// Its instances: postponed first, then to create, then reminders, each group in date order.
    pub instances: Vec<Instance>,
}

/// A Since Last Run proposal: what is due on `today`. It changes nothing; [`super::apply`] acts on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// The day it was made for.
    pub today: Date,
    /// The auto read-only threshold date, if the Book has one.
    pub threshold: Option<Date>,
    /// One entry per enabled scheduled transaction with at least one instance, in book order.
    pub schedules: Vec<ScheduleInstances>,
}

impl Proposal {
    /// Every instance with its scheduled transaction's entry, in index order.
    pub fn instances(&self) -> impl Iterator<Item = (&ScheduleInstances, &Instance)> {
        self.schedules.iter().flat_map(|s| s.instances.iter().map(move |i| (s, i)))
    }

    /// The instance at `index`.
    pub fn instance(&self, index: usize) -> Option<(&ScheduleInstances, &Instance)> {
        self.instances().find(|(_, i)| i.index == index)
    }

    /// Counts of its instances.
    pub fn summary(&self) -> Summary {
        self.instances().fold(Summary::default(), |mut s, (schedule, i)| {
            match i.state {
                InstanceState::ToCreate => s.to_create += 1,
                InstanceState::Reminder => s.reminders += 1,
                InstanceState::Postponed => s.postponed += 1,
                InstanceState::Ignored | InstanceState::Created => {}
            }
            s.auto_create += usize::from(schedule.auto_create);
            s.need_variables += usize::from(i.state == InstanceState::ToCreate && i.unbound().next().is_some());
            s.read_only_by_date += usize::from(i.read_only_by_date);
            s.errors += usize::from(!i.errors.is_empty());
            s
        })
    }
}

/// How many instances a proposal holds, by what they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    /// Proposed to create.
    pub to_create: usize,
    /// Proposed as reminders.
    pub reminders: usize,
    /// Postponed before.
    pub postponed: usize,
    /// Of auto-create scheduled transactions.
    pub auto_create: usize,
    /// To create, with a variable still unbound.
    pub need_variables: usize,
    /// Dated before the read-only threshold.
    pub read_only_by_date: usize,
    /// With errors.
    pub errors: usize,
}

/// Propose the due instances of every enabled scheduled transaction on `today` (the local day of the caller's clock),
/// as `_gnc_sx_gen_instances` builds them (`gnc-sx-instance-model.c:482-556`), with red-ale's deviation (a) of
/// `design/api/scheduled-transactions.md` ("Deviations in Since Last Run", 1):
///
/// - each Deferred instance is proposed `postponed` at the next occurrence after its own saved state; one with no next
///   occurrence left (its schedule changed since) is not proposed;
/// - then, from the scheduled transaction's own temporal state, never from a Deferred instance's (where GnuCash
///   proposes again the instances created after a postponed one), every occurrence up to `today` plus the days to
///   create in advance is `toCreate`, and the ones after it up to that plus the days to remind are `reminder`.
///
/// Auto-create scheduled transactions are proposed like any other. `format` reads the Formulas when listing the
/// Variables.
pub fn since_last_run(book: &Book, today: Date, format: NumberFormat) -> Result<Proposal, RecurrenceError> {
    let threshold = auto_readonly_threshold(book, today);
    let mut next_index = 0;
    let schedules = book
        .scheduled
        .values()
        .filter(|sx| sx.enabled)
        .map(|sx| schedule_instances(book, sx, today, threshold, format, &mut next_index))
        .filter(|s| s.as_ref().map_or(true, |s| !s.instances.is_empty()))
        .collect::<Result<_, _>>()?;
    Ok(Proposal {
        today,
        threshold,
        schedules,
    })
}

fn schedule_instances(
    book: &Book,
    sx: &ScheduledTransaction,
    today: Date,
    threshold: Option<Date>,
    format: NumberFormat,
    next_index: &mut usize,
) -> Result<ScheduleInstances, RecurrenceError> {
    let due = due(sx, today)?;
    let (names, problems) = if due.is_empty() {
        (BTreeSet::new(), Vec::new())
    } else {
        template_problems(book, sx, format)
    };
    let instances = due
        .into_iter()
        .map(|(date, state, before)| {
            let index = *next_index;
            *next_index += 1;
            let mut instance = Instance {
                index,
                date,
                state,
                before,
                variables: unbound(&names, before.instance_count),
                read_only_by_date: threshold.is_some_and(|t| date < t),
                errors: problems.iter().cloned().map(|e| e.at(index)).collect(),
                transactions: None,
            };
            preview(book, sx, format, &mut instance);
            instance
        })
        .collect();
    Ok(ScheduleInstances {
        scheduled: sx.guid,
        name: sx.name.clone(),
        auto_create: sx.auto_create,
        notify: sx.auto_create_notify,
        instances,
    })
}

/// A scheduled transaction's Variables, with the problems of its template that no value can fix: Formulas that don't
/// parse, and splits whose account can't take the split.
fn template_problems(book: &Book, sx: &ScheduledTransaction, format: NumberFormat) -> (BTreeSet<String>, Vec<ScheduledError>) {
    let (names, mut errors) = variables(book, sx, format);
    let accounts = template_transactions(book, sx)
        .into_iter()
        .flat_map(|t| t.splits.iter().map(move |s| (t, s)))
        .filter_map(|(t, s)| check_account(book, sx, t, s).err());
    accounts.for_each(|e| {
        if !errors.contains(&e) {
            errors.push(e);
        }
    });
    (names, errors)
}

/// When an instance has every variable bound and no template problem, what creating it would make, or why it can't.
fn preview(book: &Book, sx: &ScheduledTransaction, format: NumberFormat, instance: &mut Instance) {
    if instance.unbound().next().is_some() || !instance.errors.is_empty() {
        return;
    }
    match build_instance(book, sx, &instance.bindings(), format) {
        Ok(built) => instance.transactions = Some(built),
        Err(e) => instance.errors.push(e.at(instance.index)),
    }
}

/// The dates, states and temporal states of a scheduled transaction's instances on `today`.
fn due(sx: &ScheduledTransaction, today: Date) -> Result<Vec<(Date, InstanceState, TemporalState)>, RecurrenceError> {
    let creation_end = today.add_days(i64::from(sx.advance_create_days));
    let remind_end = creation_end.add_days(i64::from(sx.advance_remind_days));
    let postponed = sx.deferred.iter().map(TemporalState::of_deferred).filter_map(|before| {
        next_instance(sx, Some(&before))
            .map(|date| date.map(|d| (d, InstanceState::Postponed, before)))
            .transpose()
    });
    let coming = Upcoming::after(sx, TemporalState::of(sx))
        .take_while(|o| o.as_ref().map_or(true, |o| o.date <= creation_end.max(remind_end)))
        .map(|o| {
            o.map(|o| {
                let state = if o.date <= creation_end {
                    InstanceState::ToCreate
                } else {
                    InstanceState::Reminder
                };
                (o.date, state, o.before)
            })
        });
    postponed.chain(coming).collect()
}

/// The Variables of one instance: each unbound and editable but `i`, the Instance count
/// (`gnc_sx_instance_new`, `gnc-sx-instance-model.c:424-459`).
fn unbound(names: &BTreeSet<String>, instance_count: i32) -> Vec<Variable> {
    names
        .iter()
        .map(|name| {
            let i = name == INSTANCE_COUNT_VARIABLE;
            Variable {
                name: name.clone(),
                value: i.then(|| Numeric::from_int(i64::from(instance_count))),
                editable: !i,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
