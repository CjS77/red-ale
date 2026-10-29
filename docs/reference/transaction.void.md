<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/transaction.void
related: method/transaction.unvoid, method/transaction.reverse, method/transaction.delete
---
Voids a transaction as GnuCash does: each split's value and amount are kept in slots and set to zero, the splits are
marked `v`, and the transaction becomes read-only with notes `Voided transaction`. The history stays and the
balances stop counting it. `reason` is optional.

It is refused with `conflict` if the transaction is already void, is read-only, or has a cleared, reconciled or
frozen split; GnuCash's register silently ignores the already-void case. Undo it with
[transaction.unvoid](method/transaction.unvoid).

## Examples

```example fixture=reports/test-data now=2024-07-01T12:00:00Z
{"method": "transaction.void", "params": {"guid": "c0000000000000000000000000000000", "reason": "Duplicate"}}
{"result": {"void": true, "voidReason": "Duplicate", "voidTime": "2024-07-01T12:00:00Z", "readOnly": "Transaction Voided", "notes": "Voided transaction", "splits": [{"value": {"num": "0", "denom": "100"}, "reconcileState": "v", "…": "…"}, {"reconcileState": "v", "…": "…"}], "…": "…"}}
```
