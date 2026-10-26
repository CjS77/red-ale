//! Matching a batch of imported transactions against the Book: the non-GUI parts of GnuCash's matcher window
//! (`5.17:gnucash/import-export/import-main-matcher.cpp:363-458`, `:2256-2461`) and the Online ID check
//! (`5.17:gnucash/import-export/import-backend.cpp:1034-1083`).

use std::collections::{BTreeMap, BTreeSet};

use red_ale_core::book_options::uses_split_action_for_num;
use red_ale_core::ops::transactions::order::split_order;
use red_ale_core::transaction::slot_paths::ONLINE_ID;
use red_ale_core::{Book, DomainResult, Guid, Split, Time64, Transaction};
use red_ale_locale::Locale;

use super::score::split_find_match;
use super::trans_info::{LastSplitInfo, TransInfo};
use super::ImportSettings;

/// Seconds in a day (`import-main-matcher.cpp:2311`).
const SECS_PER_DAY: i64 = 86_400;

/// One transaction an importer built, with what it knows about the balancing split: what the CSV importer hands the
/// matcher (`gnc_gen_trans_list_add_trans_with_split_data`, `import-main-matcher.cpp:2297-2302`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedTransaction {
    /// The transaction, not in the Book; split 0 is the imported split.
    pub transaction: Transaction,
    /// The balancing split's account, amount and price, when the file gives them.
    pub last_split: Option<LastSplitInfo>,
}

/// What [`match_batch`] decided.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchBatch {
    /// One per imported transaction not dropped, in arrival order.
    pub infos: Vec<TransInfo>,
    /// The arrival indexes of the transactions dropped because their imported split's Online ID is already in its
    /// account.
    pub duplicates: Vec<usize>,
}

/// The Online IDs on each account's splits, gathered once per account as GnuCash gathers them once per matcher
/// (`hash_account_online_ids` and `acct_id_hash`, `import-backend.cpp:1034-1073`): so a duplicate within one batch,
/// whose earlier copy is not in the Book yet, is not caught.
#[derive(Debug, Default)]
pub struct OnlineIds(BTreeMap<Guid, BTreeSet<String>>);

impl OnlineIds {
    /// `gnc_import_exists_online_id` (`import-backend.cpp:1050-1083`): true when split 0 has an `online_id` slot
    /// that another split in its account already holds. An empty ID is looked up and never found.
    pub fn exists(&mut self, book: &Book, t: &Transaction) -> bool {
        let Some(split) = t.splits.first() else { return false };
        let Some(id) = split.slots.get_key(ONLINE_ID).and_then(|v| v.as_str()) else { return false };
        let Some(account) = split.account else { return false };
        self.0.entry(account).or_insert_with(|| account_online_ids(book, account)).contains(id)
    }
}

fn online_id(split: &Split) -> Option<&str> {
    split.slots.get_key(ONLINE_ID).and_then(|v| v.as_str()).filter(|id| !id.is_empty())
}

fn account_online_ids(book: &Book, account: Guid) -> BTreeSet<String> {
    book.splits_of(account).filter_map(|(_, s)| online_id(s).map(str::to_owned)).collect()
}

/// Match a batch: GnuCash's `gnc_gen_trans_list_add_trans_internal` for each transaction in arrival order (dropping
/// Online ID duplicates, `import-main-matcher.cpp:2256-2283`), then `gnc_gen_trans_list_create_matches` and
/// `resolve_conflicts` over its list, which is in reverse order of arrival (`:2446-2461`, `:409-458`).
pub fn match_batch(book: &Book, settings: &ImportSettings, imported: Vec<ImportedTransaction>, locale: &Locale) -> DomainResult<MatchBatch> {
    let mut ids = OnlineIds::default();
    let mut duplicates = Vec::new();
    let mut list = Vec::new();
    imported.into_iter().enumerate().try_for_each(|(index, it)| -> DomainResult<()> {
        if ids.exists(book, &it.transaction) {
            duplicates.push(index);
        } else {
            list.insert(0, TransInfo::new(book, settings, index, it.transaction, it.last_split.as_ref(), locale)?);
        }
        Ok(())
    })?;
    create_matches(book, settings, &mut list);
    resolve_conflicts(&mut list);
    list.reverse();
    Ok(MatchBatch { infos: list, duplicates })
}

