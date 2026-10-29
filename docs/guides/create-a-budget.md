<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/create-a-budget
title: Create a budget
summary: Plan income and spending per account and period, fill the amounts by hand or estimate them from past transactions, and compare plan with actuals in the Budget Report.
related: method/budget.create, method/budget.setAmounts, method/budget.setNotes, method/budget.estimate, method/budget.get, method/budget.list, method/report.run, report/budget-report
---
A budget is a plan: how much you expect each account to receive or spend in each period. Comparing it with what
actually happened tells you where the money went. A budget has a number of periods (for example 12 months) that start on
a date, and an amount for any account in any period.

## Steps

1. **Create the budget** with [budget.create](method/budget.create): a `name`, `numPeriods` (1 to 60) and a
   `recurrence` saying how long a period is and when the first starts. The first budget becomes the default one.
2. **Set amounts** with [budget.setAmounts](method/budget.setAmounts): a list of `{account, period, amount}`. Periods
   count from 0. Amounts keep the account's natural sign: income is negative (a credit), expenses positive. One call is
   atomic, so a bad period leaves the budget unchanged.
3. **Or estimate them** with [budget.estimate](method/budget.estimate): it fills the periods of the accounts you name
   from what happened in the past, either period by period from a `startDate`, or as an average (`useAverage`). It
   overwrites those accounts' amounts, so use it on a budget you have not filled yet.
4. **Add notes** to cells with [budget.setNotes](method/budget.setNotes).
5. **Read it back** with [budget.get](method/budget.get), and list budgets with [budget.list](method/budget.list).
6. **Compare with actuals.** Run the Budget Report with [report.run](method/report.run): it shows, per account and period,
   the budget, the actual and the difference ([Budget Report](report/budget-report)). Other budget reports are listed
   by [report.list](method/report.list). Set the budget it uses in the `General/Budget` option.

## Example

A three-month budget for rent and salary, with rent estimated from two past months.

```example now=2024-04-15T12:00:00Z
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Salary", "type": "INCOME", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-01-05", "description": "Rent", "splits": [{"account": "Rent", "value": "750.00"}, {"account": "Checking", "value": "-750.00"}]}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-02-05", "description": "Rent", "splits": [{"account": "Rent", "value": "812.50"}, {"account": "Checking", "value": "-812.50"}]}}
{"result": "…"}
{"method": "budget.create", "params": {"name": "Spring", "numPeriods": 3, "recurrence": {"mult": 1, "periodType": "month", "start": "2024-04-01"}}}
{"result": {"name": "Spring", "numPeriods": 3, "isDefault": true, "…": "…"}}
{"method": "budget.setAmounts", "params": {"budget": "00ed000000000000000000000000000a", "amounts": [{"account": "Salary", "period": 0, "amount": "-3000.00"}, {"account": "Salary", "period": 1, "amount": "-3000.00"}, {"account": "Salary", "period": 2, "amount": "-3000.00"}]}}
{"result": {"set": 3, "unset": 0, "…": "…"}}
{"method": "budget.estimate", "params": {"budget": "00ed000000000000000000000000000a", "accounts": ["Rent"], "startDate": "2024-01-01", "significantDigits": 2}}
{"result": {"amounts": [{"accountName": "Rent", "period": 0, "amount": {"num": "750", "denom": "1"}, "…": "…"}, {"period": 1, "amount": {"num": "813", "denom": "1"}, "…": "…"}, {"period": 2, "amount": {"num": "0", "denom": "1"}, "…": "…"}], "…": "…"}}
{"method": "report.run", "params": {"report": "Budget Report"}}
{"result": {"title": "…", "sections": "…", "…": "…"}}
```
