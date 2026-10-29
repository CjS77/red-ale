<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/advanced-portfolio
related: method/report.run, method/report.options, report/investment-portfolio, report/investment-lots
---
Shows each investment account's holding at a date: shares, price, basis, value, the money paid in and taken out,
realized and unrealized gains, income, fees and the return. It is the detailed view of a portfolio; the [Investment
Portfolio](report/investment-portfolio) report is the short one. In GnuCash: Reports ▸ Assets & Liabilities ▸ Advanced
Portfolio.

The options that matter are `General/Date`, `Accounts/Accounts` (the stock and fund accounts, or their parents),
`General/Basis calculation method`, `General/How to report brokerage fees` and `General/Price Source`, which decides
what price values the shares.

## Examples

```example fixture=reports/stock-test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Advanced Portfolio", "options": {"General": {"Date": {"absolute": "1981-04-01"}}}}}
{"result": {"title": "Advanced Portfolio 04/01/81", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", {"role": "grandTotal", "cells": [{"text": "Total", "…": "…"}, "…", "…", "…", "…", "…", {"display": "$484.88", "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
