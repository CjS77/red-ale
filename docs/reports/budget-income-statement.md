<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/budget-income-statement
related: method/report.run, method/report.options, report/budget-profit-loss, report/budget-report, report/income-statement
---
The income statement for a budget: the budgeted revenue and expenses over the budget's periods, with the net income
the budget would give. Use it to see what the budget plans to earn and spend. It has the same figures as the [Budget
Profit & Loss](report/budget-profit-loss), which GnuCash offers under both names. In GnuCash: Reports ▸ Budget ▸
Budget Income Statement.

The options that matter are `General/Budget` and, with `General/Report for range of budget periods`, `General/Range
start` and `Range end`; the rest are those of the [Income Statement](report/income-statement).

## Examples

```example fixture=reports/budget now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Budget Income Statement"}}
{"result": {"title": " Budget Income Statement for Budget test budget", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", {"role": "total", "cells": [{"text": "Total Revenue", "…": "…"}, {"display": "$195.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
