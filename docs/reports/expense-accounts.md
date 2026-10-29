<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/expense-accounts
related: method/report.run, method/report.options, report/income-accounts, report/expense-over-time, report/expenses-vs-day-of-week
---
A pie chart of the expense accounts' totals between two dates, largest first, with the smallest slices merged into
`Other`. Use it to see where your money went. Each slice links to the report of its account. In GnuCash: Reports ▸
Income & Expense ▸ Expense Piechart.

The options that matter are `Accounts/Accounts`, `Accounts/Levels of Subaccounts`, `General/Start Date` and `End Date`
and `Display/Maximum Slices`.

## Examples

```example fixture=reports/category-multi now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Expense Accounts", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000001"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}}}}}
{"result": {"title": "Expense Accounts", "sections": [{"blocks": [{"kind": "chart", "type": "pie", "title": ["Expense Accounts", "03/01/24 to 03/31/24: $496.00"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
