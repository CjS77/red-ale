//! Applying a QIF proposal: the conversion again, into the Book, then the assistant's finish
//! (`gnc_ui_qif_import_finish_cb`, `5.17:gnucash/import-export/qif-imp/assistant-qif-import.c:3500-3540`): the
//! transactions ticked as duplicates removed (`gnc:prune-matching-transactions`) and the maps returned for the
//! service to save (`qif-import:save-map-prefs`). The account tree needs no merge: red-ale's conversion makes each
//! account where the merge would leave it.

use red_ale_core::ops::transactions::transfer::PriceChange;
use red_ale_core::{Book, DomainError, DomainResult, Env, Guid};

use super::super::guess_map::SavedMap;
use super::super::merge_groups::prune_matching_transactions;
use super::QifState;
use super::convert::{self, Run, run_warnings};
use crate::matcher::ImportAction;
use crate::pipeline::apply::{changed_accounts, imbalance_split};
use crate::pipeline::review::read_only_by_date;
use crate::pipeline::{Applied, Change, Proposal, Warning};

/// Apply a QIF proposal with the caller's changes; the caller holds `book` inside one Mutation. Only *add* and *skip*
/// are QIF's actions: *skip* is ticking a duplicate.
pub fn apply(book: &mut Book, env: &mut dyn Env, proposal: &Proposal, state: &QifState, changes: &[Change]) -> DomainResult<Applied> {
    if !state.date_formats.is_empty() {
        return Err(DomainError::invalid(
            "the QIF proposal is waiting for a date format: preview again with dateFormat",
        ));
    }
    let mut actions: Vec<ImportAction> = proposal.transactions.iter().map(|t| t.info.action).collect();
    changes.iter().try_for_each(|c| change(proposal, &mut actions, c))?;
    let before = book.clone();
    let run = convert::run(book, env, &before, state)?;
    if run.converted.transactions.len() != actions.len() {
        return Err(DomainError::conflict(
            "converting again made other transactions than the preview; preview again",
        ));
    }
    let skipped: Vec<Guid> = run
        .converted
        .transactions
        .iter()
        .zip(&actions)
        .filter(|(_, a)| **a == ImportAction::Skip)
        .map(|(c, _)| c.transaction)
        .collect();
    prune_matching_transactions(book, &skipped)?;
    let mut applied = summarize(proposal, &run, &actions, &before);
    applied.warnings.extend(run_warnings(state, &run.converted));
    (applied.accounts_created, applied.accounts_updated) = changed_accounts(&before, book);
    applied.qif_account_map =
        Some(SavedMap::default().merged([&run.maps.accounts, &run.maps.categories, &run.maps.payees], &run.stock_map));
    Ok(applied)
}

/// One change: an action, *add* or *skip*, and the duplicate ticked.
fn change(proposal: &Proposal, actions: &mut [ImportAction], c: &Change) -> DomainResult<()> {
    let t = proposal
        .transaction(c.index)
        .ok_or_else(|| DomainError::invalid(format!("no transaction {} in the proposal", c.index)))?;
    if c.account.is_some() || c.amount.is_some() {
        return Err(DomainError::invalid(format!(
            "transaction {}: a QIF transaction has all its splits; only its action changes",
            c.index
        )));
    }
    if let Some(split) = c.match_split
        && !t.info.matches.iter().any(|m| m.split == split)
    {
        return Err(DomainError::invalid(format!(
            "split {split} is not one of transaction {}'s matches",
            c.index
        )));
    }
    match c.action {
        Some(a @ (ImportAction::Add | ImportAction::Skip)) => {
            if let Some(slot) = actions.get_mut(c.index) {
                *slot = a;
            }
            Ok(())
        }
        Some(other) => Err(DomainError::invalid(format!(
            "a QIF transaction is added or skipped, not {}",
            other.as_str()
        ))),
        None => Ok(()),
    }
}

/// What the run did, by index.
fn summarize(proposal: &Proposal, run: &Run, actions: &[ImportAction], before: &Book) -> Applied {
    let mut out = Applied {
        commodities_created: run.created.iter().map(|c| c.id.clone()).collect(),
        ..Applied::default()
    };
    run.converted
        .transactions
        .iter()
        .zip(actions)
        .enumerate()
        .for_each(|(index, (c, action))| {
            c.prices.iter().for_each(|p| match p {
                PriceChange::Created(g) => out.prices.0.push(*g),
                PriceChange::Updated(g) => out.prices.1.push(*g),
                PriceChange::Kept(g) => out.prices.2.push(*g),
            });
            if *action == ImportAction::Skip {
                out.skipped.push(index);
                return;
            }
            out.added.push((index, c.transaction));
            out.imbalance_splits
                .extend(c.repairs.iter().filter_map(|r| imbalance_split(index, r)));
        });
    out.prices.0.extend(&run.converted.prices);
    let imbalances = out.imbalance_splits.iter().map(|s| {
        let message = format!(
            "transaction {} was balanced with split {} of {}",
            s.transaction,
            s.split,
            s.value.to_decimal_string()
        );
        Warning::about(s.index, "imbalance", message)
    });
    let read_only: Vec<Warning> = proposal
        .threshold
        .map(|threshold| {
            proposal
                .transactions
                .iter()
                .zip(actions)
                .filter(|(t, a)| **a == ImportAction::Add && read_only_by_date(before, Some(threshold), &t.info))
                .map(|(t, _)| {
                    Warning::about(
                        t.info.index,
                        "read-only-by-date",
                        format!("applied although dated before the read-only threshold {threshold}"),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    out.warnings = imbalances.chain(read_only).collect();
    out
}
