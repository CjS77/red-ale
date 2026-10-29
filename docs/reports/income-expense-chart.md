<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/income-expense-chart
related: method/report.run, method/report.options, report/income-expense-linechart, report/cash-flow-barchart
---
A bar chart of the selected income and expense accounts' totals in each step between two dates, with the net profit.
Use it to see how income compares with spending over time. `report.exportCsv` returns its data as CSV. In GnuCash:
Reports ▸ Income & Expense ▸ Income & Expense Barchart.

The options that matter are `Accounts/Accounts`, `General/Start Date` and `End Date` and `General/Step Size`.

## Examples

```example fixture=reports/net-days now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Income/Expense Chart", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000002", "a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}, "Step Size": "WeekDelta"}}}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "chart", "type": "bar", "title": ["Income/Expense Chart", "03/01/24 to 03/31/24"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
