//! The Import proposal as `import.preview` returns it (`design/api/import.md`, "The Import proposal").

use red_ale_core::ops::transactions::posted_date;
use red_ale_core::{Book, Split, Transaction};
use red_ale_import::business::Records;
use red_ale_import::matcher::{DestSource, MatchInfo, TransInfo};
use red_ale_import::pipeline::{
    AccountName, IgnoredLine, ImbalanceInfo, NewAccount, ProposedTransaction, RowError, Source, TransactionReview, Warning,
};
use red_ale_service::import::Previewed;
use schemars::JsonSchema;
use serde::Serialize;

use super::business::{self, DocumentWire, OwnerWire};
use super::params::ActionWire;
use crate::wire::{Amount, CommodityId, Day, Id, Timestamp};

/// The Import proposal.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProposalResult {
    /// The proposal's ID, for `import.apply` and `import.discard`.
    pub id: String,
    /// The format, as given.
    pub format: String,
    /// The Book revision it was made at: the number of Mutations the session had applied.
    pub book_revision: u64,
    /// The auto read-only threshold date at the time of the preview, or null with no threshold.
    pub threshold_date: Option<Day>,
    /// Counts.
    pub summary: Summary,
    /// Lines the reader skipped.
    pub ignored_lines: Vec<IgnoredLineWire>,
    /// Rows that can't be imported, with GnuCash's message. None of their records appear below.
    pub errors: Vec<RowErrorWire>,
    /// Things the caller should know about the whole import; each transaction's own are in its `warnings`.
    pub warnings: Vec<WarningWire>,
    /// Accounts the import will create, by full name, parents first.
    pub new_accounts: Vec<NewAccountWire>,
    /// Commodities the import will create (QIF securities); none for CSV.
    pub new_commodities: Vec<NewCommodityWire>,
    /// One per transaction the file holds, in the order the importer builds them.
    pub transactions: Vec<ImportedTransactionWire>,
    /// CSV's own state: each account name the file uses and where it resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub csv: Option<CsvWire>,
    /// `csvDocuments`: the documents, by index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documents: Option<Vec<DocumentWire>>,
    /// `csvOwners`: the customers or vendors, by index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owners: Option<Vec<OwnerWire>>,
}

/// Counts of a proposal.
#[derive(Debug, Default, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Records to add.
    pub add: usize,
    /// Transactions that clear a match.
    pub clear: usize,
    /// Records that update an existing one (a transaction is then also cleared).
    pub update: usize,
    /// Records skipped.
    pub skip: usize,
    /// Error rows.
    pub errors: usize,
    /// Transactions that would get an Imbalance split.
    pub imbalanced: usize,
    /// Transactions, and documents posted, read-only by date once applied.
    pub read_only_by_date: usize,
}

/// A line the reader skipped.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IgnoredLineWire {
    /// The file's index in `files`.
    pub file: usize,
    /// The line, from 1.
    pub line: usize,
    /// Why: the layout setting that skips it (`skipStartLines`, `skipEndLines`, `skipAltLines`).
    pub reason: String,
}

/// A row that can't be imported.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RowErrorWire {
    /// The file's index in `files`.
    pub file: usize,
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1, when the error is a column's.
    pub column: Option<usize>,
    /// GnuCash's message.
    pub message: String,
}

/// A warning.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WarningWire {
    /// `imbalance`, `read-only-by-date`, `missing-rate`, `other-split-not-updated`, `online-id`, `cannot-apply`, …
    pub code: String,
    /// What happened.
    pub message: String,
    /// The transaction's index, when it is about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

/// An account the import will create.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NewAccountWire {
    /// Its full name.
    pub name: String,
    /// Its type, as GnuCash names it (`BANK`).
    #[serde(rename = "type")]
    pub account_type: String,
    /// Its commodity.
    pub commodity: Option<CommodityId>,
}

