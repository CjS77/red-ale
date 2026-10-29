<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/lot.computeGains
related: method/lot.scrub, method/lot.get
---
Recomputes a lot's capital gains: for each selling split, the gain is its value less the basis of the shares it sells,
and it is recorded in a two-split gains transaction, `Realized Gain/Loss`, in the lot and in the gains account (or
`gainsAccount`). A gain of zero records nothing, and a gains transaction that is already right is not rewritten.
Unlike [lot.scrub](method/lot.scrub) it does not move splits between lots.

## Examples

```example fixture=reports/lots-baseline
{"method": "lot.computeGains", "params": {"lot": "10000000000000000000000000000000"}}
{"result": {"lotsCreated": [], "transactionsCreated": [], "transactionsChanged": [], "…": "…"}}
```
