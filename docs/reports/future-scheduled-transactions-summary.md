<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/future-scheduled-transactions-summary
related: method/report.run, method/report.options, report/balance-forecast, report/account-summary
---
The effect the scheduled transactions will have on each account between two dates, one row for each account that they
touch. Use it to see what recurring bills and income will do before they are due. It does not enter anything: the
scheduled transactions are only read. In GnuCash: Reports ▸ Future Scheduled Transactions Summary.

The options that matter are `General/Start Date` and `End Date` and the `Accounts` options. Make scheduled
transactions with [scheduledTransaction.create](method/scheduledTransaction.create).

## Examples

```example now=2024-03-15T12:00:00Z
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "scheduledTransaction.create", "params": {"name": "Monthly rent", "startDate": "2024-04-01", "schedule": [{"mult": 1, "periodType": "month"}], "templateTransactions": [{"description": "Rent", "splits": [{"account": "Rent", "debitFormula": "750"}, {"account": "Checking", "creditFormula": "750"}]}]}}
{"result": "…"}
{"method": "report.run", "params": {"report": "Future Scheduled Transactions Summary", "options": {"General": {"Start Date": {"absolute": "2024-04-01"}, "End Date": {"absolute": "2024-06-30"}}}}}
{"result": {"title": " Future Scheduled Transactions Summary For Period Covering 04/01/24 to 06/30/24", "sections": [{"blocks": [{"kind": "table", "rows": [{"role": "account", "cells": ["…", {"text": "Rent", "…": "…"}, {"display": "$2,250.00", "…": "…"}], "…": "…"}, {"role": "account", "cells": ["…", {"text": "Checking", "…": "…"}, {"display": "-$2,250.00", "…": "…"}], "…": "…"}], "…": "…"}], "…": "…"}], "…": "…"}}
```
