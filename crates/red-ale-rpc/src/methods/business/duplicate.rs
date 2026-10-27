//! Method: duplicating a business document, as GnuCash's Duplicate does. The copy is
//! `red_ale_core::ops::business::duplicate`. See `design/api/business-documents.md`, "Duplicating a document".

use red_ale_core::ops::business::duplicate::{Duplicate, duplicate_invoice};
use schemars::JsonSchema;
use serde::Deserialize;

use super::contacts::local_today;
use super::documents::{InvoiceDetail, get_invoice};
use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{Day, Id};

/// The duplicate method.
pub fn methods() -> Vec<Method> {
    vec![method(
        "invoice.duplicate",
        "Copy a document and its lines into a new unposted document with a new number and date, as GnuCash's Duplicate does.",
        Effect::Write,
        duplicate,
    )]
}

/// Params of `invoice.duplicate`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DuplicateParams {
    /// The document to copy, posted or not.
    pub invoice: Id,
    /// The date of the copy and of each of its lines, stored at its day-neutral time. Defaults to today.
    #[serde(default)]
    pub date: Option<Day>,
    /// The copy's number. Absent or empty takes the next number from the counter of its kind.
    #[serde(default)]
    pub id: Option<String>,
}

fn duplicate(ctx: &mut Context, p: DuplicateParams) -> Result<InvoiceDetail, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let date = match p.date {
            Some(d) => d.0,
            None => local_today(env)?,
        };
        let d = Duplicate {
            source: p.invoice.0,
            id: p.id.unwrap_or_default(),
            date: env.zone().neutral_time(date),
        };
        let guid = duplicate_invoice(book, env, &d)?;
        get_invoice(book, guid)
    })
}
