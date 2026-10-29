<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/price.nearest
related: method/price.latest, method/price.list
---
Gives the price of a commodity in a currency nearest to a `date` or `time`, in both directions. `mode` is `nearest`
(the default: the closer of the newest price at or before and the oldest after, a tie going to the older), `before`
(the newest at or before) or `sameDay`. GnuCash has no "nearest after", and neither does red-ale. Conversions at a
past date, such as a foreign-currency transfer, use this.

## Examples

```example fixture=reports/test-data
{"method": "price.nearest", "params": {"commodity": "GBP", "currency": "USD", "date": "2000-01-20"}}
{"result": {"price": {"guid": "e0000000000000000000000000000000", "date": "2000-01-15", "…": "…"}, "reversed": true, "rate": {"num": "7", "denom": "5"}}}
```
