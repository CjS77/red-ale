<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/record-prices
title: Record prices and commodities
summary: Add a security, record its price by hand, turn on online quotes, and tidy old prices out of the price database.
related: method/commodity.create, method/commodity.update, method/price.add, method/price.list, method/price.fetchQuotes, method/price.fetchStatus, method/price.removeOld, method/config.set, error/network-disabled
---
Reports that show what your shares are worth, and accounts held in a foreign currency, need prices: how many dollars one
share, or one euro, was worth on a day. The book keeps them in its **price database**, one price per commodity,
currency and day. You can enter prices by hand, or let red-ale fetch them from an online quote service.

## Steps

1. **Add the commodity** with [commodity.create](method/commodity.create) (see
   [Buy and sell shares](guide/buy-and-sell-shares)). Currencies are already known and need no creating.
2. **Enter a price by hand** with [price.add](method/price.add): the `commodity`, the `currency` the price is in, the
   `value` (currency units per commodity unit, as an exact string) and the `date`. A price already stored for the same pair
   and day is replaced, or kept, according to which source is more trusted, as in GnuCash; `outcome` says what happened.
3. **Look at the prices** with [price.list](method/price.list), by commodity, currency and dates.
4. **Fetch prices online.** Mark the commodity with [commodity.update](method/commodity.update): `quoteFlag: true` and a
   `quoteSource` such as `yahoo_json`. Then switch on the network, which is off by default, with
   [config.set](method/config.set) (`{"changes": {"quotes.network": true}}`), and call
   [price.fetchQuotes](method/price.fetchQuotes). It answers at once with a fetch ID; ask
   [price.fetchStatus](method/price.fetchStatus) for the result per commodity. Until the network is switched on the call
   fails with [network-disabled](error/network-disabled) and sends nothing. Over MCP the setting cannot be changed, so a
   person turns it on.
5. **Tidy the database** with [price.removeOld](method/price.removeOld): choose the commodities, a `before` date, which
   `sources` may go (`fetched`, `user` or `app`) and what to `keep` (the last price of each week, month or quarter, or
   `none`).

## Example

```example now=2024-07-01T12:00:00Z
{"method": "commodity.create", "params": {"namespace": "NASDAQ", "mnemonic": "AAPL", "fullname": "Apple Inc"}}
{"result": {"id": "NASDAQ::AAPL", "…": "…"}}
{"method": "commodity.update", "params": {"commodity": "NASDAQ::AAPL", "quoteFlag": true, "quoteSource": "yahoo_json"}}
{"result": {"quoteFlag": true, "quoteSource": "yahoo_json", "…": "…"}}
{"method": "price.add", "params": {"commodity": "NASDAQ::AAPL", "currency": "USD", "value": "150.25", "date": "2023-12-29"}}
{"result": {"outcome": "added", "…": "…"}}
{"method": "price.add", "params": {"commodity": "NASDAQ::AAPL", "currency": "USD", "value": "187.10", "date": "2024-06-28"}}
{"result": {"outcome": "added", "…": "…"}}
{"method": "price.list", "params": {"commodity": "NASDAQ::AAPL"}}
{"result": [{"date": "2024-06-28", "value": {"num": "18710", "denom": "100"}, "…": "…"}, {"date": "2023-12-29", "value": {"num": "15025", "denom": "100"}, "…": "…"}]}
{"method": "price.fetchQuotes", "params": {}}
{"error": {"code": 5000, "message": "…", "data": {"help": "error/network-disabled"}}}
{"method": "price.removeOld", "params": {"commodities": ["NASDAQ::AAPL"], "before": "2024-01-01", "sources": ["app"], "keep": "none"}}
{"result": {"removed": ["…"]}}
```
