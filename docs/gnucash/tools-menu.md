<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: gnucash/tools-menu
title: Tools, scheduled transactions and budgets
summary: GnuCash's Tools menu and the Actions submenus for scheduled transactions and budgets mapped to methods - Find, General Journal, Price Database, Security Editor, Close Book, Lots, Import Map Editor, Since Last Run, Scheduled Transaction Editor, the loan assistant and Budget. The Loan Repayment Calculator and Online Banking Setup are not provided.
related: concept/scheduled-transactions, concept/budgets, concept/prices-and-exchange-rates, concept/lots-and-capital-gains, concept/closing-the-books-and-accounting-period, method/transaction.find, method/price.add, method/commodity.create, method/book.closeBooks, method/lot.list, method/scheduledTransaction.sinceLastRun, guide/find-transactions, guide/run-and-export-a-report, guide/schedule-a-transaction, guide/record-prices, guide/capital-gains-with-lots
from: manual/ch_GUIMenus.docbook#AccTree-tools-menu
---
## Tools menu

| GnuCash | red-ale |
|---|---|
| Price Database | [`price.list`](method/price.list), [`price.add`](method/price.add), [`price.update`](method/price.update), [`price.delete`](method/price.delete). The Remove Old Prices button is [`price.removeOld`](method/price.removeOld); the Get Quotes button is [`price.fetchQuotes`](method/price.fetchQuotes) ([Prices and exchange rates](concept/prices-and-exchange-rates)) |
| Security Editor | [`commodity.list`](method/commodity.list), [`commodity.create`](method/commodity.create), [`commodity.update`](method/commodity.update), [`commodity.delete`](method/commodity.delete); namespaces with [`commodity.namespaces`](method/commodity.namespaces). "Show National Currencies" is [`currency.list`](method/currency.list). The display symbol is the Commodity's `userSymbol` |
| Loan Repayment Calculator | **Not provided.** The arithmetic is in the formulas of scheduled transactions; [Loans and interest](concept/loans-and-interest) gives the equation. [`scheduledTransaction.loan`](method/scheduledTransaction.loan) returns the payment of a loan you set up |
| Close Book | [`book.closeBooks`](method/book.closeBooks): a closing date, the equity Accounts for income and for expense, a description. Not `book.close` ([Closing the books](concept/closing-the-books-and-accounting-period)) |
| Import Map Editor | [`importMap.list`](method/importMap.list), [`importMap.delete`](method/importMap.delete) |
| Transaction Linked Documents | [`docLink.transactions`](method/docLink.transactions) |
| General Journal | [`transaction.list`](method/transaction.list) per Account, or the [General Journal](report/general-journal) report, which lists every Transaction for a date range |
| Online Banking Setup... | **Not supported** ([Not in red-ale](gnucash/not-in-red-ale)) |
| Tax Report Options (Edit menu) | See [Accounts and Actions](gnucash/account-menu) |

## Find

| GnuCash | red-ale |
|---|---|
| Edit > Find... (Find Transaction) | [`transaction.find`](method/transaction.find): criteria over Splits (text, amounts, dates, states, Accounts) combined with all, any or not. The result is Splits, like the register of the results |
| Find Customer, Vendor, Employee, Job, Invoice, Bill, Voucher | The `list` methods of [Business](gnucash/business-menu) |

## Lots Editor

GnuCash's **Actions > View Lots** window assigns the Splits of a stock Account to Lots and records gains. See
[Lots and capital gains](concept/lots-and-capital-gains).

| GnuCash | red-ale |
|---|---|
| The Lots in This Account panel (and "Show only open lots") | [`lot.list`](method/lot.list) with `state` of `all`, `open` or `closed`; [`lot.get`](method/lot.get) reads one with its Splits |
| The Splits free panel | [`lot.freeSplits`](method/lot.freeSplits), which can show the gain each Split would realise against a Lot |
| New Lot, Delete Lot | [`lot.create`](method/lot.create), [`lot.delete`](method/lot.delete) |
| Title, Notes | [`lot.update`](method/lot.update) |
| The >> and << buttons | [`lot.assignSplit`](method/lot.assignSplit), [`lot.removeSplit`](method/lot.removeSplit) |
| Scrub Account, Scrub Lot | [`lot.scrubAccount`](method/lot.scrubAccount), [`lot.scrub`](method/lot.scrub) |
| Compute capital gains | [`lot.computeGains`](method/lot.computeGains) |
| Lots of invoices and payments | Created by [`invoice.post`](method/invoice.post) and the payment methods; [`lot.list`](method/lot.list) lists them too |

## Scheduled transactions

| GnuCash | red-ale |
|---|---|
| Actions > Scheduled Transactions > Scheduled Transaction Editor | [`scheduledTransaction.list`](method/scheduledTransaction.list), `.get`, `.create`, `.update`, `.delete` |
| Since Last Run... | [`scheduledTransaction.sinceLastRun`](method/scheduledTransaction.sinceLastRun) builds a proposal and changes nothing; [`scheduledTransaction.apply`](method/scheduledTransaction.apply) applies it with your choices. Never automatic: GnuCash runs it whenever a book opens, red-ale only when asked |
| Mortgage & Loan Repayment... | [`scheduledTransaction.loan`](method/scheduledTransaction.loan) |
| Actions > Schedule... (in a register) | [`scheduledTransaction.fromTransaction`](method/scheduledTransaction.fromTransaction) |
| The window's calendar of upcoming dates | [`scheduledTransaction.occurrences`](method/scheduledTransaction.occurrences), and the [Future Scheduled Transactions Summary](report/future-scheduled-transactions-summary) report |

## Budget

| GnuCash | red-ale |
|---|---|
| Actions > Budget > New Budget | [`budget.create`](method/budget.create) |
| Open Budget | [`budget.get`](method/budget.get), [`budget.list`](method/budget.list) |
| Copy Budget | [`budget.copy`](method/budget.copy) |
| Delete Budget | [`budget.delete`](method/budget.delete) |
| The budget window's cells | [`budget.setAmounts`](method/budget.setAmounts), [`budget.setNotes`](method/budget.setNotes) |
| Options (name, period, number of periods) | [`budget.update`](method/budget.update) |
| Estimate | [`budget.estimate`](method/budget.estimate) |
| The sign scrub GnuCash runs when a file opens | [`budget.scrubSigns`](method/budget.scrubSigns), only when you ask |
| Default budget (Book options) | [`book.options`](method/book.options), [`book.setOptions`](method/book.setOptions) `defaultBudget` |
