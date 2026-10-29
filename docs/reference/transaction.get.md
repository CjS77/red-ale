<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/transaction.get
related: method/transaction.list, method/transaction.find, method/transaction.post
---
Returns one transaction with all its splits, found by its GUID. Each split has its `account`, its `value` (in the
transaction's `currency`) and its `amount` (in the account's commodity); the values add up to zero. Void, read-only
and reversed-by state are included. GUIDs come from the call that posted the transaction, from
[transaction.list](method/transaction.list) or from [transaction.find](method/transaction.find).

## Examples

```example fixture=reports/test-data
{"method": "transaction.get", "params": {"guid": "c0000000000000000000000000000007"}}
{"result": {"currency": "CURRENCY::USD", "description": "$100bank -> $80expenses + $20wallet", "datePosted": "1971-02-14", "void": false, "balanced": true, "splits": [{"accountName": "Root:Expenses", "value": {"num": "8000", "denom": "100"}, "…": "…"}, {"accountName": "Root:Asset:Wallet", "value": {"num": "2000", "denom": "100"}, "…": "…"}, {"accountName": "Root:Asset:Bank", "value": {"num": "-10000", "denom": "100"}, "…": "…"}], "…": "…"}}
```
