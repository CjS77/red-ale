<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/loans-and-interest
title: Loans and interest
summary: Principal, interest, compounding and amortisation; how a loan is kept in the books as a liability, the payment formulas red-ale computes exactly, and the loan assistant as scheduledTransaction.loan. There is no loan calculator.
related: concept/scheduled-transactions, concept/account-types, concept/investments, method/scheduledTransaction.loan, method/scheduledTransaction.create
from: guide/ch_loans.docbook#loans_concepts1
---
A loan is a financial arrangement in which someone pays for the use of someone else's money: a credit card, a car loan,
a mortgage, a business loan, or money you lend to a friend.

## Terms

| Term | Meaning |
|------|---------|
| Principal | The original amount of the loan, or the part of it still owed. Each payment repays some principal and pays some interest |
| Interest | What the borrower pays for the use of the money, usually a yearly percentage of the principal (the annual percentage rate, APR) |
| Compounding | Interest that is added to the principal, so later interest is charged on it too |
| Amortisation | A repayment plan that pays the loan off by equal periodic payments, each split into principal and interest; the principal share grows and the interest share shrinks |
| Disbursement | An amount of the loan paid out to the borrower. Some loans have several |
| Loan fee | A processing fee taken from the principal when the loan is made |
| Escrow | Money held by the lender for taxes or insurance, paid with each instalment |

## How the books hold a loan

- A loan *you owe* is a liability: an Account of type `LIABILITY` for the loan, with the balance negative (credits).
  The money you receive goes to your bank Account (a debit); each payment moves money out of the bank Account, reduces
  the liability by the principal share and charges the interest share to an `EXPENSE` Account.
- A loan *you made* is an asset: a short-term receivable such as `Assets:Current Assets:LoanToJoe`. Lending debits it
  and credits the bank; each repayment debits the bank and credits it. The interest you earn is income.
- A mortgage's tax and insurance paid through escrow are expenses too.

The books record what the lender's statement says, so reconcile the loan Account against it
([Reconciliation](concept/reconciliation)).

## The arithmetic

The fundamental financial equation links five variables: the number of periods *n*, the interest rate per period *i*,
the present value *PV* (what is owed now), the periodic payment *PMT* and the future value *FV* (what is owed at the
end, usually zero):

```
PV * (1 + i)^n + PMT * (1 + i*X) * ((1 + i)^n - 1) / i + FV = 0
```

with *X* 0 for payments at the end of each period and 1 for payments at the beginning. For 100,000 over 30 years at 4%
compounded monthly, *n* = 360 and *i* = 1/300 per month, so the monthly payment is 477.42.

A **nominal** annual rate must first be turned into the effective rate per payment period: with *CF* compounding
periods and *PF* payments per year, `ieff = (1 + rate/CF)^(CF/PF) - 1`.

**There is no Loan Repayment Calculator in red-ale.** The same arithmetic is available where it matters, in the
financial functions of scheduled-transaction formulas: `pmt`, `ipmt`, `ppmt` and the others
([Scheduled transactions](concept/scheduled-transactions)). Given a whole number of periods red-ale computes them with
exact rationals and rounds the result to 12 significant figures and then to the currency's fraction, as GnuCash
does; GnuCash computes them in floating point, so a result can differ by a cent when the exact figure lies within a hair
of a half cent. Functions with a non-whole power of the rate are refused.

## The loan assistant

[`scheduledTransaction.loan`](method/scheduledTransaction.loan) is GnuCash's Mortgage and Loan Repayment assistant as one
call: give the loan Account, the principal, the rate and its type, the start date, the length and the repayment
accounts, and it creates the scheduled transaction GnuCash's assistant would, with the same `pmt`, `ppmt` and `ipmt`
formulas, so GnuCash computes the same repayments from it. The result includes `payment`, the periodic payment. Escrow
and other payments (taxes, insurance, PMI) can be added.

An adjustable-rate loan changes its rate on a schedule (an ARM 10/1 is fixed for 10 years, then changes yearly). The
assistant makes a fixed-rate schedule; edit it or make a new one when the rate changes
([`scheduledTransaction.update`](method/scheduledTransaction.update)). The final payment is an ordinary Transaction with
the figures from the bank.
