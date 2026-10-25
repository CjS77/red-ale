//! `transaction.find`: the wire form of a find query and its translation to the core's.

use red_ale_core::Book;
use red_ale_core::ops::transactions::find::{self, AccountMatch, Compare, DateMatch, NumericMatch, Query, Sign, StringMatch, StringOp};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ReconcileFlag;
use crate::dispatch::Context;
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, Day, Id};

/// How text is compared. Matching ignores case unless `matchCase` is set.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TextOp {
    /// Contains the text (the default).
    #[default]
    Contains,
    /// Does not contain the text.
    NotContains,
    /// Equals the text.
    Equals,
    /// Does not equal the text.
    NotEquals,
    /// Matches a regular expression anywhere (Rust `regex` syntax, close to POSIX extended).
    MatchesRegex,
    /// Does not match a regular expression.
    NotMatchesRegex,
}

/// A text criterion.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TextCriterion {
    /// The comparison; `contains` by default.
    #[serde(default)]
    pub op: TextOp,
    /// The text or regular expression; not empty.
    pub text: String,
    /// Compare case-sensitively.
    #[serde(default)]
    pub match_case: bool,
}

/// How a date is compared, as the dialog offers it.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DateOp {
    /// Before the date (the default).
    #[default]
    Before,
    /// Before or on the date.
    OnOrBefore,
    /// On the date.
    On,
    /// Not on the date.
    NotOn,
    /// After the date.
    After,
    /// On or after the date.
    OnOrAfter,
}

/// A date criterion.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DateCriterion {
    /// The comparison; `before` by default.
    #[serde(default)]
    pub op: DateOp,
    /// The date.
    pub date: Day,
}

/// A numeric comparison.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NumberOp {
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Equal within 1/10000 (the default).
    #[default]
    Eq,
    /// Not equal within 1/10000.
    Ne,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
}

/// Which signs a numeric criterion accepts.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SignFilter {
    /// Any (the default).
    #[default]
    Any,
    /// Debits: zero or more.
    Debit,
    /// Credits: zero or less.
    Credit,
}

/// A numeric criterion. GnuCash compares the magnitude of the split's number with the amount.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NumberCriterion {
    /// The comparison; `eq` by default.
    #[serde(default)]
    pub op: NumberOp,
    /// The amount compared with.
    pub amount: Amount,
    /// Which signs pass; `any` by default.
    #[serde(default)]
    pub sign: SignFilter,
}

/// A reconcile-state criterion.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReconcileCriterion {
    /// The states.
    pub states: Vec<ReconcileFlag>,
    /// Match splits whose state is not one of them.
    #[serde(default)]
    pub not: bool,
}

/// How accounts match.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AccountsHow {
    /// The split's account is one of them (the default).
    #[default]
    Any,
    /// The split's account is none of them.
    None,
    /// The split's transaction has a split in every one of them (the dialog's "All Accounts").
    All,
}

/// An account criterion.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AccountCriterion {
    /// The accounts; at least one.
    pub accounts: Vec<AccountRef>,
    /// How they match; `any` by default.
    #[serde(default)]
    pub matching: AccountsHow,
}

/// A find query over splits: one criterion, or `all`, `any` or `not` of others. Written as an object with exactly
/// one key, such as `{"memo": {"text": "rent"}}` or `{"all": [{...}, {...}]}`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub enum FindQuery {
    /// Every query holds (the dialog's "all criteria are met"); an empty list matches every split.
    All(Vec<FindQuery>),
    /// Some query holds ("any criteria are met").
    Any(Vec<FindQuery>),
    /// The query does not hold.
    Not(Box<FindQuery>),
    /// Transaction description.
    Description(TextCriterion),
    /// Transaction notes.
    Notes(TextCriterion),
    /// Split memo.
    Memo(TextCriterion),
    /// Description, notes or memo: true when any of the three matches, also for negated operators, as in GnuCash.
    DescriptionNotesOrMemo(TextCriterion),
    /// Transaction number.
    Num(TextCriterion),
    /// Split action.
    Action(TextCriterion),
    /// Date posted.
    DatePosted(DateCriterion),
    /// Date entered.
    DateEntered(DateCriterion),
    /// Date reconciled.
    DateReconciled(DateCriterion),
    /// Split value.
    Value(NumberCriterion),
    /// Split amount (the dialog's "Shares").
    Amount(NumberCriterion),
    /// Share price: value / amount.
    SharePrice(NumberCriterion),
    /// Reconcile state.
    Reconcile(ReconcileCriterion),
    /// Book-closing entries (true) or not (false).
    Closing(bool),
    /// Balanced transactions (true) or not (false).
    Balanced(bool),
    /// Voided transactions (true) or not (false).
    Void(bool),
    /// Accounts.
    Accounts(AccountCriterion),
}