/// `filter_existing_splits_on_account_and_date` (`import-main-matcher.cpp:2309-2350`): one query for the whole batch,
/// for splits in the imported splits' accounts, posted from the batch's earliest date less the window to its latest
/// plus the window, inclusive, in the query's default order (`xaccSplitOrder`).
fn candidate_splits<'a>(book: &'a Book, settings: &ImportSettings, list: &[TransInfo]) -> Vec<(&'a Transaction, &'a Split)> {
    let accounts: BTreeSet<Guid> = list.iter().filter_map(TransInfo::account).collect();
    let (Some(min), Some(max)) = (list.iter().map(|i| i.trans.date_posted.0).min(), list.iter().map(|i| i.trans.date_posted.0).max()) else {
        return Vec::new();
    };
    let limit = i64::from(settings.match_date_hardlimit).saturating_mul(SECS_PER_DAY);
    let (from, to) = (Time64(min.saturating_sub(limit)), Time64(max.saturating_add(limit)));
    let mut found: Vec<(&Transaction, &Split)> = book
        .transactions
        .values()
        .filter(|t| t.date_posted >= from && t.date_posted <= to)
        .flat_map(|t| t.splits.iter().filter(|s| s.account.is_some_and(|a| accounts.contains(&a))).map(move |s| (t.as_ref(), s)))
        .collect();
    let num_action = uses_split_action_for_num(book);
    found.sort_by(|(ta, sa), (tb, sb)| split_order(ta, sa, tb, sb, num_action));
    found
}

/// `gnc_gen_trans_list_create_matches`, `create_hash_of_potential_matches` and `perform_matching`
/// (`import-main-matcher.cpp:2355-2461`): splits with an Online ID are never candidates; each imported transaction is
/// scored against the candidates in its own split 0's account, then its matches are sorted and its action chosen.
/// GnuCash's two prepends (into the account's bucket, then into the match list) cancel, so equal scores keep the
/// query's order. Transactions still open are never candidates in GnuCash; in red-ale every transaction in the Book
/// is committed.
fn create_matches(book: &Book, settings: &ImportSettings, list: &mut [TransInfo]) {
    let candidates = candidate_splits(book, settings, list);
    let mut by_account: BTreeMap<Guid, Vec<(&Transaction, &Split)>> = BTreeMap::new();
    candidates.into_iter().filter(|(_, s)| online_id(s).is_none()).for_each(|(t, s)| {
        if let Some(a) = s.account {
            by_account.entry(a).or_default().push((t, s));
        }
    });
    list.iter_mut().for_each(|info| {
        let bucket = info.account().and_then(|a| by_account.get(&a)).map_or(&[][..], Vec::as_slice);
        info.matches = bucket.iter().filter_map(|(t, s)| split_find_match(book, &info.trans, t, s, settings)).collect();
        info.init_matches(settings);
    });
}

/// `resolve_conflicts` and `get_conflict_list` (`import-main-matcher.cpp:363-458`): greedily, when imported
/// transactions share the same best-matching existing **transaction**, the one with the highest score keeps it (the
/// earlier in the list on a tie, as only a strictly higher score replaces it), and each other one drops its best match
/// ([`TransInfo::remove_top_match`], which does not recompute the action); then the scan starts again from the top.
/// Only transactions after the one being looked at are compared with it, as in GnuCash.
pub(crate) fn resolve_conflicts(list: &mut [TransInfo]) {
    let mut i = 0;
    while i < list.len() {
        let conflicts = conflicts_of(list, i);
        if conflicts.is_empty() {
            i += 1;
        } else {
            conflicts.into_iter().for_each(|c| list[c].remove_top_match());
            i = 0;
        }
    }
}

/// `get_conflict_list`: the transactions from `i` on whose best match is `i`'s best match's transaction, less the one
/// with the best score.
fn conflicts_of(list: &[TransInfo], i: usize) -> Vec<usize> {
    let Some(top) = list[i].matches.first() else { return Vec::new() };
    let rivals: Vec<(usize, i32)> = std::iter::once((i, top.probability))
        .chain(
            list.iter()
                .enumerate()
                .skip(i + 1)
                .filter_map(|(j, info)| info.matches.first().filter(|m| m.transaction == top.transaction).map(|m| (j, m.probability))),
        )
        .collect();
    let best = rivals.iter().fold(rivals[0], |best, r| if r.1 > best.1 { *r } else { best }).0;
    rivals.into_iter().map(|(j, _)| j).filter(|j| *j != best).collect()
}
