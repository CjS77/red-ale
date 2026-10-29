<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/reconcile.status
related: method/reconcile.start, method/reconcile.finish
---
Recomputes a reconciliation for a statement and a set of ticks, so you can see the `difference` change as you tick
splits. Pass `ticked` (the splits ticked so far; leave it out to start from GnuCash's pre-ticked cleared splits) and
`mark` or `unmark` to add or remove some. A tick that is not one of the listed splits is refused with `invalid`.
Nothing is changed.

## Examples

Ticking the 15.00 payment lowers the reconciled balance by 15.00, so the account is 15.00 off the statement:

```example fixture=reports/reconcile
{"method": "reconcile.status", "params": {"account": "Root:Asset:Bank", "statementDate": "1970-03-01", "endingBalance": "21.00", "mark": ["d0000000000000000000000000000006"]}}
{"result": {"tickedBalance": {"num": "1400", "denom": "100"}, "reconciledBalance": {"num": "600", "denom": "100"}, "difference": {"num": "1500", "denom": "100"}, "…": "…"}}
```
