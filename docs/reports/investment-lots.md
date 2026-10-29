<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/investment-lots
related: method/report.run, method/report.options, report/advanced-portfolio, report/investment-portfolio
---
For each investment account, its lots: when each was opened and closed, the amount and basis bought, what was sold,
what is left, and the realized and unrealized gains (short term and long term) and annualized returns. Use it to see
how each purchase has done. Warnings flag lots that look wrong, such as a split in no lot. In GnuCash: Reports ▸
Assets & Liabilities ▸ Investment Lots.

The options that matter are `General/Start date` and `End date`, `Accounts/Accounts`, the `Columns` options that turn
groups of columns on and off, `Columns/Long term gains age (years)` and `General/Price source`. An account with no
price at the end date shows `No price found`. Create lots with [lot.create](method/lot.create).

## Examples

```example fixture=reports/lots-baseline now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Investment Lots", "options": {"General": {"Start date": {"absolute": "2019-12-31"}, "End date": {"absolute": "2020-12-31"}}}}}
{"result": {"title": "Investment Lots, 12/31/19 to 12/31/20", "sections": [{"blocks": ["…", {"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", {"role": "data", "cells": ["…", {"display": "01/01/20", "…": "…"}, "…", {"text": "100. AAPL", "…": "…"}, {"display": "$5,000.00", "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
