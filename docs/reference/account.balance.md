<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.balance
related: method/transaction.post, method/account.create
---
Gives an account's balance, with its cleared and reconciled balances, in the account's commodity: today's, or as of
a date with `asOf`, and with its sub-accounts' balances added when `includeSubaccounts` is true. Balances use the
account's raw sign, debits positive and credits negative, so an income account's balance is usually negative.

## Examples

```example now=2024-07-15T12:00:00Z
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Salary", "type": "INCOME", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-01", "description": "Pay", "splits": [{"account": "Checking", "value": "2500.00"}, {"account": "Salary", "value": "-2500.00"}]}}
{"result": "…"}
{"method": "account.balance", "params": {"account": "Checking"}}
{"result": {"account": "…", "includesSubaccounts": false, "balances": {"commodity": "CURRENCY::USD", "balance": {"num": "250000", "denom": "100"}, "cleared": {"num": "0", "denom": "1"}, "reconciled": {"num": "0", "denom": "1"}}, "unconverted": [], "…": "…"}}
{"method": "account.balance", "params": {"account": "Salary"}}
{"result": {"balances": {"commodity": "CURRENCY::USD", "balance": {"num": "-250000", "denom": "100"}, "…": "…"}, "…": "…"}}
```
