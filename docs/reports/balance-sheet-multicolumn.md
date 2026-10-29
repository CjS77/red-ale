<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/balance-sheet-multicolumn
related: method/report.run, method/report.options, report/balance-sheet, report/income-statement-multicolumn
---
A balance sheet with one column for each period between two dates, so you can compare the same accounts over time. The
result is a group of blocks, one for each period or section. Use it for monthly or yearly comparisons; for a single
date use the [Balance Sheet](report/balance-sheet). In GnuCash: Reports ▸ Assets & Liabilities ▸ Balance Sheet
(Multicolumn).

The options that matter are `General/Start Date` and `End Date`, `General/Period duration` (the length of a column),
`General/Period order is most recent first`, `General/Enable dual columns` and the `Commodities` options. Foreign
accounts are converted at the rate for the date of each column, and `Commodities/Show Exchange Rates` lists the rates
used.

## Examples

```example fixture=reports/balsheet-pnl-extras now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Balance Sheet (Multicolumn)", "options": {"General": {"Start Date": {"absolute": "2023-01-01"}, "End Date": {"absolute": "2024-01-01"}}}}}
{"result": {"title": "Balance Sheet (Multicolumn) 01/01/24", "sections": [{"blocks": [{"kind": "group", "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
