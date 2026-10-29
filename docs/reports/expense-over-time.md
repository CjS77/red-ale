<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/expense-over-time
related: method/report.run, method/report.options, report/income-over-time, report/expense-accounts
---
A bar chart of the selected expense accounts' totals in each step between two dates, one bar group per step. Use it to
see how your spending moves from week to week or month to month. `report.exportCsv` returns its data as CSV. In
GnuCash: Reports ▸ Income & Expense ▸ Expense Chart.

The options that matter are `Accounts/Accounts`, `General/Start Date` and `End Date`, `General/Step Size`, and
`Display/Show Average`, which averages over a longer period.

## Examples

```example fixture=reports/category-multi now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Expense Over Time", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000001"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}, "Step Size": "WeekDelta"}}}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "chart", "type": "bar", "title": ["Expense Over Time", "03/01/24 to 03/31/24"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
