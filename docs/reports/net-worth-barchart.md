<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/net-worth-barchart
related: method/report.run, method/report.options, report/net-worth-linechart, report/assets-over-time
---
A bar chart of assets, liabilities and net worth at the end of each step between two dates. Use it to follow your net
worth. `report.exportCsv` returns its data as CSV. In GnuCash: Reports ▸ Assets & Liabilities ▸ Net Worth Barchart.

The options that matter are `Accounts/Accounts` (asset and liability accounts), `General/Start Date` and `End Date` and
`General/Step Size`.

## Examples

```example fixture=reports/category-daily now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Net Worth Barchart", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000001", "a0000000000000000000000000000002", "a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}, "Step Size": "WeekDelta"}}}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "chart", "type": "bar", "title": ["Net Worth Barchart", "03/01/24 to 03/31/24"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
