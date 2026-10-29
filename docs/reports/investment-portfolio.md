<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/investment-portfolio
related: method/report.run, method/report.options, report/advanced-portfolio, report/securities
---
A short table of the investment accounts at a date: the shares held, the price and the value, with a total. Use it for
a quick portfolio value; the [Advanced Portfolio](report/advanced-portfolio) adds basis and gains. In GnuCash: Reports
▸ Assets & Liabilities ▸ Investment Portfolio.

The options that matter are `General/Date`, `Accounts/Accounts`, `General/Price Source` and `General/Share decimal
places`. Accounts with no shares are left out unless `Accounts/Include accounts with no shares` is on.

## Examples

```example fixture=reports/stock-test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Investment Portfolio", "options": {"General": {"Date": {"absolute": "1981-04-01"}}}}}
{"result": {"title": "Investment Portfolio 04/01/81", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", {"role": "grandTotal", "cells": [{"text": "Total", "…": "…"}, {"display": "$252.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
