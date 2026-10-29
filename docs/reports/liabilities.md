<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/liabilities
related: method/report.run, method/report.options, report/assets, report/liabilities-over-time
---
A pie chart of the liability accounts' balances at a date, largest first, with the smallest slices merged into
`Other`. It is the liability side of the [Asset Piechart](report/assets). In GnuCash: Reports ▸ Assets & Liabilities ▸
Liability Piechart.

The options that matter are `Accounts/Accounts`, `General/End Date` and `Display/Maximum Slices`. A report with no
liability balance shows a `No data` warning.

## Examples

```example fixture=reports/category-liability now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Liabilities", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000002"]}, "General": {"End Date": {"absolute": "2017-05-31"}}}}}
{"result": {"title": "Liabilities", "sections": [{"blocks": [{"kind": "chart", "type": "pie", "title": ["Liabilities", "Balance at 05/31/17: $55.00"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
