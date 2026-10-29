<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: gnucash/transaction-menu
title: Transaction menu and the register
summary: GnuCash's register and Transaction menu mapped to methods - entering a Transaction is transaction.post, the Transfer dialog is transaction.transfer, and Void, Unvoid, Reverse, Delete, Duplicate, document links and the register's columns each have a method or a stated replacement.
related: concept/transactions-and-splits, concept/value-and-amount, concept/double-entry, gnucash/windows-and-registers, method/transaction.post, method/transaction.update, method/transaction.transfer, method/transaction.void, method/transaction.reverse, guide/transfer-between-accounts
from: manual/ch_GUIMenus.docbook#Trans-transaction-menu
---
GnuCash enters Transactions in an Account **register**, a ledger of one Account's Splits. red-ale has no register. You
post a whole Transaction with all its Splits in one call, and read an Account's Transactions back with a list. The
register's menus map as follows.

## Transaction menu

| GnuCash | red-ale |
|---|---|
| Enter Transaction (the Enter key) | [`transaction.post`](method/transaction.post): a date, a description and two or more Splits whose values sum to zero |
| Cancel Transaction | Not needed: nothing is recorded until you call `post` |
| Delete Transaction | [`transaction.delete`](method/transaction.delete) |
| Remove Other Splits | [`transaction.update`](method/transaction.update) with `removeSplits`; the result must still balance and keep a Split |
| Void Transaction | [`transaction.void`](method/transaction.void), with a `reason` |
| Unvoid Transaction | [`transaction.unvoid`](method/transaction.unvoid) |
| Add Reversing Transaction | [`transaction.reverse`](method/transaction.reverse): a new Transaction with every value negated, dated today unless you give `date` |
| Duplicate Transaction | **No dedicated method.** [`transaction.get`](method/transaction.get) it and [`transaction.post`](method/transaction.post) the same Splits with a new date and num |
| Cut, Copy, Paste Transaction | The same: get, then post. Delete the original for a cut |
| Manage Document Link... | `docLink` on [`transaction.post`](method/transaction.post) and [`transaction.update`](method/transaction.update); an empty value removes the link |
| Open Linked Document | [`docLink.transactions`](method/docLink.transactions) lists the Transactions that have a link, each with the URI it opens. A link with no `scheme://` is relative to the path head ([`docLink.pathHead`](method/docLink.pathHead), [`docLink.setPathHead`](method/docLink.setPathHead)). red-ale does not open the file |
| Jump to Invoice | A Transaction posted from a business document names it; [`invoice.get`](method/invoice.get) reads the document ([Business](gnucash/business-menu)) |
| Edit > Assign as payment... | [`payment.fromTransaction`](method/payment.fromTransaction) |

## Actions menu of the register

| GnuCash | red-ale |
|---|---|
| Split Transaction | Always: a Transaction carries all its Splits. A "basic ledger" two-line Transaction is two Splits |
| Edit Exchange Rate | The Split `amount` and `value` ([Value and amount](concept/value-and-amount)); or [`transaction.update`](method/transaction.update) with `recordPrice` |
| Schedule... | [`scheduledTransaction.fromTransaction`](method/scheduledTransaction.fromTransaction) |
| Blank Transaction, Go to Date, Jump to the other account | Not needed. [`transaction.list`](method/transaction.list) takes an Account and a date range; [`transaction.find`](method/transaction.find) finds Splits |
| Transfer... | [`transaction.transfer`](method/transaction.transfer), below |

## The register's columns

A line of a register is a Split. Its columns map to a Split's fields:

| Register | Field |
|---|---|
| Date | `datePosted` of the Transaction |
| Num | `num` of the Transaction; with the book option `splitActionForNum` on, the Split's `action`. The Split's `number` field gives whichever applies |
| Description | `description` |
| Transfer | The other Splits' Accounts (`account`) |
| R | `reconcileState`: `n`, `c`, `y`, `f`, `v` ([Reconciliation](concept/reconciliation)) |
| Deposit, Withdrawal, and the other debit and credit labels | The `value` and `amount` of the Split: positive is a debit, negative a credit. Methods always use this raw sign. GnuCash's formal accounting labels and reversed-sign display are display preferences and are not modelled ([Double entry](concept/double-entry)) |
| Balance | [`account.balance`](method/account.balance), with `asOf` for another date |
| Memo, Action | `memo`, `action` of the Split |
| Notes | `notes` of the Transaction |
| Shares, Price (a stock register) | `amount` and `value / amount` ([Investments](concept/investments)) |

Autocompletion, the Tab-through entry, Cut and Paste of Splits, and the register styles (Basic Ledger, Auto-Split Ledger,
Transaction Journal) have no counterpart. The Transaction Journal is the shape of a Transaction in the API, and the
[General Journal](report/general-journal) report lists them all.

## The Transfer dialog

GnuCash's Transfer dialog makes a two-Split Transaction between two Accounts and, when their Commodities differ,
collects an exchange rate or the amount received. [`transaction.transfer`](method/transaction.transfer) is that dialog:

- `from` (the account that gives, which must hold a currency), `to`, `amount` (in the `from` currency), `date`,
  `description`, `num`, `notes` and `memo`;
- for Accounts in different Commodities, the `rate` (units of the `to` Commodity per unit of the `from` currency) or the
  `toAmount` received, or neither to use the price database's rate for the date. With no price at all the transfer is
  refused;
- `recordPrice` (default true) records the rate as a Price, as the dialog does.

The result gives the Transaction, the amount received, the rate and what happened to the Price. Entering a transfer fee is a
Split you add by posting the Transaction yourself, as it is in GnuCash. The dialog's Fetch Rate button is
[`price.fetchQuotes`](method/price.fetchQuotes) ([Prices and exchange rates](concept/prices-and-exchange-rates)).

## Read-only Transactions

The register greys out Transactions it will not let you edit. Those that the methods refuse (`conflict`) are voided ones,
ones posted from a business document, and those before the book's auto read-only threshold
([Transactions and Splits](concept/transactions-and-splits)).
