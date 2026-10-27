//! Methods: document links. Listing the transactions and business documents that have one, as GnuCash's Transaction
//! and Business Document Links dialogs do, and the path head relative links start from, a red-ale setting rather than
//! book data (GnuCash keeps it in user preferences). See `design/api/transactions.md`, "Document links".

use red_ale_core::ops::doc_links::{doc_link, is_relative, legacy_transaction_link, normalise_path_head, resolve};
use red_ale_core::ops::transactions::{self as txns, ListFilter, posted_date};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::business::documents::{DocumentKindWire, type_wire};
use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{Day, Id, NoParams, Timestamp};

/// The document link methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "docLink.transactions",
            "Transactions with a document link, with the URI each one opens.",
            Effect::Read,
            transactions,
        ),
        method(
            "docLink.documents",
            "Invoices, bills and vouchers with a document link, with the URI each one opens.",
            Effect::Read,
            documents,
        ),
        method(
            "docLink.pathHead",
            "The configured folder relative document links start from.",
            Effect::Read,
            path_head,
        ),
        method(
            "docLink.setPathHead",
            "Set or remove the folder relative document links start from, in red-ale's configuration.",
            Effect::Write,
            set_path_head,
        )
        .on_configuration(),
    ]
}

/// A transaction's document link.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransactionLink {
    /// The transaction.
    pub transaction: Id,
    /// Its posted date.
    pub date_posted: Option<Day>,
    /// Its description.
    pub description: Option<String>,
    /// The link as stored.
    pub doc_link: String,
    /// The link is a path relative to the path head.
    pub relative: bool,
    /// The URI to open; `null` for a relative link when no path head is configured.
    pub uri: Option<String>,
}

/// A business document's document link.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLink {
    /// The document.
    pub invoice: Id,
    /// Its number.
    pub id: String,
    /// Its kind.
    #[serde(rename = "type")]
    pub kind: Option<DocumentKindWire>,
    /// When it was opened.
    pub date_opened: Timestamp,
    /// The link as stored.
    pub doc_link: String,
    /// The link is a path relative to the path head.
    pub relative: bool,
    /// The URI to open; `null` for a relative link when no path head is configured.
    pub uri: Option<String>,
}

/// The links of one kind, with the path head they were resolved against.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Links<T> {
    /// The configured path head, if any.
    pub path_head: Option<String>,
    /// The links.
    pub links: Vec<T>,
}

fn configured_head(ctx: &Context) -> Result<Option<String>, RpcError> {
    Ok(ctx.settings.config.load()?.doc_link_path_head()?.map(str::to_owned))
}

fn transactions(ctx: &mut Context, _: NoParams) -> Result<Links<TransactionLink>, RpcError> {
    let head = configured_head(ctx)?;
    let book = ctx.book()?;
    let links = txns::list(
        book,
        &ListFilter {
            account: None,
            from: None,
            to: None,
        },
    )
    .into_iter()
    .filter_map(|t| doc_link(&t.slots).filter(|l| !l.is_empty()).map(|l| (t, l)))
    .map(|(t, stored)| {
        let link = legacy_transaction_link(stored);
        TransactionLink {
            transaction: t.guid.into(),
            date_posted: posted_date(t).map(Day),
            description: t.description.clone(),
            doc_link: stored.to_owned(),
            relative: is_relative(link),
            uri: resolve(link, head.as_deref()),
        }
    })
    .collect();
    Ok(Links { path_head: head, links })
}

fn documents(ctx: &mut Context, _: NoParams) -> Result<Links<DocumentLink>, RpcError> {
    let head = configured_head(ctx)?;
    let book = ctx.book()?;
    let links = book
        .business
        .invoices
        .values()
        .filter_map(|inv| doc_link(&inv.slots).filter(|l| !l.is_empty()).map(|l| (inv, l)))
        .map(|(inv, link)| DocumentLink {
            invoice: inv.guid.into(),
            id: inv.id.clone(),
            kind: type_wire(book, inv),
            date_opened: inv.opened.into(),
            doc_link: link.to_owned(),
            relative: is_relative(link),
            uri: resolve(link, head.as_deref()),
        })
        .collect();
    Ok(Links { path_head: head, links })
}

/// The path head setting.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PathHeadInfo {
    /// The path head, a `file://` URI ending in `/`, if configured.
    pub path_head: Option<String>,
    /// The configuration file, if the setting is kept in one; otherwise it lasts as long as the server.
    pub config_file: Option<String>,
}

fn describe(ctx: &Context) -> Result<PathHeadInfo, RpcError> {
    Ok(PathHeadInfo {
        path_head: configured_head(ctx)?,
        config_file: ctx.settings.config.path().map(|p| p.display().to_string()),
    })
}

fn path_head(ctx: &mut Context, _: NoParams) -> Result<PathHeadInfo, RpcError> {
    describe(ctx)
}

/// Params of `docLink.setPathHead`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetPathHeadParams {
    /// An absolute path or a `file://` URI; a trailing `/` is added. Absent or `null` removes the setting.
    #[serde(default)]
    pub path_head: Option<String>,
}

fn set_path_head(ctx: &mut Context, p: SetPathHeadParams) -> Result<PathHeadInfo, RpcError> {
    let head = p.path_head.as_deref().map(normalise_path_head).transpose()?;
    let mut config = ctx.settings.config.load()?;
    config.set_doc_link_path_head(head.as_deref());
    ctx.settings.config.save(&config)?;
    describe(ctx)
}
