//! Import maps: what GnuCash's importers learn, kept in account Slots.
//!
//! Translated from `5.17:libgnucash/engine/Account.cpp:5146-5777` (GPL-2.0-or-later; Copyright (C) 1997 Robin D.
//! Clark, Copyright (C) 1997-2003 Linas Vepstas, Copyright (C) 2007 David Hampton). Each function cites the lines it
//! follows.
//!
//! - The *plain* map is the frame `import-map/<category>/<key>`, each value the GUID of the target account; this
//!   module.
//! - The *Bayesian* map is one Slot per token and target, keyed by the whole flat path
//!   `import-map-bayes/<token>/<guid>` with an integer count; [`bayes`].
//! - Bayesian data written before GnuCash 2.7 is nested frames; [`convert`] turns it into the flat form.
//!
//! Every path here is a list of whole keys ([`Frame::get_in`]): a key may hold `/`, and is never split.
//!
//! Reads take a `&Book` and see old Bayesian data through the conversion, in memory. Writing Bayesian data and
//! deleting any entry take a `&mut Book`, and first write the conversion to the whole Book, with the Book feature, as
//! GnuCash does on its first access (`design/api/import.md`, "Import maps", "What red-ale does").

pub mod bayes;
pub mod convert;

use std::sync::Arc;

use red_ale_core::ops::accounts::descendants_sorted;
use red_ale_core::{Account, Book, DomainError, DomainResult, Frame, Guid, SlotValue};

/// The plain map's frame (`IMAP_FRAME`, `5.17:libgnucash/engine/Account.cpp:5152`).
pub const IMAP_FRAME: &str = "import-map";
/// The prefix of every Bayesian key (`IMAP_FRAME_BAYES`, `Account.cpp:5153`).
pub const IMAP_FRAME_BAYES: &str = "import-map-bayes";
/// The category the matcher learns and looks up descriptions in (`GNCIMPORT_DESC`,
/// `5.17:gnucash/import-export/import-backend.cpp:51`).
pub const CATEGORY_DESC: &str = "desc";
/// The category the matcher learns memos in; nothing looks it up (`GNCIMPORT_MEMO`, `import-backend.cpp:52`).
pub const CATEGORY_MEMO: &str = "memo";
/// The category the CSV importer maps account names in, on the target account itself (`IMAP_CAT_CSV`,
/// `5.17:gnucash/import-export/csv-imp/gnc-imp-props-tx.hpp:80`).
pub const CATEGORY_CSV_ACCOUNT_MAP: &str = "csv-account-map";

/// One entry of an account's Import map, as `gnc_account_imap_get_info` and `gnc_account_imap_get_info_bayes` list
/// it (`GncImapInfo`, `5.17:libgnucash/engine/Account.h`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImapInfo {
    /// The account holding the entry.
    pub source_account: Guid,
    /// The account the entry points at, when its GUID names one in the Book.
    pub map_account: Option<Guid>,
    /// `import-map` for a plain entry; the whole flat key for a Bayesian one.
    pub head: String,
    /// The plain entry's category; `None` for a Bayesian one (GnuCash writes `" "`).
    pub category: Option<String>,
    /// The plain entry's key, or the Bayesian entry's token.
    pub match_string: String,
    /// The Bayesian entry's count; `None` for a plain one (GnuCash writes `" "`).
    pub count: Option<i64>,
}

/// The account, or `NotFound`.
pub(crate) fn account(book: &Book, guid: Guid) -> DomainResult<&Account> {
    book.account(guid).ok_or_else(|| DomainError::not_found("account", &guid))
}

/// The account's Slots, to change, or `NotFound`.
pub(crate) fn slots_mut(book: &mut Book, guid: Guid) -> DomainResult<&mut Frame> {
    book.accounts.get_mut(&guid).map(|a| &mut Arc::make_mut(a).slots).ok_or_else(|| DomainError::not_found("account", &guid))
}

/// `xaccAccountLookup` of a GUID value: the account it names, if it is in the Book.
fn lookup(book: &Book, value: Option<&SlotValue>) -> Option<Guid> {
    value.and_then(SlotValue::as_guid).filter(|g| book.account(*g).is_some())
}

