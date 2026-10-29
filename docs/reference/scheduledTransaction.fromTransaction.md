<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scheduledTransaction.fromTransaction
related: method/scheduledTransaction.create, method/transaction.post
---
Makes a scheduled transaction from an existing transaction, as GnuCash's Schedule from transaction does. The
transaction is not changed. The scheduled transaction has one template transaction with the transaction's description,
number, notes and currency; each split keeps its memo, action and account, and its value becomes a formula (a positive
value as the debit formula, otherwise the credit formula).

`frequency` is `daily` (the default), `weekly`, `biWeekly`, `monthly`, `quarterly` or `yearly`. The first occurrence
is the first one of that frequency after the transaction's date, not the transaction's own date, unless you give
`startDate`. `yearly` is stored as every 12 months, as GnuCash stores it.

## Examples

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-05-01", "description": "Rent", "splits": [{"account": "Rent", "value": "750.00"}, {"account": "Checking", "value": "-750.00"}]}}
{"result": {"guid": "00ed0000000000000000000000000005", "…": "…"}}
{"method": "scheduledTransaction.fromTransaction", "params": {"transaction": "00ed0000000000000000000000000005", "frequency": "monthly"}}
{"result": {"name": "Rent", "startDate": "2024-06-01", "schedule": [{"mult": 1, "periodType": "month", "start": "2024-06-01", "…": "…"}], "templateTransactions": [{"description": "Rent", "splits": [{"accountName": "Rent", "debitFormula": "750.00", "…": "…"}, {"accountName": "Checking", "creditFormula": "750.00", "…": "…"}], "…": "…"}], "…": "…"}}
```
