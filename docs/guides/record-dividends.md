<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/record-dividends
title: Record dividends and return of capital
summary: Record a cash dividend as income, a reinvested dividend as new shares, and a return of capital as a reduction in cost.
related: method/transaction.post, method/account.create, method/account.balance, guide/buy-and-sell-shares
---
A company that shares its profit pays a *dividend*. You can take it as cash, or have it reinvested in more shares. A
*return of capital* looks the same on the statement but is not income: it hands part of your original investment back, so
it lowers the cost you paid for the shares. All three are ordinary transactions.

You need the accounts of [Buy and sell shares](guide/buy-and-sell-shares) and an income account for dividends.

## Steps

1. **Cash dividend.** The dividend is income, so credit an income account, and the cash lands in your brokerage cash
   account: cash `+`, `Dividends` `-`.
2. **Reinvested dividend.** The dividend is income and you spend it at once on shares: the share account `+` with its
   `amount` in shares and its `value` in dollars, and `Dividends` `-`. No cash moves. Set `currency`, since the first
   split may be in shares.
3. **Return of capital.** Cash arrives and the cost of the shares falls: cash `+`, and the share account `-` with a value
   and an `amount` of `0`. The shares are the same; they cost less.
4. **Foreign withholding tax or fees** are further expense splits in the same transaction.
5. **Check** with [account.balance](method/account.balance).

## Example

```example now=2024-07-31T12:00:00Z
{"method": "commodity.create", "params": {"namespace": "NASDAQ", "mnemonic": "AAPL", "fullname": "Apple Inc"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Cash", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "AAPL", "type": "STOCK", "commodity": "NASDAQ::AAPL"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Dividends", "type": "INCOME", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Opening Balances", "type": "EQUITY", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-06-01", "description": "Opening", "currency": "USD", "splits": [{"account": "AAPL", "value": "1500.00", "amount": "10"}, {"account": "Opening Balances", "value": "-1500.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-10", "description": "Cash dividend", "splits": [{"account": "Cash", "value": "24.00"}, {"account": "Dividends", "value": "-24.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-15", "description": "Reinvested dividend", "currency": "USD", "splits": [{"account": "AAPL", "value": "24.00", "amount": "0.15"}, {"account": "Dividends", "value": "-24.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-20", "description": "Return of capital", "currency": "USD", "splits": [{"account": "Cash", "value": "50.00"}, {"account": "AAPL", "value": "-50.00", "amount": "0"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "account.balance", "params": {"account": "AAPL"}}
{"result": {"balances": {"commodity": "NASDAQ::AAPL", "balance": {"num": "101500", "denom": "10000"}, "…": "…"}, "…": "…"}}
{"method": "account.balance", "params": {"account": "Dividends"}}
{"result": {"balances": {"balance": {"num": "-4800", "denom": "100"}, "…": "…"}, "…": "…"}}
```