fn plain_path<'a>(category: Option<&'a str>, key: &'a str) -> Vec<&'a str> {
    std::iter::once(IMAP_FRAME).chain(category).chain(std::iter::once(key)).collect()
}

/// The account `acc`'s plain map sends `key` to (`gnc_account_imap_find_account`,
/// `5.17:libgnucash/engine/Account.cpp:5155-5167`): `None` when there is no entry or its GUID names no account.
pub fn find_account(book: &Book, acc: Guid, category: Option<&str>, key: &str) -> Option<Guid> {
    lookup(book, book.account(acc)?.slots.get_in(&plain_path(category, key)))
}

/// The first account, in the sorted tree, whose own plain map holds `key` under `category`
/// (`gnc_account_imap_find_any`, `Account.cpp:5169-5192`): how the CSV importer resolves an account name through
/// `csv-account-map`, which each target account holds about itself.
pub fn find_any(book: &Book, category: Option<&str>, key: &str) -> Option<Guid> {
    descendants_sorted(book, book.root).into_iter().find(|a| find_account(book, a.guid, category, key).is_some()).map(|a| a.guid)
}

/// Map `key` under `category` to `added` in `acc`'s plain map, replacing what was there
/// (`gnc_account_imap_add_account`, `Account.cpp:5194-5206`). An empty key writes nothing; returns whether it wrote.
pub fn add_account(book: &mut Book, acc: Guid, category: Option<&str>, key: &str, added: Guid) -> DomainResult<bool> {
    account(book, added)?;
    account(book, acc)?;
    if key.is_empty() {
        return Ok(false);
    }
    slots_mut(book, acc)?.set_in(&plain_path(category, key), SlotValue::Guid(added));
    Ok(true)
}

/// Remove `key` from `acc`'s plain map, then the category frame and the `import-map` frame if that left them empty
/// (`gnc_account_imap_delete_account`, `Account.cpp:5208-5225`). Returns whether there was an entry.
pub fn delete_account(book: &mut Book, acc: Guid, category: Option<&str>, key: &str) -> DomainResult<bool> {
    account(book, acc)?;
    convert::check_import_map_data(book)?;
    let slots = slots_mut(book, acc)?;
    if slots.remove_in(&plain_path(category, key)).is_none() {
        return Ok(false);
    }
    if let Some(category) = category {
        slots.remove_in_if_empty(&[IMAP_FRAME, category]);
    }
    slots.remove_in_if_empty(&[IMAP_FRAME]);
    Ok(true)
}

/// The plain entries of `acc` under `category` (or directly under `import-map` with none), in key order
/// (`gnc_account_imap_get_info` and `build_non_bayes`, `Account.cpp:5610-5642`, `:5690-5715`). Only GUID values are
/// entries, as GnuCash takes only boxed values.
pub fn plain_info(book: &Book, acc: Guid, category: Option<&str>) -> Vec<ImapInfo> {
    let path: Vec<&str> = std::iter::once(IMAP_FRAME).chain(category).collect();
    let Some(frame) = book.account(acc).and_then(|a| a.slots.get_in(&path)).and_then(SlotValue::as_frame) else { return Vec::new() };
    frame
        .iter()
        .filter(|(_, v)| v.as_guid().is_some())
        .map(|(key, value)| ImapInfo {
            source_account: acc,
            map_account: lookup(book, Some(value)),
            head: IMAP_FRAME.to_owned(),
            category: category.map(str::to_owned),
            match_string: key.clone(),
            count: None,
        })
        .collect()
}

/// Delete the slot at `head`, then `category`, then `match_string` (each optional, each a whole key) as the import
/// map editor does (`gnc_account_delete_map_entry`, `Account.cpp:5735-5760`): with `empty`, only when it is an empty
/// frame. Returns whether the slot was there.
pub fn delete_map_entry(book: &mut Book, acc: Guid, head: &str, category: Option<&str>, match_string: Option<&str>, empty: bool) -> DomainResult<bool> {
    account(book, acc)?;
    convert::check_import_map_data(book)?;
    let path: Vec<&str> = std::iter::once(head).chain(category).chain(match_string).collect();
    let slots = slots_mut(book, acc)?;
    if slots.get_in(&path).is_none() {
        return Ok(false);
    }
    if empty {
        slots.remove_in_if_empty(&path);
    } else {
        slots.remove_in(&path);
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
