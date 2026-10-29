<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/reconcile.setState
related: method/reconcile.finish, method/transaction.get
---
Sets the reconcile state of one split: `n` (new), `c` (cleared), `y` (reconciled) or `f` (frozen). Setting `y` stamps
the reconcile date with `date` (an RFC 3339 timestamp, default now), which GnuCash's engine leaves to its reconcile
window; red-ale stamps it so a reconciled split always says when. Prefer [reconcile.finish](method/reconcile.finish)
to reconcile a whole statement.

`v` (void) cannot be set; it comes only from [transaction.void](method/transaction.void), and the splits of a voided
transaction cannot be changed (`conflict`).

## Examples

```example fixture=reports/reconcile
{"method": "reconcile.setState", "params": {"split": "d0000000000000000000000000000000", "state": "c"}}
{"result": {"split": "d0000000000000000000000000000000", "transaction": "c0000000000000000000000000000000", "state": "c", "reconcileDate": null}}
{"method": "reconcile.setState", "params": {"split": "d0000000000000000000000000000002", "state": "c"}}
{"error": {"code": 2003, "message": "…", "data": {"help": "error/conflict"}}}
```
