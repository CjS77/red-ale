//! The import map editor: listing and deleting Import map and online entries, as GnuCash's
//! `5.17:gnucash/gnome/dialog-imap-editor.c` does (GPL-2.0-or-later; Copyright (C) 2015 Robert Fewell), without its
//! window. Spec: `design/api/import.md`, "`importMap.list`" and "`importMap.delete`".
//!
//! The editor shows three lists: Bayesian entries, plain entries by category (`desc`, `memo`, `csv-account-map`),
//! and online entries, which are not Import maps but the IDs online banking left on an account: its `online_id`, its
//! `hbci` frame, and `ofx/associated-income-account` (one Slot key holding `/`, as GnuCash 5.17 reads and writes it,
//! `5.17:libgnucash/engine/Account.cpp:64`). Each list's entries are found and deleted by the functions in
//! [`super`] and [`super::bayes`].

use std::collections::BTreeMap;

use red_ale_core::ops::accounts::descendants_sorted;
use red_ale_core::{Book, DomainError, DomainResult, Guid, SlotValue};

use super::bayes::{bayes_info, delete_all_bayes_maps};
use super::{CATEGORY_CSV_ACCOUNT_MAP, CATEGORY_DESC, CATEGORY_MEMO, IMAP_FRAME, ImapInfo, delete_map_entry, plain_info};

/// The account Slot holding an account's Online ID (`dialog-imap-editor.c:718-738`).
pub const ONLINE_ID: &str = "online_id";
/// The frame AqBanking keeps an account's bank code and account ID in (`dialog-imap-editor.c:742-768`).
pub const HBCI: &str = "hbci";
/// The account Slot naming the income account of OFX investment imports (`Account.cpp:64`).
pub const OFX_INCOME_ACCOUNT: &str = "ofx/associated-income-account";

/// What kind of entry: the editor's three lists, the plain one by category. In the order entries are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MapKind {
    /// A Bayesian entry: a token's count for a target.
    Bayes,
    /// A plain `desc` entry.
    Desc,
    /// A plain `memo` entry.
    Memo,
    /// A `csv-account-map` entry, held by its target.
    CsvAccountMap,
    /// The account's `online_id`.
    OnlineId,
    /// The account's `hbci` bank code and account ID.
    Hbci,
    /// The account's `ofx/associated-income-account`.
    OfxIncomeAccount,
}

impl MapKind {
    /// Every kind, in listing order.
    pub const ALL: [Self; 7] = [Self::Bayes, Self::Desc, Self::Memo, Self::CsvAccountMap, Self::OnlineId, Self::Hbci, Self::OfxIncomeAccount];

    /// The name `design/api/import.md` gives it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bayes => "bayes",
            Self::Desc => "desc",
            Self::Memo => "memo",
            Self::CsvAccountMap => "csvAccountMap",
            Self::OnlineId => "onlineId",
            Self::Hbci => "hbci",
            Self::OfxIncomeAccount => "ofxIncomeAccount",
        }
    }

    /// The kind with this name.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == name)
    }

    fn category(self) -> Option<&'static str> {
        match self {
            Self::Desc => Some(CATEGORY_DESC),
            Self::Memo => Some(CATEGORY_MEMO),
            Self::CsvAccountMap => Some(CATEGORY_CSV_ACCOUNT_MAP),
            _ => None,
        }
    }
}

/// One entry, as the editor lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapEntry {
    /// The account holding it.
    pub account: Guid,
    /// Its kind.
    pub kind: MapKind,
    /// The token, the plain key, or the online ID (`bank-code,account-id` for HBCI, as the editor shows it).
    pub key: String,
    /// The account it points at, when its GUID names one. For Online IDs and HBCI the editor shows the holder, so
    /// this is `None` for them.
    pub target: Option<Guid>,
    /// A Bayesian entry's count.
    pub count: Option<i64>,
    /// False when the editor's "Delete invalid mappings" would remove it (`find_invalid_mappings`,
    /// `dialog-imap-editor.c:292-315`): a Bayesian or plain entry whose account doesn't resolve, or an empty
    /// Online ID.
    pub valid: bool,
    /// The Slot key the entry starts at: the whole flat key of a Bayesian entry.
    head: String,
}

/// Which entries to list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListFilter {
    /// Only this account's.
    pub account: Option<Guid>,
    /// Only this kind.
    pub kind: Option<MapKind>,
    /// Only invalid ones.
    pub invalid_only: bool,
}

/// Every entry the filter allows, by account full name, then kind, then key, bytewise.
pub fn list(book: &Book, filter: ListFilter) -> Vec<MapEntry> {
    let names: BTreeMap<Guid, String> = descendants_sorted(book, book.root).iter().map(|a| (a.guid, book.full_name(a.guid).unwrap_or_default())).collect();
    let mut entries: Vec<MapEntry> = names
        .keys()
        .filter(|g| filter.account.is_none_or(|a| a == **g))
        .flat_map(|g| MapKind::ALL.into_iter().filter(|k| filter.kind.is_none_or(|f| f == *k)).flat_map(move |k| entries_of(book, *g, k)))
        .filter(|e| !filter.invalid_only || !e.valid)
        .collect();
    entries.sort_by(|a, b| (names.get(&a.account), a.kind, &a.key, &a.head).cmp(&(names.get(&b.account), b.kind, &b.key, &b.head)));
    entries
}

