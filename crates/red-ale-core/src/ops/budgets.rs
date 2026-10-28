//! Budget operations: create, update, copy and delete Budgets, set their amounts and notes, read their periods, and
//! the default budget. GnuCash splits these between the engine (`5.17:libgnucash/engine/gnc-budget.cpp`) and the GUI
//! (`5.17:gnucash/gnome/gnc-plugin-budget.c`, `gnc-plugin-page-budget.cpp`); where the GUI adds a rule that decides
//! what books hold, it is followed. See `design/api/budgets.md` and `design/notes/engine-budgets.md`.

mod estimate;
mod signs;

use std::collections::BTreeSet;
use std::sync::Arc;

pub use estimate::{Estimate, actual, estimate, period_times};
pub use signs::{BudgetScrub, ScrubReport, SignPolicy, scrub_signs};

use crate::book_options::FEATURES;
use crate::budget::{DEFAULT_NAME, DEFAULT_NUM_PERIODS, NOTES};
use crate::ops::business::contacts::set_feature_used;
use crate::{Book, Budget, Date, DomainError, DomainResult, Env, Frame, Guid, Numeric, PeriodType, Recurrence, SlotValue, WeekendAdjust};

/// The Book feature that says Budget amounts carry natural signs (`GNC_FEATURE_BUDGET_UNREVERSED`,
/// `5.17:libgnucash/engine/gnc-features.h:62`).
pub const NATURAL_SIGNS_FEATURE: &str = "Use natural signs in budget amounts";

/// The Book option naming the default budget, a GUID Slot (`5.17:libgnucash/engine/qofbook.cpp:176-179`,
/// `5.17:libgnucash/engine/qofbookslots.h:69-70`).
pub const OPT_DEFAULT_BUDGET: [&str; 3] = ["options", "Budgeting", "Default Budget"];

/// The most periods the Budget Options dialog's spin button holds (`5.17:gnucash/gtkbuilder/gnc-plugin-page-budget.glade:401-407`).
pub const MAX_NUM_PERIODS: u32 = 60;

/// What `create` and `update` set; `None` leaves a field as it is (or at GnuCash's default on create).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BudgetFields {
    /// The name.
    pub name: Option<String>,
    /// The description.
    pub description: Option<String>,
    /// The number of periods, 1 to [`MAX_NUM_PERIODS`].
    pub num_periods: Option<u32>,
    /// The Recurrence: never `once`, weekend adjustment `none`. Stored as `recurrenceSet` normalises it.
    pub recurrence: Option<Recurrence>,
}

/// One cell of a Budget: an Account and a 0-based period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cell {
    /// The Account; it must be in the account tree and not the root.
    pub account: Guid,
    /// The period, below the Budget's number of periods.
    pub period: u32,
}

/// How many amount and note Slots a Budget keeps without listing them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeptSlots {
    /// Amounts of accounts in the tree at or beyond the number of periods.
    pub hidden_amounts: usize,
    /// Notes of accounts in the tree at or beyond the number of periods.
    pub hidden_notes: usize,
    /// Amounts of GUIDs that name no account in the tree.
    pub orphaned_amounts: usize,
    /// Notes of GUIDs that name no account in the tree.
    pub orphaned_notes: usize,
}

/// Where the default budget in effect comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultFrom {
    /// The Book option names it.
    Option,
    /// The option is unset or stale, so it is the first Budget in book order.
    First,
}

/// True when the Book declares [`NATURAL_SIGNS_FEATURE`] (`gnc_features_check_used`).
pub fn uses_natural_signs(book: &Book) -> bool {
    book.slots.get_in(&[FEATURES, NATURAL_SIGNS_FEATURE]).is_some()
}

/// True when the Book has Budgets and lacks [`NATURAL_SIGNS_FEATURE`]: their sign convention is unknown until
/// [`scrub_signs`] runs, so edits are refused.
pub fn needs_sign_scrub(book: &Book) -> bool {
    !book.budgets.is_empty() && !uses_natural_signs(book)
}

