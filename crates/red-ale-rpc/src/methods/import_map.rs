//! Methods: the import map editor (`design/api/import.md`, "`importMap.list`", "`importMap.delete`"), GnuCash's
//! `dialog-imap-editor.c` without its window. The listing and deleting are `red_ale_import::import_map::editor`'s.

use red_ale_core::{Book, Guid};
use red_ale_import::import_map::editor::{self, DeleteRequest, ListFilter, MapEntry, MapKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Id};

/// The import map editor's methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "importMap.list",
            "List Import map entries (Bayesian, desc, memo, csv-account-map) and online entries.",
            Effect::Read,
            list,
        ),
        method(
            "importMap.delete",
            "Delete Import map or online entries, one by one, by kind, or every invalid one.",
            Effect::Destructive,
            delete,
        ),
    ]
}

/// The kind of an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum KindWire {
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

impl From<KindWire> for MapKind {
    fn from(k: KindWire) -> Self {
        match k {
            KindWire::Bayes => Self::Bayes,
            KindWire::Desc => Self::Desc,
            KindWire::Memo => Self::Memo,
            KindWire::CsvAccountMap => Self::CsvAccountMap,
            KindWire::OnlineId => Self::OnlineId,
            KindWire::Hbci => Self::Hbci,
            KindWire::OfxIncomeAccount => Self::OfxIncomeAccount,
        }
    }
}

impl From<MapKind> for KindWire {
    fn from(k: MapKind) -> Self {
        match k {
            MapKind::Bayes => Self::Bayes,
            MapKind::Desc => Self::Desc,
            MapKind::Memo => Self::Memo,
            MapKind::CsvAccountMap => Self::CsvAccountMap,
            MapKind::OnlineId => Self::OnlineId,
            MapKind::Hbci => Self::Hbci,
            MapKind::OfxIncomeAccount => Self::OfxIncomeAccount,
        }
    }
}

/// Params of `importMap.list`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListParams {
    /// Only the entries this account holds (for `csvAccountMap`, the target, which holds it).
    #[serde(default)]
    pub account: Option<AccountRef>,
    /// Only this kind.
    #[serde(default)]
    pub kind: Option<KindWire>,
    /// Only the entries the editor's "Delete invalid mappings" would remove.
    #[serde(default)]
    pub invalid_only: bool,
}

/// An account, by GUID and full name.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountLabel {
    /// Its GUID.
    pub guid: Id,
    /// Its full name.
    pub full_name: String,
}

/// One entry.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ImportMapEntry {
    /// The account holding it.
    pub account: AccountLabel,
    /// Its kind.
    pub kind: KindWire,
    /// The token (`bayes`), the text (`desc`, `memo`, `csvAccountMap`), or the online ID (`bank-code,account-id`
    /// for `hbci`).
    pub key: String,
    /// The account it points at, or null when its GUID names no account. Null for `onlineId` and `hbci`.
    pub target: Option<AccountLabel>,
    /// For `bayes`: the count.
    pub count: Option<i64>,
    /// False when the editor counts it invalid: its target doesn't resolve, or its online ID is empty.
    pub valid: bool,
}

/// The entries.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ListResult {
    /// By holding account's full name, then kind, then key, bytewise.
    pub entries: Vec<ImportMapEntry>,
}

fn label(book: &Book, guid: Guid) -> AccountLabel {
    AccountLabel {
        guid: Id(guid),
        full_name: book.full_name(guid).unwrap_or_default(),
    }
}

fn entry(book: &Book, e: &MapEntry) -> ImportMapEntry {
    let target = e
        .target
        .filter(|_| !matches!(e.kind, MapKind::OnlineId | MapKind::Hbci))
        .map(|t| label(book, t));
    ImportMapEntry {
        account: label(book, e.account),
        kind: e.kind.into(),
        key: e.key.clone(),
        target,
        count: e.count,
        valid: e.valid,
    }
}

fn list(ctx: &mut Context, p: ListParams) -> Result<ListResult, RpcError> {
    let book = ctx.book()?;
    let filter = ListFilter {
        account: p.account.map(|a| a.resolve(book)).transpose()?,
        kind: p.kind.map(MapKind::from),
        invalid_only: p.invalid_only,
    };
    Ok(ListResult {
        entries: editor::list(book, filter).iter().map(|e| entry(book, e)).collect(),
    })
}

/// One deletion.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EntryParam {
    /// The account holding the entries.
    pub account: AccountRef,
    /// Their kind.
    pub kind: KindWire,
    /// The key; absent for every entry of the kind on the account (the editor's parent row).
    #[serde(default)]
    pub key: Option<String>,
    /// For `bayes`: only the token's entry for this target (a GUID, which need not name an account any more);
    /// absent for every target's.
    #[serde(default)]
    pub target: Option<AccountRef>,
}

/// Params of `importMap.delete`: `entries` or `invalid`, not both.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeleteParams {
    /// The entries to delete.
    #[serde(default)]
    pub entries: Option<Vec<EntryParam>>,
    /// Instead of `entries`: every invalid entry in the book, as the editor's "Delete invalid mappings".
    #[serde(default)]
    pub invalid: bool,
}

/// The entries deleted.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DeleteResult {
    /// Each as it was listed.
    pub deleted: Vec<ImportMapEntry>,
}

/// A GUID given for an account that may no longer exist, such as an entry's target; else a full name.
fn guid_or_account(book: &Book, r: &AccountRef) -> Result<Guid, RpcError> {
    Guid::parse(r.0.trim()).or_else(|_| r.resolve(book))
}

fn request(book: &Book, e: &EntryParam) -> Result<DeleteRequest, RpcError> {
    Ok(DeleteRequest {
        account: e.account.resolve(book)?,
        kind: e.kind.into(),
        key: e.key.clone(),
        target: e.target.as_ref().map(|t| guid_or_account(book, t)).transpose()?,
    })
}

fn delete(ctx: &mut Context, p: DeleteParams) -> Result<DeleteResult, RpcError> {
    let requests = match (&p.entries, p.invalid) {
        (Some(entries), false) => Some(
            entries
                .iter()
                .map(|e| request(ctx.book()?, e))
                .collect::<Result<Vec<_>, RpcError>>()?,
        ),
        (None, true) => None,
        _ => return Err(RpcError::invalid("give entries or invalid: true, not both or neither")),
    };
    ctx.mutate_rpc(|book, _| {
        let deleted = match &requests {
            Some(requests) => editor::delete(book, requests)?,
            None => editor::delete_invalid(book)?,
        };
        Ok(DeleteResult {
            deleted: deleted.iter().map(|e| entry(book, e)).collect(),
        })
    })
}
