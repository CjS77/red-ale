<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/transactions-and-splits
title: Transactions and Splits
summary: A Transaction is a dated, balanced exchange made of two or more Splits, each in one Account. Its dates, description, num, notes, and a Split's memo, action and reconcile state; read-only Transactions.
related: concept/double-entry, concept/value-and-amount, concept/reconciliation, method/transaction.post, method/transaction.get, method/transaction.list, method/transaction.update
from: guide/ch_basics.docbook#basics-transactions2
---
Money moves between Accounts in **Transactions**. Whenever you spend or receive money, or transfer it between
Accounts, that is one Transaction: paying a phone bill, moving money from savings to checking, depositing a paycheck.

In double entry every Transaction involves at least two Accounts, a source and a destination. red-ale records this as
a line for each Account affected, called a **Split**. A Transaction has any number of Splits, so a paycheck can credit
`Salary` once and debit `Checking`, `Taxes` and `Pension` in one Transaction. Its Split values sum to exactly zero
([Double entry](concept/double-entry)).

## A Transaction

| Field | Meaning |
|-------|---------|
| `currency` | The Transaction currency, in which every Split's value is expressed |
| `datePosted` | The date the Transaction takes effect: the date reports use. A calendar date, with no time |
| `dateEntered` | When it was first recorded; separate from the posted date, and now unless you say otherwise |
| `description` | A short free-text description (the payee, say) |
| `num` | A free-text check or reference number |
| `notes` | Longer free text. **⚠** Not a Split's memo |
| `splits` | Its Splits |

Posting a Transaction with [`transaction.post`](method/transaction.post) either records it whole or, if it does not
balance or breaks another rule, records nothing. [`transaction.update`](method/transaction.update) changes fields and
Splits the same way. See [Dates and time zones](concept/dates-and-time-zones) for what a posted date means.

## A Split

| Field | Meaning |
|-------|---------|
| `account` | The Account the Split belongs to; not a placeholder or the root |
| `value` | Its size in the Transaction currency; positive is a debit, negative a credit |
| `amount` | Its size in the Account's Commodity. **⚠** What GnuCash calls the *quantity* in a file; see [Value and amount](concept/value-and-amount) |
| `memo` | A note on this line. **⚠** Not the Transaction's `notes` |
| `action` | A free-text action field (`Buy`, `Sell`, `Split`, ...) |
| `reconcileState` | `n` new, `c` cleared, `y` reconciled, `f` frozen, `v` voided; see [Reconciliation](concept/reconciliation) |

A Split can also belong to a [Lot](concept/lots-and-capital-gains). The Transaction's `num` is what you would call the
check number; some books use the Split's `action` as the number instead (the book option `splitActionForNum`, see
[The Configuration](concept/configuration)).

## Finding them

- [`transaction.get`](method/transaction.get) returns one Transaction.
- [`transaction.list`](method/transaction.list) lists the Transactions in an Account and a date range.
- [`transaction.find`](method/transaction.find) finds Splits by any combination of text, amounts, dates, states and
  Accounts, as GnuCash's Find dialog does.

GnuCash's register is a view of one Account's Splits. red-ale has no register: ask for the Account's Transactions
and its balance.

## Changing a Transaction later

- **Void** keeps the Transaction with its Split figures set to zero, the old figures and a reason in its Slots:
  [`transaction.void`](method/transaction.void), undone by [`transaction.unvoid`](method/transaction.unvoid).
- **Reverse** creates a second Transaction with every Split negated:
  [`transaction.reverse`](method/transaction.reverse).
- **Delete** removes it: [`transaction.delete`](method/transaction.delete).

## Read-only Transactions

Some Transactions refuse change (`conflict`):

- voided ones, and those a business document posted, which carry a read-only mark;
- those posted before the book's auto read-only threshold, when it is set (the book option `autoReadOnlyDays`),
  which GnuCash's register shows behind its "red line".

The threshold is a book option: see [`book.setOptions`](method/book.setOptions).
