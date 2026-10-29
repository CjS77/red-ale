<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/balance-sheet
related: method/report.run, method/report.options, report/balance-sheet-multicolumn, report/income-statement, report/trial-balance, report/equity-statement
---
Assets, liabilities and equity on one date, with a total for each. Use it to see what you own, what you owe and what
is left. Income and expenses to date appear as `Retained Earnings` in equity, with `Unrealized Gains` or `Unrealized
Losses` for the effect of prices, so that assets always equal liabilities plus equity. In GnuCash: Reports ▸ Assets &
Liabilities ▸ Balance Sheet.

The options that matter are `General/Balance Sheet Date`, `Accounts/Accounts` and `Levels of Subaccounts`,
`Commodities/Report's currency` and `Price Source`, and the `Display` options that choose which sections and totals
show. `General/Single column Balance Sheet` and `General/Use standard US layout` choose the layout.

## Examples

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Balance Sheet", "options": {"General": {"Balance Sheet Date": {"absolute": "1978-12-31"}}}}}
{"result": {"title": " Balance Sheet 12/31/78", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", {"role": "total", "cells": [{"text": "Total Assets", "…": "…"}, {"display": "$3,057.80", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
