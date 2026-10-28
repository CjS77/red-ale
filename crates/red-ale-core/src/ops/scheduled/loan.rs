//! GnuCash's Mortgage and Loan Repayment assistant (`5.17:gnucash/gnome/assistant-loan.cpp`, here `AL`;
//! `design/notes/engine-scheduled-transactions.md` §6) as one operation: the loan in, the repayment scheduled
//! transaction `loan_create_sxes` builds out (`AL:2838-3046`), with the same `pmt`, `ppmt` and `ipmt` formulas and
//! instance count, so GnuCash computes the same repayments from it. The defects the note records (§6.3) are not
//! copied (`design/api/scheduled-transactions.md`, "The loan assistant").

use super::edit::{Edited, End, Fields, Progress, RecurrenceSpec, SplitSpec, TemplateSpec, create_with};
use super::error::ScheduledError;
use super::text::{formula_number, places_of};
use crate::formula::{NumberFormat, Variables, evaluate};
use crate::ops::commit::commodity_fraction;
use crate::{AccountType, Book, CommodityRef, Date, DomainError, Env, Guid, Numeric, PeriodType, WeekendAdjust};

/// The interest rate's kind (`IRateType`, `AL:166-175`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RateType {
    /// "Interest Rate": the annual rate divided by the payments per year.
    #[default]
    Simple,
    /// An APR compounded daily.
    AprDaily,
    /// An APR compounded weekly.
    AprWeekly,
    /// An APR compounded monthly.
    AprMonthly,
    /// An APR compounded quarterly.
    AprQuarterly,
    /// An APR compounded twice a year.
    AprSemiAnnually,
    /// An APR compounded once a year.
    AprAnnually,
}

impl RateType {
    /// Compounding periods a year, as `loan_get_formula_internal` passes them (`AL:2449-2473`).
    fn compounding(self) -> Option<i64> {
        match self {
            Self::Simple => None,
            Self::AprDaily => Some(365),
            Self::AprWeekly => Some(52),
            Self::AprMonthly => Some(12),
            Self::AprQuarterly => Some(4),
            Self::AprSemiAnnually => Some(2),
            Self::AprAnnually => Some(1),
        }
    }
}

/// The loan's length (`AL:1011-1034`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Length {
    /// In months, 1–9999.
    Months(u32),
    /// In years, 1–9999.
    Years(u32),
}

/// The four fixed other payments (`REPAY_DEFAULTS`, `AL:123-132`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentKind {
    /// Taxes, via escrow by default.
    Taxes,
    /// Insurance, via escrow by default.
    Insurance,
    /// Private mortgage insurance, via escrow by default.
    Pmi,
    /// Another expense, never via escrow by default.
    Other,
}

impl PaymentKind {
    fn memo(self) -> &'static str {
        match self {
            Self::Taxes => "Tax Payment",
            Self::Insurance => "Insurance Payment",
            Self::Pmi => "PMI Payment",
            Self::Other => "Miscellaneous Payment",
        }
    }
}

/// An other payment made as part of the repayment ("Part of Payment Transaction", `AL:1645-1658`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherPayment {
    /// Which.
    pub kind: PaymentKind,
    /// The memo; the kind's by default.
    pub memo: Option<String>,
    /// The amount, written exactly.
    pub amount: Numeric,
    /// The account it is paid from; the repayment's by default.
    pub from: Option<Guid>,
    /// The account it is paid to.
    pub to: Guid,
    /// Whether it goes through the escrow account; by default yes for all but `Other` when there is one.
    pub via_escrow: Option<bool>,
}

/// Where the repayment comes from and goes to, and when (`AL:550-605`, `:916-921`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repayment {
    /// BANK, CASH, CREDIT, ASSET or LIABILITY.
    pub from: Guid,
    /// The principal's account; the loan account by default.
    pub principal_to: Option<Guid>,
    /// The interest's account.
    pub interest_to: Guid,
    /// The first repayment; the loan's start by default.
    pub start: Option<Date>,
    /// One Recurrence's multiplier, period type and weekend adjustment; monthly by default.
    pub frequency: Option<(u16, PeriodType, WeekendAdjust)>,
}

