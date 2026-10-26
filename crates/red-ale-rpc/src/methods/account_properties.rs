//! Methods: balance limit checks, the opening balance account lookup and cascading account properties. The
//! properties themselves are fields of `account.create`, `account.update` and `AccountInfo`. See
//! `design/api/accounts.md`, "Account properties".

use red_ale_core::ops::account_properties::{self as ops, Cascade, FoundBy, LimitState};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::accounts::{AccountInfo, info};
use super::business::contacts::local_today;
use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, CommodityId, Day, Id};

/// The account property methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "account.checkBalanceLimit",
            "Compare an account's balance with its balance limits, as GnuCash's account tree does to warn.",
            Effect::Read,
            check_balance_limit,
        ),
        method(
            "account.openingBalanceAccount",
            "Find the account GnuCash would put a currency's opening balances in: by its flag, then by name.",
            Effect::Read,
            opening_balance_account,
        ),
        method("account.cascade", "Set colour, placeholder or hidden on an account and all its sub-accounts.", Effect::Write, cascade),
    ]
}

/// Params of `account.checkBalanceLimit`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CheckParams {
    /// The account.
    pub account: AccountRef,
    /// The balance at the end of this day, local time; by default today, as GnuCash checks it.
    #[serde(default)]
    pub as_of: Option<Day>,
}

/// The limit a balance has crossed.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LimitStateWire {
    /// Above the higher limit.
    Above,
    /// Below the lower limit.
    Below,
    /// Both limits are zero and the balance is not.
    ShouldBeZero,
}

impl From<LimitState> for LimitStateWire {
    fn from(s: LimitState) -> Self {
        match s {
            LimitState::Above => Self::Above,
            LimitState::Below => Self::Below,
            LimitState::ShouldBeZero => Self::ShouldBeZero,
        }
    }
}

/// Sub-account balances in another commodity with no price to the account's, counted as zero as GnuCash counts them.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Unconverted {
    /// The commodity.
    pub commodity: CommodityId,
    /// The sub-accounts' balances in it.
    pub balance: Amount,
}

/// The result of `account.checkBalanceLimit`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    /// The account.
    pub account: Id,
    /// The day the balance is taken at the end of.
    pub as_of: Day,
    /// The balance compared, in raw sign, with sub-accounts when the account's flag is set: those in other commodities
    /// converted at the latest price and rounded to the account commodity's fraction.
    pub balance: Amount,
    /// The higher limit, if set.
    pub higher_limit: Option<Amount>,
    /// The lower limit, if set.
    pub lower_limit: Option<Amount>,
    /// Sub-accounts are included.
    pub includes_subaccounts: bool,
    /// The limit crossed: `above`, `below`, `shouldBeZero`; null when none is.
    pub state: Option<LimitStateWire>,
    /// Sub-accounts in other commodities that no price converts, by commodity; they add nothing to `balance`.
    pub unconverted: Vec<Unconverted>,
}

fn check_balance_limit(ctx: &mut Context, p: CheckParams) -> Result<CheckResult, RpcError> {
    ctx.query(|book, env| {
        let guid = p.account.resolve(book)?;
        let day = match p.as_of {
            Some(d) => d.0,
            None => local_today(env)?,
        };
        let c = ops::check_balance_limit(book, guid, Some(env.zone().day_end(day)), env.now())?;
        Ok(CheckResult {
            account: guid.into(),
            as_of: Day(day),
            balance: c.balance.into(),
            higher_limit: c.higher.map(Amount),
            lower_limit: c.lower.map(Amount),
            includes_subaccounts: c.includes_subaccounts,
            state: c.state.map(Into::into),
            unconverted: c.unconverted.into_iter().map(|(cm, b)| Unconverted { commodity: CommodityId(cm), balance: b.into() }).collect(),
        })
    })
}

/// Params of `account.openingBalanceAccount`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OpeningBalanceParams {
    /// The currency.
    pub currency: CommodityId,
}

/// How the opening balance account was found.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FoundByWire {
    /// Its `equity-type` slot (only with the Book feature).
    EquityType,
    /// The name `Opening Balances`.
    Name,
    /// The name `Opening Balances - <MNEMONIC>`.
    NameWithCurrency,
}

impl From<FoundBy> for FoundByWire {
    fn from(f: FoundBy) -> Self {
        match f {
            FoundBy::EquityType => Self::EquityType,
            FoundBy::Name => Self::Name,
            FoundBy::NameWithCurrency => Self::NameWithCurrency,
        }
    }
}

/// The result of `account.openingBalanceAccount`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpeningBalanceResult {
    /// The account; null when GnuCash would create one.
    pub account: Option<AccountInfo>,
    /// How it was found.
    pub found_by: Option<FoundByWire>,
    /// The book declares GnuCash's opening balance account Book feature.
    pub feature_used: bool,
}

fn opening_balance_account(ctx: &mut Context, p: OpeningBalanceParams) -> Result<OpeningBalanceResult, RpcError> {
    let book = ctx.book()?;
    let currency = red_ale_core::ops::commodities::require(book, &p.currency.0)?;
    let found = ops::opening_balance_account(book, &currency)?;
    Ok(OpeningBalanceResult {
        account: found.map(|(g, _)| info(book, g)).transpose()?,
        found_by: found.map(|(_, f)| f.into()),
        feature_used: ops::uses_opening_balance_feature(book),
    })
}

/// Params of `account.cascade`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CascadeParams {
    /// The account; it must have sub-accounts.
    pub account: AccountRef,
    /// Cascade this colour (`#rrggbb`, `#rgb` or `rgb(r,g,b)`); empty cascades no colour.
    #[serde(default)]
    pub color: Option<String>,
    /// Replace colours already set (GnuCash's "Replace any existing account colors"); without it only accounts
    /// with no colour change.
    #[serde(default)]
    pub replace_colors: bool,
    /// Cascade the placeholder flag.
    #[serde(default)]
    pub placeholder: Option<bool>,
    /// Cascade the hidden flag.
    #[serde(default)]
    pub hidden: Option<bool>,
}

/// The result of `account.cascade`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CascadeResult {
    /// The accounts that changed: the account first, then its sub-accounts depth first.
    pub changed: Vec<Id>,
}

fn cascade(ctx: &mut Context, p: CascadeParams) -> Result<CascadeResult, RpcError> {
    let c = Cascade { color: p.color, replace_colors: p.replace_colors, placeholder: p.placeholder, hidden: p.hidden };
    ctx.mutate_rpc(|book, _| {
        let guid = p.account.resolve(book)?;
        Ok(CascadeResult { changed: ops::cascade(book, guid, &c)?.into_iter().map(Id).collect() })
    })
}
