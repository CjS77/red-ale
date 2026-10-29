<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/period.get
related: method/period.set, method/period.dates
---
Reads the accounting period: the start and end bounds that reports and account totals use by default (GnuCash's
Accounting Period preferences), and the fiscal year end. A bound is a relative choice (`today`, `month`,
`previous-month`, `quarter`, `previous-quarter`, `year`, `previous-year`, `fiscal-year`, `previous-fiscal-year`) or a
`YYYY-MM-DD` date. The default is `year` for both. GnuCash keeps these as user preferences, not in the book; red-ale
keeps them in its Configuration, so the call works with no book open.

## Examples

```example book=none
{"method": "period.get", "params": {}}
{"result": {"start": "year", "end": "year", "fiscalYearEnd": null, "fiscalYearEndSource": null, "choices": ["today", "month", "previous-month", "quarter", "previous-quarter", "year", "previous-year", "fiscal-year", "previous-fiscal-year"], "…": "…"}}
```
