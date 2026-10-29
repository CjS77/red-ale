<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/buy-and-sell-shares
title: Buy and sell shares
summary: Set up a security and a brokerage account, record a purchase with its commission, record a sale with the realised gain, and keep the share price up to date.
related: method/commodity.create, method/account.create, method/transaction.post, method/price.add, method/account.balance, method/lot.assignSplit, concept/investments, gnucash/stock-assistants
from: guide/ch_invest.docbook#chapter_invest
---
Shares are a commodity that is not a currency. A holding of shares is an account of type `STOCK` whose commodity is the
security, so its balance counts shares, not dollars. In a transaction, the share account's split has an *amount* in shares
and a *value* in the transaction's currency: ten shares at 150.00 are an amount of 10 and a value of 1,500.00.

## Steps

1. **Create the security** with [commodity.create](method/commodity.create): a `namespace` (the exchange, such as
   `NASDAQ`), a `mnemonic` (the ticker) and a `fullname`.
2. **Create the accounts.** A brokerage placeholder (`ASSET`), a cash account for it (`BANK`), a `STOCK` account per
   security with `commodity` set to the security's ID (`NASDAQ::AAPL`), an expense for commissions and an income account
   for realised gains.
3. **Buy.** [transaction.post](method/transaction.post) in the cash account's currency (set `currency`, since the first
   split may be in shares): the share account `+value` with `amount` in shares, the commission expense `+`, and the cash
   account `-` for the total.
4. **Record the price** with [price.add](method/price.add), the commodity priced in the currency, so reports can value the
   holding. A buy or sell in the register records its price too if you pass `recordPrice: true`.
5. **Sell.** The cash account gets the proceeds less commission. The share account is credited at what the shares cost
   (the shares' amount negative, their value the cost basis). The difference between proceeds and cost is the realised gain,
   credited to the gains income account, or debited if it is a loss.
6. **Check** the number of shares with [account.balance](method/account.balance); the balance is in shares.

For which shares were sold, with several purchases at different prices, see
[Capital gains with lots](guide/capital-gains-with-lots).

## Example

Buy 10 shares at 150.00 with a 9.95 commission, then sell 4 at 160.00. The cost of the 4 is 600.00; they are sold for
640.00, a gain of 40.00.

```example now=2024-07-31T12:00:00Z
{"method": "commodity.create", "params": {"namespace": "NASDAQ", "mnemonic": "AAPL", "fullname": "Apple Inc"}}
{"result": {"id": "NASDAQ::AAPL", "…": "…"}}
{"method": "account.create", "params": {"name": "Brokerage", "type": "ASSET", "commodity": "USD", "placeholder": true}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Cash", "type": "BANK", "parent": "Brokerage"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "AAPL", "type": "STOCK", "parent": "Brokerage", "commodity": "NASDAQ::AAPL"}}
{"result": {"commodity": "NASDAQ::AAPL", "…": "…"}}
{"method": "account.create", "params": {"name": "Commissions", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Realised gains", "type": "INCOME", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Opening Balances", "type": "EQUITY", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-01", "description": "Deposit", "splits": [{"account": "Brokerage:Cash", "value": "5000.00"}, {"account": "Opening Balances", "value": "-5000.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-02", "description": "Buy AAPL", "currency": "USD", "splits": [{"account": "Brokerage:AAPL", "value": "1500.00", "amount": "10"}, {"account": "Commissions", "value": "9.95"}, {"account": "Brokerage:Cash", "value": "-1509.95"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "price.add", "params": {"commodity": "NASDAQ::AAPL", "currency": "USD", "value": "160.00", "date": "2024-07-15"}}
{"result": {"outcome": "added", "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-15", "description": "Sell AAPL", "currency": "USD", "splits": [{"account": "Brokerage:Cash", "value": "630.05"}, {"account": "Commissions", "value": "9.95"}, {"account": "Brokerage:AAPL", "value": "-600.00", "amount": "-4"}, {"account": "Realised gains", "value": "-40.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "account.balance", "params": {"account": "Brokerage:AAPL"}}
{"result": {"balances": {"commodity": "NASDAQ::AAPL", "balance": {"num": "60000", "denom": "10000"}, "…": "…"}, "…": "…"}}
{"method": "account.balance", "params": {"account": "Brokerage:Cash"}}
{"result": {"balances": {"commodity": "CURRENCY::USD", "balance": {"num": "412010", "denom": "100"}, "…": "…"}, "…": "…"}}
```
