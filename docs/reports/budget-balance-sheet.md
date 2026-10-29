<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/budget-balance-sheet
related: method/report.run, method/report.options, report/budget-report, report/budget-income-statement, report/balance-sheet
---
The balance sheet as it would be at the end of a budget's last period if the budget came true: the real balances plus
the budget's amounts. Assets are split into existing, allocated (what the budget adds) and unallocated. Use it to see
where a budget would leave you. In GnuCash: Reports ▸ Budget ▸ Budget Balance Sheet.

The option that matters is `General/Budget`, the budget to use (the book's default budget if you leave it out); the
other options are those of the [Balance Sheet](report/balance-sheet).

## Examples

```example fixture=reports/budget now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Budget Balance Sheet"}}
{"result": {"title": " Budget Balance Sheet test budget", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", "…", "…", "…", {"role": "total", "cells": [{"text": "Total Assets", "…": "…"}, {"display": "$3,223.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
