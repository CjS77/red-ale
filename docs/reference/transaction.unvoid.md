<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/transaction.unvoid
related: method/transaction.void
---
Restores a voided transaction: the values and amounts come back from the slots void left, the notes are restored, the
splits' state is reset to `n` and the transaction is editable again. A transaction that is not void is refused with
`conflict`, where GnuCash does nothing.

## Examples

```example fixture=reports/test-data
{"method": "transaction.unvoid", "params": {"guid": "c0000000000000000000000000000001"}}
{"result": {"void": false, "readOnly": null, "splits": [{"reconcileState": "n", "…": "…"}, {"reconcileState": "n", "…": "…"}], "…": "…"}}
```
