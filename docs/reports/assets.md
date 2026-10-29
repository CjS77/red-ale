<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/assets
related: method/report.run, method/report.options, report/liabilities, report/assets-over-time, report/securities
---
A pie chart of the asset accounts' balances at a date, largest first, with the smallest slices merged into one
`Other`. Use it to see where your assets sit. Each slice links to a report of its account. In GnuCash: Reports ▸
Assets & Liabilities ▸ Asset Piechart.

The options that matter are `Accounts/Accounts`, `General/End Date` (the date of the balances), `Display/Maximum Slices`
and `Display/Sort Method`. Negative balances cannot be drawn in a pie, and a report with no positive balance shows a
`No data` warning in place of the chart.

## Examples

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Assets", "options": {"General": {"End Date": {"absolute": "1978-12-31"}}}}}
{"result": {"title": "Assets", "sections": [{"blocks": [{"kind": "chart", "type": "pie", "title": ["Assets", "Balance at 12/31/78: $3,057.80"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
