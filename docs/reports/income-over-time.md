<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/income-over-time
related: method/report.run, method/report.options, report/expense-over-time, report/income-accounts
---
A bar chart of the selected income accounts' totals in each step between two dates, one bar group per step. Use it to
see how your income varies. `report.exportCsv` returns its data as CSV. In GnuCash: Reports ▸ Income & Expense ▸
Income Chart.

The options that matter are `Accounts/Accounts`, `General/Start Date` and `End Date` and `General/Step Size`.

## Examples

```example fixture=reports/category-daily now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Income Over Time", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}, "Step Size": "WeekDelta"}}}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "chart", "type": "bar", "title": ["Income Over Time", "03/01/24 to 03/31/24"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
