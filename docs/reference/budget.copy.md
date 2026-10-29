<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/budget.copy
related: method/budget.update, method/budget.delete
---
Copies a budget as GnuCash's Copy Budget does: a new budget named `Copy of <name>` with the same description,
recurrence, number of periods and every amount. Notes are not copied, nor are amounts hidden beyond the period count,
and the copy goes last in book order. Rename it with [budget.update](method/budget.update).

## Examples

```example
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "budget.create", "params": {"name": "2024", "numPeriods": 3, "recurrence": {"mult": 1, "periodType": "month", "start": "2024-01-01"}}}
{"result": "…"}
{"method": "budget.setAmounts", "params": {"budget": "00ed0000000000000000000000000002", "amounts": [{"account": "Rent", "period": 0, "amount": "750.00"}]}}
{"result": "…"}
{"method": "budget.setNotes", "params": {"budget": "00ed0000000000000000000000000002", "notes": [{"account": "Rent", "period": 0, "note": "New lease"}]}}
{"result": "…"}
{"method": "budget.copy", "params": {"budget": "00ed0000000000000000000000000002"}}
{"result": {"name": "Copy of 2024", "isDefault": false, "amounts": [{"accountName": "Rent", "period": 0, "amount": {"num": "75000", "denom": "100"}, "…": "…"}], "notes": [], "…": "…"}}
```
