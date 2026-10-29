<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/split-transactions
title: Split transactions
summary: Record one payment that is divided among several accounts (a pay slip, a shop receipt), then add, change or remove a split while keeping the transaction balanced.
related: method/transaction.post, method/transaction.update, method/transaction.get, error/unbalanced
---
Most transactions have two splits: money leaves one account and arrives in another. A *split transaction* has more:
your salary arrives in the bank as one deposit but is really gross pay less tax and pension; a supermarket receipt is
one payment covering food and household goods. Each split names an account and a value, and the values must still sum
to exactly zero.

## Steps

1. **Work out the parts.** For a 2,000.00 pay slip with 300.00 tax and 100.00 pension, the bank receives 1,600.00. The
   parts are: bank `+1600.00`, tax expense `+300.00`, pension expense `+100.00`, and the salary income account
   `-2000.00`. They sum to zero.
2. **Post it** with [transaction.post](method/transaction.post), listing every split. A split may carry a `memo`,
   which describes that line only.
3. **Look at it** with [transaction.get](method/transaction.get). Every split has a GUID.
4. **Change a split** with [transaction.update](method/transaction.update): `changeSplits` takes the split's `guid`
   and the fields to change, `addSplits` takes new splits, `removeSplits` takes GUIDs. The result must balance, so a
   change that leaves the transaction short is refused with [unbalanced](error/unbalanced) and nothing changes.
   Change two splits in one call to keep it balanced.
5. **Remove a split** by listing its GUID in `removeSplits` and changing another to take up the difference in the same
   call.

## Example

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Salary", "type": "INCOME", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Tax", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Pension", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-01", "description": "July pay slip", "splits": [{"account": "Checking", "value": "1600.00"}, {"account": "Tax", "value": "300.00", "memo": "Income tax"}, {"account": "Pension", "value": "100.00"}, {"account": "Salary", "value": "-2000.00"}]}}
{"result": {"guid": "…", "balanced": true, "splits": ["…", "…", "…", "…"], "…": "…"}}
```

To change a split you need its GUID, from [transaction.get](method/transaction.get) or the result of the post. This
removes the wallet split of a three-split transaction and makes the expense split take the whole 100.00, in one call:

```example fixture=reports/test-data
{"method": "transaction.update", "params": {"guid": "c0000000000000000000000000000007", "removeSplits": ["d0000000000000000000000000000010"], "changeSplits": [{"guid": "d000000000000000000000000000000f", "value": "100.00"}]}}
{"result": {"balanced": true, "splits": [{"accountName": "Root:Expenses", "value": {"num": "10000", "denom": "100"}, "…": "…"}, {"accountName": "Root:Asset:Bank", "value": {"num": "-10000", "denom": "100"}, "…": "…"}], "…": "…"}}
{"method": "transaction.update", "params": {"guid": "c0000000000000000000000000000007", "changeSplits": [{"guid": "d000000000000000000000000000000f", "value": "90.00"}]}}
{"error": {"code": 2002, "message": "…", "data": {"imbalance": "…", "currency": "…", "help": "error/unbalanced"}}}
```