/// The assistant's inputs (`AL:550-735`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loan {
    /// The repayment's name; `Loan` by default (`AL:923`).
    pub name: Option<String>,
    /// The loan, a LIABILITY.
    pub account: Guid,
    /// The principal, more than 0, in the loan account's commodity.
    pub principal: Numeric,
    /// The annual rate in percent, 0.001–100 with at most 3 decimals.
    pub rate: Numeric,
    /// The rate's kind.
    pub rate_type: RateType,
    /// The loan's start; today by default.
    pub start: Option<Date>,
    /// Its length.
    pub length: Length,
    /// Months still to pay, 1 to the length; by default the length less the months from the start to today.
    pub months_remaining: Option<u32>,
    /// The repayment.
    pub repayment: Repayment,
    /// The escrow account, if any.
    pub escrow: Option<Guid>,
    /// Other payments made with each repayment.
    pub other_payments: Vec<OtherPayment>,
}

/// The repayment scheduled transaction and the payment its `pmt` formula gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoanCreated {
    /// The scheduled transaction.
    pub edited: Edited,
    /// The `pmt` formula's value, to 12 significant figures.
    pub payment: Numeric,
}

/// The three formulas of a repayment and the amounts' decimal places.
struct Formulas {
    pmt: String,
    ppmt: String,
    ipmt: String,
    places: u32,
}

/// Make the repayment scheduled transaction (`AL:2850-2985`) with `xaccSchedXactionMalloc`'s defaults otherwise:
/// started on the loan's start, last occurrence one month before the first repayment, ending `months remaining − 1`
/// months after it, with Instance count `length in months − months remaining + 1` (`AL:2853-2865`).
pub fn loan(book: &mut Book, env: &mut dyn Env, loan: &Loan, format: NumberFormat) -> Result<LoanCreated, ScheduledError> {
    let today = env
        .today()
        .ok_or_else(|| DomainError::invalid("the clock is outside the calendar"))?;
    let currency = check_accounts(book, loan)?;
    check_figures(loan)?;
    let start = loan.start.unwrap_or(today);
    let months = match loan.length {
        Length::Months(m) => m,
        Length::Years(y) => y.saturating_mul(12),
    };
    let remaining = loan
        .months_remaining
        .map_or_else(|| default_remaining(start, today, months), i64::from);
    if !(1..=i64::from(months)).contains(&remaining) {
        return Err(ScheduledError::invalid(
            "months-remaining",
            format!("months remaining must be 1 to {months}, not {remaining}"),
        ));
    }
    let remaining = i32::try_from(remaining).unwrap_or(i32::MAX);
    let (mult, period_type, weekend_adjust) = loan.repayment.frequency.unwrap_or((1, PeriodType::Month, WeekendAdjust::None));
    let first = loan.repayment.start.unwrap_or(start);
    let formulas = formulas(book, loan, &currency, months, (mult, period_type), format)?;
    let payment = evaluate(&formulas.pmt, format, &Variables::new()).map_err(|e| DomainError::invalid(e.to_string()))?;
    let name = loan.name.clone().unwrap_or_else(|| "Loan".to_owned());
    let fields = Fields {
        templates: Some(templates(loan, &name, &currency, &formulas, format)?),
        name: Some(name),
        start: Some(start),
        schedule: Some(vec![RecurrenceSpec {
            mult,
            period_type,
            start: Some(first),
            weekend_adjust,
        }]),
        end: Some(End::Date(first.add_months(remaining - 1))),
        ..Fields::default()
    };
    let progress = Progress {
        last: Some(first.add_months(-1)),
        instance_count: i32::try_from(months).unwrap_or(i32::MAX) - remaining + 1,
    };
    let edited = create_with(book, env, fields, progress, format)?;
    Ok(LoanCreated { edited, payment })
}

