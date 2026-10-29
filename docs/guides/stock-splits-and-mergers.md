<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/stock-splits-and-mergers
title: Stock splits and mergers
summary: Record a stock split with stock.split, which adds shares at no cost; record a merger by hand with ordinary transactions.
related: method/stock.split, method/transaction.post, method/account.balance, method/price.add, guide/buy-and-sell-shares
from: guide/ch_invest.docbook#invest-splitsnmergers1
---
A **stock split** gives you more shares for the ones you have, without changing what the holding is worth: two-for-one
doubles your shares and halves each one's price. The cost of the holding stays the same. A **merger** or takeover
exchanges your shares of one company for shares of another, or for cash.

## Stock split

1. Work out how many shares you *gain*. A two-for-one split of 90 shares gives 90 more; the `shares` you pass is the
   increase, and a negative number records a reverse split.
2. Call [stock.split](method/stock.split) with the share `account`, the `date` and `shares`. Add `price` (the new
   price per share) if you want it recorded in the price database. It posts a transaction that adds shares with a value of
   zero, so no cash moves and the cost basis is unchanged.
3. Check the new share count with [account.balance](method/account.balance).

If a different price is already stored for that day, `blockingPrice` shows it and the new price is not stored.

## Merger

There is no merger method. Record one as ordinary transactions, which keeps your records easy to follow:

1. In the old share account, post a transaction that takes out the old shares at their cost: old shares `-` (value the
   cost, `amount` the old share count).
2. In a new share account for the new company, bring in the new shares with the same total value: new shares `+` (value
   the cost, `amount` the new shares).
3. If you also received cash, add a cash split, and put any gain in an income account, as in
   [Buy and sell shares](guide/buy-and-sell-shares).
4. Add the new company's price with [price.add](method/price.add).

## Example

```example fixture=reports/investment-lots
{"method": "stock.split", "params": {"account": "Root:Assets:Broker:AAPL", "date": "2021-01-01", "shares": "90", "price": "55.00"}}
{"result": {"transaction": "…", "priceStatus": "added", "price": "…", "blockingPrice": null}}
{"method": "account.balance", "params": {"account": "Root:Assets:Broker:AAPL"}}
{"result": {"balances": {"commodity": "NASDAQ::AAPL", "balance": {"num": "180", "denom": "1"}, "…": "…"}, "…": "…"}}
```
