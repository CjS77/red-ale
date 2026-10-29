<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/expenses-vs-day-of-week
related: method/report.run, method/report.options, report/income-vs-day-of-week, report/expense-accounts
---
A pie chart of the expenses between two dates by the weekday they fell on. Use it to see whether you spend more at
weekends. `reports.weekStart` in the Configuration decides which day the week starts on. In GnuCash: Reports ▸
Examples ▸ Expenses vs. Day of Week.

The options that matter are `Accounts/Accounts`, `General/Start Date` and `End Date` and `Accounts/Include
Sub-Accounts`.

## Examples

```example fixture=reports/category-multi now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Expenses vs. Day of Week", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000001"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}}}}}
{"result": {"title": "Expenses vs. Day of Week", "sections": [{"blocks": [{"kind": "chart", "type": "pie", "title": ["Expenses vs. Day of Week", "03/01/24 to 03/31/24: $3,472.00"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
