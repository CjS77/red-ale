<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/income-expense-linechart
related: method/report.run, method/report.options, report/income-expense-chart, report/net-worth-linechart
---
A line chart of the income and expense accounts' totals in each step between two dates, and the net profit between
them. It is the line version of the [Income/Expense Chart](report/income-expense-chart). `report.exportCsv` returns
its data as CSV. In GnuCash: Reports ▸ Income & Expense ▸ Income & Expense Linechart.

The options that matter are `Accounts/Accounts`, `General/Start Date` and `End Date` and `General/Step Size`.

## Examples

```example fixture=reports/net-days now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Income & Expense Linechart", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000002", "a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}, "Step Size": "WeekDelta"}}}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "chart", "type": "line", "title": ["Income & Expense Linechart", "03/01/24 to 03/31/24"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
