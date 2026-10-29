<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/transaction.post
related: method/account.create, method/account.balance, error/unbalanced
---
Records a transaction: a date, a description and two or more splits, each moving a value into or out of one
account. The splits' values must sum to exactly zero, or the transaction is refused with `unbalanced` and the book is
left as it was. Values are in the transaction's currency; a split in an account of another commodity also needs its
`amount` in that commodity, which is how a purchase of shares or foreign currency records its price.

Positive values are debits, negative values credits: paying 750.00 of rent from a bank account is `-750.00` on the
bank account and `750.00` on the expense account.

## Examples

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-01", "description": "Rent", "splits": [{"account": "Checking", "value": "-750.00"}, {"account": "Rent", "value": "750.00"}]}}
{"result": {"guid": "…", "currency": "CURRENCY::USD", "description": "Rent", "datePosted": "2024-07-01", "balanced": true, "splits": [{"account": "…", "accountName": "Rent", "value": {"num": "75000", "denom": "100"}, "amount": {"num": "75000", "denom": "100"}, "…": "…"}, {"accountName": "Checking", "value": {"num": "-75000", "denom": "100"}, "…": "…"}], "…": "…"}}
```

Splits that don't balance are refused, and nothing is recorded:

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"description": "Rent", "splits": [{"account": "Checking", "value": "-750.00"}, {"account": "Rent", "value": "705.00"}]}}
{"error": {"code": 2002, "message": "…", "data": {"imbalance": "-45.00", "currency": "USD", "help": "error/unbalanced"}}}
```
