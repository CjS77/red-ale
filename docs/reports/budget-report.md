<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/budget-report
related: method/report.run, method/report.options, report/budget-chart, report/budget-flow, method/budget.list
---
A table of a budget, one row for each account and one column pair for each budget period, with the budgeted and the
actual amount and the difference between them. This is GnuCash's main budget report: use it to review how a budget is
doing, period by period. In GnuCash: Reports ▸ Budget ▸ Budget Report.

The options that matter are `General/Budget`, `General/Report for range of budget periods` with `Range start` and
`Range end`, the `Display` options that switch the budget, actual, difference and notes columns on and off, and
`Accounts/Account Display Depth`. An account with no budget amount shows `.` in its budget cell.

## Examples

```example fixture=reports/budget now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Budget Report"}}
{"result": {"title": "Budget Report: test budget ", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", {"role": "account", "cells": [{"text": "Asset", "…": "…"}, {"display": "$20.00", "…": "…"}, {"display": "$35.00", "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
