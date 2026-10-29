<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/period.set
related: method/period.get, method/period.dates, error/invalid
---
Sets the accounting period's start and end bounds or the fiscal year end (`MM-DD`) in red-ale's Configuration, and
returns the result as [period.get](method/period.get) does. It changes no book and works with no book open. Use
`clearFiscalYearEnd: true` to remove the fiscal year end.

A fiscal choice (`fiscal-year`, `previous-fiscal-year`) is refused with `invalid` unless a fiscal year end is known,
from the Configuration or from the open book's own slot; GnuCash would silently return the epoch.

## Examples

```example book=none
{"method": "period.set", "params": {"start": "fiscal-year"}}
{"error": {"code": 2001, "message": "…", "data": {"help": "error/invalid"}}}
{"method": "period.set", "params": {"start": "fiscal-year", "end": "fiscal-year", "fiscalYearEnd": "03-31"}}
{"result": {"start": "fiscal-year", "end": "fiscal-year", "fiscalYearEnd": "03-31", "fiscalYearEndSource": "config", "…": "…"}}
```
