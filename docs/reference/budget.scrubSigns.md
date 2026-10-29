<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/budget.scrubSigns
related: method/budget.list, method/budget.create
---
Runs GnuCash's natural-signs scrub on a book whose budgets predate GnuCash 3.8, which stored amounts with the signs of
the user's display preference. For each budget it guesses the old convention from the sums of its amounts and flips
the amounts to natural signs, then sets the book feature `Use natural signs in budget amounts`. It rewrites amounts on a
guess, so it is destructive: look at the budgets after, as GnuCash asks.

GnuCash runs this every time a file opens; red-ale does not, and until you run it every budget edit is refused with
`conflict` (reason `natural-signs`). `budget.list` tells you when it is needed. With no budgets it only unsets a
feature left over. Each result says which `policy` was guessed (`incomeExpense`, `creditAccounts`, `none`) and how many
amounts were flipped.

## Examples

```example fixture=reports/budget
{"method": "budget.setAmounts", "params": {"budget": "f0000000000000000000000000000000", "amounts": [{"account": "Root:Expenses", "period": 0, "amount": "1"}]}}
{"error": {"code": 2003, "message": "…", "data": {"reason": "natural-signs", "help": "error/conflict"}}}
{"method": "budget.scrubSigns", "params": {}}
{"result": {"featureSet": true, "featureUnset": false, "budgets": [{"budget": "f0000000000000000000000000000000", "policy": "none", "flipped": 0}]}}
{"method": "budget.list", "params": {}}
{"result": {"naturalSigns": true, "needsSignScrub": false, "…": "…"}}
```
