<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/lot.freeSplits
related: method/lot.assignSplit, method/lot.scrubAccount
---
Lists the splits of an account that are in no lot, as the lot viewer's free splits. With `lot`, each also shows the
gain it would realise against that lot. Use it to see what [lot.assignSplit](method/lot.assignSplit) or
[lot.scrubAccount](method/lot.scrubAccount) still has to place.

## Examples

```example fixture=reports/investment-lots
{"method": "lot.freeSplits", "params": {"account": "Root:Assets:Broker:AAPL"}}
{"result": [{"split": "d0000000000000000000000000000001", "description": "Buy AAPL lot 1", "amount": {"num": "100", "denom": "1"}, "…": "…"}, {"split": "d0000000000000000000000000000004", "description": "Buy AAPL lot 2", "…": "…"}, {"split": "d0000000000000000000000000000007", "description": "Sell AAPL", "amount": {"num": "-60", "denom": "1"}, "…": "…"}]}
```
