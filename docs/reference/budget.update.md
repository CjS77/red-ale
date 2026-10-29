<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/budget.update
related: method/budget.create, method/budget.get
---
Changes a budget's name, description, number of periods or recurrence.

Lowering `numPeriods` hides amounts and notes beyond the new count but does not delete them: they stay in the book,
`hidden` counts them, and raising the count shows them again, as in GnuCash, which has no way to drop them. Changing
the recurrence moves no amount: amounts belong to a period number, so only the dates of the periods change.

## Examples

```example
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "budget.create", "params": {"name": "2024", "numPeriods": 6, "recurrence": {"mult": 1, "periodType": "month", "start": "2024-01-01"}}}
{"result": "…"}
{"method": "budget.setAmounts", "params": {"budget": "00ed0000000000000000000000000002", "amounts": [{"account": "Rent", "period": 4, "amount": "750.00"}]}}
{"result": "…"}
{"method": "budget.update", "params": {"budget": "00ed0000000000000000000000000002", "numPeriods": 3}}
{"result": {"numPeriods": 3, "amounts": [], "hidden": {"amounts": 1, "notes": 0}, "…": "…"}}
```
