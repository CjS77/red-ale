<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/price.latest
related: method/price.nearest, method/price.list
---
Gives the newest price of a commodity in a currency, looking in both directions. The result has the stored `price`,
`reversed` (true when the price is stored the other way round) and `rate`, the value in the direction you asked for:
the exact inverse when reversed. The result is null when there is no price.

## Examples

```example fixture=reports/test-data
{"method": "price.latest", "params": {"commodity": "USD", "currency": "GBP"}}
{"result": {"price": {"guid": "e0000000000000000000000000000001", "…": "…"}, "reversed": true, "rate": {"num": "2", "denom": "3"}}}
```
