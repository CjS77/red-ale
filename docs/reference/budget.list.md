<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/budget.list
related: method/budget.get, method/budget.scrubSigns, method/book.options
---
Lists the budgets in book order, without their periods, amounts and notes, with the default one (the budget the budget
reports use unless told otherwise). `defaultFrom` says whether the book names it (`option`) or it is just the first
(`first`). Set the default with `book.setOptions`.

`needsSignScrub` is true when the book has budgets but not the natural-signs feature, so that edits are refused until
[budget.scrubSigns](method/budget.scrubSigns) runs. Listing and reading work either way.

## Examples

```example fixture=reports/budget
{"method": "budget.list", "params": {}}
{"result": {"budgets": [{"guid": "f0000000000000000000000000000000", "name": "test budget", "numPeriods": 6, "isDefault": true, "…": "…"}], "default": "f0000000000000000000000000000000", "defaultFrom": "first", "naturalSigns": false, "needsSignScrub": true}}
```