/// A commodity the import will create.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NewCommodityWire {
    /// Its ID.
    pub id: CommodityId,
    /// Its full name.
    pub fullname: String,
    /// Its fraction.
    pub fraction: i64,
}

/// Where a transaction came from.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SourceWire {
    /// The file's index in `files`.
    pub file: usize,
    /// The first and last line.
    pub lines: [usize; 2],
}

/// One split of an imported transaction.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportedSplitWire {
    /// Its account.
    pub account: Option<Id>,
    /// The account's full name.
    pub account_name: Option<String>,
    /// Value, in the transaction currency.
    pub value: Amount,
    /// Amount, in the account's commodity.
    pub amount: Amount,
    /// Memo.
    pub memo: Option<String>,
    /// Action.
    pub action: Option<String>,
    /// Reconcile state: n, c, y, f or v.
    pub reconcile_state: String,
    /// When it was reconciled.
    pub reconcile_date: Option<Timestamp>,
}

/// The balancing split GnuCash would add on *add*.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OtherWire {
    /// Its account, or null when none was found.
    pub account: Option<Id>,
    /// The account's full name.
    pub account_name: Option<String>,
    /// Where the account came from: `file`, `bayes`, `map`, `caller`, or null.
    pub from: Option<String>,
    /// Its value: minus the transaction's imbalance.
    pub value: Amount,
    /// Its amount; zero when unknown (applying then takes the value, a rate of 1).
    pub amount: Amount,
}

/// A candidate match.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchWire {
    /// The existing split.
    pub split: Id,
    /// Its transaction.
    pub transaction: Id,
    /// The Match score.
    pub score: Option<i32>,
    /// True when *update* is proposed: the amount or date was not an exact match.
    pub update_proposed: bool,
    /// The existing transaction's posted date.
    pub date_posted: Option<Day>,
    /// Its description.
    pub description: Option<String>,
    /// Its number.
    pub num: Option<String>,
    /// The split's memo.
    pub memo: Option<String>,
    /// The split's amount.
    pub amount: Amount,
    /// The split's value.
    pub value: Amount,
    /// The split's reconcile state.
    pub reconcile_state: String,
}

/// The Imbalance split applying would make.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImbalanceWire {
    /// Its value: what the transaction is out by, negated.
    pub value: Amount,
    /// The transaction currency.
    pub currency: CommodityId,
    /// The `Imbalance-<CUR>` account, or null when it will be created (it is then in `newAccounts`).
    pub account: Option<Id>,
    /// Its full name.
    pub account_name: String,
}

/// One imported transaction.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportedTransactionWire {
    /// Its position: the key `import.apply` changes it by.
    pub index: usize,
    /// Where it came from.
    pub source: SourceWire,
    /// The transaction currency.
    pub currency: CommodityId,
    /// The posted date.
    pub date_posted: Option<Day>,
    /// Number.
    pub num: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// The void reason, when the file voids it.
    pub void_reason: Option<String>,
    /// The splits; split 0 is the imported split, which the matcher matches on.
    pub splits: Vec<ImportedSplitWire>,
    /// The balancing split *add* makes; null when the transaction already balances.
    pub other: Option<OtherWire>,
    /// The Import action.
    pub action: ActionWire,
    /// Candidate matches, best first.
    pub matches: Vec<MatchWire>,
    /// The split of the match `clear` or `update` would use.
    pub selected_match: Option<Id>,
    /// The Imbalance split applying would make, with the current action and accounts.
    pub imbalance: Option<ImbalanceWire>,
    /// True when the transaction added, cleared or updated would be read-only by date.
    pub read_only_by_date: bool,
    /// Warnings for this transaction.
    pub warnings: Vec<WarningWire>,
}

/// CSV's own state.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CsvWire {
    /// Each account name the file uses.
    pub account_names: Vec<AccountNameWire>,
}

