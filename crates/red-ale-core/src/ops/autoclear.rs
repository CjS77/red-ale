//! Auto-clear: find the one set of uncleared splits in an account that brings its cleared balance to a given figure,
//! and mark them cleared.
//!
//! This follows GnuCash's search step for step (`gnc_account_get_autoclear_splits`,
//! `5.17:libgnucash/app-utils/gnc-autoclear.cpp:111-252`), so the same book and figure give the same splits or the same
//! failure. GnuCash stops after five seconds; red-ale stops after [`STEP_LIMIT`] steps, so its answer never depends on
//! the machine. See `design/api/reconcile.md`, "Auto-clear".

use std::cmp::Reverse;

use crate::ops::balances::account_splits;
use crate::ops::commit::account_scu;
use crate::ops::reconcile::set_split_state;
use crate::{Book, DomainError, DomainResult, Env, Guid, Numeric, ReconcileState, Round};

/// How many search steps red-ale takes before giving up, where GnuCash gives up after five seconds
/// (`MAX_AUTOCLEAR_SECONDS`, `5.17:gnucash/gnome/window-autoclear.c:142`).
pub const STEP_LIMIT: u64 = 1 << 25;

/// What the search found (`Autoclear`, `5.17:libgnucash/app-utils/gnc-autoclear.h:35-42`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Exactly one set of splits: these, in account order.
    Found(Vec<Guid>),
    /// The cleared balance is already the figure asked for (`ABORT_NOP`).
    AlreadyCleared,
    /// More than one set of splits gives the figure (`ABORT_MULTI`).
    Ambiguous,
    /// No set of splits gives the figure (`ABORT_UNREACHABLE`).
    Unreachable,
    /// The search stopped at [`STEP_LIMIT`] (GnuCash's `ABORT_TIMEOUT`).
    Limit,
}

impl Outcome {
    /// GnuCash's message for the outcome, untranslated (`5.17:libgnucash/app-utils/gnc-autoclear.cpp:130`, `:149`,
    /// `:205`, `:239`). A found set has no message.
    pub fn message(&self) -> Option<&'static str> {
        match self {
            Self::Found(_) => None,
            Self::AlreadyCleared => Some("Account is already at Auto-Clear Balance."),
            Self::Ambiguous => Some("Cannot uniquely clear splits. Found multiple possibilities."),
            Self::Unreachable => Some("The selected amount cannot be cleared."),
            Self::Limit => Some("Auto-clear stopped at its step limit before finishing the search."),
        }
    }
}

/// The result of an auto-clear search on one account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoClear {
    /// The account's cleared balance now: every split not `n`, raw sign, at the account's SCU.
    pub current: Numeric,
    /// The cleared balance asked for, rounded to the account's SCU with banker's rounding.
    pub target: Numeric,
    /// What the search found.
    pub outcome: Outcome,
}

/// An uncleared split the search may pick: its amount in whole SCUs and where it stands in account order.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    amount: i128,
    position: usize,
}

/// Find the splits of `account` that bring its cleared balance to `target` (raw sign). Reads only.
pub fn find(book: &Book, account: Guid, target: Numeric) -> DomainResult<AutoClear> {
    find_with_limit(book, account, target, STEP_LIMIT)
}

/// [`find`], stopping after `limit` search steps.
pub fn find_with_limit(book: &Book, account: Guid, target: Numeric, limit: u64) -> DomainResult<AutoClear> {
    book.account(account).ok_or_else(|| DomainError::not_found("account", &account))?;
    let scu = account_scu(book, account)?;
    let target = target.to_denom(scu, Round::Bankers)?;
    let splits = account_splits(book, account);
    let amounts = splits.iter().map(|(_, s)| whole_units(s.amount, scu, s.guid)).collect::<DomainResult<Vec<i128>>>()?;
    let is_new = |i: usize| splits[i].1.reconcile_state == ReconcileState::New;
    let cleared: i128 = (0..splits.len()).filter(|&i| !is_new(i)).map(|i| amounts[i]).sum();
    let current = Numeric::new(i64::try_from(cleared).map_err(|_| DomainError::invalid("the cleared balance is too large"))?, scu);
    let remaining = i128::from(target.num()) - cleared;
    let candidates: Vec<Candidate> =
        (0..splits.len()).filter(|&i| is_new(i) && amounts[i] != 0).map(|i| Candidate { amount: amounts[i], position: i }).collect();
    let outcome = if remaining == 0 {
        Outcome::AlreadyCleared
    } else {
        match search(candidates, remaining, limit) {
            Search::Found(mut positions) => {
                positions.sort_unstable();
                Outcome::Found(positions.into_iter().map(|i| splits[i].1.guid).collect())
            }
            Search::Ambiguous => Outcome::Ambiguous,
            Search::Unreachable => Outcome::Unreachable,
            Search::Limit => Outcome::Limit,
        }
    };
    Ok(AutoClear { current, target, outcome })
}

