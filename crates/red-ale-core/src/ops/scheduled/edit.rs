//! Creating, changing and deleting scheduled transactions: what GnuCash's editor saves (`gnc_sxed_save_sx`,
//! `5.17:gnucash/gnome/dialog-sx-editor.c:916-1026`) and its list page deletes (`_destroy_sx`,
//! `5.17:gnucash/gnome/gnc-plugin-page-sx-list.cpp:1019-1031`), with the editor's checks: what it refuses is refused,
//! what it asks about is a [`Warning`] (`design/api/scheduled-transactions.md`, "Methods").

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use super::checks::{check, warnings};
use super::error::ScheduledError;
use super::write::{account_of, new_template_account, set_templates};
use crate::formula::NumberFormat;
use crate::{
    Book, CommodityRef, Date, DomainError, DomainResult, Env, Frame, Guid, Occurrences, PeriodType, Recurrence, ScheduledTransaction,
    WeekendAdjust,
};

/// How a scheduled transaction ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// It never ends.
    Never,
    /// No occurrence after this date.
    Date(Date),
    /// At most `total` occurrences in all, `remaining` of them still to come.
    Occurrences {
        /// How many in all, at least 1.
        total: i32,
        /// How many still to come, 0 to `total`.
        remaining: i32,
    },
}

/// One Recurrence of a Schedule as the caller gives it. With no start it starts on the scheduled transaction's start
/// date, as the editor gives both from one date (`dialog-sx-editor.c:1009-1023`). It is stored as `recurrenceSet`
/// normalises it (`5.17:libgnucash/engine/Recurrence.cpp:87-139`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecurrenceSpec {
    /// Steps between occurrences; 0 becomes 1 except for `once`.
    pub mult: u16,
    /// The step.
    pub period_type: PeriodType,
    /// The first occurrence, if not the scheduled transaction's start date.
    pub start: Option<Date>,
    /// Where an occurrence on a weekend moves to.
    pub weekend_adjust: WeekendAdjust,
}

impl RecurrenceSpec {
    fn normalised(self, start: Date) -> Recurrence {
        Recurrence::normalised(self.mult, self.period_type, self.start.unwrap_or(start), self.weekend_adjust)
    }
}

/// A template split as the caller gives it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SplitSpec {
    /// An existing template split of this scheduled transaction to keep the GUID of; `None` for a new one.
    pub guid: Option<Guid>,
    /// The real account the created split goes to.
    pub account: Guid,
    /// The memo.
    pub memo: Option<String>,
    /// The action.
    pub action: Option<String>,
    /// The debit Formula; empty or absent means no debit.
    pub debit_formula: Option<String>,
    /// The credit Formula; empty or absent means no credit.
    pub credit_formula: Option<String>,
}

/// A template transaction as the caller gives it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemplateSpec {
    /// An existing template transaction of this scheduled transaction to keep; `None` for a new one.
    pub guid: Option<Guid>,
    /// The description.
    pub description: Option<String>,
    /// The num.
    pub num: Option<String>,
    /// The notes.
    pub notes: Option<String>,
    /// The currency; by default the first split's account commodity, when that is a currency.
    pub currency: Option<CommodityRef>,
    /// The splits, in order.
    pub splits: Vec<SplitSpec>,
}

/// What `create` and `update` set; `None` leaves a field as it is, or at its default on create.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    /// The name, not empty; required on create.
    pub name: Option<String>,
    /// Since Last Run leaves out a disabled one; default true.
    pub enabled: Option<bool>,
    /// The first possible occurrence; default today.
    pub start: Option<Date>,
    /// The Schedule, at least one Recurrence; required on create.
    pub schedule: Option<Vec<RecurrenceSpec>>,
    /// The end; default never. Setting it clears the other kind of end, as the editor does.
    pub end: Option<End>,
    /// Create instances without asking; default false.
    pub auto_create: Option<bool>,
    /// Say so; default true with auto-create, refused without it.
    pub notify: Option<bool>,
    /// Days ahead to create, at least 0; default 0.
    pub advance_create_days: Option<i32>,
    /// Days ahead to remind, at least 0; default 0.
    pub advance_remind_days: Option<i32>,
    /// The template transactions, replacing all of them; default none.
    pub templates: Option<Vec<TemplateSpec>>,
}

/// Something the editor would ask the user to confirm, reported instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// Another scheduled transaction has this name (`gnc_sxed_check_names`, `dialog-sx-editor.c:536-545`).
    DuplicateName(String),
    /// It never occurs, or not before its end date (`gnc_sxed_check_endpoint`, `dialog-sx-editor.c:594-611`).
    NeverRuns,
}

