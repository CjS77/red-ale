<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/budget.estimate
related: method/budget.setAmounts, method/budget.get
---
Fills the budget amounts of the accounts you name from what happened in the past, as GnuCash's Estimate dialog does.
History periods follow the budget's own recurrence starting at `startDate`; the account's change over history period
*n* becomes budget period *n*. With `useAverage` the change over all the history periods, divided by the number of
periods, goes into every period. Each account's change includes its sub-accounts. Closing transactions are left out.

It overwrites every period of every account you name, set amounts included, so it is destructive. Figures are rounded
to `significantDigits` (default 1) significant digits, but never to the left of the decimal point: 812.50 at 2 digits
is 813, and 5.6789 at 1 digit is 6. A period with no change is a set zero.

## Examples

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "budget.create", "params": {"name": "2024", "numPeriods": 3, "recurrence": {"mult": 1, "periodType": "month", "start": "2024-01-01"}}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-01-05", "description": "Rent", "splits": [{"account": "Rent", "value": "750.00"}, {"account": "Checking", "value": "-750.00"}]}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-02-05", "description": "Rent", "splits": [{"account": "Rent", "value": "812.50"}, {"account": "Checking", "value": "-812.50"}]}}
{"result": "…"}
{"method": "budget.estimate", "params": {"budget": "00ed0000000000000000000000000003", "accounts": ["Rent"], "startDate": "2024-01-01", "significantDigits": 2}}
{"result": {"amounts": [{"accountName": "Rent", "period": 0, "amount": {"num": "750", "denom": "1"}, "…": "…"}, {"period": 1, "amount": {"num": "813", "denom": "1"}, "…": "…"}, {"period": 2, "amount": {"num": "0", "denom": "1"}, "…": "…"}], "…": "…"}}
```
