<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/equity-statement
related: method/report.run, method/report.options, report/balance-sheet, report/income-statement, report/trial-balance
---
How the owner's capital changed between two dates: the capital at the start, the net income for the period, the
investments (equity increases), the withdrawals, the unrealized losses and the capital at the end. Use it at the close
of a period alongside the [Income Statement](report/income-statement) and [Balance Sheet](report/balance-sheet). In
GnuCash: Reports ▸ Income & Expense ▸ Equity Statement.

The options that matter are `General/Start Date` and `End Date`, `Accounts/Accounts` and the `Entries/Closing Entries
pattern`, which names the closing transactions so that they do not count as income and expenses.

## Examples

```example fixture=reports/adjusting-entries now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Equity Statement", "options": {"General": {"Start Date": {"absolute": "1970-01-01"}, "End Date": {"absolute": "2005-01-01"}}}}}
{"result": {"title": " Equity Statement For Period Covering 01/01/70 to 01/01/05", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", {"role": "heading", "cells": [{"text": "Capital, 01/01/05", "…": "…"}, {"display": "$3,115.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
