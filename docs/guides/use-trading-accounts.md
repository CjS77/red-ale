<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/use-trading-accounts
title: Use trading accounts
summary: Turn on the Use Trading Accounts book option so every transaction that mixes currencies balances in each currency, and see the Trading accounts it creates.
related: method/book.setOptions, method/book.options, method/transaction.post, method/account.list, guide/multiple-currencies
---
A transaction in two currencies balances in *value*, but each currency taken alone does not: you gave up 108.50 dollars and
received 100.00 euros. Trading accounts fix that. With the option on, red-ale adds balancing splits to a hidden tree of
`TRADING` accounts (`Trading:CURRENCY:USD`, `Trading:CURRENCY:EUR`), so that the splits in each currency sum to zero on
their own. The Trading account for each currency then shows how much of it you have bought or sold, which is what
accountants use to find a currency gain or loss.

It is a book option, so it is stored in the book and GnuCash honours it when it opens the file.

## Steps

1. **Turn it on** with [book.setOptions](method/book.setOptions) and `useTradingAccounts: true`. No existing transaction
   changes; only transactions you post or edit afterwards get trading splits. [book.options](method/book.options) shows
   the current setting.
2. **Post as usual** with [transaction.post](method/transaction.post). Give the transaction's `currency`, and an
   `amount` on each split whose account is in another commodity (see [Multiple currencies](guide/multiple-currencies)).
   red-ale adds the trading splits itself.
3. **Look at the result.** The transaction has two more splits than you sent. [account.list](method/account.list) now
   includes the `Trading` accounts, which are placeholders: do not post to them yourself.

Turning the option off stops new trading splits from being added, but keeps the ones already there.

## Example

```example now=2024-07-15T12:00:00Z
{"method": "book.setOptions", "params": {"useTradingAccounts": true}}
{"result": {"useTradingAccounts": true, "…": "…"}}
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Euro Bank", "type": "BANK", "commodity": "EUR"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-02", "description": "Buy euros", "currency": "USD", "splits": [{"account": "Euro Bank", "value": "108.50", "amount": "100.00"}, {"account": "Checking", "value": "-108.50"}]}}
{"result": {"balanced": true, "splits": [{"accountName": "Euro Bank", "…": "…"}, {"accountName": "Trading:CURRENCY:USD", "…": "…"}, {"accountName": "Checking", "…": "…"}, {"accountName": "Trading:CURRENCY:EUR", "value": {"num": "-10850", "denom": "100"}, "amount": {"num": "-10000", "denom": "100"}, "…": "…"}], "…": "…"}}
```
