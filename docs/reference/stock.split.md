<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/stock.split
related: method/lot.scrubAccount, method/price.list, error/invalid
---
Records a stock split, as GnuCash's Stock Split assistant does: a transaction on `date` with one split in the account
adding `shares` (a negative number is a reverse split) at value zero, so the holding changes and the cost does not.
Give `shares` as the number of shares to add, not the ratio: a 2-for-1 split of 90 shares is `"90"`.

Optionally `price` records the new price per share (source `user:stock-split`), and `cash` pays cash in lieu of a
fraction, from `assetAccount` to `incomeAccount`. The result's `priceStatus` is `none`, `added`, `duplicate` or
`rejected`; a same-day price from a higher-ranked source blocks the new one (`blockingPrice`), and the split is still
recorded. The account must be `STOCK`, `MUTUAL` or `CURRENCY`.

## Examples

```example fixture=reports/investment-lots
{"method": "stock.split", "params": {"account": "Root:Assets:Broker:AAPL", "date": "2021-01-01", "shares": "90", "price": "55.00"}}
{"result": {"transaction": "…", "priceStatus": "added", "price": "…", "blockingPrice": null}}
{"method": "account.balance", "params": {"account": "Root:Assets:Broker:AAPL"}}
{"result": {"balances": {"commodity": "NASDAQ::AAPL", "balance": {"num": "180", "denom": "1"}, "…": "…"}, "…": "…"}}
```

A cash account is not a holding that can split:

```example fixture=reports/investment-lots
{"method": "stock.split", "params": {"account": "Root:Assets:Broker:Cash", "date": "2021-01-01", "shares": "90"}}
{"error": {"code": 2001, "message": "…", "data": {"help": "error/invalid"}}}
```
