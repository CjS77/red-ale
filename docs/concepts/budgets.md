<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/budgets
title: Budgets
summary: A Budget plans an amount per Account for each Budget period. Budget periods, set and unset amounts, natural signs, notes, actuals, estimating from past Transactions, and the budget reports.
related: concept/reports, concept/scheduled-transactions, method/budget.create, method/budget.setAmounts, method/budget.setNotes, method/budget.estimate, method/budget.get, report/budget-report, guide/create-a-budget
from: guide/ch_budgets.docbook#budget_concepts1
---
A **budget** is a tool for estimating expected income and expenses. It lets you plan how your finances should change
over a period, and compare your actual Transactions to the plan.

## Kinds of budget

- A **cash budget** plans receipts and disbursements: where the money comes from, where it goes, how much.
- An **expense budget** plans what you spend; it ignores things like loan repayment, though it counts interest. Buying
  100 of groceries on a credit card is a 100 expense; paying the card bill of 110 adds 10 of interest expense, and the
  repayment of the 100 is not budgeted.
- A **capital budget** plans how to pay for a large future expense, with savings and borrowing.

The kind is decided by which Accounts you give amounts to. Expense Accounts only make an expense budget; assets,
liabilities, income and expenses together make a cash flow budget. Your Accounts must be at least as specific as your
budget: you cannot budget for electricity and water separately with only `Expenses:Utilities`.

## What a Budget holds

A Budget has a name, a description, a number of **Budget periods** and a Recurrence that dates them. The common periods
are a month or a year; several periods in one Budget let you plan for seasons (an annual budget of 12 months, with more
for heating in winter).

Budget period *p* is numbered from 0. It runs from the Recurrence's *p*-th occurrence to the day before the next.
**⚠** A Budget period has nothing to do with the *accounting period* of the Configuration.

For each Account and period the Budget has:

- a **Budget amount**: what you plan for that Account's balance to change by. Amounts are kept as written, with no
  rounding to the SCU;
- a **Budget note**: free text.

An amount is **set** or **unset**. An explicit zero is set. Only an unset amount lets a report sum the Account's children
instead. **⚠** Unset is not zero, though GnuCash reads both as 0; red-ale's methods never confuse them, `null` is unset
and `"0"` a set zero.

A book may hold several Budgets. The one reports start from is the **default budget**, named by a book option
([`book.options`](method/book.options)); if none is named, the first.

## Natural signs

Budget amounts carry the engine's sign, as the actuals do: debits positive, so an expense budget is positive, an income
budget negative and a liability increase negative ([Double entry](concept/double-entry)). GnuCash has stored them so
since 3.8, and the Book feature `Use natural signs in budget amounts` says a book's amounts are. **⚠** *Natural* means
unreversed, not the sign a report shows.

A book from before 3.8 holds Budgets without the feature and with amounts signed as the user's preference then. red-ale
never converts on opening. Its budget methods that change anything refuse with `conflict` until you run
[`budget.scrubSigns`](method/budget.scrubSigns), GnuCash's own scrub; [`budget.list`](method/budget.list) tells you when
(`needsSignScrub`).

## Making one

1. [`budget.create`](method/budget.create) with a name, the number of periods and a Recurrence (the default: monthly from
   the start of this month). The first Budget in a book sets the natural-signs feature.
2. Fill in amounts with [`budget.setAmounts`](method/budget.setAmounts), by hand, all or none, for many cells at once, or
3. let [`budget.estimate`](method/budget.estimate) work them out from past Transactions: it takes a start date, looks at
   the same number of periods from there and stores each Account's change, rounded to a number of significant digits
   (`useAverage` stores the average in every period). It overwrites every period of the Accounts you name, so it is a
   destructive method.
4. Add notes with [`budget.setNotes`](method/budget.setNotes).

[`budget.copy`](method/budget.copy) copies a Budget, [`budget.update`](method/budget.update) changes its options and
[`budget.delete`](method/budget.delete) removes it. [`budget.get`](method/budget.get) returns it with its periods, amounts and
notes.

## Actuals and reports

The **actual** for an Account and period is what that Account and its sub-accounts did in the period: the change in its
no-closing balance (Closing transactions left out), sub-accounts converted to the Account's commodity. It is computed
and never stored. The [Budget Report](report/budget-report) shows the planned and actual amounts side by side for each
period; the [Budget Income Statement](report/budget-income-statement) and [Budget Balance Sheet](report/budget-balance-sheet) are
the statements as the budget predicts them, and the [Budget Flow](report/budget-flow) and [Budget Chart](report/budget-chart)
compare them. Pick the Budget with the report's *Budget* option. See [Reports](concept/reports).
