<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/reconcile.info
related: method/reconcile.setInfo, method/reconcile.start
---
Reads an account's reconcile information: the date of the last reconciliation, the interval between statements, a
postponed reconciliation (its date and balance) and whether sub-accounts are included. All null for an account never
reconciled. [reconcile.start](method/reconcile.start) uses the last date plus the interval as the next statement date.

## Examples

```example fixture=reports/reconcile
{"method": "reconcile.info", "params": {"account": "Root:Asset:Bank"}}
{"result": {"lastDate": null, "lastInterval": null, "postponeDate": null, "postponeBalance": null, "includeSubaccounts": false, "…": "…"}}
```
