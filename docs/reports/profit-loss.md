<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/profit-loss
related: method/report.run, method/report.options, report/income-statement, report/balance-sheet
---
Revenues and expenses between two dates, with the net income. It is the same report as the [Income
Statement](report/income-statement) under its other name, and takes the same options. In GnuCash: Reports ▸ Income &
Expense ▸ Profit & Loss.



## Examples

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Profit & Loss", "options": {"General": {"Start Date": {"absolute": "1978-01-01"}, "End Date": {"absolute": "1978-12-31"}}}}}
{"result": {"title": " Profit & Loss For Period Covering 01/01/78 to 12/31/78", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", "…", "…", "…", {"role": "grandTotal", "cells": [{"text": "Net income for Period", "…": "…"}, {"display": "$3,136.80", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
