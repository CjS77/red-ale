<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/ifrs-weighted-average-cost-basis-report
related: method/report.run, method/report.options, report/investment-lots, report/advanced-portfolio
---
For stocks, a line for each buy, sell, split and return of capital with the running quantity, the weighted-average
cost per share, the cost of what was sold and the realized gain or loss, in the report currency and with the effect of
exchange rates. It is for cost basis by weighted average, as IFRS and some tax regimes require; it is not for FIFO,
LIFO or specific-lot methods, and it is not made for options. Each report also ends with notes saying so. In GnuCash:
Reports ▸ Experimental ▸ IFRS weighted-average cost basis report.

The options that matter are `General/Report's currency`, the dates, and the accounts the report reads: `General/Stock
Account`, `Proceeds Account`, `Dividend Account`, `Fees Account` and `Cap Gains Account`.

## Examples

```example fixture=reports/ifrs-cost-basis now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "IFRS weighted-average cost basis report", "options": {"General": {"Report's currency": "CAD", "Proceeds Account": "a0000000000000000000000000000005", "Fees Account": "a000000000000000000000000000000d", "Start Date": {"absolute": "2019-01-01"}, "End Date": {"absolute": "2021-01-01"}}}}}
{"result": {"title": "Average-Cost (Basis) Report: From 01/01/19 to 01/01/21. Report-currency CAD", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", {"role": "data", "cells": [{"display": "03/18/20", "…": "…"}, {"text": "Sell SPY", "…": "…"}, {"display": "-75. SPY", "…": "…"}, {"display": "75. SPY", "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
