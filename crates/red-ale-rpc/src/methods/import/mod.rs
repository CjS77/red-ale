//! Methods: importing files through an Import proposal (`design/api/import.md`). `import.preview` reads a file and
//! returns a proposal the server holds by ID; `import.apply` applies it with the caller's changes as one Mutation;
//! `import.discard` drops it. The reading, matching and applying are `red_ale_import`'s and `red_ale_service`'s; this
//! module translates the wire.

mod params;
mod proposal;
mod records;

use red_ale_import::pipeline::Applied;
use red_ale_service::import;
use schemars::JsonSchema;
use serde::Serialize;

pub use params::{
    AccountsPreviewParam, ActionWire, ApplyParams, ChangeParam, CsvPreviewParam, DiscardParams, ImportFileParam, LayoutParam, PreviewParam,
    PriceLayoutParam, PricesPreviewParam, QifDateFormatWire, QifFileParam, QifPreviewParam, QifReconcileWire, QifSecurityParam, SeparatorWire,
};
pub use proposal::ProposalResult;
pub use records::{AccountFieldsWire, AccountRecordWire, ExistingWire, FieldChangeWire, PriceRecordWire};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{Amount, Id};

/// The import methods.
pub fn methods() -> Vec<Method> {
    vec![
        method("import.preview", "Read a file and propose what importing it would do: the Import proposal, held by ID. Changes nothing.", Effect::Read, preview),
        method("import.apply", "Apply a held Import proposal with the caller's changes, as one Mutation.", Effect::Destructive, apply),
        method("import.discard", "Drop a held Import proposal.", Effect::Read, discard),
    ]
}

fn preview(ctx: &mut Context, p: PreviewParam) -> Result<ProposalResult, RpcError> {
    let config = ctx.settings.config.load()?;
    let session = ctx.session()?;
    let (files, params) = p.resolve(session.book())?;
    let previewed = import::preview(session, &config, &files, &params)?;
    Ok(proposal::proposal_result(session.book(), &previewed))
}

fn apply(ctx: &mut Context, p: ApplyParams) -> Result<ApplyResult, RpcError> {
    let store = ctx.settings.config.clone();
    let session = ctx.session()?;
    let (changes, choices) = p.resolve(session.book())?;
    let applied = import::apply(session, &store, &p.proposal, &changes, choices)?;
    Ok(ApplyResult::from(applied))
}

/// The result of `import.discard`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Discarded {
    /// Always true.
    pub discarded: bool,
}

fn discard(ctx: &mut Context, p: DiscardParams) -> Result<Discarded, RpcError> {
    import::discard(ctx.session()?, &p.proposal)?;
    Ok(Discarded { discarded: true })
}

/// A transaction applied, by its index in the proposal.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndexedTransaction {
    /// Its index in the proposal.
    pub index: usize,
    /// The transaction added, or the existing one cleared or updated.
    pub transaction: Id,
}

/// Accounts applying created or changed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AccountsChanged {
    /// Created: `Imbalance-<CUR>` accounts, or a CSV account import's accounts.
    pub created: Vec<Id>,
    /// Changed: Slots of a transaction import (Import maps learned, the append choice saved), or the fields a CSV
    /// account import updated.
    pub updated: Vec<Id>,
}

/// Prices recorded: from added transactions (source `user:split-import`), or a CSV price import's (`user:price`).
#[derive(Debug, Serialize, JsonSchema)]
pub struct PricesChanged {
    /// New prices.
    pub added: Vec<Id>,
    /// New prices that replaced a same-day price (for transactions, the same-day price updated).
    pub replaced: Vec<Id>,
    /// Same-day prices kept, which the new price duplicated.
    pub duplicate: Vec<Id>,
}

/// An `Imbalance-<CUR>` split applying made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ImbalanceSplitWire {
    /// The imported transaction's index.
    pub index: usize,
    /// The transaction.
    pub transaction: Id,
    /// The Imbalance split.
    pub split: Id,
    /// Its account.
    pub account: Id,
    /// Its value.
    pub value: Amount,
}

