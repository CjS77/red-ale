//! The natural-signs scrub, ported from GnuCash.
//!
//! Ported from `5.17:libgnucash/engine/ScrubBudget.c` (GPL-2.0-or-later):
//! Copyright (c) 2020 Christoher Lam

use std::sync::Arc;

use super::{NATURAL_SIGNS_FEATURE, tree_accounts, uses_natural_signs, visible};
use crate::book_options::FEATURES;
use crate::ops::accounts::fundamental_type;
use crate::ops::business::contacts::set_feature_used;
use crate::{AccountType, Book, Budget, Guid, Numeric, SlotValue};

/// Which pre-3.8 sign convention the scrub guessed for a Budget (`SignReversals`, `ScrubBudget.c:39-44`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignPolicy {
    /// Income and expense amounts were both reversed (`HEURISTICS_INC_EXP`).
    IncomeExpense,
    /// The amounts already carry natural signs (`HEURISTICS_NONE`).
    None,
    /// Liability, equity and income amounts were reversed (`HEURISTICS_CREDIT_ACC`).
    CreditAccounts,
}

/// What the scrub did to one Budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetScrub {
    /// The Budget.
    pub budget: Guid,
    /// The convention guessed.
    pub policy: SignPolicy,
    /// How many set amounts were negated, zeros included.
    pub flipped: usize,
}

/// What [`scrub_signs`] did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScrubReport {
    /// The feature was set.
    pub feature_set: bool,
    /// The feature was unset, as the book has no Budgets.
    pub feature_unset: bool,
    /// Each Budget scrubbed, in book order.
    pub budgets: Vec<BudgetScrub>,
}

/// `gnc_maybe_scrub_all_budget_signs` (`ScrubBudget.c:193-216`): with no Budgets, unset the feature if set; with the
/// feature set, nothing; otherwise guess and flip each Budget, then set the feature.
pub fn scrub_signs(book: &mut Book) -> ScrubReport {
    let featured = uses_natural_signs(book);
    if book.budgets.is_empty() {
        if featured {
            book.slots.remove_in(&[FEATURES, NATURAL_SIGNS_FEATURE]);
        }
        return ScrubReport {
            feature_unset: featured,
            ..ScrubReport::default()
        };
    }
    if featured {
        return ScrubReport::default();
    }
    let accounts: Vec<(Guid, Option<AccountType>)> = tree_accounts(book)
        .into_iter()
        .map(|g| (g, book.account(g).and_then(|a| fundamental_type(a.account_type))))
        .collect();
    let budgets = book
        .budgets
        .values_mut()
        .map(|b| scrub_budget(Arc::make_mut(b), &accounts))
        .collect();
    set_feature_used(book, NATURAL_SIGNS_FEATURE);
    ScrubReport {
        feature_set: true,
        feature_unset: false,
        budgets,
    }
}

/// `maybe_scrub_budget` (`ScrubBudget.c:172-191`).
fn scrub_budget(budget: &mut Budget, accounts: &[(Guid, Option<AccountType>)]) -> BudgetScrub {
    let policy = policy(budget, accounts);
    let flips = |t: Option<AccountType>| match policy {
        SignPolicy::IncomeExpense => matches!(t, Some(AccountType::Income | AccountType::Expense)),
        SignPolicy::CreditAccounts => matches!(t, Some(AccountType::Liability | AccountType::Equity | AccountType::Income)),
        SignPolicy::None => false,
    };
    let negated: Vec<(Guid, u32, Numeric)> = accounts
        .iter()
        .filter(|(_, t)| flips(*t))
        .flat_map(|(g, _)| set_amounts(budget, *g).into_iter().map(|(p, n)| (*g, p, n.neg())))
        .collect();
    negated.iter().for_each(|(g, p, n)| {
        budget.slots.set_in(&[&g.to_string(), &p.to_string()], SlotValue::Numeric(*n));
    });
    BudgetScrub {
        budget: budget.guid,
        policy,
        flipped: negated.len(),
    }
}

/// `heuristics_on_budget` (`ScrubBudget.c:105-125`): per account, the sign of its set amounts summed with
/// `gnc_numeric_add_fixed`, 0 when the sum fails as `gnc_numeric_compare` gives 0 for an error
/// (`5.17:libgnucash/engine/gnc-numeric.cpp:837-842`), tallied by fundamental type (`:58-103`).
fn policy(budget: &Budget, accounts: &[(Guid, Option<AccountType>)]) -> SignPolicy {
    let tally = |kind: AccountType| -> i64 {
        accounts
            .iter()
            .filter(|(_, t)| *t == Some(kind))
            .map(|(g, _)| {
                Numeric::sum_fixed(set_amounts(budget, *g).into_iter().map(|(_, n)| n))
                    .map_or(0, |sum| match sum.cmp_value(Numeric::ZERO) {
                        std::cmp::Ordering::Less => -1,
                        std::cmp::Ordering::Equal => 0,
                        std::cmp::Ordering::Greater => 1,
                    })
            })
            .sum()
    };
    if tally(AccountType::Expense) < 0 {
        SignPolicy::IncomeExpense
    } else if tally(AccountType::Income) < 0 {
        SignPolicy::None
    } else {
        SignPolicy::CreditAccounts
    }
}

fn set_amounts(budget: &Budget, account: Guid) -> Vec<(u32, Numeric)> {
    visible(budget, &[], account)
        .filter_map(|(p, v)| v.as_numeric().map(|n| (p, n)))
        .collect()
}
