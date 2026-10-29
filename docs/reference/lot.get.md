<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/lot.get
related: method/lot.list, method/lot.update
---
Describes one lot with its splits. Each split shows its amount, its adjusted amount (the amount restated in the units
after any later stock split), its value, the capital gain it realised and its `kind`: `normal`, `gains` or
`stockSplit`. Reading never writes: gains that are out of date are reported as recorded, where GnuCash's viewer
recomputes them. Use [lot.computeGains](method/lot.computeGains) to bring them up to date.

## Examples

```example fixture=reports/lots-baseline
{"method": "lot.get", "params": {"lot": "10000000000000000000000000000000"}}
{"result": {"guid": "10000000000000000000000000000000", "closed": false, "balance": {"num": "100", "denom": "1"}, "splits": [{"split": "d0000000000000000000000000000000", "description": "Buy AAPL lot baseline", "amount": {"num": "100", "denom": "1"}, "value": {"num": "500000", "denom": "100"}, "kind": "normal", "…": "…"}], "…": "…"}}
```
