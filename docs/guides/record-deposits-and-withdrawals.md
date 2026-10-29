<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/record-deposits-and-withdrawals
title: Record deposits and withdrawals
summary: Enter income, expenses and cash withdrawals as balanced transactions, then check the balance and list what you have entered.
related: method/transaction.post, method/account.balance, method/transaction.list, method/account.create, error/unbalanced, gnucash/windows-and-registers
from: guide/ch_basics.docbook#basics-together1
---
Day-to-day bookkeeping is a sequence of transactions: your pay arrives in the bank, you buy groceries, you take cash
out of a machine. In double-entry bookkeeping each one is a date, a description and two or more splits whose values
sum to exactly zero. A *value* is in the transaction's currency; positive values are debits, negative values credits.

## The sign rule

The account you name decides what a sign means. A positive value on a bank account is a deposit, a negative one a
withdrawal. On an income account the signs run the other way, because income is a credit: pay of 2,500.00 is
`+2500.00` on the bank account and `-2500.00` on the income account. An expense is `+` on the expense account and `-`
on the account you paid from.

## Steps

1. **Make the accounts** the transaction needs (see [Set up accounts](guide/set-up-accounts)): a bank account, a
   cash account, an income account and an expense account.
2. **Record a deposit**: [transaction.post](method/transaction.post) with `datePosted`, `description` and two splits,
   the bank account `+` and the income account `-`.
3. **Record an expense**: the expense account `+` and the account that paid `-`.
4. **Record a cash withdrawal**: a transfer from the bank account to the cash account, `-` and `+`. It is neither
   income nor expense, so no category is involved.
5. **Check** the balance with [account.balance](method/account.balance), and review the entries with
   [transaction.list](method/transaction.list), optionally for one account and a date range.

If the splits do not sum to zero the call fails with [unbalanced](error/unbalanced) and nothing is recorded, so a
typing slip never leaves a half-entered transaction.

## Example

```example now=2024-07-15T12:00:00Z
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Wallet", "type": "CASH", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Salary", "type": "INCOME", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Groceries", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-01", "description": "July pay", "splits": [{"account": "Checking", "value": "2500.00"}, {"account": "Salary", "value": "-2500.00"}]}}
{"result": {"description": "July pay", "balanced": true, "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-03", "description": "ATM", "splits": [{"account": "Wallet", "value": "100.00"}, {"account": "Checking", "value": "-100.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-05", "description": "Market", "splits": [{"account": "Groceries", "value": "63.40"}, {"account": "Wallet", "value": "-63.40"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "account.balance", "params": {"account": "Checking"}}
{"result": {"balances": {"commodity": "CURRENCY::USD", "balance": {"num": "240000", "denom": "100"}, "…": "…"}, "…": "…"}}
{"method": "account.balance", "params": {"account": "Wallet"}}
{"result": {"balances": {"balance": {"num": "3660", "denom": "100"}, "…": "…"}, "…": "…"}}
{"method": "transaction.list", "params": {"account": "Wallet"}}
{"result": {"transactions": [{"description": "ATM", "…": "…"}, {"description": "Market", "…": "…"}]}}
```
