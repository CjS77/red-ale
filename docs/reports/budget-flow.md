<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/budget-flow
related: method/report.run, method/report.options, report/budget-report, report/budget-chart
---
For one budget period, the money each account of the chosen types was budgeted to receive or pay and the money that
actually went through it, grouped by account type with a total for each. Use it to see how a period's budget compares
with what happened in cash terms. In GnuCash: Reports ▸ Budget ▸ Budget Flow.

The options that matter are `General/Budget`, `General/Period` (which period, counting from 1) and `Accounts/Account`.
The first column of figures is the budget, the second the actual.

## Examples

```example fixture=reports/budget now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Budget Flow"}}
{"result": {"title": "Budget Flow: test budget - 03/01/24", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", {"role": "total", "cells": [{"text": "Total:", "…": "…"}, {"display": "$20.00", "…": "…"}, {"display": "$35.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