impl Warning {
    /// The API's warning code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DuplicateName(_) => "duplicate-name",
            Self::NeverRuns => "never-runs",
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateName(name) => write!(f, "a scheduled transaction with the name {name:?} already exists"),
            Self::NeverRuns => f.write_str("this scheduled transaction will never run"),
        }
    }
}

/// A scheduled transaction created or changed, with what the editor would have asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    /// The scheduled transaction.
    pub guid: Guid,
    /// The editor's confirmations.
    pub warnings: Vec<Warning>,
}

/// How far a new scheduled transaction has got: none for the editor and Schedule from transaction, the payments
/// already made for a loan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Progress {
    /// The last occurrence acted on.
    pub last: Option<Date>,
    /// The Instance count.
    pub instance_count: i32,
}

impl Progress {
    /// Never run, `i` = 1 for the first instance, as the editor sets it (`dialog-sx-editor.c:1546-1549`).
    pub const NEW: Self = Self {
        last: None,
        instance_count: 1,
    };
}

/// The scheduled transaction with this GUID.
pub fn get(book: &Book, guid: Guid) -> DomainResult<&ScheduledTransaction> {
    book.scheduled
        .get(&guid)
        .map(AsRef::as_ref)
        .ok_or_else(|| DomainError::not_found("scheduledTransaction", &guid))
}

/// Create a scheduled transaction with `xaccSchedXactionMalloc`'s defaults (`5.17:libgnucash/engine/SchedXaction.cpp:71-88`,
/// `:384-420`) where `fields` gives nothing: its template account under the template root, named by its GUID, and its
/// template transactions. It goes last in book order.
pub fn create(book: &mut Book, env: &mut dyn Env, fields: Fields, format: NumberFormat) -> Result<Edited, ScheduledError> {
    create_with(book, env, fields, Progress::NEW, format)
}

pub(crate) fn create_with(
    book: &mut Book,
    env: &mut dyn Env,
    mut fields: Fields,
    progress: Progress,
    format: NumberFormat,
) -> Result<Edited, ScheduledError> {
    if fields.name.is_none() {
        return Err(no_name());
    }
    if fields.schedule.is_none() {
        return Err(no_schedule());
    }
    let today = env
        .today()
        .ok_or_else(|| DomainError::invalid("the clock is outside the calendar"))?;
    let guid = env.new_guid();
    let mut sx = ScheduledTransaction {
        guid,
        name: String::new(),
        enabled: true,
        auto_create: false,
        auto_create_notify: false,
        advance_create_days: 0,
        advance_remind_days: 0,
        instance_count: progress.instance_count,
        start: today,
        last: progress.last,
        end: None,
        occurrences: None,
        template_account: None,
        schedule: Vec::new(),
        deferred: Vec::new(),
        slots: Frame::new(),
    };
    let templates = fields.templates.take().unwrap_or_default();
    apply_fields(&mut sx, fields)?;
    sx.template_account = Some(new_template_account(book, env, guid));
    book.scheduled.insert(guid, Arc::new(sx));
    set_templates(book, env, guid, templates, format)?;
    finish(book, guid, true, format)
}

/// A scheduled transaction not saved, as never run (instance count 1): what the editor's and the Schedule from
/// transaction dialog's example calendars show (`gnc_sxed_update_cal`, `dialog-sx-editor.c:1651`). The Schedule and
/// the end are checked as `create` checks them.
pub fn unsaved(start: Date, schedule: Vec<RecurrenceSpec>, end: End) -> Result<ScheduledTransaction, ScheduledError> {
    let mut sx = ScheduledTransaction {
        guid: Guid::default(),
        name: String::new(),
        enabled: true,
        auto_create: false,
        auto_create_notify: false,
        advance_create_days: 0,
        advance_remind_days: 0,
        instance_count: Progress::NEW.instance_count,
        start,
        last: None,
        end: None,
        occurrences: None,
        template_account: None,
        schedule: Vec::new(),
        deferred: Vec::new(),
        slots: Frame::new(),
    };
    apply_fields(
        &mut sx,
        Fields {
            schedule: Some(schedule),
            end: Some(end),
            ..Fields::default()
        },
    )?;
    Ok(sx)
}

/// Change a scheduled transaction as the editor saves it. Its last occurrence, Instance count and Deferred instances
/// are left alone: only Since Last Run moves them.
pub fn update(book: &mut Book, env: &mut dyn Env, guid: Guid, mut fields: Fields, format: NumberFormat) -> Result<Edited, ScheduledError> {
    let mut sx = get(book, guid)?.clone();
    let name_changed = fields.name.as_ref().is_some_and(|n| *n != sx.name);
    let templates = fields.templates.take();
    apply_fields(&mut sx, fields)?;
    book.scheduled.insert(guid, Arc::new(sx));
    if let Some(templates) = templates {
        set_templates(book, env, guid, templates, format)?;
    }
    finish(book, guid, name_changed, format)
}

