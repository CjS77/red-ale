<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/depreciate-an-asset
title: Depreciate an asset
summary: Spread the cost of equipment over its useful life with a monthly scheduled transaction that moves an equal amount into a depreciation expense.
related: method/scheduledTransaction.create, method/scheduledTransaction.occurrences, method/scheduledTransaction.sinceLastRun, method/scheduledTransaction.apply, method/transaction.post, method/account.balance, concept/other-assets-and-depreciation
from: guide/ch_dep.docbook#chapter_dep
---
Equipment loses value as it is used. Rather than count the whole price as an expense the day you buy it, accountants spread
it over the years it will last. This is *depreciation*. The simplest way is the straight line: if a 1,200.00 machine will
last 12 months, 100.00 a month is charged as an expense.

The books keep the machine at what you paid, in one account, and the total depreciation so far in a second asset
account ("accumulated depreciation"). Together they give the machine's current worth. Each month's charge goes to a
depreciation expense account, which reduces your profit.

## Steps

1. **Create the accounts** ([Set up accounts](guide/set-up-accounts)): `Equipment` and `Accumulated depreciation` (both
   `ASSET`), `Depreciation` (`EXPENSE`) and the bank account you paid from.
2. **Record the purchase**: Equipment `+`, bank `-`, for the price.
3. **Schedule the charge** with [scheduledTransaction.create](method/scheduledTransaction.create): a monthly
   `schedule` from the first date, and a template transaction with two splits, Depreciation as a `debitFormula` of `100`
   and Accumulated depreciation as a `creditFormula` of `100`. Add `end` to stop after 12 occurrences.
4. **Check the dates** with [scheduledTransaction.occurrences](method/scheduledTransaction.occurrences), which lists
   future occurrences of a schedule without creating anything.
5. **Create what is due** each time you open the book: [scheduledTransaction.sinceLastRun](method/scheduledTransaction.sinceLastRun)
   proposes every missed occurrence and changes nothing; [scheduledTransaction.apply](method/scheduledTransaction.apply)
   with the proposal's `id` creates the transactions. See [Schedule a transaction](guide/schedule-a-transaction).
6. **Check** the expense with [account.balance](method/account.balance).

A declining-balance method changes the formula each period, or is entered by hand with
[transaction.post](method/transaction.post) from your own calculation.

## Example

Buy the machine, schedule the charge, and see that three months are due on 15 October.

```example now=2024-10-15T12:00:00Z
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Equipment", "type": "ASSET", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Accumulated depreciation", "type": "ASSET", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Depreciation", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-08-01", "description": "Buy machine", "splits": [{"account": "Equipment", "value": "1200.00"}, {"account": "Checking", "value": "-1200.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "scheduledTransaction.create", "params": {"name": "Depreciate machine", "startDate": "2024-08-01", "schedule": [{"mult": 1, "periodType": "month"}], "end": {"occurrences": 12}, "templateTransactions": [{"description": "Depreciation", "splits": [{"account": "Depreciation", "debitFormula": "100"}, {"account": "Accumulated depreciation", "creditFormula": "100"}]}]}}
{"result": {"name": "Depreciate machine", "enabled": true, "startDate": "2024-08-01", "…": "…"}}
{"method": "scheduledTransaction.occurrences", "params": {"schedule": [{"mult": 1, "periodType": "month"}], "startDate": "2024-08-01", "count": 3}}
{"result": {"occurrences": [{"date": "2024-08-01", "…": "…"}, {"date": "2024-09-01", "…": "…"}, {"date": "2024-10-01", "…": "…"}]}}
{"method": "scheduledTransaction.sinceLastRun", "params": {}}
{"result": {"id": "slr-1", "summary": {"toCreate": 3, "…": "…"}, "…": "…"}}
```

The proposal's `id` goes into the next call, which creates the three transactions (the `id` counts up in each server
process, so it is not always `slr-1`):

```json
{"method": "scheduledTransaction.apply", "params": {"proposal": "slr-1"}}
```

The Depreciation account then has a balance of 300.00 and Accumulated depreciation of -300.00, and the machine is worth
1,200.00 less 300.00.
