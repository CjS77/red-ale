<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/budget.setAmounts
related: method/budget.get, method/budget.setNotes, method/budget.estimate
---
Sets or unsets many budget amounts at once, all or none. Each cell is an account and a period (counting from 0); an
`amount` sets it, `null` unsets it, and `"0"` is a set zero, which is not the same as unset. Amounts are kept exactly as
given (`"750.00"` is stored as 75000/100) in natural signs: expense budgets positive, income budgets negative.

Refused with `invalid`: a period at or beyond the budget's count (GnuCash silently does nothing), the same cell twice in
one call, the root account. Placeholder and hidden accounts may have amounts.

## Examples

```example
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Salary", "type": "INCOME", "commodity": "USD"}}
{"result": "…"}
{"method": "budget.create", "params": {"name": "2024", "numPeriods": 6, "recurrence": {"mult": 1, "periodType": "month", "start": "2024-01-01"}}}
{"result": "…"}
{"method": "budget.setAmounts", "params": {"budget": "00ed0000000000000000000000000003", "amounts": [{"account": "Rent", "period": 0, "amount": "750.00"}, {"account": "Rent", "period": 1, "amount": "750.00"}, {"account": "Salary", "period": 0, "amount": "-3000.00"}]}}
{"result": {"budget": "00ed0000000000000000000000000003", "set": 3, "unset": 0}}
{"method": "budget.setAmounts", "params": {"budget": "00ed0000000000000000000000000003", "amounts": [{"account": "Rent", "period": 9, "amount": "1"}]}}
{"error": {"code": 2001, "message": "period 9 is past the budget's 6 periods (they count from 0)", "data": {"help": "error/invalid"}}}
```
