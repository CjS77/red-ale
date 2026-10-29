<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/income-vs-day-of-week
related: method/report.run, method/report.options, report/expenses-vs-day-of-week, report/income-accounts
---
A pie chart of income between two dates by the weekday it arrived on. See [Expenses vs. Day of Week](report/expenses-
vs-day-of-week) for the spending side. In GnuCash: Reports ▸ Examples ▸ Income vs. Day of Week.

The options that matter are `Accounts/Accounts`, `General/Start Date` and `End Date`. The chart's title shows the
total, which is negative because income is a credit.

## Examples

```example fixture=reports/category-daily now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Income vs. Day of Week", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}}}}}
{"result": {"title": "Income vs. Day of Week", "sections": [{"blocks": [{"kind": "chart", "type": "pie", "title": ["Income vs. Day of Week", "03/01/24 to 03/31/24: -$496.00"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
