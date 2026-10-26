//! Converting the files into a Book and finding their duplicates: the assistant's conversion page
//! (`gnc_ui_qif_import_convert_progress_start_cb`, `5.17:gnucash/import-export/qif-imp/assistant-qif-import.c:3120-3300`)
//! with its commodity update (`gnc_ui_qif_import_commodity_update`, `:1064-1110`).
//!
//! The preview runs it on a copy of the Book with GUIDs counted from a seed, and reads off what applying makes: the
//! transactions, their Imbalance splits and duplicates, the new accounts and commodities.

use std::collections::{BTreeMap, BTreeSet};

use red_ale_core::ops::commodities::{NewSecurity, create_security, lookup};
use red_ale_core::ops::scrub::Repair;
use red_ale_core::{Book, CommodityRef, Date, DomainError, DomainResult, Env, FixedEnv, Guid, Numeric, Time64, TimeZone};

use super::super::dialog_utils::{SECURITY_FRACTION, SecurityChoice};
use super::super::merge_groups::{Candidate, find_duplicates};
use super::super::to_gnc::{ConvertedXtn, Converted, Maps, Options, qif_to_gnc};
use super::{NewCommodity, QifError, QifState};
use crate::matcher::{ImportAction, MatchInfo, TransInfo};
use crate::pipeline::review::read_only_by_date;
use crate::pipeline::{ImbalanceInfo, NewAccount, ProposedTransaction, Review, Source, TransactionReview, Warning};

/// The first GUID the preview's copy counts up from: far from any a Book holds.
const PREVIEW_GUID_SEED: u128 = 0x7265_642d_616c_6520_7169_6600_0000_0000;

/// One conversion.
#[derive(Debug)]
pub(super) struct Run {
    /// The commodities created.
    pub created: Vec<NewCommodity>,
    /// QIF security name → commodity, after the commodity update.
    pub stock_map: BTreeMap<String, CommodityRef>,
    /// The maps as converting left them.
    pub maps: Maps,
    /// What converting did.
    pub converted: Converted,
    /// The existing splits each converted transaction may repeat.
    pub duplicates: BTreeMap<Guid, Vec<Candidate>>,
}

/// Create the new securities, convert, and find the duplicates of the converted transactions in `old`, the Book
/// before.
pub(super) fn run(book: &mut Book, env: &mut dyn Env, old: &Book, state: &QifState) -> DomainResult<Run> {
    let (stock_map, created) = commodity_update(book, &state.security_hash)?;
    let currency = state.currency.clone().ok_or_else(|| DomainError::invalid("the QIF proposal has no currency"))?;
    let mut files = state.files.clone();
    let mut maps = state.maps.clone();
    let opts = Options { stock_map: &stock_map, default_currency: &currency, status_pref: state.status_pref };
    let converted = qif_to_gnc(book, env, &mut files, &mut maps, opts)?;
    let added: Vec<Guid> = converted.transactions.iter().map(|c| c.transaction).collect();
    let left_out: BTreeSet<Guid> = converted.transactions.iter().flat_map(|c| &c.repairs).filter_map(made_split).collect();
    let duplicates = find_duplicates(old, book, &added, &left_out);
    Ok(Run { created, stock_map, maps, converted, duplicates })
}

/// A split committing made, which is no part of the import's account tree.
fn made_split(r: &Repair) -> Option<Guid> {
    match r {
        Repair::BalanceSplitAdded { split, .. } | Repair::OrphanAssigned { split, .. } => Some(*split),
        _ => None,
    }
}

/// `gnc_ui_qif_import_commodity_update`: each new security is created unless the Book has its namespace and
/// mnemonic by now, and the hash then names the Book's commodity.
fn commodity_update(book: &mut Book, hash: &BTreeMap<String, SecurityChoice>) -> DomainResult<(BTreeMap<String, CommodityRef>, Vec<NewCommodity>)> {
    let mut created = Vec::new();
    let map = hash
        .iter()
        .map(|(name, choice)| {
            if let Some(found) = lookup(book, choice.id()) {
                return Ok((name.clone(), found.id.clone()));
            }
            let SecurityChoice::New { id, fullname } = choice else {
                return Err(DomainError::not_found("commodity", choice.id()));
            };
            let new = NewSecurity {
                namespace: id.namespace.clone(),
                mnemonic: id.mnemonic.clone(),
                fullname: fullname.clone(),
                fraction: SECURITY_FRACTION,
                ..NewSecurity::default()
            };
            let id = create_security(book, &new)?;
            created.push(NewCommodity { id: id.clone(), fullname: fullname.clone(), fraction: SECURITY_FRACTION });
            Ok((name.clone(), id))
        })
        .collect::<DomainResult<_>>()?;
    Ok((map, created))
}

/// What the preview read off its copy.
#[derive(Debug)]
pub(super) struct Preview {
    pub transactions: Vec<ProposedTransaction>,
    pub new_commodities: Vec<NewCommodity>,
    pub new_account_names: BTreeMap<Guid, String>,
    pub review: Review,
}

