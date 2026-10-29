<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/budget.create
related: method/budget.setAmounts, method/budget.update, method/budget.list, method/budget.scrubSigns
---
Creates a budget: a name, a number of periods (1 to 60, default 12) and a recurrence that dates them (default every
month from the first of this month). It has no amounts yet; fill them with [budget.setAmounts](method/budget.setAmounts).

The first budget in a book sets the book feature `Use natural signs in budget amounts`, as GnuCash's New Budget does.
In a book that has budgets but lacks the feature, which GnuCash older than 3.8 wrote, creating (and every other edit)
is refused with `conflict` until [budget.scrubSigns](method/budget.scrubSigns) has run: red-ale never changes
budgets when it opens a book. GnuCash's New Budget also fills the description with `Created: <date>`; red-ale does
not, send it yourself if you want it.

## Examples

```example
{"method": "budget.create", "params": {"name": "2024", "numPeriods": 6, "recurrence": {"mult": 1, "periodType": "month", "start": "2024-01-01"}}}
{"result": {"guid": "…", "name": "2024", "numPeriods": 6, "isDefault": true, "periods": [{"index": 0, "startDate": "2024-01-01", "endDate": "2024-01-31"}, {"index": 1, "startDate": "2024-02-01", "endDate": "2024-02-29"}, "…"], "amounts": [], "notes": [], "…": "…"}}
{"method": "budget.create", "params": {"numPeriods": 61}}
{"error": {"code": 2001, "message": "a budget has 1 to 60 periods, not 61", "data": {"help": "error/invalid"}}}
```