/// One account name a CSV file uses.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountNameWire {
    /// The name as the file writes it.
    pub name: String,
    /// The account it resolved to, or null: its rows are then errors until `accountNames` names it.
    pub account: Option<Id>,
    /// The account's full name.
    pub account_name: Option<String>,
    /// How: `param` (the caller's `accountNames`), `map` (the `csv-account-map` Import map) or `fullName`.
    pub from: Option<String>,
}

impl From<&Warning> for WarningWire {
    fn from(w: &Warning) -> Self {
        Self { code: w.code.to_owned(), message: w.message.clone(), index: w.index }
    }
}

impl From<&RowError> for RowErrorWire {
    fn from(e: &RowError) -> Self {
        Self { file: e.file, line: e.line, column: e.column, message: e.message.clone() }
    }
}

impl From<&IgnoredLine> for IgnoredLineWire {
    fn from(l: &IgnoredLine) -> Self {
        Self { file: l.file, line: l.line, reason: l.reason.to_owned() }
    }
}

impl From<&NewAccount> for NewAccountWire {
    fn from(a: &NewAccount) -> Self {
        Self { name: a.name.clone(), account_type: a.account_type.as_str().to_owned(), commodity: a.commodity.clone().map(CommodityId) }
    }
}

impl From<Source> for SourceWire {
    fn from(s: Source) -> Self {
        Self { file: s.file, lines: [s.first_line, s.last_line] }
    }
}

/// The proposal and its review, as the result of `import.preview`.
pub fn proposal_result(book: &Book, p: &Previewed) -> ProposalResult {
    let proposal = &p.proposal;
    let transactions: Vec<ImportedTransactionWire> =
        proposal.transactions.iter().zip(&p.review.transactions).map(|(t, r)| imported_transaction(book, t, r)).collect();
    let warnings = proposal.warnings.iter().chain(&p.review.warnings).map(WarningWire::from).collect();
    let (documents, owners) = match &proposal.business {
        Some(Records::Documents(d)) => (Some(d.iter().map(|d| business::document_wire(book, d)).collect::<Vec<_>>()), None),
        Some(Records::Owners(o)) => (None, Some(o.iter().map(business::owner_wire).collect::<Vec<_>>())),
        None => (None, None),
    };
    let mut summary = summary(&transactions, proposal.errors.len());
    documents.iter().flatten().for_each(|d| count(&mut summary, d.action, d.read_only_by_date));
    owners.iter().flatten().for_each(|o| count(&mut summary, o.action, false));
    ProposalResult {
        id: p.id.clone(),
        format: proposal.format.as_str().to_owned(),
        book_revision: p.revision,
        threshold_date: proposal.threshold.map(Day),
        summary,
        ignored_lines: proposal.ignored_lines.iter().map(IgnoredLineWire::from).collect(),
        errors: proposal.errors.iter().map(RowErrorWire::from).collect(),
        warnings,
        new_accounts: p.review.new_accounts.iter().map(NewAccountWire::from).collect(),
        new_commodities: Vec::new(),
        transactions,
        csv: proposal.csv.as_ref().map(|c| CsvWire { account_names: c.account_names.iter().map(|n| account_name(book, n)).collect() }),
        documents,
        owners,
    }
}

/// Count a business record in the summary.
fn count(s: &mut Summary, action: ActionWire, read_only_by_date: bool) {
    match action {
        ActionWire::Add => s.add += 1,
        ActionWire::Clear => s.clear += 1,
        ActionWire::Update => s.update += 1,
        ActionWire::Skip => s.skip += 1,
    }
    s.read_only_by_date += usize::from(read_only_by_date);
}

fn summary(transactions: &[ImportedTransactionWire], errors: usize) -> Summary {
    transactions.iter().fold(Summary { errors, ..Summary::default() }, |mut s, t| {
        match t.action {
            ActionWire::Add => s.add += 1,
            ActionWire::Clear => s.clear += 1,
            ActionWire::Update => s.update += 1,
            ActionWire::Skip => s.skip += 1,
        }
        s.imbalanced += usize::from(t.imbalance.is_some());
        s.read_only_by_date += usize::from(t.read_only_by_date);
        s
    })
}

