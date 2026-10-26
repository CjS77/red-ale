//! What applying a proposal as it stands would do: the Imbalance split each transaction would get, the accounts that
//! would be created, and which transactions are read-only by date (`design/api/import.md`, "The Import proposal").
//!
//! GnuCash balances an imported transaction when it is committed, with an `Imbalance-<CUR>` split
//! (`xaccTransScrubImbalance` from `xaccTransCommitEdit`, `5.17:libgnucash/engine/Transaction.cpp:1549-1558`), and
//! checks no auto read-only threshold. red-ale does both, and says so before anything changes: the proposal is
//! applied to a copy of the Book, which is then dropped, so the flags come from the very code `import.apply` runs.

use std::collections::BTreeSet;

use red_ale_core::book_options::auto_readonly_threshold;
use red_ale_core::ops::scrub::Repair;
use red_ale_core::ops::transactions::posted_date;
use red_ale_core::{AccountType, Book, CommodityRef, Date, FixedEnv, Guid, Numeric, Time64, TimeZone};

use super::{ApplyChoices, Format, Proposal, ProposedTransaction, Warning};
use crate::matcher::{ApplyOptions, ImportAction, ProcessWarning, Processed, TransInfo, append_text, apply_batch};

/// The first GUID the copy's new splits and accounts count up from: far from any a Book holds, so a new Imbalance
/// account never takes the place of an existing one in the copy.
const REVIEW_GUID_SEED: u128 = 0x7265_642d_616c_6520_7265_7669_6577_0000;

/// The `Imbalance-<CUR>` split applying would make for a transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct ImbalanceInfo {
    /// The split's value.
    pub value: Numeric,
    /// The transaction's currency.
    pub currency: CommodityRef,
    /// The Imbalance account, or `None` when it would be created.
    pub account: Option<Guid>,
    /// Its full name.
    pub account_name: String,
}

/// An account applying would create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAccount {
    /// Its full name.
    pub name: String,
    /// Its type.
    pub account_type: AccountType,
    /// Its commodity.
    pub commodity: Option<CommodityRef>,
}

/// What applying would do to one transaction.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransactionReview {
    /// The Imbalance split it would get, with its current action and accounts.
    pub imbalance: Option<ImbalanceInfo>,
    /// True when the transaction added, cleared or updated is read-only by date.
    pub read_only_by_date: bool,
    /// Warnings for it.
    pub warnings: Vec<Warning>,
}

/// What applying the whole proposal would do.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Review {
    /// One per transaction, in the proposal's order.
    pub transactions: Vec<TransactionReview>,
    /// Accounts it would create, parents first.
    pub new_accounts: Vec<NewAccount>,
    /// Warnings about the whole import, such as an apply that would fail.
    pub warnings: Vec<Warning>,
}

/// Apply `proposal` as it stands to a copy of `book`, and report what that did. `now` and `zone` are the session's.
pub fn review(book: &Book, now: Time64, zone: &TimeZone, proposal: &Proposal) -> Review {
    match proposal.format {
        Format::CsvAccounts | Format::CsvPrices | Format::CsvDocuments | Format::CsvOwners => {
            return records_review(book, now, zone, proposal);
        }
        // The preview converted into a copy already; nothing in a QIF proposal changes before it is applied.
        Format::Qif => return proposal.qif.as_ref().map(|q| q.review.clone()).unwrap_or_default(),
        Format::CsvTransactions => {}
    }
    let infos: Vec<TransInfo> = proposal.transactions.iter().map(|t| t.info.clone()).collect();
    let mut copy = book.clone();
    let mut env = FixedEnv::new(now, REVIEW_GUID_SEED).with_zone(zone.clone());
    let append = infos.last().and_then(TransInfo::account).is_some_and(|a| append_text(book, a));
    let opts = ApplyOptions { use_bayes: proposal.settings.use_bayes, append_text: append };
    let flags = |t: &ProposedTransaction| read_only_by_date(book, proposal.threshold, &t.info);
    match apply_batch(&mut copy, &mut env, &infos, opts) {
        Ok(processed) => Review {
            transactions: proposal.transactions.iter().zip(&processed).map(|(t, p)| transaction_review(book, &copy, proposal, t, p)).collect(),
            new_accounts: new_accounts(&copy, &processed),
            warnings: Vec::new(),
        },
        Err(e) => Review {
            transactions: proposal.transactions.iter().map(|t| TransactionReview { read_only_by_date: flags(t), ..TransactionReview::default() }).collect(),
            new_accounts: Vec::new(),
            warnings: vec![Warning { code: "cannot-apply", message: format!("applying this proposal as it stands would fail: {e}"), index: None }],
        },
    }
}

