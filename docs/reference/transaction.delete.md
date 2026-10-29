<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/transaction.delete
related: method/transaction.void, method/transaction.reverse, error/conflict
---
Deletes a transaction and all its splits; the splits leave their lots. A read-only transaction (voided, or protected
by the auto read-only threshold) is refused with `conflict`. Deleting erases the record; to cancel a posted entry
and keep the history, use [transaction.void](method/transaction.void) or [transaction.reverse](method/transaction.reverse).

## Examples

```example fixture=reports/test-data
{"method": "transaction.delete", "params": {"guid": "c0000000000000000000000000000006"}}
{"result": {}}
{"method": "transaction.get", "params": {"guid": "c0000000000000000000000000000006"}}
{"error": {"code": 2000, "message": "…", "data": {"kind": "transaction", "id": "c0000000000000000000000000000006", "help": "error/not-found"}}}
```
