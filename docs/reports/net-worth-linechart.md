<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/net-worth-linechart
related: method/report.run, method/report.options, report/net-worth-barchart, report/income-expense-linechart
---
A line chart of assets, liabilities and net worth at the end of each step between two dates: the line version of the
[Net Worth Barchart](report/net-worth-barchart). `report.exportCsv` returns its data as CSV. In GnuCash: Reports ▸
Assets & Liabilities ▸ Net Worth Linechart.

The options that matter are `Accounts/Accounts`, `General/Start Date` and `End Date` and `General/Step Size`.

## Examples

```example fixture=reports/category-daily now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Net Worth Linechart", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000001", "a0000000000000000000000000000002", "a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}, "Step Size": "WeekDelta"}}}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "chart", "type": "line", "title": ["Net Worth Linechart", "03/01/24 to 03/31/24"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
