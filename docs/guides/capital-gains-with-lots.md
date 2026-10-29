<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/capital-gains-with-lots
title: Capital gains with lots
summary: Match each sale of shares to the purchases it came from by putting them in lots, so the book records the realised gain of every sale as GnuCash does.
related: method/lot.scrubAccount, method/lot.create, method/lot.assignSplit, method/lot.computeGains, method/lot.list, method/lot.scrub, guide/buy-and-sell-shares
---
When you buy the same security several times at different prices and sell part of it, the gain depends on *which* shares
you sold. A **lot** is a group of splits in one security account that opens with a purchase and closes when its
shares are all sold. Putting each sale's shares into the lots they came from fixes the cost of each, and so the
gain, a figure tax rules often ask for. The usual rule is first in, first out (FIFO): a sale uses the oldest lot first.

The gain is written into the book as a split of its own in an income account, in the same transaction as the sale, so
the books keep balancing in dollars.

## Steps

1. **Record the buys and the sale** as in [Buy and sell shares](guide/buy-and-sell-shares). At this point the sale's value
   is its proceeds; no gain is recorded anywhere.
2. **Assign by FIFO** with [lot.scrubAccount](method/lot.scrubAccount). It puts the account's splits into lots, splits a
   sale that spans two lots into one piece per lot, and calculates the gains. It lists the lots, transactions and accounts
   it created or changed; the gains go to an income account it creates, `Orphaned Gains-USD`.
3. **Look at the lots** with [lot.list](method/lot.list): each lot's open and close dates, its remaining shares
   (`balance`), whether it is `closed`, and its `realizedGains`.
4. **Choose the lots yourself?** Create a lot with [lot.create](method/lot.create), put splits in it one at a time with
   [lot.assignSplit](method/lot.assignSplit) and its `lot` (a split larger than the lot needs is cut into pieces and the
   rest is returned as `rest`), and compute its gains with [lot.computeGains](method/lot.computeGains). To fix a lot after other
   edits, use [lot.scrub](method/lot.scrub).
5. **Use your own gains account.** Pass `gainsAccount` to [lot.computeGains](method/lot.computeGains) to credit an
   income account you made, instead of `Orphaned Gains-USD`.

## Example

Twenty shares are bought in two lots, at 150.00 and at 170.00, and fifteen are sold for 2,700.00 (180.00 each). FIFO
sells all of the first lot, a gain of 300.00, and five of the second, a gain of 50.00.

```example now=2024-07-01T12:00:00Z
{"method": "commodity.create", "params": {"namespace": "NASDAQ", "mnemonic": "AAPL", "fullname": "Apple Inc"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Brokerage", "type": "ASSET", "commodity": "USD", "placeholder": true}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Cash", "type": "BANK", "parent": "Brokerage"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "AAPL", "type": "STOCK", "parent": "Brokerage", "commodity": "NASDAQ::AAPL"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Opening Balances", "type": "EQUITY", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-01-02", "description": "Deposit", "splits": [{"account": "Brokerage:Cash", "value": "10000.00"}, {"account": "Opening Balances", "value": "-10000.00"}]}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-02-01", "description": "Buy 10 AAPL", "currency": "USD", "splits": [{"account": "Brokerage:AAPL", "value": "1500.00", "amount": "10"}, {"account": "Brokerage:Cash", "value": "-1500.00"}]}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-03-01", "description": "Buy 10 AAPL", "currency": "USD", "splits": [{"account": "Brokerage:AAPL", "value": "1700.00", "amount": "10"}, {"account": "Brokerage:Cash", "value": "-1700.00"}]}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-06-03", "description": "Sell 15 AAPL", "currency": "USD", "splits": [{"account": "Brokerage:Cash", "value": "2700.00"}, {"account": "Brokerage:AAPL", "value": "-2700.00", "amount": "-15"}]}}
{"result": "…"}
{"method": "lot.scrubAccount", "params": {"account": "Brokerage:AAPL"}}
{"result": {"lotsCreated": ["…", "…"], "accountsCreated": ["…"], "…": "…"}}
{"method": "lot.list", "params": {"account": "Brokerage:AAPL"}}
{"result": [{"closed": true, "openDate": "2024-02-01", "closeDate": "2024-06-03", "realizedGains": {"num": "30000", "denom": "100"}, "…": "…"}, {"closed": false, "openDate": "2024-03-01", "realizedGains": {"num": "5000", "denom": "100"}, "…": "…"}]}
```

To choose lots by hand, create one and assign a split to it:

```example fixture=reports/investment-lots
{"method": "lot.create", "params": {"account": "Root:Assets:Broker:AAPL"}}
{"result": {"closed": false, "splitCount": 0, "…": "…"}}
{"method": "lot.assignSplit", "params": {"split": "d0000000000000000000000000000001"}}
{"result": {"pieces": [{"split": "d0000000000000000000000000000001", "lot": "…"}], "rest": null, "newLots": ["…"]}}
```