/// The length in months less the months from `start` to today, counted by adding a month until reaching today, as
/// `loan_info_calc_update_cb` counts them (`AL:1011-1034`).
fn default_remaining(start: Date, today: Date, months: u32) -> i64 {
    let elapsed = std::iter::successors(Some(start), |d| Some(d.add_months(1)))
        .take_while(|d| *d < today)
        .count();
    i64::from(months) - i64::try_from(elapsed).unwrap_or(i64::MAX)
}

fn account_type(book: &Book, guid: Guid) -> Result<AccountType, ScheduledError> {
    book.account(guid)
        .map(|a| a.account_type)
        .ok_or_else(|| DomainError::not_found("account", &guid).into())
}

fn of_type(book: &Book, guid: Guid, role: &str, allowed: &[AccountType]) -> Result<(), ScheduledError> {
    let ty = account_type(book, guid)?;
    if allowed.contains(&ty) {
        Ok(())
    } else {
        Err(ScheduledError::invalid(
            "account-type",
            format!("the {role} account can't be of type {}", ty.as_str()),
        ))
    }
}

/// The account types the assistant's selectors offer (`AL:550-605`); returns the loan account's currency, which the
/// templates are in.
fn check_accounts(book: &Book, loan: &Loan) -> Result<CommodityRef, ScheduledError> {
    use AccountType::{Asset, Bank, Cash, Credit, Expense, Liability};
    const PAY_FROM: [AccountType; 5] = [Bank, Cash, Credit, Asset, Liability];
    const PAY_TO: [AccountType; 6] = [Bank, Cash, Credit, Asset, Liability, Expense];
    of_type(book, loan.account, "loan", &[Liability])?;
    of_type(book, loan.repayment.from, "payment from", &PAY_FROM)?;
    of_type(book, loan.repayment.principal_to.unwrap_or(loan.account), "principal to", &PAY_TO)?;
    of_type(book, loan.repayment.interest_to, "interest to", &PAY_TO)?;
    loan.escrow.map(|e| account_type(book, e)).transpose()?;
    loan.other_payments
        .iter()
        .flat_map(|p| p.from.into_iter().chain([p.to]))
        .try_for_each(|g| account_type(book, g).map(drop))?;
    let currency = book.account(loan.account).and_then(|a| a.commodity.clone());
    currency
        .filter(CommodityRef::is_currency)
        .ok_or_else(|| ScheduledError::invalid("currency", "the loan account's commodity must be a currency"))
}

/// The principal is positive; the rate is the spin box's, 0.001–100 with 3 decimals (`AL:194`, `:698-703`); the
/// length is 1–9999.
fn check_figures(loan: &Loan) -> Result<(), ScheduledError> {
    if !loan.principal.is_positive() {
        return Err(ScheduledError::invalid("principal", "the principal must be more than 0"));
    }
    let thousandths = loan
        .rate
        .mul(Numeric::from_int(1000), crate::DENOM_AUTO, crate::How::EXACT)?
        .reduce();
    if thousandths.denom() != 1 || !(1..=100_000).contains(&thousandths.num()) {
        return Err(ScheduledError::invalid(
            "rate",
            "the rate must be 0.001 to 100 percent, with at most 3 decimals",
        ));
    }
    let (Length::Months(n) | Length::Years(n)) = loan.length;
    if (1..=9999).contains(&n) {
        Ok(())
    } else {
        Err(ScheduledError::invalid("length", format!("the length must be 1 to 9999, not {n}")))
    }
}

