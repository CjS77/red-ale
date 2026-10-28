//! `scheduledTransaction.fromTransaction` and `scheduledTransaction.loan`: GnuCash's Schedule from transaction dialog
//! and mortgage and loan assistant, each one call (`design/api/scheduled-transactions.md`, "Schedule from
//! transaction" and "The loan assistant").

use red_ale_core::Book;
use red_ale_core::ops::scheduled::{
    self as ops, End, Frequency, FromTransaction, Length, Loan, OtherPayment, PaymentKind, RateType, Repayment,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ScheduledInfo, edited, format};
use crate::dispatch::Context;
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, Day, EndWire, Id, PeriodTypeWire, WeekendAdjustWire};

/// The dialog's frequencies (`5.17:gnucash/gnome/dialog-sx-from-trans.cpp:70-77`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FrequencyWire {
    /// Every day.
    #[default]
    Daily,
    /// Every week.
    Weekly,
    /// Every two weeks.
    BiWeekly,
    /// Every month.
    Monthly,
    /// Every three months.
    Quarterly,
    /// Every twelve months (month ×12, as GnuCash makes it).
    Yearly,
}

/// Params of `scheduledTransaction.fromTransaction`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FromTransactionParams {
    /// The transaction to copy; it is not changed.
    pub transaction: Id,
    /// The name; the transaction's description by default.
    #[serde(default)]
    pub name: Option<String>,
    /// How often; daily by default.
    #[serde(default)]
    pub frequency: FrequencyWire,
    /// The first occurrence; the frequency's first after the transaction's date by default.
    #[serde(default)]
    pub start_date: Option<Day>,
    /// The end; never by default.
    #[serde(default)]
    pub end: Option<EndWire>,
    /// Create due instances without asking; false by default.
    #[serde(default)]
    pub auto_create: Option<bool>,
    /// And say so; true with `autoCreate` by default.
    #[serde(default)]
    pub notify: Option<bool>,
    /// Days ahead to create; 0 by default.
    #[serde(default)]
    #[schemars(range(min = 0))]
    pub advance_create_days: Option<i32>,
    /// Days ahead to remind; 0 by default.
    #[serde(default)]
    #[schemars(range(min = 0))]
    pub advance_remind_days: Option<i32>,
}

pub(super) fn from_transaction(ctx: &mut Context, p: FromTransactionParams) -> Result<ScheduledInfo, RpcError> {
    let format = format(ctx)?;
    let request = FromTransaction {
        transaction: p.transaction.0,
        name: p.name,
        frequency: match p.frequency {
            FrequencyWire::Daily => Frequency::Daily,
            FrequencyWire::Weekly => Frequency::Weekly,
            FrequencyWire::BiWeekly => Frequency::BiWeekly,
            FrequencyWire::Monthly => Frequency::Monthly,
            FrequencyWire::Quarterly => Frequency::Quarterly,
            FrequencyWire::Yearly => Frequency::Yearly,
        },
        start: p.start_date.map(|d| d.0),
        end: p.end.map(EndWire::to_core).transpose()?.unwrap_or(End::Never),
        auto_create: p.auto_create,
        notify: p.notify,
        advance_create_days: p.advance_create_days,
        advance_remind_days: p.advance_remind_days,
    };
    ctx.mutate_rpc(|book, env| {
        let e = ops::from_transaction(book, env, request, format)?;
        edited(book, &e, format)
    })
}

/// The interest rate's kind (`IRateType`, `5.17:gnucash/gnome/assistant-loan.cpp:166-175`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RateTypeWire {
    /// "Interest Rate".
    #[default]
    Simple,
    /// APR compounded daily.
    AprDaily,
    /// APR compounded weekly.
    AprWeekly,
    /// APR compounded monthly.
    AprMonthly,
    /// APR compounded quarterly.
    AprQuarterly,
    /// APR compounded twice a year.
    AprSemiAnnually,
    /// APR compounded once a year.
    AprAnnually,
}

/// A length in months or in years, 1 to 9999.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LengthParam {
    /// In months.
    #[serde(default)]
    pub months: Option<u32>,
    /// Or in years.
    #[serde(default)]
    pub years: Option<u32>,
}

/// One Recurrence's step without a start: the repayment frequency.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FrequencyParam {
    /// How many steps apart.
    pub mult: u16,
    /// The step; not `once`.
    pub period_type: PeriodTypeWire,
    /// Where repayments on a weekend move to; `none` by default.
    #[serde(default)]
    pub weekend_adjust: WeekendAdjustWire,
}

/// The repayment page.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RepaymentParam {
    /// Paid from: BANK, CASH, CREDIT, ASSET or LIABILITY.
    pub from: AccountRef,
    /// The principal goes to; the loan account by default.
    #[serde(default)]
    pub principal_to: Option<AccountRef>,
    /// The interest goes to: those types or EXPENSE.
    pub interest_to: AccountRef,
    /// The first repayment; the loan's start by default.
    #[serde(default)]
    pub start: Option<Day>,
    /// How often; monthly by default.
    #[serde(default)]
    pub frequency: Option<FrequencyParam>,
}

