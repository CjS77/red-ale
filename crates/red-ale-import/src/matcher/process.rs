//! What each Import action does to the Book (`gnc_import_process_trans_item`, `process_reconcile`,
//! `update_desc_and_notes` and `matchmap_store_destination`, `5.17:gnucash/import-export/import-backend.cpp:526-1032`;
//! `on_matcher_ok_clicked`, `5.17:gnucash/import-export/import-main-matcher.cpp:534-604`).
//!
//! Every function here changes a `&mut Book` that the caller holds inside one Mutation: on an error the caller drops
//! the Book, so nothing is half applied. Each transaction changed is committed as GnuCash commits it: the imbalance
//! scrub (`xaccTransScrubImbalance`, from `xaccTransCommitEdit`, `5.17:libgnucash/engine/Transaction.cpp:1549-1558`)
//! balances it, with an `Imbalance-<CUR>` split if needed, and its splits are sorted. The Book stays balanced.
//!
//! **Deviation** (`design/api/import.md`): GnuCash learns a destination picked by hand at the moment it is picked,
//! and keeps it if the import is cancelled; red-ale learns it in [`apply_batch`], so a discarded proposal learns
//! nothing. The cases learned are GnuCash's: every destination the caller chose, and every *clear* and *update*
//! whose matched transaction has one other split; never an *add* with a destination the maps found.

use std::sync::Arc;

use icu_normalizer::ComposingNormalizerBorrowed;
use red_ale_core::ops::commit::{round_amount, round_value, set_date_posted, set_notes, sort_splits};
use red_ale_core::ops::prices::SPLIT_IMPORT_SOURCE;
use red_ale_core::ops::scrub::{Repair, Scope, scrub_imbalance};
use red_ale_core::ops::transactions::other_split;
use red_ale_core::ops::transactions::transfer::{PriceChange, record_prices};
use red_ale_core::transaction::slot_paths::ONLINE_ID;
use red_ale_core::{Book, DomainError, DomainResult, Env, Guid, Numeric, ReconcileState, SlotValue, Split, Time64, Transaction};

use super::trans_info::imbalance;
use super::{ImportAction, MatchInfo, TransInfo};
use crate::import_map::bayes::add_account_bayes;
use crate::import_map::{CATEGORY_DESC, CATEGORY_MEMO, account, add_account, slots_mut};

/// The account slot holding the Append checkbox's last state (`xaccAccountSetAppendText`,
/// `5.17:libgnucash/engine/Account.cpp:4119-4128`).
pub const APPEND_TEXT: &str = "import-append-text";

/// The choices that apply to a whole batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyOptions {
    /// The Import setting `use-bayes`: learn into the Bayesian Import map, else the plain one.
    pub use_bayes: bool,
    /// GnuCash's Append checkbox: *update* appends the imported description and notes instead of replacing them.
    pub append_text: bool,
}

/// One Import map entry written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Learned {
    /// Each token's count for `target` went up by one in `source`'s Bayesian map.
    Bayes {
        /// The imported split's account.
        source: Guid,
        /// The destination learned.
        target: Guid,
        /// The tokens counted.
        tokens: Vec<String>,
    },
    /// `key` now maps to `target` in `source`'s plain map.
    Plain {
        /// The imported split's account.
        source: Guid,
        /// The destination learned.
        target: Guid,
        /// `desc` or `memo`.
        category: &'static str,
        /// The description or memo.
        key: String,
    },
}

/// Something applying did that the caller should know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessWarning {
    /// *add* made the balancing split in another commodity with no amount: its amount is its value, a rate of 1
    /// (`import-backend.cpp:910-921`).
    MissingRate,
    /// *clear* or *update* had no selected match, so the transaction was ignored (`import-backend.cpp:937-941`,
    /// `:1008-1013`).
    NoMatch,
    /// *update* left the matched transaction's other split alone because it is not in state `n`; its commit balanced
    /// the transaction with an Imbalance split instead (`import-backend.cpp:982-988`).
    OtherSplitNotUpdated,
}

/// What applying one imported transaction did.
#[derive(Debug, Clone, PartialEq)]
pub struct Processed {
    /// The imported transaction's index.
    pub index: usize,
    /// Its Import action.
    pub action: ImportAction,
    /// True when it changed the Book: GnuCash's return value.
    pub applied: bool,
    /// The transaction added, or the matched transaction cleared or updated.
    pub transaction: Option<Guid>,
    /// What committing a transaction changed: balancing splits and the accounts made for them.
    pub repairs: Vec<Repair>,
    /// Prices recorded from an added transaction, source `user:split-import`.
    pub prices: Vec<PriceChange>,
    /// Import map entries written.
    pub learned: Vec<Learned>,
    /// Warnings.
    pub warnings: Vec<ProcessWarning>,
}