/// Payments per year as `periods_per_year` sums them (`AL:2400-2436`): `year` 1/mult, `week` 52/mult, the month
/// types 12/mult, `day` 365/mult truncated to an integer.
fn payments_per_year(mult: u16, period_type: PeriodType) -> Result<Numeric, ScheduledError> {
    let mult = i64::from(mult.max(1));
    Ok(match period_type {
        PeriodType::Year => Numeric::new(1, mult),
        PeriodType::Week => Numeric::new(52, mult),
        PeriodType::Day => Numeric::from_int(365 / mult),
        PeriodType::Month | PeriodType::EndOfMonth | PeriodType::NthWeekday | PeriodType::LastWeekday => Numeric::new(12, mult),
        PeriodType::Once => return Err(ScheduledError::invalid("frequency", "a loan is not repaid once")),
    }
    .reduce())
}

/// The formulas of "Interest Rate" (`AL:2440-2528`): `pmt( r / ppy : n : pv : 0 : 0 )` and `ppmt`, `ipmt` with `i`,
/// the rate as a fraction to 5 decimals, the payments per year to 2 decimals (or `( a / b )` when that is not exact),
/// the full term's periods to 2 decimals and the principal with at least the currency's decimals. An APR whose
/// compounding is the payment frequency is that rate; any other needs a power that is not a whole number
/// (`loan_apr_to_simple_formula`, `AL:2296-2314`) and is refused.
fn formulas(
    book: &Book,
    loan: &Loan,
    currency: &CommodityRef,
    months: u32,
    (mult, period_type): (u16, PeriodType),
    format: NumberFormat,
) -> Result<Formulas, ScheduledError> {
    let ppy = payments_per_year(mult, period_type)?;
    if loan.rate_type.compounding().is_some_and(|m| !ppy.same_value(Numeric::from_int(m))) {
        return Err(ScheduledError::invalid(
            "non-integer-power",
            "an APR compounded other than at the payment frequency needs a power that is not a whole number",
        ));
    }
    let exact = crate::How::EXACT;
    let periods = Numeric::from_int(i64::from(months)).mul(ppy, crate::DENOM_AUTO, exact)?.reduce();
    let periods = Numeric::from_int((periods.num() / periods.denom() / 12).max(1));
    let rate = loan.rate.div(Numeric::from_int(100), crate::DENOM_AUTO, exact)?;
    let places = commodity_fraction(book, currency).map_or(2, places_of);
    let r = formula_number(rate, 5, format);
    let ppy = if ppy.mul(Numeric::from_int(100), crate::DENOM_AUTO, exact)?.reduce().denom() == 1 {
        formula_number(ppy, 2, format)
    } else {
        format!("( {} / {} )", ppy.num(), ppy.denom())
    };
    let (n, pv) = (
        formula_number(periods, 2, format),
        formula_number(loan.principal, places.max(2), format),
    );
    Ok(Formulas {
        pmt: format!("pmt( {r} / {ppy} : {n} : {pv} : 0 : 0 )"),
        ppmt: format!("ppmt( {r} / {ppy} : i : {n} : {pv} : 0 : 0 )"),
        ipmt: format!("ipmt( {r} / {ppy} : i : {n} : {pv} : 0 : 0 )"),
        places,
    })
}

fn credit(account: Guid, memo: String, formula: &str) -> SplitSpec {
    SplitSpec {
        account,
        memo: Some(memo),
        credit_formula: Some(formula.to_owned()),
        ..SplitSpec::default()
    }
}

fn debit(account: Guid, memo: String, formula: &str) -> SplitSpec {
    SplitSpec {
        account,
        memo: Some(memo),
        debit_formula: Some(formula.to_owned()),
        ..SplitSpec::default()
    }
}

