<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/reconcile.autoClear
related: method/reconcile.findAutoClear, method/reconcile.finish, error/invalid
---
Auto-clears an account: marks cleared (`c`) the one set of uncleared splits that brings its cleared balance to
`clearedBalance`. It is [reconcile.findAutoClear](method/reconcile.findAutoClear) followed by setting those splits
cleared, in one step. If no set reaches the figure, it is refused with `invalid` and `data.reason` `unreachable`, and
nothing is cleared.

## Examples

```example fixture=reports/reconcile
{"method": "reconcile.autoClear", "params": {"account": "Root:Asset:Bank", "clearedBalance": "6.00"}}
{"result": {"outcome": "found", "splits": [{"split": "d0000000000000000000000000000006", "…": "…"}], "…": "…"}}
{"method": "reconcile.autoClear", "params": {"account": "Root:Asset:Bank", "clearedBalance": "99.00"}}
{"error": {"code": 2001, "message": "…", "data": {"reason": "unreachable", "help": "error/invalid"}}}
```