fn check_signs(book: &Book) -> DomainResult<()> {
    if needs_sign_scrub(book) {
        Err(DomainError::NaturalSigns)
    } else {
        Ok(())
    }
}

/// The Budget with this GUID.
pub fn get(book: &Book, guid: Guid) -> DomainResult<&Budget> {
    book.budget(guid).ok_or_else(|| DomainError::not_found("budget", &guid))
}

fn get_mut(book: &mut Book, guid: Guid) -> DomainResult<&mut Budget> {
    book.budgets
        .get_mut(&guid)
        .map(Arc::make_mut)
        .ok_or_else(|| DomainError::not_found("budget", &guid))
}

/// Create a Budget with `gnc_budget_init`'s defaults (`5.17:libgnucash/engine/gnc-budget.cpp:103-119`): `Unnamed
/// Budget`, no description, 12 periods, monthly from the first of the current local month. The first Budget sets
/// [`NATURAL_SIGNS_FEATURE`], as New Budget does (`5.17:gnucash/gnome/gnc-plugin-budget.c:181-186`). It goes last in
/// book order.
pub fn create(book: &mut Book, env: &mut dyn Env, fields: BudgetFields) -> DomainResult<Guid> {
    check_signs(book)?;
    let today = env
        .today()
        .ok_or_else(|| DomainError::invalid("the clock is outside the calendar"))?;
    let first = Date::from_ymd(today.year(), today.month(), 1).unwrap_or(today);
    let mut budget = Budget {
        guid: env.new_guid(),
        name: DEFAULT_NAME.to_owned(),
        description: String::new(),
        num_periods: DEFAULT_NUM_PERIODS,
        recurrence: Recurrence::normalised(1, PeriodType::Month, first, WeekendAdjust::None),
        slots: Frame::new(),
    };
    apply(&mut budget, fields)?;
    set_feature_used(book, NATURAL_SIGNS_FEATURE);
    let guid = budget.guid;
    book.budgets.insert(guid, Arc::new(budget));
    Ok(guid)
}

/// Change a Budget's name, description, number of periods or Recurrence, as the Budget Options dialog does
/// (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:887-917`). Lowering the count only hides amounts and notes; a new
/// Recurrence moves none (`gnc_budget_set_num_periods`, `5.17:libgnucash/engine/gnc-budget.cpp:472-480`).
pub fn update(book: &mut Book, guid: Guid, fields: BudgetFields) -> DomainResult<()> {
    check_signs(book)?;
    apply(get_mut(book, guid)?, fields)
}

fn apply(budget: &mut Budget, fields: BudgetFields) -> DomainResult<()> {
    let num_periods = fields.num_periods.map(check_num_periods).transpose()?;
    let recurrence = fields.recurrence.map(check_recurrence).transpose()?;
    if let Some(name) = fields.name {
        budget.name = name;
    }
    if let Some(description) = fields.description {
        budget.description = description;
    }
    budget.num_periods = num_periods.unwrap_or(budget.num_periods);
    budget.recurrence = recurrence.unwrap_or(budget.recurrence);
    Ok(())
}

fn check_num_periods(n: u32) -> DomainResult<u32> {
    if (1..=MAX_NUM_PERIODS).contains(&n) {
        Ok(n)
    } else {
        Err(DomainError::invalid(format!(
            "a budget has 1 to {MAX_NUM_PERIODS} periods, not {n}"
        )))
    }
}

/// GnuCash's recurrence widget offers no `once` and always writes weekend adjustment `none`
/// (`5.17:gnucash/gnome-utils/gnc-recurrence.c:271-326`); what it builds goes through `recurrenceSet`.
fn check_recurrence(r: Recurrence) -> DomainResult<Recurrence> {
    if r.period_type() == PeriodType::Once {
        return Err(DomainError::invalid("a budget's recurrence must repeat, not be once"));
    }
    if r.weekend_adjust() != WeekendAdjust::None {
        return Err(DomainError::invalid("a budget's recurrence takes weekend adjustment none"));
    }
    Ok(Recurrence::normalised(r.mult(), r.period_type(), r.start(), r.weekend_adjust()))
}

