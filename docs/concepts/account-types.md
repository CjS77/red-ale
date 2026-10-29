<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/account-types
title: Account types
summary: The five basic kinds of Account and the Account types red-ale offers (BANK, CASH, ASSET, CREDIT, LIABILITY, STOCK, MUTUAL, CURRENCY, INCOME, EXPENSE, EQUITY, RECEIVABLE, PAYABLE, TRADING), which Account can sit under which, and what each is for.
related: concept/double-entry, concept/chart-of-accounts, method/account.types, method/account.create, method/account.update, guide/invoices-and-bills, guide/credit-cards
from: guide/ch_basics.docbook#basics-accounting52
---
Bookkeeping groups everything into five kinds of thing:

| Kind | What it is |
|------|------------|
| Assets | Things you own |
| Liabilities | Things you owe |
| Equity | Overall net worth |
| Income | What increases the value of your accounts |
| Expenses | What decreases it |

Your bank balance is an asset, your mortgage a liability, your paycheck income and last night's dinner an expense.
How the five relate is the subject of [Double entry](concept/double-entry).

GnuCash divides these further into **Account types**, and every Account has exactly one. Call
[`account.types`](method/account.types) for the list with each type's fundamental kind, the kind of Commodity it holds,
the types it may sit under and the types it may be changed to once it has Splits.

## The types

| Type | Fundamental kind | Use it for |
|------|------------------|------------|
| `BANK` | asset | Checking, savings and money market accounts |
| `CASH` | asset | Cash in your wallet or a till |
| `ASSET` | asset | Other things you own: a house, a car, prepaid items |
| `RECEIVABLE` | asset | Money owed to you by customers (A/Receivable) |
| `STOCK` | asset | A holding of a security. **⚠** Shares, not warehouse stock |
| `MUTUAL` | asset | A holding of a mutual fund |
| `CURRENCY` | asset | A holding of a foreign currency; deprecated, and cannot be chosen for a new Account |
| `CREDIT` | liability | A credit card |
| `LIABILITY` | liability | Other things you owe: a loan, a mortgage |
| `PAYABLE` | liability | Money you owe vendors (A/Payable) |
| `INCOME` | income | Salary, interest, dividends, sales |
| `EXPENSE` | expense | Groceries, rent, fees |
| `EQUITY` | equity | Opening balances and the owner's stake |
| `TRADING` | none | Balances multi-currency Transactions; see [Trading accounts](concept/trading-accounts) |
| `ROOT` | none | The nameless top of the Account tree; one per book, never created by you |

**⚠** *Asset* and *liability* are also accounting ideas wider than the types of the same name: a `BANK` Account holds
an asset in the first sense and is not of type `ASSET`. When this Manual means the type it writes it in capitals.

## What may sit under what

An Account's parent must be compatible with its type, or [`account.create`](method/account.create) refuses it:

- the asset and liability types (`BANK` to `PAYABLE` above) may share a branch with each other and sit directly under
  the root;
- `INCOME` and `EXPENSE` may share a branch;
- `EQUITY` goes under `EQUITY` or the root, and `TRADING` under `TRADING` or the root.

A parent that exists only to group others is usually a placeholder: it accepts no Splits. See
[The chart of accounts](concept/chart-of-accounts).

## Changing a type

An Account with Splits can change only to a type compatible with its current one, for example `BANK` to `ASSET`.
`RECEIVABLE` and `PAYABLE` can never change once they have Splits, because business documents depend on them.
[`account.update`](method/account.update) checks the sub-accounts too when you retype them with their parent.

## Expenses and credit cards

An Account of type `EXPENSE` replaces the "category" of other programs: set one up for each thing you want to track,
as detailed or as general as you like, and you can move Transactions between them later. There is nothing to
reconcile against, as no statement exists, though you may [reconcile](concept/reconciliation) one all the same. Its
balance only grows; to see one period's spending run an [Income Statement](report/income-statement), or close the
books ([Closing the books and the accounting period](concept/closing-the-books-and-accounting-period)).

A credit card is a short-term loan, not an expense: the purchases are the expenses, and the card Account (`CREDIT`)
holds what you owe. One payment to the card company is a transfer from your bank Account to the `CREDIT` Account.