fn text(c: TextCriterion) -> Result<StringMatch, RpcError> {
    let op = match c.op {
        TextOp::Contains => StringOp::Contains,
        TextOp::NotContains => StringOp::NotContains,
        TextOp::Equals => StringOp::Equals,
        TextOp::NotEquals => StringOp::NotEquals,
        TextOp::MatchesRegex => StringOp::MatchesRegex,
        TextOp::NotMatchesRegex => StringOp::NotMatchesRegex,
    };
    Ok(StringMatch::new(op, &c.text, c.match_case)?)
}

fn date(c: &DateCriterion) -> DateMatch {
    let compare = match c.op {
        DateOp::Before => Compare::Less,
        DateOp::OnOrBefore => Compare::LessOrEqual,
        DateOp::On => Compare::Equal,
        DateOp::NotOn => Compare::NotEqual,
        DateOp::After => Compare::Greater,
        DateOp::OnOrAfter => Compare::GreaterOrEqual,
    };
    DateMatch { compare, date: c.date.0 }
}

fn number(c: &NumberCriterion) -> NumericMatch {
    let compare = match c.op {
        NumberOp::Lt => Compare::Less,
        NumberOp::Le => Compare::LessOrEqual,
        NumberOp::Eq => Compare::Equal,
        NumberOp::Ne => Compare::NotEqual,
        NumberOp::Gt => Compare::Greater,
        NumberOp::Ge => Compare::GreaterOrEqual,
    };
    let sign = match c.sign {
        SignFilter::Any => Sign::Any,
        SignFilter::Debit => Sign::Debit,
        SignFilter::Credit => Sign::Credit,
    };
    NumericMatch { compare, amount: c.amount.0, sign }
}

impl FindQuery {
    /// The core query, with accounts resolved and regular expressions compiled.
    pub fn resolve(self, book: &Book) -> Result<Query, RpcError> {
        let many = |qs: Vec<FindQuery>| qs.into_iter().map(|q| q.resolve(book)).collect::<Result<Vec<_>, _>>();
        Ok(match self {
            Self::All(qs) => Query::All(many(qs)?),
            Self::Any(qs) => Query::Any(many(qs)?),
            Self::Not(q) => Query::Not(Box::new(q.resolve(book)?)),
            Self::Description(c) => Query::Description(text(c)?),
            Self::Notes(c) => Query::Notes(text(c)?),
            Self::Memo(c) => Query::Memo(text(c)?),
            Self::DescriptionNotesOrMemo(c) => Query::DescriptionNotesOrMemo(text(c)?),
            Self::Num(c) => Query::Num(text(c)?),
            Self::Action(c) => Query::Action(text(c)?),
            Self::DatePosted(c) => Query::DatePosted(date(&c)),
            Self::DateEntered(c) => Query::DateEntered(date(&c)),
            Self::DateReconciled(c) => Query::DateReconciled(date(&c)),
            Self::Value(c) => Query::Value(number(&c)),
            Self::Amount(c) => Query::Amount(number(&c)),
            Self::SharePrice(c) => Query::SharePrice(number(&c)),
            Self::Reconcile(c) => Query::Reconcile { states: c.states.into_iter().map(Into::into).collect(), negate: c.not },
            Self::Closing(b) => Query::Closing(b),
            Self::Balanced(b) => Query::Balanced(b),
            Self::Void(b) => Query::Void(b),
            Self::Accounts(c) => Query::Accounts {
                how: match c.matching {
                    AccountsHow::Any => AccountMatch::Any,
                    AccountsHow::None => AccountMatch::None,
                    AccountsHow::All => AccountMatch::All,
                },
                accounts: c.accounts.iter().map(|a| a.resolve(book)).collect::<Result<_, _>>()?,
            },
        })
    }
}

/// Params of `transaction.find`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FindParams {
    /// The query.
    pub query: FindQuery,
    /// Keep only the last this many matches, as GnuCash keeps them when a limit applies.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// A split found.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Match {
    /// The transaction.
    pub transaction: Id,
    /// The split.
    pub split: Id,
    /// The split's account.
    pub account: Option<Id>,
}

/// The result of `transaction.find`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FindResult {
    /// The splits found, in GnuCash's split order (`xaccSplitOrder`): by transaction (posted date, num, date entered,
    /// description), then memo, action, reconcile state, amount, value.
    pub matches: Vec<Match>,
}

pub(super) fn find_(ctx: &mut Context, p: FindParams) -> Result<FindResult, RpcError> {
    let book = ctx.book()?;
    let query = p.query.resolve(book)?;
    let hits = find::find(book, &query, p.limit)?;
    Ok(FindResult { matches: super::found(book, &hits) })
}
