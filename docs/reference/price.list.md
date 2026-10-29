<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/price.list
related: method/price.get, method/price.latest, method/price.nearest
---
Lists prices in the order GnuCash writes them: by commodity, then currency, newest first. Give `commodity` to list one
commodity's prices, `currency` as well to narrow it to a pair, `bothDirections` to include the pair's prices the other
way round, and `from` and `to` (inclusive local days) to limit the dates. The result is a plain array.

A book may hold several prices of a pair on one day (loading a file applies no same-day rule).

## Examples

```example fixture=reports/test-data
{"method": "price.list", "params": {"commodity": "GBP", "currency": "USD", "bothDirections": true}}
{"result": [{"guid": "e0000000000000000000000000000001", "commodity": "CURRENCY::GBP", "currency": "CURRENCY::USD", "value": {"num": "3", "denom": "2"}, "…": "…"}, {"guid": "e0000000000000000000000000000000", "commodity": "CURRENCY::USD", "currency": "CURRENCY::GBP", "value": {"num": "5", "denom": "7"}, "…": "…"}]}
```
