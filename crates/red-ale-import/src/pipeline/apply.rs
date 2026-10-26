//! Applying a proposal with the caller's changes, as GnuCash's matcher window applies its rows on OK
//! (`on_matcher_ok_clicked`, `5.17:gnucash/import-export/import-main-matcher.cpp:534-604`), but all at once: the
//! caller's [`Change`]s are the clicks GnuCash's window takes, and the whole proposal goes into one Mutation the
//! service holds, so on any error the Book is dropped and nothing is half applied.

use red_ale_core::ops::scrub::Repair;
use red_ale_core::ops::transactions::transfer::PriceChange;
use red_ale_core::{Book, CommodityRef, DomainError, DomainResult, Env, Guid, Numeric};

use super::review::process_warning;
use super::{AccountName, NameSource, Proposal, Warning};
use crate::import_map::{CATEGORY_CSV_ACCOUNT_MAP, add_account, delete_account, find_account};
use crate::matcher::{ApplyOptions, ImportAction, Learned, Processed, TransInfo, append_text, apply_batch};
use crate::qif::guess_map::SavedMap;

/// The caller's change to one transaction of a proposal (`ImportChange`, `design/api/import.md`, "`import.apply`").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Change {
    /// The transaction's index in the proposal.
    pub index: usize,
    /// A new Import action. `clear` and `update` need a match.
    pub action: Option<ImportAction>,
    /// The split of one of the transaction's candidates: GnuCash's match picker.
    pub match_split: Option<Guid>,
    /// The balancing split's account on *add*: GnuCash's "select transfer account". Learned.
    pub account: Option<Guid>,
    /// The balancing split's amount, when its account is in another commodity: GnuCash's exchange rate dialog.
    pub amount: Option<Numeric>,
}

/// The choices for the whole import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ApplyChoices {
    /// Apply although the proposal has error rows, leaving them out: GnuCash's "Skip errors"
    /// (`5.17:gnucash/import-export/csv-imp/gnc-import-tx.cpp:298`).
    pub skip_errors: bool,
    /// For *update*, append the imported text instead of replacing it. `None` takes the import account's
    /// `import-append-text` slot, as GnuCash's checkbox defaults (`import-main-matcher.cpp:504-516`).
    pub append_text: Option<bool>,
}

/// An `Imbalance-<CUR>` split applying made.
#[derive(Debug, Clone, PartialEq)]
pub struct ImbalanceSplit {
    /// The imported transaction's index.
    pub index: usize,
    /// The transaction balanced.
    pub transaction: Guid,
    /// The Imbalance split.
    pub split: Guid,
    /// Its account.
    pub account: Guid,
    /// Its value.
    pub value: Numeric,
}

/// The Import map entries applying wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LearnedEntries {
    /// Bayesian: in `source`'s map, each token counted once more for `target`.
    pub bayes: Vec<(Guid, Guid, Vec<String>)>,
    /// Plain: in `source`'s map, `category`/`key` now sends to `target`.
    pub plain: Vec<(Guid, Guid, &'static str, String)>,
    /// `csv-account-map`: the name now maps to the account, which holds the entry.
    pub csv_account_map: Vec<(String, Guid)>,
}

/// What applying did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    /// Transactions added, by index.
    pub added: Vec<(usize, Guid)>,
    /// Existing transactions cleared, by index.
    pub cleared: Vec<(usize, Guid)>,
    /// Existing transactions updated and cleared, by index.
    pub updated: Vec<(usize, Guid)>,
    /// Indexes skipped.
    pub skipped: Vec<usize>,
    /// Accounts created (Imbalance accounts).
    pub accounts_created: Vec<Guid>,
    /// Accounts whose Slots changed: Import maps learned, the append choice saved.
    pub accounts_updated: Vec<Guid>,
    /// Prices recorded from added transactions (`user:split-import`): added, replaced, kept as duplicates.
    pub prices: (Vec<Guid>, Vec<Guid>, Vec<Guid>),
    /// Every Imbalance split made.
    pub imbalance_splits: Vec<ImbalanceSplit>,
    /// What was learned.
    pub learned: LearnedEntries,
    /// Commodities created (QIF securities).
    pub commodities_created: Vec<CommodityRef>,
    /// The QIF account map the import used, for the service to merge over the Configuration's
    /// (`qif-import:save-map-prefs`).
    pub qif_account_map: Option<SavedMap>,
    /// Every Imbalance split again, every transaction applied read-only by date, and the matcher's warnings.
    pub warnings: Vec<Warning>,
}