/// Copy a Budget as Copy Budget does (`5.17:gnucash/gnome/gnc-plugin-budget.c:259-264`, `gnc_budget_clone`,
/// `5.17:libgnucash/engine/gnc-budget.cpp:354-383`): named `Copy of <name>`, with the description, Recurrence, count
/// and every set amount of an account in the tree below the count. Notes and other Slots are not copied.
pub fn copy(book: &mut Book, env: &mut dyn Env, guid: Guid) -> DomainResult<Guid> {
    check_signs(book)?;
    let source = get(book, guid)?;
    let mut slots = Frame::new();
    tree_accounts(book).iter().for_each(|a| {
        let key = a.to_string();
        visible(source, &[], *a)
            .filter_map(|(p, v)| v.as_numeric().map(|n| (p, n)))
            .for_each(|(p, n)| slots.set_in(&[&key, &p.to_string()], SlotValue::Numeric(n)));
    });
    let copy = Budget {
        guid: env.new_guid(),
        name: format!("Copy of {}", source.name),
        description: source.description.clone(),
        num_periods: source.num_periods,
        recurrence: source.recurrence,
        slots,
    };
    let new_guid = copy.guid;
    book.budgets.insert(new_guid, Arc::new(copy));
    Ok(new_guid)
}

/// Delete a Budget and every Slot it holds. Deleting the last unsets [`NATURAL_SIGNS_FEATURE`], as the budget page
/// does (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:944-948`); returns whether it did. The default budget option
/// is left alone. Allowed in a book that needs the sign scrub.
pub fn delete(book: &mut Book, guid: Guid) -> DomainResult<bool> {
    book.budgets
        .shift_remove(&guid)
        .ok_or_else(|| DomainError::not_found("budget", &guid))?;
    let unset = book.budgets.is_empty() && uses_natural_signs(book);
    if unset {
        book.slots.remove_in(&[FEATURES, NATURAL_SIGNS_FEATURE]);
    }
    Ok(unset)
}

/// Set (`Some`) or unset (`None`) Budget amounts, all or none: `gnc_budget_set_account_period_value` stores the
/// amount as given, an explicit zero included, and `gnc_budget_unset_account_period_value` deletes the Slot
/// (`5.17:libgnucash/engine/gnc-budget.cpp:516-576`). Returns how many cells were set and how many set cells unset.
pub fn set_amounts(book: &mut Book, guid: Guid, edits: &[(Cell, Option<Numeric>)]) -> DomainResult<(usize, usize)> {
    check_signs(book)?;
    check_cells(book, get(book, guid)?, edits.iter().map(|(c, _)| *c))?;
    let budget = get_mut(book, guid)?;
    Ok(edits.iter().fold((0, 0), |(set, unset), (cell, amount)| {
        let path = [cell.account.to_string(), cell.period.to_string()];
        let path = [path[0].as_str(), path[1].as_str()];
        if let Some(n) = amount {
            budget.slots.set_in(&path, SlotValue::Numeric(*n));
            (set + 1, unset)
        } else {
            let was_set = budget.slots.remove_in(&path).is_some_and(|v| v.as_numeric().is_some());
            (set, unset + usize::from(was_set))
        }
    }))
}

/// Set or delete Budget notes, all or none (`gnc_budget_set_account_period_note`,
/// `5.17:libgnucash/engine/gnc-budget.cpp:599-636`). An empty note deletes, as the GUI passes NULL for empty text
/// (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:1295-1296`). Returns how many were set and how many deleted.
pub fn set_notes(book: &mut Book, guid: Guid, edits: &[(Cell, Option<String>)]) -> DomainResult<(usize, usize)> {
    check_signs(book)?;
    check_cells(book, get(book, guid)?, edits.iter().map(|(c, _)| *c))?;
    let budget = get_mut(book, guid)?;
    Ok(edits.iter().fold((0, 0), |(set, deleted), (cell, note)| {
        let (account, period) = (cell.account.to_string(), cell.period.to_string());
        let path = [NOTES, account.as_str(), period.as_str()];
        match note.as_deref().filter(|n| !n.is_empty()) {
            Some(text) => {
                budget.slots.set_in(&path, SlotValue::String(text.to_owned()));
                (set + 1, deleted)
            }
            None => (set, deleted + usize::from(budget.slots.remove_in(&path).is_some())),
        }
    }))
}