/// Run the search and, when it finds exactly one set of splits, mark each one cleared, as the auto-clear window's OK
/// does (`5.17:gnucash/gnome/window-autoclear.c:159-164`). Reconcile dates are left alone. Any other outcome is
/// refused with its message.
pub fn apply(book: &mut Book, env: &mut dyn Env, account: Guid, target: Numeric) -> DomainResult<AutoClear> {
    let result = find(book, account, target)?;
    mark_cleared(book, env, &result)?;
    Ok(result)
}

/// Mark cleared the splits a search found; refused, with GnuCash's message, unless it found exactly one set.
pub fn mark_cleared(book: &mut Book, env: &mut dyn Env, result: &AutoClear) -> DomainResult<()> {
    let Outcome::Found(splits) = &result.outcome else {
        return Err(DomainError::invalid(result.outcome.message().unwrap_or_default()));
    };
    splits.iter().try_for_each(|s| set_split_state(book, env, *s, ReconcileState::Cleared, None))
}

/// An amount as a whole number of SCUs (`numeric_to_int64`, `5.17:libgnucash/app-utils/gnc-autoclear.cpp:178-184`).
/// GnuCash's conversion fails silently when the amount is not a whole number of SCUs; red-ale refuses.
fn whole_units(amount: Numeric, scu: i64, split: Guid) -> DomainResult<i128> {
    amount
        .to_denom(scu, Round::Never)
        .map(|a| i128::from(a.num()))
        .map_err(|_| DomainError::invalid(format!("split {split}'s amount {} is not a whole number of 1/{scu}", amount.to_decimal_string())))
}

/// What the subset-sum search ends with.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Search {
    Found(Vec<usize>),
    Ambiguous,
    Unreachable,
    Limit,
}

/// One pending step of the depth-first search.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// Visit candidate `index` with `target` still to reach.
    Visit { index: usize, target: i128 },
    /// Take the last candidate back off the path.
    Pop,
}

/// GnuCash's `subset_sum` (`5.17:libgnucash/app-utils/gnc-autoclear.cpp:111-169`), with an explicit stack so a long
/// account cannot overflow the call stack. Candidates are sorted largest first, positive before negative on equal
/// size (`:209-216`); the sort is stable, so equal amounts keep account order. Returns positions in account order.
fn search(mut candidates: Vec<Candidate>, target: i128, limit: u64) -> Search {
    candidates.sort_by_key(|c| (Reverse(c.amount.abs()), Reverse(c.amount)));
    let (rem_pos, rem_neg) = remaining_sums(&candidates);
    let mut path: Vec<usize> = Vec::new();
    let mut solution: Option<Vec<usize>> = None;
    let mut steps = 0_u64;
    let mut stack = vec![Step::Visit { index: 0, target }];
    while let Some(step) = stack.pop() {
        let (index, target) = match step {
            Step::Pop => {
                path.pop();
                continue;
            }
            Step::Visit { index, target } => (index, target),
        };
        if target == 0 {
            if solution.is_some() {
                return Search::Ambiguous;
            }
            solution = Some(path.iter().map(|&i| candidates[i].position).collect());
            continue;
        }
        if index == candidates.len() {
            continue;
        }
        steps += 1;
        if steps > limit {
            return Search::Limit;
        }
        if target < rem_neg[index] || target > rem_pos[index] {
            continue;
        }
        // Taken first, then left: the stack runs them in reverse order of pushing.
        stack.push(Step::Visit { index: index + 1, target });
        stack.push(Step::Pop);
        stack.push(Step::Visit { index: index + 1, target: target - candidates[index].amount });
        path.push(index);
    }
    solution.map_or(Search::Unreachable, Search::Found)
}

