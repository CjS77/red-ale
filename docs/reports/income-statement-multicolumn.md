<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/income-statement-multicolumn
related: method/report.run, method/report.options, report/income-statement, report/balance-sheet-multicolumn
---
An income statement with one column for each period between two dates, for comparing revenues and expenses over time.
The result is a group of blocks. For a single period use the [Income Statement](report/income-statement). In GnuCash:
Reports ▸ Income & Expense ▸ Income Statement (Multicolumn).

The options that matter are `General/Start Date` and `End Date`, `General/Period duration` (the length of a column),
`General/Period order is most recent first` and the `Commodities` options.

## Examples

```example fixture=reports/balsheet-pnl-extras now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Income Statement (Multicolumn)", "options": {"General": {"Start Date": {"absolute": "2023-01-01"}, "End Date": {"absolute": "2024-03-31"}}}}}
{"result": {"title": "Income Statement (Multicolumn) 01/01/23 to 03/31/24", "sections": [{"blocks": [{"kind": "group", "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
