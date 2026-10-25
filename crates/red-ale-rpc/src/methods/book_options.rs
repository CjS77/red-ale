//! Methods: the Book options red-ale reads, sets and follows: the num field source and the auto read-only threshold
//! (`design/api/book-options.md`), and trading accounts (`design/api/trading-accounts.md`). The rules live in
//! `red_ale_core::book_options`.

use red_ale_core::book_options::{self as opts, MAX_AUTO_READONLY_DAYS};
use red_ale_core::{Book, Env};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::methods::transactions::today;
use crate::wire::{Day, NoParams};

/// The book options methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "book.options",
            "The book's num field source, auto read-only threshold (with today's threshold date) and trading accounts option.",
            Effect::Read,
            get,
        ),
        method(
            "book.setOptions",
            "Set the num field source, the auto read-only threshold or trading accounts, as GnuCash's book options dialog does.",
            Effect::Write,
            set,
        ),
    ]
}

/// The book options, as `book.options` and `book.setOptions` return them.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BookOptions {
    /// "Use Split Action Field for Number": a split's Number is its action, and the transaction's num is the T-Num.
    pub split_action_for_num: bool,
    /// "Day Threshold for Read-Only Transactions (red line)": 0 when off.
    pub auto_read_only_days: i64,
    /// Today minus the threshold's days: transactions posted before it are read-only. Absent when the threshold is off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_threshold: Option<Day>,
    /// "Use Trading Accounts": every commit keeps the trading splits of multi-commodity transactions.
    pub use_trading_accounts: bool,
}

/// Params of `book.setOptions`; an absent option is left as it is.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetParams {
    /// Use the split action as the Number. Turning it on also sets the Book feature `Number Field Source`.
    #[serde(default)]
    pub split_action_for_num: Option<bool>,
    /// Days after which transactions become read-only: 0 (off) to 3650.
    #[serde(default)]
    #[schemars(range(min = 0, max = MAX_AUTO_READONLY_DAYS))]
    pub auto_read_only_days: Option<i64>,
    /// Balance multi-commodity transactions through trading accounts. No existing transaction changes.
    #[serde(default)]
    pub use_trading_accounts: Option<bool>,
}

fn describe(book: &Book, env: &mut dyn Env) -> Result<BookOptions, RpcError> {
    let today = today(env)?;
    Ok(BookOptions {
        split_action_for_num: opts::uses_split_action_for_num(book),
        auto_read_only_days: opts::auto_readonly_days(book),
        read_only_threshold: opts::auto_readonly_threshold(book, today).map(Day),
        use_trading_accounts: opts::uses_trading_accounts(book),
    })
}

fn get(ctx: &mut Context, _: NoParams) -> Result<BookOptions, RpcError> {
    ctx.query(|book, env| describe(book, env))
}

fn set(ctx: &mut Context, p: SetParams) -> Result<BookOptions, RpcError> {
    ctx.mutate_rpc(|book, env| {
        if let Some(on) = p.split_action_for_num {
            opts::set_split_action_for_num(book, on);
        }
        if let Some(days) = p.auto_read_only_days {
            opts::set_auto_readonly_days(book, days)?;
        }
        if let Some(on) = p.use_trading_accounts {
            opts::set_trading_accounts(book, on);
        }
        describe(book, env)
    })
}