/// Each cell names an account in the tree (not the root), a period below the count, and appears once. GnuCash warns
/// and does nothing for a period past the count (`5.17:libgnucash/engine/gnc-budget.cpp:549-553`).
fn check_cells(book: &Book, budget: &Budget, cells: impl Iterator<Item = Cell>) -> DomainResult<()> {
    let mut seen = BTreeSet::new();
    cells.into_iter().try_for_each(|cell| {
        check_account(book, cell.account)?;
        if cell.period >= budget.num_periods {
            return Err(DomainError::invalid(format!(
                "period {} is past the budget's {} periods (they count from 0)",
                cell.period, budget.num_periods
            )));
        }
        if !seen.insert(cell) {
            return Err(DomainError::invalid(format!(
                "the cell {}/{} is given twice",
                cell.account, cell.period
            )));
        }
        Ok(())
    })
}

/// An account that may hold Budget amounts: any account in the tree but the root.
pub fn check_account(book: &Book, account: Guid) -> DomainResult<()> {
    if account == book.root {
        return Err(DomainError::invalid("the root account has no budget amounts"));
    }
    if in_tree(book, account) {
        Ok(())
    } else {
        Err(DomainError::not_found("account", &account))
    }
}

fn in_tree(book: &Book, account: Guid) -> bool {
    account != book.root && std::iter::successors(Some(account), |g| book.account(*g).and_then(|a| a.parent)).any(|g| g == book.root)
}

/// Every account in the tree, depth first: the accounts `gnc_account_foreach_descendant` of the root visits.
fn tree_accounts(book: &Book) -> Vec<Guid> {
    book.descendants(book.root).iter().map(|a| a.guid).collect()
}

/// The Slots of one account under `prefix` whose keys are periods as GnuCash writes them (`std::to_string`), with
/// their periods.
fn period_slots<'a>(budget: &'a Budget, prefix: &[&str], account: &str) -> impl Iterator<Item = (u32, &'a SlotValue)> {
    let path: Vec<&str> = prefix.iter().copied().chain([account]).collect();
    budget
        .slots
        .get_in(&path)
        .and_then(SlotValue::as_frame)
        .into_iter()
        .flat_map(Frame::iter)
        .filter_map(|(k, v)| k.parse::<u32>().ok().filter(|p| p.to_string() == *k).map(|p| (p, v)))
}

/// The account's Slots under `prefix` below the count, in period order.
fn visible<'a>(budget: &'a Budget, prefix: &[&str], account: Guid) -> impl Iterator<Item = (u32, &'a SlotValue)> {
    let mut slots: Vec<(u32, &SlotValue)> = period_slots(budget, prefix, &account.to_string())
        .filter(|(p, _)| *p < budget.num_periods)
        .collect();
    slots.sort_by_key(|(p, _)| *p);
    slots.into_iter()
}

/// Accounts of the tree by full name.
fn by_full_name(book: &Book) -> Vec<Guid> {
    let mut accounts: Vec<(String, Guid)> = tree_accounts(book)
        .into_iter()
        .map(|g| (book.full_name(g).unwrap_or_default(), g))
        .collect();
    accounts.sort();
    accounts.into_iter().map(|(_, g)| g).collect()
}

/// Every set amount of an account in the tree below the count, by account full name then period.
pub fn amounts(book: &Book, budget: &Budget) -> Vec<(Cell, Numeric)> {
    by_full_name(book)
        .into_iter()
        .flat_map(|account| {
            visible(budget, &[], account).filter_map(move |(period, v)| v.as_numeric().map(|n| (Cell { account, period }, n)))
        })
        .collect()
}

