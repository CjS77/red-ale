<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/budget-profit-loss
related: method/report.run, method/report.options, report/budget-income-statement, report/budget-report, report/profit-loss
---
The profit and loss statement for a budget: the budgeted revenue and expenses over its periods and the net income. It
gives the same figures as the [Budget Income Statement](report/budget-income-statement); GnuCash offers both names, as
it does for the [Income Statement](report/income-statement) and [Profit & Loss](report/profit-loss). In GnuCash:
Reports ▸ Budget ▸ Budget Profit & Loss.

The options that matter are `General/Budget` and, with `General/Report for range of budget periods`, `General/Range
start` and `Range end`.

## Examples

```example fixture=reports/budget now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Budget Profit & Loss"}}
{"result": {"title": " Budget Profit & Loss for Budget test budget", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", {"role": "total", "cells": [{"text": "Total Revenue", "…": "…"}, {"display": "$195.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
