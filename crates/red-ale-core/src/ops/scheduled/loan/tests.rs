//! The loan assistant's scheduled transaction (`5.17:gnucash/gnome/assistant-loan.cpp`, `AL`), and the note's
//! hand-worked cases H2 and H3 (`design/notes/engine-scheduled-transactions.md` §9.3): red-ale's formula engine,
//! evaluating the formulas red-ale writes, gives the figures worked by hand from `5.17:bindings/guile/fin.scm:34-97`.

use super::*;
use crate::ops::scheduled::fixture::{Fixture, date, usd};
use crate::ops::scheduled::{create_instance, template_transactions};
use crate::{AccountType, Recurrence, TemplateSplit};

struct Accounts {
    mortgage: Guid,
    interest: Guid,
    escrow: Guid,
    taxes: Guid,
}

fn accounts(f: &mut Fixture) -> Accounts {
    Accounts {
        mortgage: f.account("Mortgage", AccountType::Liability, usd()),
        interest: f.account("Interest", AccountType::Expense, usd()),
        escrow: f.account("Escrow", AccountType::Bank, usd()),
        taxes: f.account("Taxes", AccountType::Expense, usd()),
    }
}

/// 100,000 at 6 % "Interest Rate" over 30 years, repaid monthly from Bank.
fn thirty_years(f: &Fixture, a: &Accounts) -> Loan {
    Loan {
        name: None,
        account: a.mortgage,
        principal: Numeric::from_int(100_000),
        rate: Numeric::from_int(6),
        rate_type: RateType::Simple,
        start: Some(date(2026, 1, 1)),
        length: Length::Years(30),
        months_remaining: None,
        repayment: Repayment {
            from: f.bank,
            principal_to: None,
            interest_to: a.interest,
            start: None,
            frequency: None,
        },
        escrow: None,
        other_payments: Vec::new(),
    }
}

fn run(f: &mut Fixture, loan: &Loan) -> Result<LoanCreated, ScheduledError> {
    super::loan(&mut f.book, &mut f.env, loan, NumberFormat::default())
}

/// Each template's description, and its splits' accounts, memos and formulas (debit, credit).
type Template = (String, Vec<(Guid, String, String, String)>);

fn templates(f: &Fixture, sx: Guid) -> Vec<Template> {
    template_transactions(&f.book, &f.book.scheduled[&sx])
        .iter()
        .map(|t| {
            let splits = t
                .splits
                .iter()
                .map(|s| {
                    let x = TemplateSplit::of(s).unwrap();
                    let text = |v: Option<&str>| v.unwrap_or_default().to_owned();
                    (x.account().unwrap(), s.memo.clone().unwrap_or_default(), text(x.debit_formula()), text(x.credit_formula()))
                })
                .collect();
            (t.description.clone().unwrap_or_default(), splits)
        })
        .collect()
}

const PMT: &str = "pmt( 0.06000 / 12.00 : 360.00 : 100000.00 : 0 : 0 )";
const PPMT: &str = "ppmt( 0.06000 / 12.00 : i : 360.00 : 100000.00 : 0 : 0 )";
const IPMT: &str = "ipmt( 0.06000 / 12.00 : i : 360.00 : 100000.00 : 0 : 0 )";

fn at(formula: &str, i: i64) -> Numeric {
    let bindings = Variables::from([("i".to_owned(), Numeric::from_int(i))]);
    evaluate(formula, NumberFormat::default(), &bindings).unwrap()
}

/// `loan_create_sxes` (`AL:2850-2985`) without escrow: one template `Loan - Payment`, Bank credit `pmt`, the loan
/// debit `ppmt`, Interest debit `ipmt`, in the formula text GnuCash writes under `C` (the note §6.2); the numerics
/// as the register rule sets them.
#[test]
fn a_thirty_year_loan_has_gnucashs_formulas() {
    let mut f = Fixture::new();
    let a = accounts(&mut f);
    let loan = thirty_years(&f, &a);
    let created = run(&mut f, &loan).unwrap();
    let sx = created.edited.guid;
    let s = |account, memo: &str, debit: &str, credit: &str| (account, memo.to_owned(), debit.to_owned(), credit.to_owned());
    assert_eq!(templates(&f, sx), [("Loan - Payment".to_owned(), vec![
        s(f.bank, "Loan - Payment", "", PMT),
        s(a.mortgage, "Loan - Principal", PPMT, ""),
        s(a.interest, "Loan - Interest", IPMT, ""),
    ])]);
    let t = template_transactions(&f.book, &f.book.scheduled[&sx])[0];
    assert_eq!(t.currency, usd());
    let numerics: Vec<_> = t.splits.iter().map(|s| TemplateSplit::of(s).unwrap()).map(|x| (x.debit_numeric(), x.credit_numeric())).collect();
    let zero = Some(Numeric::new(0, 1));
    assert_eq!(numerics, [(zero, Some(created.payment)), (zero, zero), (zero, zero)]);
}

