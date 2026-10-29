<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/assets-over-time
related: method/report.run, method/report.options, report/liabilities-over-time, report/net-worth-barchart, report/assets
---
A bar chart of the balance of the selected accounts at the end of each step between two dates, one bar group per step
and one bar per account. Use it to see how your assets grew or shrank. `report.exportCsv` returns its data as CSV. In
GnuCash: Reports ▸ Assets & Liabilities ▸ Asset Chart.

The options that matter are `Accounts/Accounts`, `General/Start Date` and `End Date`, and `General/Step Size` (day,
week, month, and so on). A `Display/Show table` option adds the figures as a table under the chart.

## Examples

```example fixture=reports/category-daily now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Assets Over Time", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000001"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}, "Step Size": "WeekDelta"}}}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "chart", "type": "bar", "title": ["Assets Over Time", "Balances 03/01/24 to 03/31/24"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
