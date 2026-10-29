<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/budget.setNotes
related: method/budget.setAmounts, method/budget.get
---
Sets or deletes many budget notes at once, all or none. A note belongs to an account and a period; an empty string or
`null` deletes it. Notes have no length limit. The checks are those of [budget.setAmounts](method/budget.setAmounts).

## Examples

```example
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "budget.create", "params": {"name": "2024", "numPeriods": 6, "recurrence": {"mult": 1, "periodType": "month", "start": "2024-01-01"}}}
{"result": "…"}
{"method": "budget.setNotes", "params": {"budget": "00ed0000000000000000000000000002", "notes": [{"account": "Rent", "period": 0, "note": "New lease"}]}}
{"result": {"budget": "00ed0000000000000000000000000002", "set": 1, "deleted": 0}}
{"method": "budget.setNotes", "params": {"budget": "00ed0000000000000000000000000002", "notes": [{"account": "Rent", "period": 0, "note": ""}]}}
{"result": {"budget": "00ed0000000000000000000000000002", "set": 0, "deleted": 1}}
```