/// H2: `pmt(0.005 : 360 : 100000 : 0 : 0)` is 599.55052515275239… exactly, 599.550525153 to 12 significant figures
/// (`gnc-exp-parser.c:346-357`), a split value of 599.55. H3: `ipmt`/`ppmt` at period 1 are 500 and 99.5505251528, at
/// period 360 2.98283843360 and 596.567686719.
#[test]
fn the_loans_formulas_give_the_hand_worked_figures() {
    let mut f = Fixture::new();
    let a = accounts(&mut f);
    let loan = thirty_years(&f, &a);
    let created = run(&mut f, &loan).unwrap();
    assert!(created.payment.same_value(Numeric::new(599_550_525_153, 1_000_000_000)), "{}", created.payment);
    assert!(at(PMT, 1).same_value(created.payment));
    assert!(at(IPMT, 1).same_value(Numeric::from_int(500)));
    assert!(at(PPMT, 1).same_value(Numeric::new(995_505_251_528, 10_000_000_000)));
    assert!(at(IPMT, 360).same_value(Numeric::new(298_283_843_360, 100_000_000_000)));
    assert!(at(PPMT, 360).same_value(Numeric::new(596_567_686_719, 1_000_000_000)));
    let i = Variables::from([("i".to_owned(), Numeric::from_int(1))]);
    let made = create_instance(&mut f.book, &mut f.env, created.edited.guid, date(2026, 1, 1), &i, NumberFormat::default()).unwrap();
    let t = &f.book.transactions[&made[0].transaction];
    let mut values: Vec<(Option<Guid>, Numeric)> = t.splits.iter().map(|s| (s.account, s.value)).collect();
    values.sort_by_key(|(_, v)| v.num());
    assert_eq!(values, [
        (Some(f.bank), Numeric::new(-59955, 100)),
        (Some(a.mortgage), Numeric::new(9955, 100)),
        (Some(a.interest), Numeric::new(50000, 100)),
    ]);
    assert!(made[0].imbalance.is_none(), "the cents balance (the note's defect 6 does not show here)");
}

/// The schedule `AL:2850-2865` sets: start on the loan's start; last occurrence a month before the first repayment;
/// end `months remaining − 1` months after it; Instance count `months − remaining + 1`. Months remaining by default
/// count months from the start until today (`AL:1011-1034`): twelve payments made gives 13.
#[test]
fn the_schedule_and_the_instance_count() {
    let mut f = Fixture::new();
    let a = accounts(&mut f);
    let mut loan = thirty_years(&f, &a);
    loan.start = Some(date(2025, 1, 1));
    let created = run(&mut f, &loan).unwrap();
    let sx = &f.book.scheduled[&created.edited.guid];
    assert_eq!((sx.start, sx.last, sx.end, sx.instance_count), (date(2025, 1, 1), Some(date(2024, 12, 1)), Some(date(2053, 12, 1)), 13));
    assert_eq!(sx.schedule, [Recurrence::new(1, PeriodType::Month, date(2025, 1, 1), WeekendAdjust::None).unwrap()]);
    assert_eq!((sx.enabled, sx.auto_create, sx.advance_create_days, sx.occurrences), (true, false, 0, None));

    loan.name = Some("House".into());
    loan.months_remaining = Some(300);
    loan.repayment.start = Some(date(2025, 3, 31));
    loan.repayment.frequency = Some((1, PeriodType::EndOfMonth, WeekendAdjust::Back));
    let created = run(&mut f, &loan).unwrap();
    let sx = &f.book.scheduled[&created.edited.guid];
    assert_eq!((sx.name.as_str(), sx.last, sx.end, sx.instance_count), ("House", Some(date(2025, 2, 28)), Some(date(2050, 2, 28)), 61));
    assert_eq!(sx.schedule, [Recurrence::new(1, PeriodType::EndOfMonth, date(2025, 3, 31), WeekendAdjust::Back).unwrap()]);
}