/// The review of a format that imports records (accounts, prices) rather than transactions: the accounts applying
/// creates, or why applying would fail.
fn records_review(book: &Book, now: Time64, zone: &TimeZone, proposal: &Proposal) -> Review {
    let mut copy = book.clone();
    let mut env = FixedEnv::new(now, REVIEW_GUID_SEED).with_zone(zone.clone());
    match super::apply(&mut copy, &mut env, proposal, &[], ApplyChoices { skip_errors: true, append_text: None }) {
        Ok(applied) => Review {
            new_accounts: applied
                .accounts_created
                .iter()
                .filter_map(|g| copy.account(*g))
                .map(|a| NewAccount { name: copy.full_name(a.guid).unwrap_or_else(|| a.name.clone()), account_type: a.account_type, commodity: a.commodity.clone() })
                .collect(),
            ..Review::default()
        },
        Err(e) => Review { warnings: vec![Warning { code: "cannot-apply", message: format!("applying this proposal as it stands would fail: {e}"), index: None }], ..Review::default() },
    }
}

fn transaction_review(book: &Book, copy: &Book, proposal: &Proposal, t: &ProposedTransaction, p: &Processed) -> TransactionReview {
    let index = t.info.index;
    let imbalance = imbalance(book, copy, p);
    let read_only = read_only_by_date(book, proposal.threshold, &t.info);
    let mut warnings: Vec<Warning> = p.warnings.iter().map(|w| process_warning(index, w)).collect();
    if let Some(i) = &imbalance {
        let message = format!("applying adds a split of {} {} in {}", i.value.to_decimal_string(), i.currency.mnemonic, i.account_name);
        warnings.push(Warning::about(index, "imbalance", message));
    }
    if let Some(threshold) = proposal.threshold.filter(|_| read_only) {
        warnings.push(Warning::about(index, "read-only-by-date", format!("dated before the read-only threshold {threshold}")));
    }
    TransactionReview { imbalance, read_only_by_date: read_only, warnings }
}

/// The warning a [`ProcessWarning`] becomes.
pub fn process_warning(index: usize, w: &ProcessWarning) -> Warning {
    match w {
        ProcessWarning::MissingRate => {
            Warning::about(index, "missing-rate", "the balancing split's account is in another commodity and no amount was given: its amount is its value, a rate of 1")
        }
        ProcessWarning::NoMatch => Warning::about(index, "no-match", "no match is selected, so nothing was done"),
        ProcessWarning::OtherSplitNotUpdated => Warning::about(
            index,
            "other-split-not-updated",
            "the matched transaction's other split is cleared or reconciled, so it was left and the transaction balanced with an Imbalance split",
        ),
    }
}

/// The Imbalance split the scrub added (or changed) when the transaction was committed.
fn imbalance(book: &Book, copy: &Book, p: &Processed) -> Option<ImbalanceInfo> {
    p.repairs.iter().find_map(|r| match r {
        Repair::BalanceSplitAdded { transaction, account, value, .. } => Some((*transaction, *account, *value)),
        Repair::BalanceSplitChanged { transaction, account, to, .. } => Some((*transaction, *account, *to)),
        _ => None,
    })
    .and_then(|(transaction, account, value)| {
        Some(ImbalanceInfo {
            value,
            currency: copy.transaction(transaction)?.currency.clone(),
            account: book.account(account).map(|a| a.guid),
            account_name: copy.full_name(account)?,
        })
    })
}

/// The accounts applying created, in the order they were made (a parent before its children).
fn new_accounts(copy: &Book, processed: &[Processed]) -> Vec<NewAccount> {
    let mut seen = BTreeSet::new();
    processed
        .iter()
        .flat_map(|p| &p.repairs)
        .filter_map(|r| match r {
            Repair::AccountCreated { account, .. } if seen.insert(*account) => copy.account(*account),
            _ => None,
        })
        .map(|a| NewAccount { name: copy.full_name(a.guid).unwrap_or_else(|| a.name.clone()), account_type: a.account_type, commodity: a.commodity.clone() })
        .collect()
}

/// Whether the transaction the action adds, clears or updates is read-only by date: posted strictly before the
/// threshold. An *update* also counts when it moves the matched transaction to such a date. GnuCash's importers
/// check no threshold; red-ale flags these and applies them.
pub fn read_only_by_date(book: &Book, threshold: Option<Date>, info: &TransInfo) -> bool {
    let Some(threshold) = threshold else { return false };
    let before = |date: Option<Date>| date.is_some_and(|d| d < threshold);
    let matched = || info.selected().and_then(|m| book.transaction(m.transaction)).and_then(posted_date);
    match info.action {
        ImportAction::Skip => false,
        ImportAction::Add => before(posted_date(&info.trans)),
        ImportAction::Clear => before(matched()),
        ImportAction::Update => before(matched()) || before(posted_date(&info.trans)),
    }
}

/// The threshold date for `today`, as the proposal records it.
pub fn threshold(book: &Book, today: Option<Date>) -> Option<Date> {
    today.and_then(|today| auto_readonly_threshold(book, today))
}