/// The templates `loan_create_sxes` builds (`AL:2870-2985`): without escrow one, `<name> - Payment`; with escrow
/// `<name> - Escrow Payment` into escrow, then `<name> - Payment` out of it. Then each other payment.
fn templates(
    loan: &Loan,
    name: &str,
    currency: &CommodityRef,
    f: &Formulas,
    format: NumberFormat,
) -> Result<Vec<TemplateSpec>, ScheduledError> {
    let template = |description: String, splits| TemplateSpec {
        description: Some(description),
        currency: Some(currency.clone()),
        splits,
        ..TemplateSpec::default()
    };
    let principal_to = loan.repayment.principal_to.unwrap_or(loan.account);
    let source = loan.escrow.unwrap_or(loan.repayment.from);
    let payment = vec![
        credit(source, format!("{name} - Payment"), &f.pmt),
        debit(principal_to, format!("{name} - Principal"), &f.ppmt),
        debit(loan.repayment.interest_to, format!("{name} - Interest"), &f.ipmt),
    ];
    let mut templates = match loan.escrow {
        None => vec![template(format!("{name} - Payment"), payment)],
        Some(escrow) => vec![
            template(
                format!("{name} - Escrow Payment"),
                vec![
                    credit(loan.repayment.from, name.to_owned(), &f.pmt),
                    debit(escrow, name.to_owned(), &f.pmt),
                ],
            ),
            template(format!("{name} - Payment"), payment),
        ],
    };
    loan.other_payments
        .iter()
        .try_for_each(|p| add_other_payment(loan, &mut templates, p, f.places, format))?;
    Ok(templates)
}

fn find(splits: &mut [SplitSpec], account: Guid) -> Option<&mut SplitSpec> {
    splits.iter_mut().find(|s| s.account == account)
}

fn append(formula: &mut Option<String>, amount: &str) {
    *formula = Some(match formula.take().filter(|f| !f.is_empty()) {
        Some(f) => format!("{f} + {amount}"),
        None => amount.to_owned(),
    });
}

/// One other payment as `ld_setup_repayment_sx` adds it to the repayment (`AL:2615-2819`): the amount added to the
/// paying split's credit, or a new credit split in `from` when it is not the repayment's; a debit to `to`; via
/// escrow, the deposit into escrow in the first template and the transfer to `to` in the second. The amount is
/// written exactly, where GnuCash rounds it to two significant figures (`AL:2686-2699`).
fn add_other_payment(
    loan: &Loan,
    templates: &mut [TemplateSpec],
    p: &OtherPayment,
    places: u32,
    format: NumberFormat,
) -> Result<(), ScheduledError> {
    let via_escrow = p.via_escrow.unwrap_or(loan.escrow.is_some() && p.kind != PaymentKind::Other);
    let escrow = match (via_escrow, loan.escrow) {
        (true, None) => return Err(ScheduledError::invalid("escrow", "a payment via escrow needs an escrow account")),
        (true, Some(e)) => Some(e),
        (false, _) => None,
    };
    let amount = formula_number(p.amount, places, format);
    let memo = p.memo.clone().unwrap_or_else(|| p.kind.memo().to_owned());
    let from = p.from.filter(|f| *f != loan.repayment.from);
    let (main, rest) = templates
        .split_first_mut()
        .ok_or_else(|| DomainError::invalid("the loan has no template"))?;
    if let Some(escrow) = escrow {
        if let Some(s) = find(&mut main.splits, escrow) {
            append(&mut s.debit_formula, &amount);
        }
        let second = rest
            .first_mut()
            .ok_or_else(|| DomainError::invalid("the escrow template is missing"))?;
        match from {
            Some(_) => find(&mut second.splits, escrow)
                .into_iter()
                .for_each(|s| append(&mut s.credit_formula, &amount)),
            None => second.splits.push(credit(escrow, memo.clone(), &amount)),
        }
    }
    match from.map_or_else(|| find(&mut main.splits, loan.repayment.from), |_| None) {
        Some(s) => append(&mut s.credit_formula, &amount),
        None => main.splits.push(credit(from.unwrap_or(loan.repayment.from), memo.clone(), &amount)),
    }
    let to = match escrow {
        Some(_) => rest.first_mut().map_or(&mut main.splits, |t| &mut t.splits),
        None => &mut main.splits,
    };
    to.push(debit(p.to, memo, &amount));
    Ok(())
}

#[cfg(test)]
mod tests;
