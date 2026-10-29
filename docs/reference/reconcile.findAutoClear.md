<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/reconcile.findAutoClear
related: method/reconcile.autoClear, method/reconcile.start
---
Finds, without changing anything, the one set of uncleared splits that would bring an account's cleared balance to
`clearedBalance`, as GnuCash's Auto-clear button searches. `outcome` says whether it was `found`; the `splits` are
those to clear. If several sets reach the figure, or none does, no choice is made for you.

## Examples

```example fixture=reports/reconcile
{"method": "reconcile.findAutoClear", "params": {"account": "Root:Asset:Bank", "clearedBalance": "6.00"}}
{"result": {"currentClearedBalance": {"num": "2100", "denom": "100"}, "outcome": "found", "splits": [{"split": "d0000000000000000000000000000006", "description": "desc-4", "amount": {"num": "-1500", "denom": "100"}, "…": "…"}], "…": "…"}}
```