/// For each candidate, the sum of the positive and of the negative amounts from it to the end (`:218-225`).
fn remaining_sums(candidates: &[Candidate]) -> (Vec<i128>, Vec<i128>) {
    let mut pos: Vec<i128> = candidates
        .iter()
        .rev()
        .scan(0_i128, |sum, c| {
            *sum += c.amount.max(0);
            Some(*sum)
        })
        .collect();
    let mut neg: Vec<i128> = candidates
        .iter()
        .rev()
        .scan(0_i128, |sum, c| {
            *sum += c.amount.min(0);
            Some(*sum)
        })
        .collect();
    pos.reverse();
    neg.reverse();
    (pos, neg)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::ops::commit::{NewSplit, NewTransaction, add_transaction};
    use crate::{Account, AccountType, CommodityRef, Date, FixedEnv, Time64};

    /// One split of a GnuCash test case: its amount in cents and whether it starts cleared.
    type Datum = (i64, bool);

    struct World {
        book: Book,
        env: FixedEnv,
        bank: Guid,
        splits: Vec<Guid>,
    }

    /// An account holding `data` as splits, one transaction each, on consecutive days so account order is data order.
    fn world(data: &[Datum]) -> World {
        let mut env = FixedEnv::new(Time64(1_700_000_000), 7);
        let mut book = Book::new_empty(&mut env);
        let usd = CommodityRef::currency("USD");
        let mut add = |name: &str, ty| {
            let a = Account::new(env.new_guid(), name, ty, Some(usd.clone()), Some(book.root));
            let g = a.guid;
            book.accounts.insert(g, Arc::new(a));
            g
        };
        let bank = add("Bank", AccountType::Bank);
        let other = add("Other", AccountType::Expense);
        let splits = data
            .iter()
            .zip(1..)
            .map(|(&(cents, cleared), day)| {
                let mut new = NewTransaction::new(usd.clone(), Date::from_ymd(2024, 1, day).unwrap());
                let mut s = NewSplit::new(bank, Numeric::new(cents, 100));
                if cleared {
                    s.reconcile_state = ReconcileState::Cleared;
                }
                new.splits = vec![s, NewSplit::new(other, Numeric::new(-cents, 100))];
                let t = add_transaction(&mut book, &mut env, new).unwrap();
                book.transaction(t).unwrap().splits.iter().find(|s| s.account == Some(bank)).unwrap().guid
            })
            .collect();
        World { book, env, bank, splits }
    }

    fn outcome(w: &World, cents: i64) -> Outcome {
        find(&w.book, w.bank, Numeric::new(cents, 100)).unwrap().outcome
    }

    fn picks(w: &World, positions: &[usize]) -> Outcome {
        Outcome::Found(positions.iter().map(|&i| w.splits[i]).collect())
    }

    /// GnuCash's own cases (`5.17:libgnucash/app-utils/test/test-autoclear.cpp:44-219`), checked by outcome.
    fn check(data: &[Datum], cases: &[(i64, Option<Outcome>)]) {
        let w = world(data);
        cases.iter().for_each(|(cents, expected)| {
            let got = outcome(&w, *cents);
            match expected {
                Some(e) => assert_eq!(&got, e, "target {cents}"),
                None => assert!(matches!(got, Outcome::Found(_)), "target {cents}: {got:?}"),
            }
        });
    }

    #[test]
    fn easy_case() {
        let cleared = [-8234, -156_326, -4500, -694_056, -7358, -11700, -20497, -11900, -8275, -58700, 100_000, -13881, -5000, 200_000];
        let more_cleared = [-16800, -152_000];
        let open = [160_000, -63610, -2702, -15400, -3900, -22042, -2900, -10900, -44400, -9200, -7900, -1990, -7901, -61200];
        let data: Vec<Datum> =
            cleared.iter().chain(&more_cleared).map(|&a| (a, true)).chain(open.iter().map(|&a| (a, false))).collect();
        check(
            &data,
            &[
                (0, Some(Outcome::Unreachable)),
                (-869_227, Some(Outcome::AlreadyCleared)),
                (-869_300, Some(Outcome::Unreachable)),
                (-869_230, None),
                (-963_272, None),
            ],
        );
        let w = world(&data);
        assert_eq!(outcome(&w, -963_272), picks(&w, &(16..30).collect::<Vec<_>>()), "every open split");
    }

    #[test]
    fn ambiguous_case() {
        let data = [(-10, false), (-10, false), (-10, false)];
        check(&data, &[(-10, Some(Outcome::Ambiguous)), (-20, Some(Outcome::Ambiguous)), (-30, None)]);
    }

    #[test]
    fn sequential_cases() {
        let three = [(2, false), (3, false), (5, false)];
        let w = world(&three);
        [1, 4, 6, 9, 11, 12].iter().for_each(|&t| assert_eq!(outcome(&w, t), Outcome::Unreachable, "{t}"));
        assert_eq!(outcome(&w, 5), Outcome::Ambiguous);
        assert_eq!(outcome(&w, 2), picks(&w, &[0]));
        assert_eq!(outcome(&w, 3), picks(&w, &[1]));
        assert_eq!(outcome(&w, 7), picks(&w, &[0, 2]));
        assert_eq!(outcome(&w, 8), picks(&w, &[1, 2]));
        assert_eq!(outcome(&w, 10), picks(&w, &[0, 1, 2]));
        let four = [(2, false), (3, false), (5, false), (5, false)];
        let w = world(&four);
        [5, 7, 10].iter().for_each(|&t| assert_eq!(outcome(&w, t), Outcome::Ambiguous, "{t}"));
        assert_eq!(outcome(&w, 2), picks(&w, &[0]));
        assert_eq!(outcome(&w, 3), picks(&w, &[1]));
        assert_eq!(outcome(&w, 12), picks(&w, &[0, 2, 3]));
        assert_eq!(outcome(&w, 13), picks(&w, &[1, 2, 3]));
        assert_eq!(outcome(&w, 15), picks(&w, &[0, 1, 2, 3]));
    }

    #[test]
    fn slow_case_finishes_within_the_limit() {
        let data: Vec<Datum> = (0..21).map(|i| (1 << i, false)).collect();
        check(
            &data,
            &[
                (6_000_000, Some(Outcome::Unreachable)),
                (5_000_000, Some(Outcome::Unreachable)),
                (4_000_000, Some(Outcome::Unreachable)),
                (3_000_000, Some(Outcome::Unreachable)),
                (2_000_000, None),
            ],
        );
        let w = world(&data);
        assert_eq!(find_with_limit(&w.book, w.bank, Numeric::new(2_000_000, 100), 10).unwrap().outcome, Outcome::Limit);
    }

    #[test]
    fn mixed_signs_and_zeros() {
        check(&[(30000, false), (-20000, false), (10000, false), (500, false), (-400, false), (300, false)], &[(30300, None)]);
        check(
            &[(10000, false), (0, false), (10000, false), (5000, false)],
            &[(19999, Some(Outcome::Unreachable)), (20001, Some(Outcome::Unreachable)), (20000, None)],
        );
    }

    #[test]
    fn a_solution_is_not_extended_past_its_target() {
        // Sorted 10, 5, -5: taking 10 reaches the target, so {10, 5, -5} is never tried and 10 alone is the answer.
        let w = world(&[(5, false), (-5, false), (10, false)]);
        assert_eq!(outcome(&w, 10), picks(&w, &[2]));
    }

    #[test]
    fn apply_clears_the_splits_found_and_refuses_otherwise() {
        let mut w = world(&[(2, false), (3, false), (5, false)]);
        let r = apply(&mut w.book, &mut w.env, w.bank, Numeric::new(7, 100)).unwrap();
        assert_eq!((r.current, r.target), (Numeric::new(0, 100), Numeric::new(7, 100)));
        let state = |b: &Book, s: Guid| b.transaction_of_split(s).and_then(|t| t.split(s)).map(|s| s.reconcile_state);
        assert_eq!(state(&w.book, w.splits[0]), Some(ReconcileState::Cleared));
        assert_eq!(state(&w.book, w.splits[1]), Some(ReconcileState::New));
        assert_eq!(state(&w.book, w.splits[2]), Some(ReconcileState::Cleared));
        assert_eq!(find(&w.book, w.bank, Numeric::new(7, 100)).unwrap().outcome, Outcome::AlreadyCleared);
        let err = apply(&mut w.book, &mut w.env, w.bank, Numeric::new(1, 100)).unwrap_err();
        assert_eq!(err, DomainError::invalid("The selected amount cannot be cleared."));
    }

    #[test]
    fn the_target_is_rounded_to_the_scu_with_bankers_rounding() {
        let w = world(&[(2, false)]);
        assert_eq!(find(&w.book, w.bank, Numeric::new(25, 1000)).unwrap().target, Numeric::new(2, 100));
        assert_eq!(find(&w.book, w.bank, Numeric::new(35, 1000)).unwrap().target, Numeric::new(4, 100));
    }
}
