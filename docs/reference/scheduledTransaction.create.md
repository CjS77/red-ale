<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scheduledTransaction.create
related: method/scheduledTransaction.sinceLastRun, method/scheduledTransaction.occurrences, method/scheduledTransaction.fromTransaction, method/scheduledTransaction.update
---
Creates a scheduled transaction: a name, a Schedule (one or more recurrences, such as every month) and template
transactions, the transactions to enter each time it falls due. It enters nothing itself: transactions are made by
[Since Last Run](method/scheduledTransaction.sinceLastRun), when you ask for it.

A template split names a real `account` and a debit or credit formula: a number (`750`), or an expression using
variables such as `i`, the number of the occurrence, or the loan functions `pmt`, `ipmt` and `ppmt`. Arguments are
separated by `:`, not by commas. A formula with no variable and accounts of one commodity must balance, or the call
is refused with `unbalanced`. Template splits cannot use placeholder or root accounts.

The recurrences you give are normalised as GnuCash does, and the result shows what was stored; `warnings` carries
what GnuCash's editor would ask about, such as a name already in use. Auto-create is refused when a formula has a
variable other than `i`, or the splits span commodities, as GnuCash's editor refuses it.

## Examples

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "scheduledTransaction.create", "params": {"name": "Monthly rent", "startDate": "2024-05-01", "schedule": [{"mult": 1, "periodType": "month"}], "templateTransactions": [{"description": "Rent", "splits": [{"account": "Rent", "debitFormula": "750"}, {"account": "Checking", "creditFormula": "750"}]}]}}
{"result": {"guid": "…", "name": "Monthly rent", "enabled": true, "startDate": "2024-05-01", "end": {"never": true}, "lastOccurrence": null, "instanceCount": 1, "nextOccurrence": "2024-05-01", "schedule": [{"mult": 1, "periodType": "month", "start": "2024-05-01", "weekendAdjust": "none"}], "warnings": [], "…": "…"}}
{"method": "scheduledTransaction.create", "params": {"name": "", "schedule": [{"mult": 1, "periodType": "month"}]}}
{"error": {"code": 2001, "message": "please name the scheduled transaction", "data": {"reason": "name", "help": "error/invalid"}}}
```
