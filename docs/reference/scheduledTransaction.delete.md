<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scheduledTransaction.delete
related: method/scheduledTransaction.update, method/scheduledTransaction.list
---
Deletes a scheduled transaction with its template account and template transactions. The transactions it already
created stay in the book. To keep it but stop it running, set `enabled: false` with
[scheduledTransaction.update](method/scheduledTransaction.update).

## Examples

```example fixture=synthetic/rich
{"method": "scheduledTransaction.delete", "params": {"scheduledTransaction": "52f96837087e9670c933f9421b0ac626"}}
{"result": {"deleted": true}}
{"method": "scheduledTransaction.list", "params": {}}
{"result": {"scheduledTransactions": []}}
```