fn account_name(book: &Book, n: &AccountName) -> AccountNameWire {
    AccountNameWire {
        name: n.name.clone(),
        account: n.account.map(Id),
        account_name: n.account.and_then(|a| book.full_name(a)),
        from: n.source.map(|s| s.as_str().to_owned()),
    }
}

fn split_wire(book: &Book, s: &Split) -> ImportedSplitWire {
    ImportedSplitWire {
        account: s.account.map(Id),
        account_name: s.account.and_then(|a| book.full_name(a)),
        value: s.value.into(),
        amount: s.amount.into(),
        memo: s.memo.clone(),
        action: s.action.clone(),
        reconcile_state: s.reconcile_state.as_char().to_string(),
        reconcile_date: s.reconcile_date.map(Timestamp),
    }
}

fn imported_transaction(book: &Book, t: &ProposedTransaction, r: &TransactionReview) -> ImportedTransactionWire {
    let info = &t.info;
    let trans = &info.trans;
    ImportedTransactionWire {
        index: info.index,
        source: t.source.into(),
        currency: CommodityId(trans.currency.clone()),
        date_posted: posted_date(trans).map(Day),
        num: trans.num.clone(),
        description: trans.description.clone(),
        notes: trans.notes().map(str::to_owned),
        void_reason: void_reason(trans),
        splits: trans.splits.iter().map(|s| split_wire(book, s)).collect(),
        other: other(book, info),
        action: info.action.into(),
        matches: info.matches.iter().map(|m| match_wire(book, m)).collect(),
        selected_match: info.selected_match.map(Id),
        imbalance: r.imbalance.as_ref().map(imbalance_wire),
        read_only_by_date: r.read_only_by_date,
        warnings: r.warnings.iter().map(WarningWire::from).collect(),
    }
}

fn void_reason(t: &Transaction) -> Option<String> {
    t.slots.get_str(red_ale_core::transaction::slot_paths::VOID_REASON).map(str::to_owned)
}

fn other(book: &Book, info: &TransInfo) -> Option<OtherWire> {
    if info.dest_value.is_zero() {
        return None;
    }
    Some(OtherWire {
        account: info.dest_account.map(Id),
        account_name: info.dest_account.and_then(|a| book.full_name(a)),
        from: info.dest_source.map(|s| dest_source(s).to_owned()),
        value: info.dest_value.into(),
        amount: info.dest_amount.into(),
    })
}

fn dest_source(s: DestSource) -> &'static str {
    match s {
        DestSource::File => "file",
        DestSource::Bayes => "bayes",
        DestSource::Map => "map",
        DestSource::Caller => "caller",
    }
}

fn match_wire(book: &Book, m: &MatchInfo) -> MatchWire {
    let t = book.transaction(m.transaction);
    let s = t.and_then(|t| t.split(m.split));
    MatchWire {
        split: Id(m.split),
        transaction: Id(m.transaction),
        score: Some(m.probability),
        update_proposed: m.update_proposed,
        date_posted: t.and_then(posted_date).map(Day),
        description: t.and_then(|t| t.description.clone()),
        num: t.and_then(|t| t.num.clone()),
        memo: s.and_then(|s| s.memo.clone()),
        amount: s.map_or(red_ale_core::Numeric::ZERO, |s| s.amount).into(),
        value: s.map_or(red_ale_core::Numeric::ZERO, |s| s.value).into(),
        reconcile_state: s.map_or('n', |s| s.reconcile_state.as_char()).to_string(),
    }
}

fn imbalance_wire(i: &ImbalanceInfo) -> ImbalanceWire {
    ImbalanceWire { value: i.value.into(), currency: CommodityId(i.currency.clone()), account: i.account.map(Id), account_name: i.account_name.clone() }
}