/// Convert into a copy of `book` and report what that made.
pub(super) fn preview(book: &Book, now: Time64, zone: &TimeZone, state: &QifState, threshold: Option<Date>) -> Result<Preview, QifError> {
    let mut copy = book.clone();
    let mut env = FixedEnv::new(now, PREVIEW_GUID_SEED).with_zone(zone.clone());
    let run = run(&mut copy, &mut env, book, state)?;
    let transactions: Vec<ProposedTransaction> = run
        .converted
        .transactions
        .iter()
        .enumerate()
        .map(|(index, c)| proposed(&copy, state, &run, index, c))
        .collect::<Option<_>>()
        .ok_or_else(|| DomainError::invalid("a converted QIF transaction is missing from the copy"))?;
    let new_account_names =
        copy.accounts.keys().filter(|g| !book.accounts.contains_key(*g)).filter_map(|g| copy.full_name(*g).map(|n| (*g, n))).collect();
    let review = Review {
        transactions: transactions.iter().zip(&run.converted.transactions).map(|(p, c)| transaction_review(book, &copy, threshold, p, c)).collect(),
        new_accounts: new_accounts(book, &copy),
        warnings: run_warnings(state, &run.converted),
    };
    Ok(Preview { transactions, new_commodities: run.created, new_account_names, review })
}

/// The converted transaction as the proposal shows it: without the Imbalance split committing added (the review
/// shows that), with QIF's duplicate candidates as matches, unscored. A transaction with candidates is *skip*, its
/// first candidate selected, so a file imported twice adds nothing the second time; GnuCash's duplicates page ticks
/// nothing (`qif-merge-groups.scm:150-152`), which is a window's default, not a book's (`design/api/import.md`, QIF
/// deviations). One without is *add*.
fn proposed(copy: &Book, state: &QifState, run: &Run, index: usize, c: &ConvertedXtn) -> Option<ProposedTransaction> {
    let made: BTreeSet<Guid> = c.repairs.iter().filter_map(made_split).collect();
    let mut trans = copy.transaction(c.transaction)?.clone();
    trans.splits.retain(|s| !made.contains(&s.guid));
    let matches: Vec<MatchInfo> = run
        .duplicates
        .get(&c.transaction)
        .into_iter()
        .flatten()
        .map(|m| MatchInfo { split: m.split, transaction: m.transaction, probability: 0, update_proposed: false })
        .collect();
    let duplicate = !matches.is_empty();
    let info = TransInfo {
        index,
        trans,
        tokens: Vec::new(),
        selected_match: matches.first().map(|m| m.split),
        matches,
        match_selected_manually: false,
        action: if duplicate { ImportAction::Skip } else { ImportAction::Add },
        previous_action: ImportAction::Add,
        dest_account: None,
        dest_source: None,
        dest_value: Numeric::ZERO,
        dest_amount: Numeric::ZERO,
        dest_amount_selected_manually: false,
        price: None,
    };
    Some(ProposedTransaction { info, source: source(state, c.file, c.xtn) })
}

/// The lines of a QIF transaction.
pub(super) fn source(state: &QifState, file: usize, xtn: usize) -> Source {
    let x = state.files.get(file).and_then(|f| f.xtns.get(xtn));
    Source { file, first_line: x.map_or(0, |x| x.line), last_line: x.map_or(0, |x| x.last_line) }
}

/// The Imbalance split committing added or changed.
pub(super) fn imbalance(book: &Book, copy: &Book, repairs: &[Repair]) -> Option<ImbalanceInfo> {
    let (transaction, account, value) = repairs.iter().find_map(|r| match r {
        Repair::BalanceSplitAdded { transaction, account, value, .. } => Some((*transaction, *account, *value)),
        Repair::BalanceSplitChanged { transaction, account, to, .. } => Some((*transaction, *account, *to)),
        _ => None,
    })?;
    Some(ImbalanceInfo {
        value,
        currency: copy.transaction(transaction)?.currency.clone(),
        account: book.account(account).map(|a| a.guid),
        account_name: copy.full_name(account)?,
    })
}

fn transaction_review(book: &Book, copy: &Book, threshold: Option<Date>, p: &ProposedTransaction, c: &ConvertedXtn) -> TransactionReview {
    let index = p.info.index;
    let imbalance = imbalance(book, copy, &c.repairs);
    let read_only = read_only_by_date(book, threshold, &p.info);
    let mut warnings = Vec::new();
    if let Some(i) = &imbalance {
        let message = format!("applying adds a split of {} {} in {}", i.value.to_decimal_string(), i.currency.mnemonic, i.account_name);
        warnings.push(Warning::about(index, "imbalance", message));
    }
    if let Some(threshold) = threshold.filter(|_| read_only) {
        warnings.push(Warning::about(index, "read-only-by-date", format!("dated before the read-only threshold {threshold}")));
    }
    if !p.info.matches.is_empty() {
        let message = "may repeat an existing transaction (see matches), so it is skipped: choose add to import it anyway";
        warnings.push(Warning::about(index, "possible-duplicate", message));
    }
    TransactionReview { imbalance, read_only_by_date: read_only, warnings }
}

/// The accounts the copy has that the Book has not, in the order made.
fn new_accounts(book: &Book, copy: &Book) -> Vec<NewAccount> {
    copy.accounts
        .values()
        .filter(|a| !book.accounts.contains_key(&a.guid))
        .map(|a| NewAccount { name: copy.full_name(a.guid).unwrap_or_else(|| a.name.clone()), account_type: a.account_type, commodity: a.commodity.clone() })
        .collect()
}

/// The transactions GnuCash drops for having no split, and conversion's log.
pub(super) fn run_warnings(state: &QifState, converted: &Converted) -> Vec<Warning> {
    let dropped = converted.dropped.iter().map(|(file, xtn)| {
        let line = source(state, *file, *xtn).first_line;
        Warning { code: "no-split", message: format!("file {file}, line {line}: the transaction makes no split, so it is not imported"), index: None }
    });
    let log = converted.log.iter().map(|m| Warning { code: "qif-log", message: m.clone(), index: None });
    dropped.chain(log).collect()
}
