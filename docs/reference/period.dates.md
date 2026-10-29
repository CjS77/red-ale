<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/period.dates
related: method/period.get, method/period.set
---
Gives the first and last day of the accounting period containing a date, by default today (the local date of the
session's clock, or of the server's with no book open). `start`, `end` and `fiscalYearEnd` override the configured bounds for this call only. `startTime`
and `endTime` are the instants GnuCash uses: local 00:00:00 of the first day and 23:59:59 of the last. Use it to work
out the dates to pass to a report or to [transaction.list](method/transaction.list).

A fiscal year starts the day after the fiscal year end of the previous year, so with a year ending 31 March, 15
July 2024 falls in 2024-04-01 to 2025-03-31.

## Examples

```example fixture=reports/test-data now=2024-07-15T12:00:00Z
{"method": "period.dates", "params": {}}
{"result": {"today": "2024-07-15", "start": "2024-01-01", "end": "2024-12-31", "startTime": "2024-01-01T00:00:00Z", "endTime": "2024-12-31T23:59:59Z"}}
{"method": "period.dates", "params": {"start": "fiscal-year", "end": "fiscal-year", "fiscalYearEnd": "03-31"}}
{"result": {"start": "2024-04-01", "end": "2025-03-31", "…": "…"}}
{"method": "period.dates", "params": {"date": "2024-02-15", "start": "previous-quarter", "end": "previous-quarter"}}
{"result": {"start": "2023-10-01", "end": "2023-12-31", "…": "…"}}
```

It needs no book; today is then the server's:

```example book=none now=2024-11-20T09:30:00Z
{"method": "period.dates", "params": {"start": "quarter", "end": "quarter"}}
{"result": {"today": "2024-11-20", "start": "2024-10-01", "end": "2024-12-31", "startTime": "2024-10-01T00:00:00Z", "endTime": "2024-12-31T23:59:59Z"}}
```
