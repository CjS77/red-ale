<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/transaction.list
related: method/transaction.get, method/transaction.find, method/account.balance
---
Lists transactions, all of them or those with a split in `account`, optionally between the dates `from` and `to`
(inclusive, on the posted date). They come in GnuCash's order: by posted date, closing entries last, then number,
date entered, description and GUID. Use [transaction.find](method/transaction.find) to search by text, amount or
state instead.

A book can hold thousands of transactions, so give `account` or a date range rather than listing the lot.

## Examples

```example fixture=reports/test-data
{"method": "transaction.list", "params": {"account": "Root:Asset:Wallet", "from": "1971-01-01", "to": "1971-12-31"}}
{"result": {"transactions": [{"guid": "c0000000000000000000000000000007", "description": "$100bank -> $80expenses + $20wallet", "…": "…"}]}}
```
