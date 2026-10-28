//! Scheduled transactions at work: creating, changing and deleting them, their next occurrences, and Since Last Run.
//!
//! Ported from GnuCash 5.17: `5.17:libgnucash/engine/SchedXaction.cpp` (GPL-2.0-or-later; Copyright (C) 2001,2007
//! Joshua Sled <jsled@asynchronous.org>) and `5.17:libgnucash/app-utils/gnc-sx-instance-model.c` (GPL-2.0 and/or
//! GPL-3.0; Copyright (C) 2006 Josh Sled <jsled@asynchronous.org>), as `design/notes/engine-scheduled-transactions.md`
//! (§1.4–1.5, §3.5–3.9, §4) records them. Where red-ale departs from GnuCash, `design/api/scheduled-transactions.md`
//! ("Deviations in Since Last Run") says why.

mod apply;
mod cashflow;
mod checks;
mod create;
mod edit;
mod error;
#[cfg(test)]
mod fixture;
mod from_transaction;
mod loan;
mod occurrences;
mod proposal;
mod template;
mod text;
mod write;

pub use apply::{Applied, ApplyOptions, Choice, CreatedInstance, apply};
pub use cashflow::{CashFlow, CashFlowError, cash_flow, occurrences_in_range};
pub use create::{BuiltSplit, BuiltTransaction, Created, FROM_SCHEDULED, ImbalanceSplit, build_instance, create_built, create_instance};
pub use edit::{Edited, End, Fields, RecurrenceSpec, SplitSpec, TemplateSpec, Warning, create, delete, get, update};
pub use from_transaction::{Frequency, FromTransaction, from_transaction};
pub use loan::{Length, Loan, LoanCreated, OtherPayment, PaymentKind, RateType, Repayment, loan};
pub use error::{AccountProblem, FormulaFailure, Refusal, ScheduledError, Side, Site};
pub use occurrences::{Occurrence, TemporalState, Upcoming, advance, has_occurrence_limit, next_instance, occurrences};
pub use proposal::{Instance, InstanceState, Proposal, ScheduleInstances, Summary, Variable, since_last_run};
pub use template::{
    INSTANCE_COUNT_VARIABLE, exchange_rate_variable, is_marker, template_account, template_transactions, transaction_currency, variables,
};
pub use text::{formula_number, places_of};
pub use write::{TEMPLATE_ROOT_NAME, numeric_of};
