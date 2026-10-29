<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/cash-flow-barchart
related: method/report.run, method/report.options, report/cash-flow, report/income-expense-chart
---
A bar chart of the money into and out of the selected accounts in each step between two dates. It is the chart version
of the [Cash Flow](report/cash-flow) report. Use it to see which days or months you spent more than you received. In
GnuCash: Reports ▸ Income & Expense ▸ Cash Flow Barchart.

The options that matter are `Accounts/Accounts`, `General/Start Date` and `End Date`, and `General/Step Size`.

## Examples

```example fixture=reports/cashflow-barchart now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Cash Flow Barchart", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000003", "a0000000000000000000000000000004"]}, "General": {"Start Date": {"absolute": "2024-03-02"}, "End Date": {"absolute": "2024-03-03"}, "Step Size": "DayDelta"}}}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "chart", "type": "bar", "title": ["Cash Flow Barchart", "03/02/24 to 03/03/24"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