fn entries_of(book: &Book, account: Guid, kind: MapKind) -> Vec<MapEntry> {
    match kind {
        MapKind::Bayes => bayes_info(book, account).into_iter().map(|i| from_info(kind, i)).collect(),
        MapKind::Desc | MapKind::Memo | MapKind::CsvAccountMap => plain_info(book, account, kind.category()).into_iter().map(|i| from_info(kind, i)).collect(),
        MapKind::OnlineId | MapKind::Hbci | MapKind::OfxIncomeAccount => online_entry(book, account, kind).into_iter().collect(),
    }
}

fn from_info(kind: MapKind, info: ImapInfo) -> MapEntry {
    MapEntry {
        account: info.source_account,
        kind,
        valid: info.map_account.is_some(),
        target: info.map_account,
        count: info.count,
        head: info.head,
        key: info.match_string,
    }
}

/// `check_online_id`, `check_hbci` and `check_ofx_account` (`dialog-imap-editor.c:718-795`). Only an empty Online
/// ID is invalid: the editor counts no other online entry (`:303-306`).
fn online_entry(book: &Book, account: Guid, kind: MapKind) -> Option<MapEntry> {
    let slots = &book.account(account)?.slots;
    let entry = |head: &str, key: String, target: Option<Guid>, valid: bool| MapEntry { account, kind, key, target, count: None, valid, head: head.to_owned() };
    match kind {
        MapKind::OnlineId => slots.get_key(ONLINE_ID).and_then(SlotValue::as_str).map(|id| entry(ONLINE_ID, id.to_owned(), None, !id.is_empty())),
        MapKind::Hbci => {
            let field = |k: &str| slots.get_in(&[HBCI, k]).and_then(SlotValue::as_str).map(str::to_owned);
            Some(entry(HBCI, format!("{},{}", field("bank-code")?, field("account-id")?), None, true))
        }
        MapKind::OfxIncomeAccount => slots.get_key(OFX_INCOME_ACCOUNT).and_then(SlotValue::as_guid).map(|g| {
            let target = book.account(g).map(|a| a.guid);
            entry(OFX_INCOME_ACCOUNT, g.to_string(), target, true)
        }),
        _ => None,
    }
}

/// One deletion: an entry, or with no key every entry of that kind on the account (the editor's parent row). A
/// Bayesian key is a token: with `target`, only that target's entry goes; without, every target's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteRequest {
    /// The account holding the entries.
    pub account: Guid,
    /// Their kind.
    pub kind: MapKind,
    /// The key, or `None` for every entry of the kind.
    pub key: Option<String>,
    /// For a Bayesian token, only its entry for this account.
    pub target: Option<Guid>,
}

/// Delete entries as the editor does (`delete_selected_row`, `dialog-imap-editor.c:155-210`): an empty plain category
/// and an empty `import-map` frame go too; an online entry's head Slot goes whole; an account's Bayesian parent row
/// goes with `gnc_account_delete_all_bayes_maps`. Returns the entries deleted, as they were listed. A request
/// matching nothing is `not-found`; the caller runs this in one Mutation, so nothing is deleted then.
pub fn delete(book: &mut Book, requests: &[DeleteRequest]) -> DomainResult<Vec<MapEntry>> {
    requests.iter().try_fold(Vec::new(), |mut deleted, r| {
        let found = matching(book, r)?;
        if r.kind == MapKind::Bayes && r.key.is_none() && r.target.is_none() {
            delete_all_bayes_maps(book, r.account)?;
        } else {
            found.iter().try_for_each(|e| delete_entry(book, e))?;
        }
        deleted.extend(found);
        Ok(deleted)
    })
}

/// Delete every invalid entry in the book: the editor's "Delete invalid mappings" (`gnc_imap_remove_invalid_maps`,
/// `dialog-imap-editor.c:317-392`) over all three lists.
pub fn delete_invalid(book: &mut Book) -> DomainResult<Vec<MapEntry>> {
    let invalid = list(book, ListFilter { invalid_only: true, ..ListFilter::default() });
    invalid.iter().try_for_each(|e| delete_entry(book, e))?;
    Ok(invalid)
}

fn matching(book: &Book, r: &DeleteRequest) -> DomainResult<Vec<MapEntry>> {
    let found: Vec<MapEntry> = list(book, ListFilter { account: Some(r.account), kind: Some(r.kind), invalid_only: false })
        .into_iter()
        .filter(|e| r.key.as_ref().is_none_or(|k| *k == e.key))
        .filter(|e| r.target.is_none_or(|t| e.target == Some(t)))
        .collect();
    if found.is_empty() {
        let what = format!("{} {} on {}", r.kind.as_str(), r.key.as_deref().unwrap_or("(all)"), r.account);
        return Err(DomainError::not_found("importMapEntry", &what));
    }
    Ok(found)
}

/// `delete_info_bayes`, `delete_info_nbayes` and the online case of `delete_selected_row`
/// (`dialog-imap-editor.c:131-190`), for one listed entry.
fn delete_entry(book: &mut Book, e: &MapEntry) -> DomainResult<()> {
    match e.kind.category() {
        Some(category) => {
            delete_map_entry(book, e.account, IMAP_FRAME, Some(category), Some(&e.key), false)?;
            delete_map_entry(book, e.account, IMAP_FRAME, Some(category), None, true)?;
            delete_map_entry(book, e.account, IMAP_FRAME, None, None, true)?;
        }
        None => {
            delete_map_entry(book, e.account, &e.head, None, None, false)?;
        }
    }
    Ok(())
}