/// A Bayesian Import map entry learned.
#[derive(Debug, Serialize, JsonSchema)]
pub struct LearnedBayes {
    /// The account whose map learned (the imported split's).
    pub account: Id,
    /// The destination learned.
    pub target: Id,
    /// The tokens, each counted once more.
    pub tokens: Vec<String>,
}

/// A plain Import map entry learned.
#[derive(Debug, Serialize, JsonSchema)]
pub struct LearnedPlain {
    /// The account whose map learned.
    pub account: Id,
    /// The destination.
    pub target: Id,
    /// `desc` or `memo`.
    pub category: String,
    /// The description or memo.
    pub key: String,
}

/// A `csv-account-map` entry learned.
#[derive(Debug, Serialize, JsonSchema)]
pub struct LearnedName {
    /// The name in the file.
    pub name: String,
    /// The account it maps to, which holds the entry.
    pub account: Id,
}

/// The Import map entries written.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LearnedWire {
    /// Bayesian entries.
    pub bayes: Vec<LearnedBayes>,
    /// Plain `desc` and `memo` entries.
    pub plain: Vec<LearnedPlain>,
    /// `csv-account-map` entries.
    pub csv_account_map: Vec<LearnedName>,
}

/// The result of `import.apply`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    /// Transactions added.
    pub added: Vec<IndexedTransaction>,
    /// Existing transactions cleared.
    pub cleared: Vec<IndexedTransaction>,
    /// Existing transactions updated and cleared.
    pub updated: Vec<IndexedTransaction>,
    /// Indexes skipped: transactions, or a CSV account or price import's rows.
    pub skipped: Vec<usize>,
    /// Accounts created and changed.
    pub accounts: AccountsChanged,
    /// Commodities created, as `NAMESPACE::MNEMONIC` (QIF securities); none for CSV.
    pub commodities: Vec<String>,
    /// Prices recorded.
    pub prices: PricesChanged,
    /// Every `Imbalance-<CUR>` split made.
    pub imbalance_splits: Vec<ImbalanceSplitWire>,
    /// Import map entries written.
    pub learned: LearnedWire,
    /// Every Imbalance split again, every transaction applied read-only by date, and the matcher's own warnings.
    pub warnings: Vec<proposal::WarningWire>,
}

fn indexed(list: Vec<(usize, red_ale_core::Guid)>) -> Vec<IndexedTransaction> {
    list.into_iter().map(|(index, t)| IndexedTransaction { index, transaction: Id(t) }).collect()
}

fn ids(list: Vec<red_ale_core::Guid>) -> Vec<Id> {
    list.into_iter().map(Id).collect()
}

impl From<Applied> for ApplyResult {
    fn from(a: Applied) -> Self {
        let (added, replaced, duplicate) = a.prices;
        Self {
            added: indexed(a.added),
            cleared: indexed(a.cleared),
            updated: indexed(a.updated),
            skipped: a.skipped,
            accounts: AccountsChanged { created: ids(a.accounts_created), updated: ids(a.accounts_updated) },
            commodities: a.commodities_created.iter().map(ToString::to_string).collect(),
            prices: PricesChanged { added: ids(added), replaced: ids(replaced), duplicate: ids(duplicate) },
            imbalance_splits: a
                .imbalance_splits
                .into_iter()
                .map(|s| ImbalanceSplitWire { index: s.index, transaction: Id(s.transaction), split: Id(s.split), account: Id(s.account), value: s.value.into() })
                .collect(),
            learned: LearnedWire {
                bayes: a.learned.bayes.into_iter().map(|(account, target, tokens)| LearnedBayes { account: Id(account), target: Id(target), tokens }).collect(),
                plain: a
                    .learned
                    .plain
                    .into_iter()
                    .map(|(account, target, category, key)| LearnedPlain { account: Id(account), target: Id(target), category: category.to_owned(), key })
                    .collect(),
                csv_account_map: a.learned.csv_account_map.into_iter().map(|(name, account)| LearnedName { name, account: Id(account) }).collect(),
            },
            warnings: a.warnings.iter().map(proposal::WarningWire::from).collect(),
        }
    }
}
