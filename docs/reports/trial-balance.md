<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/trial-balance
related: method/report.run, method/report.options, report/balance-sheet, report/equity-statement, report/income-statement
---
Each account's balance at a date as a debit or credit, with the two totals, which must be equal. Use it to check a
book before closing it. The adjusting entries in the period appear in their own columns. In GnuCash: Reports ▸ Income
& Expense ▸ Trial Balance.

The options that matter are `General/Date of Report`, `General/Start of Adjusting/Closing` (the first day of the
adjusting and closing entries), `General/Report variation`, the `Entries` patterns that name the adjusting and closing
transactions, `Accounts/Accounts` and `Levels of Subaccounts`, and `Commodities/Price Source`.

## Examples

```example fixture=reports/adjusting-entries now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Trial Balance"}}
{"result": {"title": " Trial Balance 12/31/24", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", "…", "…", "…", "…", {"role": "total", "cells": [null, {"display": "$3,086.00", "…": "…"}, {"display": "$3,086.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
