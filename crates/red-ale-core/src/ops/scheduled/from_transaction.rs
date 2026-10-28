//! GnuCash's Schedule from transaction dialog (`5.17:gnucash/gnome/dialog-sx-from-trans.cpp`;
//! `design/notes/engine-scheduled-transactions.md` §5) as one operation: a scheduled transaction with one Recurrence
//! and one template transaction copied from a transaction of the Book.

use super::edit::{Edited, End, Fields, RecurrenceSpec, SplitSpec, TemplateSpec, create};
use super::error::ScheduledError;
use super::text::{formula_number, places_of};
use crate::formula::NumberFormat;
use crate::ops::commit::commodity_fraction;
use crate::{Book, Date, DomainError, Env, Guid, PeriodType, Recurrence, Split, WeekendAdjust};

/// The dialog's frequencies (`:70-77`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Frequency {
    /// Every day; the dialog's default (`:422`).
    #[default]
    Daily,
    /// Every week.
    Weekly,
    /// Every two weeks.
    BiWeekly,
    /// Every month.
    Monthly,
    /// Every three months.
    Quarterly,
    /// Every twelve months: GnuCash's Yearly is month ×12, not `year`.
    Yearly,
}

impl Frequency {
    /// The Recurrence `sxftd_update_schedule` makes from `start`, weekend adjustment none (`:278-327`).
    fn recurrence(self, start: Date) -> RecurrenceSpec {
        let (mult, period_type) = match self {
            Self::Daily => (1, PeriodType::Day),
            Self::Weekly => (1, PeriodType::Week),
            Self::BiWeekly => (2, PeriodType::Week),
            Self::Monthly => (1, PeriodType::Month),
            Self::Quarterly => (3, PeriodType::Month),
            Self::Yearly => (12, PeriodType::Month),
        };
        RecurrenceSpec {
            mult,
            period_type,
            start: Some(start),
            weekend_adjust: WeekendAdjust::None,
        }
    }
}

/// The dialog's choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FromTransaction {
    /// The transaction to copy.
    pub transaction: Guid,
    /// The name; by default the transaction's description (`:361-368`).
    pub name: Option<String>,
    /// How often.
    pub frequency: Frequency,
    /// The first occurrence; by default the frequency's first occurrence after the transaction's posted date
    /// (`:417-430`).
    pub start: Option<Date>,
    /// The end (`:154-203`, `:464-482`).
    pub end: End,
    /// Create instances without asking: the editor's preference `create-auto`, false by default (`:486-507`).
    pub auto_create: Option<bool>,
    /// And say so: the preference `notify`, true by default, stored only with auto-create.
    pub notify: Option<bool>,
    /// The preference `create-days`, 0 by default.
    pub advance_create_days: Option<i32>,
    /// The preference `remind-days`, 0 by default.
    pub advance_remind_days: Option<i32>,
}

/// Make the scheduled transaction the dialog makes (`sxftd_compute_sx`, `:440-515`): instance count 1, one
/// Recurrence from the start date, and one template transaction with the transaction's description, num, notes and
/// currency (`sxftd_add_template_trans`, `:206-275`). Each split becomes a template split with its memo, action and
/// account and its value as a formula: a positive value as the debit formula, any other as the credit formula of its
/// negation, written as [`formula_number`] writes it with at least the currency's decimal places. The transaction is
/// not changed. The checks of [`create`] apply.
pub fn from_transaction(book: &mut Book, env: &mut dyn Env, request: FromTransaction, format: NumberFormat) -> Result<Edited, ScheduledError> {
    let t = book
        .transaction(request.transaction)
        .ok_or_else(|| DomainError::not_found("transaction", &request.transaction))?;
    let start = match request.start {
        Some(start) => start,
        None => first_after(request.frequency, posted_date(env, t.date_posted)?)?,
    };
    let places = commodity_fraction(book, &t.currency).map_or(2, places_of);
    let template = TemplateSpec {
        guid: None,
        description: t.description.clone(),
        num: t.num.clone(),
        notes: t.notes().map(str::to_owned),
        currency: Some(t.currency.clone()),
        splits: t.splits.iter().map(|s| template_split(s, places, format)).collect::<Result<_, _>>()?,
    };
    let fields = Fields {
        name: Some(request.name.unwrap_or_else(|| t.description.clone().unwrap_or_default())),
        start: Some(start),
        schedule: Some(vec![request.frequency.recurrence(start)]),
        end: Some(request.end),
        auto_create: request.auto_create,
        notify: request.notify,
        advance_create_days: request.advance_create_days,
        advance_remind_days: request.advance_remind_days,
        templates: Some(vec![template]),
        ..Fields::default()
    };
    create(book, env, fields, format)
}

fn posted_date(env: &dyn Env, posted: crate::Time64) -> Result<Date, ScheduledError> {
    env.zone()
        .local_date(posted)
        .ok_or_else(|| DomainError::invalid("the transaction's posted date is outside the calendar").into())
}

/// The first occurrence after `posted` of the frequency's Recurrence started on `posted`, as the dialog proposes it
/// (`recurrenceListNextInstance`, `:425-428`).
fn first_after(frequency: Frequency, posted: Date) -> Result<Date, ScheduledError> {
    let r = frequency.recurrence(posted);
    Recurrence::normalised(r.mult, r.period_type, posted, r.weekend_adjust)
        .next_instance(posted)?
        .ok_or_else(|| ScheduledError::invalid("frequency", "the frequency has no occurrence after the transaction"))
}

fn template_split(s: &Split, places: u32, format: NumberFormat) -> Result<SplitSpec, ScheduledError> {
    let account = s
        .account
        .ok_or_else(|| DomainError::invalid(format!("split {} has no account", s.guid)))?;
    let (debit, credit) = if s.value.is_positive() {
        (Some(formula_number(s.value, places, format)), None)
    } else {
        (None, Some(formula_number(s.value.neg(), places, format)))
    };
    Ok(SplitSpec {
        guid: None,
        account,
        memo: s.memo.clone(),
        action: s.action.clone(),
        debit_formula: debit,
        credit_formula: credit,
    })
}

#[cfg(test)]
mod tests;
