<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/transaction.reverse
related: method/transaction.void, method/transaction.update
---
Creates the reversing transaction of a transaction: a copy with every split's value and amount negated, posted on
`date` (default today), and marks the original as reversed by it. Use it to cancel an entry while keeping both in the
books, as the register's Reverse Transaction does.

It is refused with `conflict` if the original has already been reversed, and, unlike GnuCash, if it is void: unvoid
it first. The new splits start unreconciled.

## Examples

```example fixture=reports/test-data
{"method": "transaction.reverse", "params": {"guid": "c0000000000000000000000000000003", "date": "2024-07-01"}}
{"result": {"description": "desc-4", "datePosted": "2024-07-01", "splits": [{"accountName": "Root:Asset:Bank", "value": {"num": "1500", "denom": "100"}, "…": "…"}, {"accountName": "Root:Expenses", "value": {"num": "-1500", "denom": "100"}, "…": "…"}], "…": "…"}}
{"method": "transaction.reverse", "params": {"guid": "c0000000000000000000000000000003"}}
{"error": {"code": 2003, "message": "…", "data": {"help": "error/conflict"}}}
```
