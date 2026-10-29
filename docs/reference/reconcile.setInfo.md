<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/reconcile.setInfo
related: method/reconcile.info
---
Sets or removes an account's reconcile information: the last reconcile date, the interval between statements
(`{months, days}`), a postponed reconciliation (`{date, balance}`) and whether sub-accounts are included. A field you
leave out is kept; `null` removes it. Use it to correct the dates after importing a book, or to drop a postponed
reconciliation you no longer want.

## Examples

```example fixture=reports/reconcile
{"method": "reconcile.setInfo", "params": {"account": "Root:Asset:Bank", "lastInterval": {"months": 1, "days": 0}}}
{"result": {"lastInterval": {"months": 1, "days": 0}, "lastDate": null, "…": "…"}}
```