/// GnuCash's four other payments.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PaymentKindWire {
    /// Taxes (`Tax Payment`).
    Taxes,
    /// Insurance (`Insurance Payment`).
    Insurance,
    /// PMI (`PMI Payment`).
    Pmi,
    /// Another expense (`Miscellaneous Payment`).
    Other,
}

/// An other payment made with each repayment.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OtherPaymentParam {
    /// Which.
    pub kind: PaymentKindWire,
    /// The memo; the kind's by default.
    #[serde(default)]
    pub memo: Option<String>,
    /// The amount, written exactly.
    pub amount: Amount,
    /// Paid from; the repayment's account by default.
    #[serde(default)]
    pub from: Option<AccountRef>,
    /// Paid to.
    pub to: AccountRef,
    /// Through the escrow account; by default yes for all but `other` when there is one. Refused without one.
    #[serde(default)]
    pub via_escrow: Option<bool>,
}

/// Params of `scheduledTransaction.loan`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LoanParams {
    /// The repayment's name; `Loan` by default.
    #[serde(default)]
    pub name: Option<String>,
    /// The loan, a LIABILITY account.
    pub loan_account: AccountRef,
    /// The principal, more than 0, in the loan account's commodity.
    pub principal: Amount,
    /// The annual rate in percent, 0.001 to 100 with at most 3 decimals.
    pub rate: Amount,
    /// The rate's kind; `simple` ("Interest Rate") by default.
    #[serde(default)]
    pub rate_type: RateTypeWire,
    /// The loan's start; today by default.
    #[serde(default)]
    pub start_date: Option<Day>,
    /// The loan's length.
    pub length: LengthParam,
    /// Months still to pay; by default the length less the months from the start to today.
    #[serde(default)]
    pub months_remaining: Option<u32>,
    /// The repayment.
    pub repayment: RepaymentParam,
    /// An escrow account the repayment goes through.
    #[serde(default)]
    pub escrow: Option<AccountRef>,
    /// Other payments made as part of each repayment.
    #[serde(default)]
    pub other_payments: Vec<OtherPaymentParam>,
}

/// The result of `scheduledTransaction.loan`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanResult {
    /// The repayment scheduled transaction.
    pub scheduled_transaction: ScheduledInfo,
    /// The `pmt` formula's value, to 12 significant figures.
    pub payment: Amount,
}

fn rate_type(r: RateTypeWire) -> RateType {
    match r {
        RateTypeWire::Simple => RateType::Simple,
        RateTypeWire::AprDaily => RateType::AprDaily,
        RateTypeWire::AprWeekly => RateType::AprWeekly,
        RateTypeWire::AprMonthly => RateType::AprMonthly,
        RateTypeWire::AprQuarterly => RateType::AprQuarterly,
        RateTypeWire::AprSemiAnnually => RateType::AprSemiAnnually,
        RateTypeWire::AprAnnually => RateType::AprAnnually,
    }
}

fn length(l: LengthParam) -> Result<Length, RpcError> {
    match (l.months, l.years) {
        (Some(m), None) => Ok(Length::Months(m)),
        (None, Some(y)) => Ok(Length::Years(y)),
        _ => Err(RpcError::invalid_params("give the length as { months } or { years }")),
    }
}

fn other_payment(book: &Book, p: OtherPaymentParam) -> Result<OtherPayment, RpcError> {
    Ok(OtherPayment {
        kind: match p.kind {
            PaymentKindWire::Taxes => PaymentKind::Taxes,
            PaymentKindWire::Insurance => PaymentKind::Insurance,
            PaymentKindWire::Pmi => PaymentKind::Pmi,
            PaymentKindWire::Other => PaymentKind::Other,
        },
        memo: p.memo,
        amount: p.amount.0,
        from: p.from.map(|a| a.resolve(book)).transpose()?,
        to: p.to.resolve(book)?,
        via_escrow: p.via_escrow,
    })
}

fn loan_of(book: &Book, p: LoanParams) -> Result<Loan, RpcError> {
    let r = p.repayment;
    Ok(Loan {
        name: p.name,
        account: p.loan_account.resolve(book)?,
        principal: p.principal.0,
        rate: p.rate.0,
        rate_type: rate_type(p.rate_type),
        start: p.start_date.map(|d| d.0),
        length: length(p.length)?,
        months_remaining: p.months_remaining,
        repayment: Repayment {
            from: r.from.resolve(book)?,
            principal_to: r.principal_to.map(|a| a.resolve(book)).transpose()?,
            interest_to: r.interest_to.resolve(book)?,
            start: r.start.map(|d| d.0),
            frequency: r.frequency.map(|f| (f.mult, f.period_type.to_core(), f.weekend_adjust.to_core())),
        },
        escrow: p.escrow.map(|a| a.resolve(book)).transpose()?,
        other_payments: p
            .other_payments
            .into_iter()
            .map(|o| other_payment(book, o))
            .collect::<Result<_, _>>()?,
    })
}

pub(super) fn loan(ctx: &mut Context, p: LoanParams) -> Result<LoanResult, RpcError> {
    let format = format(ctx)?;
    ctx.mutate_rpc(|book, env| {
        let loan = loan_of(book, p)?;
        let created = ops::loan(book, env, &loan, format)?;
        Ok(LoanResult {
            scheduled_transaction: edited(book, &created.edited, format)?,
            payment: Amount(created.payment),
        })
    })
}
