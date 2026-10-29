<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/balance-forecast
related: method/report.run, method/report.options, report/future-scheduled-transactions-summary, report/cash-flow
---
A line chart of the balance of the selected accounts over time that includes the transactions your scheduled
transactions will make. Use it to see whether an account will run low before the next pay day. It can draw a reserve
line and a target line above it (`Display/Show reserve line`, `Display/Reserve amount`, `Display/Target amount above
reserve`) and the lowest balance still to come (`Display/Show future minimum`). In GnuCash: Reports ▸ Assets &
Liabilities ▸ Balance Forecast.

The options that matter are `Accounts/Accounts` (at least one is needed, or the report says `No accounts selected`),
`General/Start Date`, `End Date` and `General/Interval`. Scheduled transactions are not entered by the report; they
come from [scheduledTransaction.create](method/scheduledTransaction.create).

## Examples

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Balance Forecast", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "1978-01-01"}, "End Date": {"absolute": "1978-12-31"}, "Interval": "MonthDelta"}}}}
{"result": {"title": "", "sections": [{"blocks": [{"kind": "chart", "type": "line", "title": ["Balance Forecast", "01/01/78 to 12/31/78"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