/// Apply `proposal` to `book` with the caller's changes. The caller holds `book` inside one Mutation.
pub fn apply(book: &mut Book, env: &mut dyn Env, proposal: &Proposal, changes: &[Change], choices: ApplyChoices) -> DomainResult<Applied> {
    if !proposal.errors.is_empty() && !choices.skip_errors {
        let n = proposal.errors.len();
        return Err(DomainError::invalid(format!("the proposal has {n} row(s) that can't be imported; apply with skipErrors to leave them out")));
    }
    if let Some(qif) = &proposal.qif {
        return crate::qif::import::apply(book, env, proposal, qif, changes);
    }
    let mut infos: Vec<TransInfo> = proposal.transactions.iter().map(|t| t.info.clone()).collect();
    changes.iter().try_for_each(|c| change(book, proposal, &mut infos, c))?;
    infos.iter().try_for_each(check_action)?;
    let before = book.clone();
    let append = choices.append_text.unwrap_or_else(|| infos.last().and_then(TransInfo::account).is_some_and(|a| append_text(book, a)));
    let processed = apply_batch(book, env, &infos, ApplyOptions { use_bayes: proposal.settings.use_bayes, append_text: append })?;
    let csv_learned = proposal.csv.as_ref().map_or(Ok(Vec::new()), |csv| learn_account_names(book, &csv.account_names))?;
    let mut applied = summarize(&processed);
    applied.learned.csv_account_map = csv_learned;
    applied.warnings.extend(read_only_warnings(&before, proposal, &infos));
    (applied.accounts_created, applied.accounts_updated) = changed_accounts(&before, book);
    Ok(applied)
}

/// One change, in GnuCash's order of clicks: the match, the destination and its amount, then the action.
fn change(book: &Book, proposal: &Proposal, infos: &mut [TransInfo], c: &Change) -> DomainResult<()> {
    let info = infos.iter_mut().find(|i| i.index == c.index).ok_or_else(|| DomainError::invalid(format!("no transaction {} in the proposal", c.index)))?;
    if let Some(split) = c.match_split
        && !info.select_match(split, true)
    {
        return Err(DomainError::invalid(format!("split {split} is not one of transaction {}'s matches", c.index)));
    }
    if let Some(account) = c.account {
        let acc = book.account(account).ok_or_else(|| DomainError::not_found("account", &account))?;
        if acc.is_placeholder() {
            return Err(DomainError::invalid(format!("{} is a placeholder account", acc.name)));
        }
        info.set_destination(book, Some(account), true);
    }
    if let Some(amount) = c.amount {
        info.set_dest_amount(book, amount);
    }
    if let Some(action) = c.action {
        if action == ImportAction::Update && !proposal.settings.enable_update {
            return Err(DomainError::invalid("update is turned off in the Import settings (enableUpdate)"));
        }
        info.set_action(action);
    }
    Ok(())
}

/// `clear` and `update` need a selected match.
fn check_action(info: &TransInfo) -> DomainResult<()> {
    match info.action {
        ImportAction::Clear | ImportAction::Update if info.selected().is_none() => {
            Err(DomainError::invalid(format!("transaction {} has no match to {}", info.index, info.action.as_str())))
        }
        _ => Ok(()),
    }
}