/// With escrow (`AL:2903-2935`) the first template moves the payment into escrow, the second out of it; taxes via
/// escrow add to the deposit and are paid from escrow, another expense not via escrow is part of the first
/// (`ld_setup_repayment_sx`, `AL:2615-2819`), written exactly where GnuCash rounds to two significant figures.
#[test]
fn escrow_and_other_payments() {
    let mut f = Fixture::new();
    let a = accounts(&mut f);
    let mut loan = thirty_years(&f, &a);
    loan.escrow = Some(a.escrow);
    loan.other_payments = vec![
        OtherPayment {
            kind: PaymentKind::Taxes,
            memo: None,
            amount: Numeric::new(25075, 100),
            from: None,
            to: a.taxes,
            via_escrow: None,
        },
        OtherPayment {
            kind: PaymentKind::Other,
            memo: Some("Gardener".into()),
            amount: Numeric::from_int(40),
            from: None,
            to: f.food,
            via_escrow: None,
        },
    ];
    let created = run(&mut f, &loan).unwrap();
    let s = |account, memo: &str, debit: &str, credit: &str| (account, memo.to_owned(), debit.to_owned(), credit.to_owned());
    let plus = |x: &str| format!("{PMT} + {x}");
    let mut got = templates(&f, created.edited.guid);
    got.sort();
    assert_eq!(got, [
        ("Loan - Escrow Payment".to_owned(), vec![
            s(f.bank, "Loan", "", &format!("{} + 40.00", plus("250.75"))),
            s(a.escrow, "Loan", &plus("250.75"), ""),
            s(f.food, "Gardener", "40.00", ""),
        ]),
        ("Loan - Payment".to_owned(), vec![
            s(a.escrow, "Loan - Payment", "", PMT),
            s(a.mortgage, "Loan - Principal", PPMT, ""),
            s(a.interest, "Loan - Interest", IPMT, ""),
            s(a.escrow, "Tax Payment", "", "250.75"),
            s(a.taxes, "Tax Payment", "250.75", ""),
        ]),
    ]);
    loan.escrow = None;
    loan.other_payments.truncate(1);
    loan.other_payments[0].via_escrow = Some(true);
    assert_eq!(reason(&run(&mut f, &loan).unwrap_err()), "escrow");
    loan.other_payments[0].via_escrow = None;
    loan.other_payments[0].from = Some(a.escrow);
    let created = run(&mut f, &loan).unwrap();
    let got = templates(&f, created.edited.guid);
    assert_eq!(got[0].1[3..], [s(a.escrow, "Tax Payment", "", "250.75"), s(a.taxes, "Tax Payment", "250.75", "")]);
}

/// A change to a loan, given the Food account.
type Change = fn(&mut Loan, Guid);

fn reason(e: &ScheduledError) -> &'static str {
    match e {
        ScheduledError::Invalid { reason, .. } => reason,
        other => panic!("not a refusal: {other:?}"),
    }
}

/// Payments per year and periods (`AL:2400-2436`): every three weeks is 52/3 a year, written `( 52 / 3 )` where
/// GnuCash prints `17.33`, and 520 periods over 30 years; an APR compounded at the payment frequency is the
/// "Interest Rate" form; in another Locale the numbers take its decimal point.
#[test]
fn rate_types_frequencies_and_the_locale() {
    let mut f = Fixture::new();
    let a = accounts(&mut f);
    let mut loan = thirty_years(&f, &a);
    loan.repayment.frequency = Some((3, PeriodType::Week, WeekendAdjust::None));
    let created = run(&mut f, &loan).unwrap();
    assert_eq!(templates(&f, created.edited.guid)[0].1[0].3, "pmt( 0.06000 / ( 52 / 3 ) : 520.00 : 100000.00 : 0 : 0 )");
    loan.repayment.frequency = None;
    loan.rate_type = RateType::AprMonthly;
    loan.rate = Numeric::new(4125, 1000);
    let de = NumberFormat {
        decimal_point: ',',
        group_separator: '.',
    };
    let created = super::loan(&mut f.book, &mut f.env, &loan, de).unwrap();
    assert_eq!(templates(&f, created.edited.guid)[0].1[0].3, "pmt( 0,04125 / 12,00 : 360,00 : 100000,00 : 0 : 0 )");
    loan.rate_type = RateType::AprDaily;
    assert_eq!(reason(&run(&mut f, &loan).unwrap_err()), "non-integer-power");
}

/// The assistant's limits: a LIABILITY loan account (`AL:566-567`), payment accounts of the selectors' types
/// (`AL:550-605`), a rate of 0.001–100 with 3 decimals (`AL:194`, `:698-703`), a repeating frequency, and months
/// remaining within the length.
#[test]
fn the_assistants_limits() {
    let mut f = Fixture::new();
    let a = accounts(&mut f);
    let base = thirty_years(&f, &a);
    let food = f.food;
    let cases: [(Change, &str); 8] = [
        (|l, food| l.account = food, "account-type"),
        (|l, food| l.repayment.from = food, "account-type"),
        (|l, _| l.rate = Numeric::ZERO, "rate"),
        (|l, _| l.rate = Numeric::new(30_005, 10_000), "rate"),
        (|l, _| l.principal = Numeric::ZERO, "principal"),
        (|l, _| l.length = Length::Months(0), "length"),
        (|l, _| l.months_remaining = Some(361), "months-remaining"),
        (|l, _| l.repayment.frequency = Some((0, PeriodType::Once, WeekendAdjust::None)), "frequency"),
    ];
    cases.into_iter().for_each(|(change, expected)| {
        let mut loan = base.clone();
        change(&mut loan, food);
        assert_eq!(reason(&run(&mut f, &loan).unwrap_err()), expected, "{loan:?}");
    });
}
