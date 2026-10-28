//! The checks GnuCash's editor makes before it saves a scheduled transaction (`gnc_sxed_check_consistent`,
//! `5.17:gnucash/gnome/dialog-sx-editor.c:796-904`), on the scheduled transaction as it would be saved: what the
//! editor refuses is refused, and what it asks about is a [`Warning`]. Where the editor can't decide (a balance that
//! depends on variables), red-ale accepts, and Since Last Run balances as GnuCash does.

use std::collections::BTreeSet;

use super::edit::Warning;
use super::error::{AccountProblem, ScheduledError, Side};
use super::template::{INSTANCE_COUNT_VARIABLE, formulas, site, split_account, template_transactions, variables};
use crate::formula::{Financial, FormulaError, NumberFormat, Variables, check_calls, evaluate, free_variables};
use crate::recurrence::next_instance_of_list;
use crate::{Book, CommodityRef, DomainError, Numeric, ScheduledTransaction, Split, TemplateSplit, Transaction};

/// Refuse what the editor refuses: a template split whose account can't take the created split, a formula that
/// doesn't parse or can't be computed, a template that provably doesn't balance, and auto-create where the editor
/// disallows it.
pub(crate) fn check(book: &Book, sx: &ScheduledTransaction, format: NumberFormat) -> Result<(), ScheduledError> {
    let templates = template_transactions(book, sx);
    templates.iter().try_for_each(|t| {
        t.splits.iter().try_for_each(|s| check_split(book, sx, t, s, format))?;
        check_balance(book, sx, t, format)
    })?;
    check_auto_create(book, sx, &templates, format)
}

/// The account must exist, hold a commodity, and be neither the root nor a placeholder: red-ale's commit rules would
/// refuse the created split there ("Split with memo %s has an invalid account.", `dialog-sx-editor.c:742-754`). Each
/// formula must parse ("… has an unparsable Credit Formula.", `:756-790`) and call only functions red-ale computes;
/// one naming no variable must evaluate.
fn check_split(book: &Book, sx: &ScheduledTransaction, t: &Transaction, split: &Split, format: NumberFormat) -> Result<(), ScheduledError> {
    let named = TemplateSplit::of(split).and_then(|t| t.account()).and_then(|g| book.account(g));
    let problem = named.and_then(|account| {
        if account.guid == book.root {
            Some(AccountProblem::Root(account.guid))
        } else if account.is_placeholder() {
            Some(AccountProblem::Placeholder(account.guid))
        } else {
            None
        }
    });
    if let Some(problem) = problem {
        return Err(ScheduledError::Account {
            site: site(sx, t, split),
            problem,
            index: None,
        });
    }
    split_account(book, sx, t, split)?;
    formulas(split)
        .into_iter()
        .try_for_each(|(side, text)| check_formula(text, format).map_err(|e| ScheduledError::formula(site(sx, t, split), side, text, e)))
}

fn check_formula(text: &str, format: NumberFormat) -> Result<(), FormulaError> {
    let names = free_variables(text, format)?;
    check_calls(text, format, &Financial)?;
    if names.is_empty() {
        evaluate(text, format, &Variables::new())?;
    }
    Ok(())
}

/// A template transaction whose formulas name no variable and whose splits' accounts hold one commodity must balance
/// exactly. The editor sums debits and credits with the variables at random values and asks "Should it still be
/// entered?" when they differ (`dialog-sx-editor.c:803-904`, `check_credit_debit_balance` at `:464-503`); with no
/// variable the sums are certain, so red-ale refuses.
fn check_balance(book: &Book, sx: &ScheduledTransaction, t: &Transaction, format: NumberFormat) -> Result<(), ScheduledError> {
    if commodities(book, sx, [t]).len() != 1 {
        return Ok(());
    }
    let entries: Vec<(&Split, Side, &str)> = t
        .splits
        .iter()
        .flat_map(|s| formulas(s).into_iter().map(move |(side, text)| (s, side, text)))
        .collect();
    if entries
        .iter()
        .any(|(_, _, text)| !free_variables(text, format).is_ok_and(|v| v.is_empty()))
    {
        return Ok(());
    }
    let total = entries.iter().try_fold(Numeric::ZERO, |total, (split, side, text)| {
        let value = evaluate(text, format, &Variables::new()).map_err(|e| ScheduledError::formula(site(sx, t, split), *side, text, e))?;
        let total = match side {
            Side::Debit => total.add_exact(value),
            Side::Credit => total.sub_exact(value),
        }?;
        Ok::<_, ScheduledError>(total)
    })?;
    if total.is_zero() {
        Ok(())
    } else {
        Err(DomainError::Unbalanced {
            imbalance: total.reduce().to_string(),
            currency: t.currency.mnemonic.clone(),
        }
        .into())
    }
}

/// The commodities of the accounts the templates' splits name, those it can find.
fn commodities<'a>(
    book: &'a Book,
    sx: &ScheduledTransaction,
    templates: impl IntoIterator<Item = &'a Transaction>,
) -> BTreeSet<&'a CommodityRef> {
    templates
        .into_iter()
        .flat_map(|t| {
            t.splits
                .iter()
                .filter_map(move |s| split_account(book, sx, t, s).ok().map(|(_, c)| c))
        })
        .collect()
}

/// Auto-create is refused with variables other than `i`, with more than one commodity, or with no template split
/// (`gnc_sxed_check_autocreate`, `dialog-sx-editor.c:617-648`).
fn check_auto_create(
    book: &Book,
    sx: &ScheduledTransaction,
    templates: &[&Transaction],
    format: NumberFormat,
) -> Result<(), ScheduledError> {
    if !sx.auto_create {
        return Ok(());
    }
    let (names, _) = variables(book, sx, format);
    if names.iter().any(|n| n != INSTANCE_COUNT_VARIABLE) || commodities(book, sx, templates.iter().copied()).len() > 1 {
        return Err(ScheduledError::invalid(
            "auto-create",
            "scheduled transactions with variables or involving more than one commodity cannot be automatically created",
        ));
    }
    if templates.iter().all(|t| t.splits.is_empty()) {
        return Err(ScheduledError::invalid(
            "auto-create",
            "scheduled transactions without a template transaction cannot be automatically created",
        ));
    }
    Ok(())
}

/// What the editor asks about: a name another scheduled transaction has, when the name is new or changed
/// (`gnc_sxed_check_names`, `dialog-sx-editor.c:517-545`); a schedule with no occurrence, or none by its end date,
/// unless it ends after a count (`gnc_sxed_check_endpoint`, `:594-611`).
pub(crate) fn warnings(book: &Book, sx: &ScheduledTransaction, name_changed: bool) -> Result<Vec<Warning>, ScheduledError> {
    let duplicate = name_changed && book.scheduled.values().any(|o| o.guid != sx.guid && o.name == sx.name);
    let never_runs = sx.occurrences.is_none()
        && next_instance_of_list(&sx.schedule, sx.start.add_days(-1))?.is_none_or(|next| sx.end.is_some_and(|end| next > end));
    Ok([
        duplicate.then(|| Warning::DuplicateName(sx.name.clone())),
        never_runs.then_some(Warning::NeverRuns),
    ]
    .into_iter()
    .flatten()
    .collect())
}
