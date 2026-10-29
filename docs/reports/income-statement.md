<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/income-statement
related: method/report.run, method/report.options, report/profit-loss, report/balance-sheet, report/income-statement-multicolumn
---
Revenues and expenses between two dates, with a total for each and the net income for the period. Use it to see
whether you earned more than you spent. It is the same report as [Profit & Loss](report/profit-loss), which GnuCash
lists as a second entry. In GnuCash: Reports ▸ Income & Expense ▸ Income Statement.

The options that matter are `General/Start Date` and `End Date`, `Accounts/Accounts` and `Levels of Subaccounts`,
`Commodities/Report's currency` and `Price Source`. A foreign account's total is converted at the date the price source
gives; `Show Exchange Rates` lists the rates.

## Examples

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Income Statement", "options": {"General": {"Start Date": {"absolute": "1978-01-01"}, "End Date": {"absolute": "1978-12-31"}}}}}
{"result": {"title": " Income Statement For Period Covering 01/01/78 to 12/31/78", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", "…", "…", "…", {"role": "grandTotal", "cells": [{"text": "Net income for Period", "…": "…"}, {"display": "$3,136.80", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