fn finish(book: &Book, guid: Guid, name_changed: bool, format: NumberFormat) -> Result<Edited, ScheduledError> {
    let sx = get(book, guid)?;
    check(book, sx, format)?;
    Ok(Edited {
        guid,
        warnings: warnings(book, sx, name_changed)?,
    })
}

/// Delete a scheduled transaction with its template transactions and template account, as `xaccSchedXactionFree`
/// does (`5.17:libgnucash/engine/SchedXaction.cpp:462-488`). Transactions it created stay.
pub fn delete(book: &mut Book, guid: Guid) -> DomainResult<()> {
    let accounts: BTreeSet<Guid> = account_of(book, get(book, guid)?).into_iter().collect();
    book.scheduled.shift_remove(&guid);
    book.templates
        .transactions
        .retain(|_, t| !t.splits.iter().any(|s| s.account.is_some_and(|a| accounts.contains(&a))));
    accounts.iter().for_each(|a| {
        book.templates.accounts.shift_remove(a);
    });
    Ok(())
}

fn no_name() -> ScheduledError {
    ScheduledError::invalid("name", "please name the scheduled transaction")
}

fn no_schedule() -> ScheduledError {
    ScheduledError::invalid("schedule", "a scheduled transaction's schedule needs at least one recurrence")
}

fn apply_fields(sx: &mut ScheduledTransaction, f: Fields) -> Result<(), ScheduledError> {
    if let Some(name) = f.name {
        if name.is_empty() {
            return Err(no_name());
        }
        sx.name = name;
    }
    sx.enabled = f.enabled.unwrap_or(sx.enabled);
    sx.start = f.start.unwrap_or(sx.start);
    if let Some(schedule) = f.schedule {
        if schedule.is_empty() {
            return Err(no_schedule());
        }
        sx.schedule = schedule.into_iter().map(|r| r.normalised(sx.start)).collect();
    }
    if let Some(end) = f.end {
        set_end(sx, end)?;
    }
    if sx.end.is_some_and(|end| end < sx.start) {
        // `xaccSchedXactionSetEndDate` refuses an end before the start (`SchedXaction.cpp:625-648`).
        return Err(ScheduledError::invalid("end", "the end date is before the start date"));
    }
    set_auto_create(sx, f.auto_create, f.notify)?;
    sx.advance_create_days = days(f.advance_create_days, sx.advance_create_days)?;
    sx.advance_remind_days = days(f.advance_remind_days, sx.advance_remind_days)?;
    Ok(())
}

/// One kind of end, clearing the other, as `gnc_sxed_save_sx` sets them (`dialog-sx-editor.c:928-964`), with the
/// editor's checks on a count (`gnc_sxed_check_endpoint`, `:547-590`).
fn set_end(sx: &mut ScheduledTransaction, end: End) -> Result<(), ScheduledError> {
    (sx.end, sx.occurrences) = match end {
        End::Never => (None, None),
        End::Date(date) => (Some(date), None),
        End::Occurrences { total, .. } if total < 1 => {
            return Err(ScheduledError::invalid("end", "there must be some number of occurrences"));
        }
        End::Occurrences { total, remaining } if !(0..=total).contains(&remaining) => {
            return Err(ScheduledError::invalid(
                "end",
                format!("the number of remaining occurrences ({remaining}) must be 0 to the number of total occurrences ({total})"),
            ));
        }
        End::Occurrences { total, remaining } => (None, Some(Occurrences { total, remaining })),
    };
    Ok(())
}

/// Notify is stored only with auto-create (`dialog-sx-editor.c:976-985`), and the editor disables it without
/// (`update_sensitivity`, `:1031-1043`): asking for it alone is refused. It defaults to on with auto-create.
fn set_auto_create(sx: &mut ScheduledTransaction, auto_create: Option<bool>, notify: Option<bool>) -> Result<(), ScheduledError> {
    let changed = auto_create.is_some_and(|a| a != sx.auto_create);
    sx.auto_create = auto_create.unwrap_or(sx.auto_create);
    if notify == Some(true) && !sx.auto_create {
        return Err(ScheduledError::invalid("notify", "notify needs auto-create"));
    }
    let kept = if changed { sx.auto_create } else { sx.auto_create_notify };
    sx.auto_create_notify = sx.auto_create && notify.unwrap_or(kept);
    Ok(())
}

fn days(given: Option<i32>, current: i32) -> Result<i32, ScheduledError> {
    match given {
        Some(d) if d < 0 => Err(ScheduledError::invalid(
            "advance-days",
            format!("days in advance can't be negative ({d})"),
        )),
        Some(d) => Ok(d),
        None => Ok(current),
    }
}

#[cfg(test)]
mod tests;
