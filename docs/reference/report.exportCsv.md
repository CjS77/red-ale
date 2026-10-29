<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.exportCsv
related: method/report.run, method/report.export
---
Runs a net or category chart and returns its data as CSV, as GnuCash's CSV export of those charts writes it. Reports that
offer it have `csv` in their `exportTypes` ([report.list](method/report.list)): Net Worth Barchart and Linechart,
Income/Expense Chart and Linechart, Income Over Time, Expense Over Time, Assets Over Time and Liabilities Over Time.
The rows are the chart's data whatever its display options show.

Dates are ISO whatever the date format. Text is quoted, and numbers are written as Guile prints inexact numbers
(`28.0`, `1.0e-4`), which is what GnuCash writes; red-ale computes them without floating point. Nothing is written to
disk; the CLI's `--output` writes the `csv` on your side. Any other report is refused with `invalid`
(`no-export`), and a chart with no figures with `no-exportable-data`.

## Examples

```example fixture=reports/category-daily now=2024-03-15T12:00:00Z
{"method": "report.exportCsv", "params": {"report": "Income Over Time", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "2024-03-01"}, "End Date": {"absolute": "2024-03-31"}, "Step Size": "WeekDelta"}}}}
{"result": {"title": "", "csv": "\"Date\",\"account-3\"\n\"2024-03-01\",28.0\n\"2024-03-08\",77.0\n\"2024-03-15\",126.0\n\"2024-03-22\",175.0\n\"2024-03-29\",90.0"}}
{"method": "report.exportCsv", "params": {"report": "Balance Sheet"}}
{"error": {"code": 2001, "message": "…", "data": {"reason": "no-export", "exportType": "csv", "help": "error/invalid"}}}
```
