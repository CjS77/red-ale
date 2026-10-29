<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/liabilities-over-time
related: method/report.run, method/report.options, report/assets-over-time, report/liabilities
---
A bar chart of the selected liability accounts' balances at the end of each step between two dates. `report.exportCsv`
returns its data as CSV. In GnuCash: Reports ▸ Assets & Liabilities ▸ Liability Chart.

The options that matter are `Accounts/Accounts`, `General/Start Date` and `End Date` and `General/Step Size`.

## Examples

```example fixture=reports/category-liability now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Liabilities Over Time", "options": {"General": {"Start Date": {"absolute": "2017-01-01"}, "End Date": {"absolute": "2018-12-31"}, "Step Size": "MonthDelta"}}}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "chart", "type": "bar", "title": ["Liabilities Over Time", "Balances 01/01/17 to 12/31/18"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
