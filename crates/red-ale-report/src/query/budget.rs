//! The `Budget` handle and the Book's budget reads (`design/reports.md` §5, "Budget"): GnuCash's `gnc-budget-*`
//! (`5.17:libgnucash/engine/gnc-budget.cpp`), read through `red_ale_core::budget` and `ops::budgets`.

use std::rc::Rc;

use red_ale_core::ops::budgets::{actual, default_budget, period_times};
use red_ale_core::time::PeriodBounds;
use red_ale_core::{Budget, Guid, Numeric};
use rhai::{Array, Dynamic, Engine, INT};

use super::args::{account_arg, small_count};
use super::book::numeric;
use super::{BookHandle, Fallible, Snapshot, script_error};
use crate::number::Rational;

/// A budget (Rhai type `Budget`).
#[derive(Debug, Clone)]
pub struct BudgetHandle {
    snap: Rc<Snapshot>,
    guid: Guid,
}

impl BudgetHandle {
    /// The budget's GUID.
    pub fn guid(&self) -> Guid {
        self.guid
    }

    fn get(&self) -> Fallible<&Budget> {
        self.snap
            .book()
            .budget(self.guid)
            .ok_or_else(|| script_error(format!("no budget {}", self.guid)))
    }

    /// The bounds of any period, past the count too, as GnuCash's reports ask for them (`engine-budgets.md` §4.3).
    fn bounds(&self, period: INT) -> Fallible<PeriodBounds> {
        let count = small_count(period, "a budget period")?
            .checked_add(1)
            .ok_or_else(|| script_error("the budget period is too large"))?;
        let times = period_times(&self.get()?.recurrence, count, self.snap.zone()).map_err(|e| script_error(e.to_string()))?;
        times.last().copied().ok_or_else(|| script_error("the budget has no such period"))
    }

    /// The account and period a cell read names.
    fn cell(&self, account: &Dynamic, period: INT) -> Fallible<(Guid, u32)> {
        Ok((account_arg(self.snap.book(), account)?, small_count(period, "a budget period")?))
    }
}

impl BookHandle {
    pub(crate) fn budget(&self, guid: Guid) -> BudgetHandle {
        BudgetHandle {
            snap: Rc::clone(&self.0),
            guid,
        }
    }
}

pub(super) fn register(engine: &mut Engine) {
    engine.register_type_with_name::<BudgetHandle>("Budget");
    engine.register_fn("==", |a: &mut BudgetHandle, b: BudgetHandle| a.guid == b.guid);
    engine.register_fn("!=", |a: &mut BudgetHandle, b: BudgetHandle| a.guid != b.guid);
    engine.register_fn("to_string", |b: &mut BudgetHandle| format!("Budget({})", b.guid));
    engine.register_get("guid", |b: &mut BudgetHandle| b.guid.to_string());
    engine.register_get("name", |b: &mut BudgetHandle| -> Fallible<String> { Ok(b.get()?.name.clone()) });
    engine.register_get("description", |b: &mut BudgetHandle| -> Fallible<String> {
        Ok(b.get()?.description.clone())
    });
    engine.register_get("numPeriods", |b: &mut BudgetHandle| -> Fallible<INT> {
        Ok(INT::from(b.get()?.num_periods))
    });
    // `gnc-budget-get-period-start-date`, `-end-date` (`gnc-budget.cpp:647-659`): local 00:00:00 of the period's
    // first day and 23:59:59 of its last.
    engine.register_fn("period_start_date", |b: &mut BudgetHandle, p: INT| -> Fallible<INT> {
        Ok(b.bounds(p)?.start.0)
    });
    engine.register_fn("period_end_date", |b: &mut BudgetHandle, p: INT| -> Fallible<INT> {
        Ok(b.bounds(p)?.end.0)
    });
    engine.register_fn("account_period_value", account_period_value);
    engine.register_fn(
        "is_account_period_value_set",
        |b: &mut BudgetHandle, a: Dynamic, p: INT| -> Fallible<bool> {
            let (account, period) = b.cell(&a, p)?;
            Ok(b.get()?.amount(account, period).is_some())
        },
    );
    engine.register_fn(
        "account_period_note",
        |b: &mut BudgetHandle, a: Dynamic, p: INT| -> Fallible<Dynamic> {
            let (account, period) = b.cell(&a, p)?;
            Ok(b.get()?.note(account, period).map_or(Dynamic::UNIT, |n| n.to_owned().into()))
        },
    );
    engine.register_fn("account_period_actual_value", account_period_actual_value);
    book_reads(engine);
}

/// `gnc-budget-get-account-period-value` (`gnc-budget.cpp:587-597`): the Budget amount, 0 when it is unset or the
/// period is past the count.
fn account_period_value(b: &mut BudgetHandle, a: Dynamic, p: INT) -> Fallible<Rational> {
    let (account, period) = b.cell(&a, p)?;
    numeric(b.get()?.amount(account, period).unwrap_or(Numeric::ZERO))
}

/// `gnc-budget-get-account-period-actual-value` (`gnc-budget.cpp:661-669`): the account's no-closing change over the
/// period with its descendants' converted, as `ops::budgets::actual` computes it. Any period may be asked for.
fn account_period_actual_value(b: &mut BudgetHandle, a: Dynamic, p: INT) -> Fallible<Rational> {
    let (account, period) = b.cell(&a, p)?;
    let found = actual(b.snap.book(), b.get()?, account, period, b.snap.zone()).map_err(|e| script_error(e.to_string()))?;
    numeric(found)
}

/// The Book's budget reads: every budget in book order, `gnc-budget-lookup` and `gnc-budget-get-default`
/// (`gnc-budget.cpp:720-752`), which falls back to the first budget in book order where GnuCash takes hash order.
fn book_reads(engine: &mut Engine) {
    engine.register_fn("budgets", |b: &mut BookHandle| -> Array {
        b.0.book().budgets.keys().map(|g| Dynamic::from(b.budget(*g))).collect()
    });
    engine.register_fn("budget", |b: &mut BookHandle, guid: &str| -> Dynamic {
        let found = Guid::parse(guid.trim()).ok().filter(|g| b.0.book().budget(*g).is_some());
        found.map_or(Dynamic::UNIT, |g| Dynamic::from(b.budget(g)))
    });
    engine.register_fn("default_budget", |b: &mut BookHandle| -> Dynamic {
        default_budget(b.0.book()).map_or(Dynamic::UNIT, |(g, _)| Dynamic::from(b.budget(g)))
    });
}