/// Every note of an account in the tree below the count, likewise. Empty notes read as none
/// (`5.17:libgnucash/engine/gnc-budget.cpp:638-645`).
pub fn notes<'a>(book: &Book, budget: &'a Budget) -> Vec<(Cell, &'a str)> {
    by_full_name(book)
        .into_iter()
        .flat_map(|account| {
            visible(budget, &[NOTES], account)
                .filter_map(move |(period, v)| v.as_str().filter(|s| !s.is_empty()).map(|s| (Cell { account, period }, s)))
        })
        .collect()
}

/// The amounts (numeric Slots) and notes (non-empty string Slots) a Budget keeps but does not list: at or beyond the
/// count, or for GUIDs that name no account in the tree.
pub fn kept_slots(book: &Book, budget: &Budget) -> KeptSlots {
    let count = |prefix: &[&str], is_entry: fn(&SlotValue) -> bool| {
        let frame = if prefix.is_empty() {
            Some(&budget.slots)
        } else {
            budget.slots.get_in(prefix).and_then(SlotValue::as_frame)
        };
        frame
            .into_iter()
            .flat_map(Frame::iter)
            .fold((0, 0), |(hidden, orphaned), (key, _)| {
                let entries = period_slots(budget, prefix, key).filter(|(_, v)| is_entry(v));
                match Guid::parse(key).ok().filter(|g| g.to_string() == *key) {
                    Some(g) if in_tree(book, g) => (hidden + entries.filter(|(p, _)| *p >= budget.num_periods).count(), orphaned),
                    Some(_) => (hidden, orphaned + entries.count()),
                    None => (hidden, orphaned),
                }
            })
    };
    let (hidden_amounts, orphaned_amounts) = count(&[], |v| v.as_numeric().is_some());
    let (hidden_notes, orphaned_notes) = count(&[NOTES], |v| v.as_str().is_some_and(|s| !s.is_empty()));
    KeptSlots {
        hidden_amounts,
        hidden_notes,
        orphaned_amounts,
        orphaned_notes,
    }
}

/// The first and last local days of periods `0..count`: period *p* runs from Recurrence instance *p* to the day before
/// instance *p* + 1 (`recurrenceGetPeriodTime`, `5.17:libgnucash/engine/Recurrence.cpp:397-418`). Instances are
/// walked once with `recurrenceNextInstance`, as `recurrenceNthInstance` does (`:384-395`).
pub fn period_dates(recurrence: &Recurrence, count: u32) -> DomainResult<Vec<(Date, Date)>> {
    let bad = |e: crate::RecurrenceError| DomainError::invalid(e.to_string());
    let mut instance = recurrence.start();
    (0..count)
        .map(|_| {
            let next = recurrence.next_instance(instance).map_err(bad)?;
            let next = next.ok_or_else(|| DomainError::invalid("a once recurrence has no budget periods"))?;
            let period = (instance, next.add_days(-1));
            instance = next;
            Ok(period)
        })
        .collect()
}

/// The GUID the default budget option holds, or `None` when it is unset. A stale GUID is returned as stored.
pub fn default_budget_option(book: &Book) -> Option<Guid> {
    book.slots.get_in(&OPT_DEFAULT_BUDGET).and_then(SlotValue::as_guid)
}

/// The default budget in effect: the option's Budget, else the first Budget in book order, where
/// `gnc_budget_get_default` takes the first in hash order (`5.17:libgnucash/engine/gnc-budget.cpp:720-752`).
pub fn default_budget(book: &Book) -> Option<(Guid, DefaultFrom)> {
    default_budget_option(book)
        .filter(|g| book.budgets.contains_key(g))
        .map(|g| (g, DefaultFrom::Option))
        .or_else(|| book.budgets.keys().next().map(|g| (*g, DefaultFrom::First)))
}

/// Name the default budget, writing the GUID Slot as the book options dialog writes a budget option
/// (`5.17:libgnucash/engine/gnc-optiondb.cpp:529-535`); the current value writes nothing.
pub fn set_default_budget(book: &mut Book, guid: Guid) -> DomainResult<()> {
    get(book, guid)?;
    if default_budget_option(book) != Some(guid) {
        book.slots.set_in(&OPT_DEFAULT_BUDGET, SlotValue::Guid(guid));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
