<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/double-entry
title: Double entry
summary: Every Transaction moves value between Accounts and its Split values sum to exactly zero. The accounting equation, debits and credits, and the raw sign red-ale uses for balances.
related: concept/account-types, concept/transactions-and-splits, concept/value-and-amount, method/transaction.post, method/account.balance, error/unbalanced
from: guide/ch_basics.docbook#basics-accounting1
---
GnuCash, and so red-ale, keeps books by double entry. You need little accounting to use red-ale, but this idea
explains most of what its methods insist on.

## The accounting equation

Everything a book tracks falls under five kinds of Account (see [Account types](concept/account-types)): assets,
liabilities, equity, income and expenses. They relate by the accounting equation:

```
Assets - Liabilities = Equity + (Income - Expenses)
```

Your net worth (assets less liabilities) is your equity, and income raises it while expenses lower it. The equation
must always hold. A paycheck that raises an income Account must raise an asset by the same amount; a loan that raises
an asset also raises a liability.

## Every change has two sides

For each change in one Account there is a balancing change in another. This is the Principle of Balance, and it is why
a [Transaction](concept/transactions-and-splits) has at least two Splits whose values sum to exactly zero. A
Transaction that does not sum to zero is refused with [`unbalanced`](error/unbalanced), and nothing is recorded.

## Debits and credits

Moving the terms of the equation so that assets and expenses stand on one side gives:

```
Assets + Expenses = Liabilities + Equity + Income
```

Accounts on the left are *debit balance* accounts: a debit increases them and a credit decreases them. Accounts on
the right are *credit balance* accounts: a credit increases them and a debit decreases them.

| Account kind | Debit | Credit |
|--------------|-------|--------|
| Asset, Expense | increases | decreases |
| Liability, Equity, Income | decreases | increases |

Debit and credit do not mean increase and decrease. In everyday speech "credit" sounds good; in accounting it means
only the right-hand side of the ledger.

## The raw sign

red-ale does not show an Account's balance as "how much you have" or "how much you owe". It uses the **raw sign** that
GnuCash's engine uses: debits are positive and credits negative. So a bank Account with money in it has a positive
balance, a credit card you owe on has a negative one, and an income Account has a negative balance, because income is
credits. GnuCash's registers and reports flip the sign of credit Accounts for display (its "Reverse Balanced
Accounts" preference); the methods do not. A `value` in [`transaction.post`](method/transaction.post) follows the same
rule: positive is a debit, negative a credit.

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Salary", "type": "INCOME", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-01", "description": "Pay", "splits": [{"account": "Checking", "value": "2500.00"}, {"account": "Salary", "value": "-2500.00"}]}}
{"result": "…"}
{"method": "account.balance", "params": {"account": "Checking"}}
{"result": {"balances": {"commodity": "CURRENCY::USD", "balance": {"num": "250000", "denom": "100"}, "…": "…"}, "…": "…"}}
{"method": "account.balance", "params": {"account": "Salary"}}
{"result": {"balances": {"commodity": "CURRENCY::USD", "balance": {"num": "-250000", "denom": "100"}, "…": "…"}, "…": "…"}}
```

## Income and expenses are Accounts

If you come from a program that files money under categories, an income or expense Account takes the place of a
category. Name the expense Account where you would have named the category: the transaction is the same. See
[Coming from GnuCash](concept/coming-from-gnucash) for the other words that change.
