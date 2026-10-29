<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/scheduled-transactions
title: Scheduled transactions
summary: A Scheduled transaction is a Transaction that recurs - a schedule of Recurrences, template Transactions with formulas and variables, and Instances created by Since Last Run. red-ale runs Since Last Run only when asked, as a proposal you apply.
related: concept/loans-and-interest, concept/budgets, method/scheduledTransaction.create, method/scheduledTransaction.sinceLastRun, method/scheduledTransaction.apply, method/scheduledTransaction.occurrences, method/scheduledTransaction.fromTransaction, method/scheduledTransaction.loan, guide/schedule-a-transaction
from: manual/ch_Transactions.docbook#trans-sched
---
A **scheduled transaction** is a Transaction that recurs: rent, salary, a loan repayment, a depreciation entry. GnuCash
calls it *SX* in its code; red-ale says scheduled transaction. Scheduled transactions either enter themselves when they
fall due or remind you, and you review what they made with **Since Last Run**.

## What one holds

| Part | Meaning |
|------|---------|
| Name | What you call it; used as the description of the Transactions it makes |
| Start date, end | The first possible date, and an end: a date, a number of occurrences, or none |
| Schedule | A list of **Recurrences**: a step repeated from a start, each with a multiplier, a period type (`day`, `week`, `month`, `endOfMonth`, `nthWeekday`, `lastWeekday`, `year`, `once`) and a weekend adjustment (`none`, `back`, `forward`) |
| Template transactions | The Transactions it copies, with accounts and amounts as formulas |
| Enabled, auto-create | Whether it is considered at all, and whether due Instances are created without asking, with a days-ahead window and an optional notice |

**⚠** A Schedule is a *list* of Recurrences, not one: "semi-monthly" is two monthly Recurrences, and "weekly on several
days" one weekly Recurrence per day. The next **Occurrence** is the earliest next date of any of them. A start day past
the end of a month clamps to its last day: monthly from 31 January gives 31 Jan, 29 Feb, 31 Mar.

```example
{"method": "scheduledTransaction.occurrences", "params": {"schedule": [{"mult": 1, "periodType": "month", "start": "2024-01-31"}], "startDate": "2024-01-31", "count": 3}}
{"result": {"occurrences": [{"date": "2024-01-31", "instanceCount": 1}, {"date": "2024-02-29", "instanceCount": 2}, {"date": "2024-03-31", "instanceCount": 3}]}}
```

## Template transactions

A scheduled transaction's template Transactions live in a hidden **template account** of its own, under the
**template root**, outside the Account tree: no balance, report, find, import, scrub or export sees them. Each template
split names the real Account the copy goes to, a memo and action, and a **formula** for its debit and its credit amount.

### Formulas and variables

A formula is arithmetic in GnuCash's expression syntax: numbers, `+ - * /`, parentheses, names and calls of financial
functions with `:` between arguments (`pmt(rate : nper : pv : fv : type)`). It is evaluated exactly. A formula that
is a single number in parentheses is negated, by the accounting convention: `(5)` is minus 5.

Every name in a formula is a **variable**, given a value for each Instance before it is created. `i` is predefined, the
**Instance count**, starting at 1; it is a counter of Instances acted on, not of Transactions made, because postponing
counts and GnuCash overwrites it from the last Instance acted on. A split in another Commodity than the Transaction
currency gets an exchange-rate variable such as `EUR -> USD`. The financial functions `pmt`, `ipmt`, `ppmt`,
`futureValue`, `computeInterestIncrement` and `periodic_rate` are supported; others are refused with
[`formula`](error/formula). See [Loans and interest](concept/loans-and-interest).

An unparsable formula is refused when the scheduled transaction is made. A template that must balance and cannot is
refused with [`unbalanced`](error/unbalanced).

## Making one

- [`scheduledTransaction.create`](method/scheduledTransaction.create) from a schedule and template Transactions.
- [`scheduledTransaction.fromTransaction`](method/scheduledTransaction.fromTransaction) from an existing Transaction, as
  GnuCash's "Schedule" action does: the quickest way to make one.
- [`scheduledTransaction.loan`](method/scheduledTransaction.loan) from a loan's terms, GnuCash's loan assistant.
- [`scheduledTransaction.occurrences`](method/scheduledTransaction.occurrences) previews the dates of one, saved or not.
- [`scheduledTransaction.update`](method/scheduledTransaction.update), `.delete`, `.get` and `.list` do the rest. Deleting an
  Account that a scheduled transaction uses is refused, naming it.

## Since Last Run

**Since Last Run** creates the Transactions of every enabled scheduled transaction that have fallen due since it last
ran, and reminds you of those coming. GnuCash runs it whenever a book opens. **red-ale never changes a book when it opens
it: you ask**, in two calls, like an import:

1. [`scheduledTransaction.sinceLastRun`](method/scheduledTransaction.sinceLastRun) changes nothing. It returns a
   *proposal* of **Instances**, each an Occurrence with its date, Instance count, variable values and a state.
2. Read it. Change an Instance's state, and give values for variables the formulas need.
3. [`scheduledTransaction.apply`](method/scheduledTransaction.apply) applies the proposal with your choices as one change.
   [`scheduledTransaction.discard`](method/scheduledTransaction.discard) drops it.

An Instance's state is one of: *toCreate* (create its Transactions), *reminder* (nothing; it is proposed again next time),
*postponed* (saved as a **Deferred instance** so Since Last Run proposes it again) or *ignored* (skipped for good). *created*
appears in results only. The server holds a proposal until it is applied, discarded, or pushed out by 16 newer ones, and
`apply` refuses a proposal made against a different state of the book (`conflict`). Instances dated before the book's
auto read-only threshold are marked, since Transactions there cannot be created.

Deferred instances cannot be held by an SQLite book.

## Forecasts

The reports [Balance Forecast](report/balance-forecast) and
[Future Scheduled Transactions Summary](report/future-scheduled-transactions-summary) read your scheduled transactions
without creating anything.
