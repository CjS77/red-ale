<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/reconcile.postpone
related: method/reconcile.start, method/reconcile.finish, method/reconcile.info
---
Postpones a reconciliation, GnuCash's Postpone button: the ticked splits become cleared (`c`) and every other listed
split goes back to new (`n`), and the statement date and ending balance are kept in the account so the next
[reconcile.start](method/reconcile.start) picks them up (`resumedPostponed`). Use it when the statement does not add
up yet and you want to carry on later.

Pitfall: a split that was cleared but is not in `ticked` loses its cleared state, so pass every split you want kept.

## Examples

```example fixture=reports/reconcile
{"method": "reconcile.postpone", "params": {"account": "Root:Asset:Bank", "statementDate": "1970-03-01", "endingBalance": "1.00", "ticked": ["d0000000000000000000000000000000"]}}
{"result": {"changed": [{"split": "d0000000000000000000000000000005", "from": "c", "to": "n", "…": "…"}, {"split": "d0000000000000000000000000000000", "from": "n", "to": "c", "…": "…"}], "…": "…"}}
{"method": "reconcile.start", "params": {"account": "Root:Asset:Bank"}}
{"result": {"resumedPostponed": true, "endingBalance": {"num": "100", "denom": "100"}, "…": "…"}}
```
