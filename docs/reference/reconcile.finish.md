<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/reconcile.finish
related: method/reconcile.start, method/reconcile.postpone, method/reconcile.info, error/invalid
---
Finishes a reconciliation: every ticked split becomes reconciled (`y`) with the statement date as its reconcile date,
and the statement date becomes the account's last reconcile date. A postponed reconciliation is cleared.

It is refused with `invalid` unless the `difference` is zero, the message saying how far off it is; `force: true`
finishes anyway, as GnuCash does when you confirm "The account is not balanced". Unticked splits keep their state.

## Examples

```example fixture=reports/reconcile
{"method": "reconcile.finish", "params": {"account": "Root:Asset:Bank", "statementDate": "1970-03-01", "endingBalance": "21.00", "ticked": ["d0000000000000000000000000000005"]}}
{"result": {"reconciled": [{"split": "d0000000000000000000000000000005", "transaction": "c0000000000000000000000000000002", "from": "c", "to": "y"}], "difference": {"num": "0", "denom": "100"}, "…": "…"}}
{"method": "reconcile.info", "params": {"account": "Root:Asset:Bank"}}
{"result": {"lastDate": "…", "postponeDate": null, "…": "…"}}
```

An unbalanced statement is refused:

```example fixture=reports/reconcile
{"method": "reconcile.finish", "params": {"account": "Root:Asset:Bank", "statementDate": "1970-03-01", "endingBalance": "9.00", "ticked": []}}
{"error": {"code": 2001, "message": "…", "data": {"help": "error/invalid"}}}
```
