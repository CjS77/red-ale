//! Methods: the book's counters and their formats. See `design/api/business-contacts.md`, "Counters".

// `format` is three-state on the wire: absent (keep), null (remove) or a value.
#![allow(clippy::option_option)]

use red_ale_core::Book;
use red_ale_core::ops::business::counters::{
    DEFAULT_FORMAT, counter_format, counter_names, counter_value, normalize_counter_format, peek_next_id, set_counter,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::contacts::nullable;
use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::NoParams;

/// The counter methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "counter.list",
            "Every counter: the last number issued, its format and the next ID it would give.",
            Effect::Read,
            list,
        ),
        method(
            "counter.set",
            "Set a counter's last number issued or its format; a null or empty format means the default.",
            Effect::Write,
            set,
        ),
    ]
}

/// A counter.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CounterInfo {
    /// Its name in the book, such as `gncInvoice`.
    pub name: String,
    /// The last number issued; 0 when none.
    pub last: i64,
    /// The format as stored, if any.
    pub format: Option<String>,
    /// GnuCash accepts the stored format; when false it numbers with the default.
    pub format_valid: bool,
    /// The format GnuCash numbers with: the stored one normalised, else the default `%.6li`.
    pub normalized_format: String,
    /// The ID the next object would get. Reading it does not take it.
    pub next: String,
}

/// The result of `counter.list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CounterList {
    /// The book options' eight counters in their order, then any others the book holds, by name.
    pub counters: Vec<CounterInfo>,
}

/// Params of `counter.set`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetParams {
    /// One of `gncCustomer`, `gncEmployee`, `gncInvoice`, `gncBill`, `gncExpVoucher`, `gncJob`, `gncOrder`, `gncVendor`.
    pub name: String,
    /// The last number issued, from 0 to 999999999; the next ID uses one more.
    #[serde(default)]
    pub last: Option<i64>,
    /// A printf format with one `%li`, `%lli` or `%I64i` conversion; null or empty removes it.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<String>")]
    pub format: Option<Option<String>>,
}

fn info(book: &Book, name: &str) -> CounterInfo {
    let format = counter_format(book, name);
    let normalized = format.and_then(|f| normalize_counter_format(f).ok());
    CounterInfo {
        name: name.to_owned(),
        last: counter_value(book, name),
        format: format.map(ToOwned::to_owned),
        format_valid: normalized.is_some(),
        normalized_format: normalized.unwrap_or_else(|| DEFAULT_FORMAT.to_owned()),
        next: peek_next_id(book, name),
    }
}

fn list(ctx: &mut Context, _: NoParams) -> Result<CounterList, RpcError> {
    let book = ctx.book()?;
    Ok(CounterList {
        counters: counter_names(book).iter().map(|n| info(book, n)).collect(),
    })
}

fn set(ctx: &mut Context, p: SetParams) -> Result<CounterInfo, RpcError> {
    ctx.mutate(|book, _| {
        set_counter(book, &p.name, p.last, p.format.as_ref().map(Option::as_deref))?;
        Ok(info(book, &p.name))
    })
}
