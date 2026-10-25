//! Methods: closing the books (GnuCash's Close Book). See `design/api/close-book.md`.

use red_ale_core::ops::close_book::{CloseBooks, CloseReport, close_books};
use red_ale_core::AccountType;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, CommodityId, Day, Id};

/// The close book methods.
pub fn methods() -> Vec<Method> {
    vec![method(
        "book.closeBooks",
        "Close the books: move income and expense balances into equity at a closing date, one closing transaction per currency. \
         Not book.close.",
        Effect::Write,
        close,
    )]
}

/// Params of `book.closeBooks`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CloseBooksParams {
    /// The closing date; balances at the end of this day, local time, are closed.
    pub date: Day,
    /// The equity account taking the period's income (GnuCash's "Income Total").
    pub income_total_account: AccountRef,
    /// The equity account taking the period's expense (GnuCash's "Expense Total"); may be the same account.
    pub expense_total_account: AccountRef,
    /// The closing transactions' description; empty by default, as in GnuCash's dialog.
    #[serde(default)]
    pub description: String,
}

/// Whether a closing transaction closes income or expense.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ClosingKind {
    /// Income accounts.
    Income,
    /// Expense accounts.
    Expense,
}

/// One closing transaction made.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClosingInfo {
    /// The transaction.
    pub guid: Id,
    /// The type of the accounts it closes.
    pub kind: ClosingKind,
    /// Its currency.
    pub currency: CommodityId,
    /// The equity account its balancing split posts to.
    pub equity_account: Id,
    /// The equity split's value: the sum of the closed balances.
    pub total: Amount,
}

/// The result of `book.closeBooks`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CloseResult {
    /// The closing transactions, income first.
    pub transactions: Vec<ClosingInfo>,
    /// Equity accounts made for a currency.
    pub created_accounts: Vec<Id>,
}

fn result(report: CloseReport) -> CloseResult {
    CloseResult {
        transactions: report
            .transactions
            .into_iter()
            .map(|t| ClosingInfo {
                guid: t.guid.into(),
                kind: if t.kind == AccountType::Income { ClosingKind::Income } else { ClosingKind::Expense },
                currency: t.currency.into(),
                equity_account: t.equity_account.into(),
                total: t.total.into(),
            })
            .collect(),
        created_accounts: report.created_accounts.into_iter().map(Id).collect(),
    }
}

fn close(ctx: &mut Context, p: CloseBooksParams) -> Result<CloseResult, RpcError> {
    ctx.mutate_rpc(|book, env| {
        let req = CloseBooks {
            date: p.date.0,
            // The last second of the day in the local time zone (`gnc_time64_get_day_end`,
            // `5.17:libgnucash/engine/gnc-date.cpp:1386-1394`).
            day_end: env.zone().day_end(p.date.0),
            income_total: p.income_total_account.resolve(book)?,
            expense_total: p.expense_total_account.resolve(book)?,
            description: p.description,
        };
        Ok(result(close_books(book, env, &req)?))
    })
}