/// The results of [`apply_batch`], in arrival order.
fn summarize(processed: &[Processed]) -> Applied {
    let mut out = Applied::default();
    processed.iter().for_each(|p| {
        let done = p.transaction.map(|t| (p.index, t));
        match (p.action, done) {
            (ImportAction::Add, Some(d)) => out.added.push(d),
            (ImportAction::Clear, Some(d)) => out.cleared.push(d),
            (ImportAction::Update, Some(d)) => out.updated.push(d),
            _ => out.skipped.push(p.index),
        }
        out.imbalance_splits.extend(p.repairs.iter().filter_map(|r| imbalance_split(p.index, r)));
        p.prices.iter().for_each(|c| match c {
            PriceChange::Created(g) => out.prices.0.push(*g),
            PriceChange::Updated(g) => out.prices.1.push(*g),
            PriceChange::Kept(g) => out.prices.2.push(*g),
        });
        p.learned.iter().for_each(|l| match l {
            Learned::Bayes { source, target, tokens } => out.learned.bayes.push((*source, *target, tokens.clone())),
            Learned::Plain { source, target, category, key } => out.learned.plain.push((*source, *target, category, key.clone())),
        });
        out.warnings.extend(p.warnings.iter().map(|w| process_warning(p.index, w)));
    });
    let imbalance_warnings: Vec<Warning> = out
        .imbalance_splits
        .iter()
        .map(|s| Warning::about(s.index, "imbalance", format!("transaction {} was balanced with split {} of {}", s.transaction, s.split, s.value.to_decimal_string())))
        .collect();
    out.warnings.extend(imbalance_warnings);
    out
}

/// The Imbalance split a repair made.
pub(crate) fn imbalance_split(index: usize, r: &Repair) -> Option<ImbalanceSplit> {
    match r {
        Repair::BalanceSplitAdded { transaction, split, account, value } => {
            Some(ImbalanceSplit { index, transaction: *transaction, split: *split, account: *account, value: *value })
        }
        Repair::BalanceSplitChanged { transaction, split, account, to, .. } => {
            Some(ImbalanceSplit { index, transaction: *transaction, split: *split, account: *account, value: *to })
        }
        _ => None,
    }
}

fn read_only_warnings(before: &Book, proposal: &Proposal, infos: &[TransInfo]) -> Vec<Warning> {
    let Some(threshold) = proposal.threshold else { return Vec::new() };
    infos
        .iter()
        .filter(|i| super::review::read_only_by_date(before, Some(threshold), i))
        .map(|i| Warning::about(i.index, "read-only-by-date", format!("applied although dated before the read-only threshold {threshold}")))
        .collect()
}

/// Accounts created and accounts changed, by comparing the Book before and after, in book order.
pub(crate) fn changed_accounts(before: &Book, after: &Book) -> (Vec<Guid>, Vec<Guid>) {
    let created = after.accounts.keys().filter(|g| !before.accounts.contains_key(*g)).copied().collect();
    let updated = after.accounts.iter().filter(|(g, a)| before.accounts.get(*g).is_some_and(|b| b != *a)).map(|(g, _)| *g).collect();
    (created, updated)
}

/// Store each name the caller mapped in `csv-account-map`, as GnuCash's account match page does: the entry is
/// deleted from the account it resolved to before, then written on the chosen account, pointing at itself
/// (`5.17:gnucash/import-export/csv-imp/assistant-csv-trans-import.cpp:1914-1920`).
fn learn_account_names(book: &mut Book, names: &[AccountName]) -> DomainResult<Vec<(String, Guid)>> {
    names
        .iter()
        .filter(|n| n.source == Some(NameSource::Param))
        .filter_map(|n| n.account.map(|a| (n, a)))
        .map(|(n, account)| {
            if let Some(previous) = n.previous.filter(|p| find_account(book, *p, Some(CATEGORY_CSV_ACCOUNT_MAP), &n.name).is_some()) {
                delete_account(book, previous, Some(CATEGORY_CSV_ACCOUNT_MAP), &n.name)?;
            }
            add_account(book, account, Some(CATEGORY_CSV_ACCOUNT_MAP), &n.name, account)?;
            Ok((n.name.clone(), account))
        })
        .collect()
}
