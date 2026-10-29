<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/budget.get
related: method/budget.list, method/budget.setAmounts
---
Returns one budget with its periods (each with its first and last day), every amount that is set and every note. Give
`accounts` to see only those. A budget amount is either set (even to zero) or unset, which `budget.get` simply does not
list: `0` and nothing are different, because the budget reports roll an unset parent up from its children but not a
parent set to zero.

Amounts are in natural signs: an expense budget is positive, an income budget negative. `hidden` counts amounts and
notes beyond the budget's period count, which are kept; `orphaned` counts those of accounts no longer in the book.

## Examples

```example fixture=reports/budget
{"method": "budget.get", "params": {"budget": "f0000000000000000000000000000000", "accounts": ["Root:Expenses"]}}
{"result": {"guid": "f0000000000000000000000000000000", "name": "test budget", "numPeriods": 6, "periods": [{"index": 0, "startDate": "2024-03-01", "endDate": "2024-03-31"}, "…"], "amounts": [{"accountName": "Root:Expenses", "period": 1, "amount": {"num": "30", "denom": "1"}, "…": "…"}, {"period": 2, "amount": {"num": "20", "denom": "1"}, "…": "…"}, {"period": 3, "amount": {"num": "40", "denom": "1"}, "…": "…"}], "notes": ["…"], "hidden": {"amounts": 0, "notes": 0}, "orphaned": {"amounts": 0, "notes": 0}, "…": "…"}}
```