impl Processed {
    fn new(info: &TransInfo) -> Self {
        Self {
            index: info.index,
            action: info.action,
            applied: false,
            transaction: None,
            repairs: Vec::new(),
            prices: Vec::new(),
            learned: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

/// The Append checkbox's default for an account (`xaccAccountGetAppendText` and `get_kvp_boolean_path`,
/// `5.17:libgnucash/engine/Account.cpp:2562-2577`): a non-zero integer or the string `true`.
pub fn append_text(book: &Book, acc: Guid) -> bool {
    match book.account(acc).and_then(|a| a.slots.get_key(APPEND_TEXT)) {
        Some(SlotValue::Integer(i)) => *i != 0,
        Some(SlotValue::String(s)) => s == "true",
        _ => false,
    }
}

/// Save the Append checkbox on an account: `"true"`, or the slot removed (`xaccAccountSetAppendText` and
/// `set_kvp_boolean_path`, `Account.cpp:2557-2561`, `:4124-4128`).
pub fn set_append_text(book: &mut Book, acc: Guid, on: bool) -> DomainResult<()> {
    let slots = slots_mut(book, acc)?;
    if on {
        slots.insert_key(APPEND_TEXT, SlotValue::String("true".to_owned()));
    } else {
        slots.remove_key(APPEND_TEXT);
    }
    Ok(())
}

/// Apply a batch as GnuCash's matcher does on OK (`on_matcher_ok_clicked`, `import-main-matcher.cpp:534-604`): the
/// Append choice is saved on the account of the first transaction in GnuCash's row order (the last to arrive), then
/// each transaction is processed in row order. Destinations the caller chose are learned first, in row order, as
/// GnuCash learned them when they were picked, before OK. `infos` are in arrival order; so are the results.
pub fn apply_batch(book: &mut Book, env: &mut dyn Env, infos: &[TransInfo], opts: ApplyOptions) -> DomainResult<Vec<Processed>> {
    if let Some(acc) = infos.last().and_then(TransInfo::account) {
        set_append_text(book, acc, opts.append_text)?;
    }
    let manual: Vec<Vec<Learned>> = infos
        .iter()
        .rev()
        .map(|info| match info.dest_account.filter(|_| info.dest_selected_manually()) {
            Some(dest) => store_destination(book, info, dest, opts.use_bayes),
            None => Ok(Vec::new()),
        })
        .collect::<DomainResult<_>>()?;
    let mut processed = infos
        .iter()
        .rev()
        .zip(manual)
        .map(|(info, learned)| {
            let mut p = process_trans_item(book, env, info, opts)?;
            p.learned.splice(0..0, learned);
            Ok(p)
        })
        .collect::<DomainResult<Vec<_>>>()?;
    processed.reverse();
    Ok(processed)
}

/// `gnc_import_process_trans_item` (`import-backend.cpp:885-1032`): apply one imported transaction's action.
pub fn process_trans_item(book: &mut Book, env: &mut dyn Env, info: &TransInfo, opts: ApplyOptions) -> DomainResult<Processed> {
    let mut out = Processed::new(info);
    match info.action {
        ImportAction::Skip => {}
        ImportAction::Add => add(book, env, info, &mut out)?,
        ImportAction::Clear | ImportAction::Update => match info.selected() {
            None => out.warnings.push(ProcessWarning::NoMatch),
            Some(selected) => {
                if info.action == ImportAction::Update {
                    update_matched(book, env, info, selected, opts.append_text, &mut out)?;
                }
                process_reconcile(book, env, info, selected, opts.use_bayes, &mut out)?;
            }
        },
    }
    Ok(out)
}

/// *add* (`import-backend.cpp:898-931`): with an imbalance and a destination, the balancing split (value
/// [`TransInfo::dest_value`], amount [`TransInfo::dest_amount`] or, when that is zero, the value); split 0 cleared
/// now; the transaction committed, which balances what is left with an Imbalance split; then
/// `xaccTransRecordPrice(trans, PRICE_SOURCE_SPLIT_IMPORT)`. Nothing is learned.
fn add(book: &mut Book, env: &mut dyn Env, info: &TransInfo, out: &mut Processed) -> DomainResult<()> {
    let mut t = info.trans.clone();
    if book.transactions.contains_key(&t.guid) {
        return Err(DomainError::conflict(format!(
            "imported transaction {} is already in the book",
            t.guid
        )));
    }
    if let Some(dest) = info.dest_account.filter(|_| !imbalance(&t).is_ok_and(Numeric::is_zero)) {
        let amount = if info.dest_amount.is_zero() {
            out.warnings.push(ProcessWarning::MissingRate);
            info.dest_value
        } else {
            info.dest_amount
        };
        account(book, dest)?;
        let split = Split::new(
            env.new_guid(),
            dest,
            round_value(book, &t.currency, info.dest_value)?,
            round_amount(book, dest, amount)?,
        );
        t.splits.push(split);
    }
    let now = env.now();
    if let Some(first) = t.splits.first_mut() {
        first.reconcile_state = ReconcileState::Cleared;
        first.reconcile_date = Some(now);
    }
    if t.date_entered == Time64(0) {
        t.date_entered = now;
    }
    let guid = t.guid;
    book.transactions.insert(guid, Arc::new(t));
    out.repairs.extend(commit(book, env, guid)?);
    out.prices = record_prices(book, env, guid, SPLIT_IMPORT_SOURCE)?;
    out.transaction = Some(guid);
    out.applied = true;
    Ok(())
}

/// `xaccTransCommitEdit`'s part that changes data: the imbalance scrub, then the splits sorted.
fn commit(book: &mut Book, env: &mut dyn Env, guid: Guid) -> DomainResult<Vec<Repair>> {
    let repairs = scrub_imbalance(book, env, Scope::Transaction(guid))?;
    let t = book
        .transactions
        .get_mut(&guid)
        .ok_or_else(|| DomainError::not_found("transaction", &guid))?;
    sort_splits(Arc::make_mut(t));
    Ok(repairs)
}

fn matched(book: &Book, selected: &MatchInfo) -> DomainResult<Transaction> {
    book.transaction(selected.transaction)
        .cloned()
        .ok_or_else(|| DomainError::not_found("transaction", &selected.transaction))
}

/// *update*'s changes to the matched transaction before it is cleared (`import-backend.cpp:932-1003`): the posted
/// date becomes the imported one's local day; the matched split takes split 0's amount and value; with a balancing
/// value and exactly one other split ([`other_split`]) in state `n`, that split's value **and amount** become the
/// balancing value (GnuCash reads the value where it meant the amount, `:968`, so its price branch never runs); split
/// 0's memo replaces the matched one unless empty; the description and notes are replaced or appended
/// ([`update_desc_and_notes`]). Then the transaction is committed.
fn update_matched(
    book: &mut Book,
    env: &mut dyn Env,
    info: &TransInfo,
    selected: &MatchInfo,
    append: bool,
    out: &mut Processed,
) -> DomainResult<()> {
    let mut t = matched(book, selected)?;
    let fsplit = info
        .trans
        .splits
        .first()
        .ok_or_else(|| DomainError::invalid("imported transaction has no split"))?;
    if let Some(date) = env.zone().local_date(info.trans.date_posted) {
        set_date_posted(&mut t, date, env.zone());
    }
    let other = other_split(book, &t, selected.split).map(|s| s.guid);
    let currency = t.currency.clone();
    let split = t
        .split_mut(selected.split)
        .ok_or_else(|| DomainError::not_found("split", &selected.split))?;
    split.amount = rounded_amount(book, split.account, fsplit.amount)?;
    split.value = round_value(book, &currency, fsplit.value)?;
    if fsplit.memo.as_deref().is_some_and(|m| !m.is_empty()) {
        split.memo.clone_from(&fsplit.memo);
    }
    if let Some(other) = other.filter(|_| !info.dest_value.is_zero()) {
        let other = t.split_mut(other).ok_or_else(|| DomainError::not_found("split", &other))?;
        if other.reconcile_state == ReconcileState::New {
            other.value = round_value(book, &currency, info.dest_value)?;
            other.amount = rounded_amount(book, other.account, info.dest_value)?;
        } else {
            out.warnings.push(ProcessWarning::OtherSplitNotUpdated);
        }
    }
    update_desc_and_notes(&mut t, &info.trans, append);
    let guid = t.guid;
    book.transactions.insert(guid, Arc::new(t));
    out.repairs.extend(commit(book, env, guid)?);
    Ok(())
}

/// `xaccSplitSetAmount`'s rounding to the account's SCU; an orphan split keeps the amount as given.
fn rounded_amount(book: &Book, account: Option<Guid>, amount: Numeric) -> DomainResult<Numeric> {
    account.map_or(Ok(amount), |a| round_amount(book, a, amount))
}

/// `update_desc_and_notes` (`import-backend.cpp:812-845`). Without append, the imported description replaces the
/// matched one even when empty, and the imported notes replace the matched ones only when the imported transaction
/// has notes: GnuCash passes its null notes to `xaccTransSetNotes`, which then changes nothing
/// (`5.17:libgnucash/engine/Transaction.cpp:2042-2046`). With append, each becomes `old|new` ([`maybe_append`]).
fn update_desc_and_notes(t: &mut Transaction, imported: &Transaction, append: bool) {
    let (description, notes) = if append {
        (
            maybe_append(t.description.as_deref(), imported.description.as_deref()),
            maybe_append(t.notes(), imported.notes()),
        )
    } else {
        (imported.description.clone(), imported.notes().map(str::to_owned))
    };
    if let Some(description) = description {
        t.description = Some(description);
    }
    if let Some(notes) = notes {
        set_notes(t, Some(&notes));
    }
}

/// `maybe_append_string` (`import-backend.cpp:789-810`): the imported text when the matched text is empty; nothing
/// when the imported text is empty; else `matched|imported`, unless the imported text, both NFC-normalised, is no
/// longer than the matched text and already in it.
pub(crate) fn maybe_append(matched: Option<&str>, imported: Option<&str>) -> Option<String> {
    let Some(matched) = matched.filter(|m| !m.is_empty()) else {
        return imported.map(str::to_owned);
    };
    let imported = imported.filter(|i| !i.is_empty())?;
    let nfc = ComposingNormalizerBorrowed::new_nfc();
    let (norm_matched, norm_imported) = (nfc.normalize(matched), nfc.normalize(imported));
    (norm_imported.chars().count() > norm_matched.chars().count() || !norm_matched.contains(norm_imported.as_ref()))
        .then(|| format!("{matched}|{imported}"))
}

/// `process_reconcile` (`import-backend.cpp:847-881`): the matched split goes to `c` if it was `n`, and is reconciled
/// now **whatever its state**; it takes split 0's Online ID if there is one; the matched transaction is committed;
/// then its other split's account is learned.
fn process_reconcile(
    book: &mut Book,
    env: &mut dyn Env,
    info: &TransInfo,
    selected: &MatchInfo,
    use_bayes: bool,
    out: &mut Processed,
) -> DomainResult<()> {
    let mut t = matched(book, selected)?;
    let online_id = info
        .trans
        .splits
        .first()
        .and_then(|s| s.slots.get_key(ONLINE_ID))
        .and_then(SlotValue::as_str)
        .filter(|id| !id.is_empty());
    let now = env.now();
    let split = t
        .split_mut(selected.split)
        .ok_or_else(|| DomainError::not_found("split", &selected.split))?;
    if split.reconcile_state == ReconcileState::New {
        split.reconcile_state = ReconcileState::Cleared;
    }
    split.reconcile_date = Some(now);
    if let Some(id) = online_id {
        split.slots.insert_key(ONLINE_ID, SlotValue::String(id.to_owned()));
    }
    let guid = t.guid;
    book.transactions.insert(guid, Arc::new(t));
    out.repairs.extend(commit(book, env, guid)?);
    let dest = book
        .transaction(guid)
        .and_then(|t| other_split(book, t, selected.split))
        .and_then(|s| s.account);
    if let Some(dest) = dest {
        out.learned.extend(store_destination(book, info, dest, use_bayes)?);
    }
    out.transaction = Some(guid);
    out.applied = true;
    Ok(())
}

/// `matchmap_store_destination` (`import-backend.cpp:526-573`): in the imported split's account, with `use-bayes`
/// every token's count for `dest` goes up by one; without, the description and split 0's memo, when not empty, map to
/// `dest` in the plain map's `desc` and `memo` categories.
fn store_destination(book: &mut Book, info: &TransInfo, dest: Guid, use_bayes: bool) -> DomainResult<Vec<Learned>> {
    let Some(source) = info.account() else { return Ok(Vec::new()) };
    if use_bayes {
        add_account_bayes(book, source, &info.tokens, dest)?;
        return Ok(vec![Learned::Bayes {
            source,
            target: dest,
            tokens: info.tokens.clone(),
        }]);
    }
    let description = info.trans.description.clone().unwrap_or_default();
    let memo = info.trans.splits.first().and_then(|s| s.memo.clone()).unwrap_or_default();
    [(CATEGORY_DESC, description), (CATEGORY_MEMO, memo)]
        .into_iter()
        .filter(|(_, key)| !key.is_empty())
        .map(|(category, key)| {
            add_account(book, source, Some(category), &key, dest)?;
            Ok(Learned::Plain {
                source,
                target: dest,
                category,
                key,
            })
        })
        .collect()
}
