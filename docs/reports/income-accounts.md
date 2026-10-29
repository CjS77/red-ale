<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/income-accounts
related: method/report.run, method/report.options, report/expense-accounts, report/income-over-time
---
A pie chart of the income accounts' totals between two dates, largest first, with the smallest slices merged into
`Other`. Use it to see where your income comes from. In GnuCash: Reports ▸ Income & Expense ▸ Income Piechart.

The options that matter are `Accounts/Accounts`, `Accounts/Levels of Subaccounts`, `General/Start Date` and `End Date`
and `Display/Maximum Slices`.

## Examples

```example fixture=reports/category-daily now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Income Accounts", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}}}}}
{"result": {"title": "Income Accounts", "sections": [{"blocks": [{"kind": "chart", "type": "pie", "title": ["Income Accounts", "03/01/24 to 03/31/24: $496.00"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
