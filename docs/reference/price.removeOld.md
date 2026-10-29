<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/price.removeOld
related: method/price.delete, method/period.get
---
GnuCash's Edit > Prices > Remove Old Prices: removes prices of the named `commodities` (in every currency) from before
`before`, keeping the last price of each week, month, quarter or fiscal year (`keep`: `none`, `lastOfWeek` (the
default), `lastOfMonth`, `lastOfQuarter`, `lastOfPeriod` or `scaled`). Only prices from the chosen `sources` are
candidates: `fetched` (the default; `Finance::Quote`), `user` (only `user:price`) and `app` (every other source,
the price editor's and the transfer dialog's included).

Pitfall: GnuCash compares periods by number only, so January 2022 is "the same month" as January 2023; red-ale
reproduces that, so thinning prices over several years can remove more than you expect. Use `lastOfPeriod` for a
fiscal-year comparison without it.

## Examples

```example fixture=reports/test-data
{"method": "price.removeOld", "params": {"commodities": ["GBP"], "before": "2001-01-01", "sources": ["app"], "keep": "none"}}
{"result": {"removed": ["e0000000000000000000000000000001"]}}
```
