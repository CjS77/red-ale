<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/closing-the-books-and-accounting-period
title: Closing the books and the accounting period
summary: The accounting period (fiscal year bounds, kept in the Configuration) and what book.closeBooks does - one closing Transaction per currency for income and for expenses, moved into equity - and why you rarely need it. Not book.close.
related: concept/dates-and-time-zones, concept/configuration, concept/account-types, method/book.closeBooks, method/period.get, method/period.set, method/period.dates
from: manual/ch_Tools_Assistants.docbook#tool-close-book-workings
---
## The accounting period

Reports and date options speak of "this year", "start of the accounting period" and so on. The **accounting period** is
the start and end bounds that those mean, plus a **fiscal year end** (a month and day). Each bound is a relative choice
(`year`, `previous-quarter`, `start of this month`, ...) or a date.

**⚠** *Accounting period* means a configured pair of bounds, not a general period of time such as a month or quarter.
It has nothing to do with a [Budget period](concept/budgets). It is a preference of the person running red-ale, kept
in the Configuration (`accountingPeriod.start`, `.end`, `.fiscalYearEnd`) and never stored in the book; without a
configured fiscal year end, red-ale uses the book's own fiscal-year-end Slot.

- [`period.get`](method/period.get) reads the settings and [`period.set`](method/period.set) changes them; both are shortcuts to
  [`config.get`](method/config.get) and [`config.set`](method/config.set).
- [`period.dates`](method/period.dates) turns a setting and a date into the first and last day.

A fiscal year starts the day after the previous fiscal year ends: with a year ending 31 March, 15 July 2024 falls in
2024-04-01 to 2025-03-31. Day boundaries are local ([Dates and time zones](concept/dates-and-time-zones)).

## Closing the books

**Closing the books** zeroes the income and expense Accounts at a date by moving their balances into equity.
[`book.closeBooks`](method/book.closeBooks) does what GnuCash's Close Book assistant does, in one step, given:

- `date`, the closing date;
- `incomeTotalAccount` and `expenseTotalAccount`, `EQUITY` Accounts that take the income and the expense (they may be
  the same Account);
- an optional `description` for the Transactions.

For income, then for expenses, it makes **one closing Transaction per currency**. Each has a Split in every income (or
expense) Account of that currency with the balance as of the end of the closing day, reversed, and one equity Split with
their total. When a currency's total does not fit the chosen Account it goes to a sub-account named after the currency's
mnemonic, made if need be. A **closing Transaction** carries the slot `book_closing`, which reports can leave out.
Accounts with a zero balance are skipped. With no balances nothing is made and the call succeeds.

**⚠** This is not [`book.close`](method/book.close), which closes the Session. After `book.closeBooks` the book stays open
and editable.

### What it does not do

It deletes no Account and no Transaction, creates no file and hides no Account. To undo it, delete the closing
Transactions. GnuCash records no closing date, so nothing else marks the book.

### Do you need it?

No. GnuCash's built-in reports work out retained earnings between periods without closing, and closing makes the
standard reports less useful: they do not understand closing Transactions, so it looks to them as if each Account's net
income for the closed period was zero. If you close your books, be prepared for inaccuracies in the standard reports of
closed periods. On the other hand the Account tree then shows current-period income and expense figures.
