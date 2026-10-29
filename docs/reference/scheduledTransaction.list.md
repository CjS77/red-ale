<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scheduledTransaction.list
related: method/scheduledTransaction.get, method/scheduledTransaction.sinceLastRun
---
Lists scheduled transactions in book order, without their template transactions. Filter to the enabled or the disabled
ones, or to those whose template splits use a given account (to see what depends on an account before changing it).

## Examples

```example fixture=synthetic/rich
{"method": "scheduledTransaction.list", "params": {}}
{"result": {"scheduledTransactions": [{"guid": "52f96837087e9670c933f9421b0ac626", "name": "Monthly rent", "enabled": true, "…": "…"}]}}
{"method": "scheduledTransaction.list", "params": {"enabled": false}}
{"result": {"scheduledTransactions": []}}
```
